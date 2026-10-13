// 上行 PCM 整形：设备原生采样率 Float32 → 16kHz mono s16le → 50ms 定长帧
// （protocol §6：上行 16kHz mono、50ms/帧 = 800 samples、pcm_s16le）。
// 纯函数/纯类实现，不依赖 Web Audio，vitest 可直接对拍。

/** 上行协议采样率与帧长（protocol §6）。 */
export const UPLINK_SAMPLE_RATE = 16000;
export const FRAME_SAMPLES = 800; // 50ms @ 16kHz

/** Float32（-1..1）→ PCM s16le 字节；长度必须为偶数语义由调用方保证。 */
export function floatToS16le(samples: Float32Array): Uint8Array {
  const out = new Uint8Array(samples.length * 2);
  const view = new DataView(out.buffer);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }
  return out;
}

/**
 * 流式线性插值重采样器：分块喂入原生采样率 Float32，产出 16kHz Float32。
 * 跨块边界的插值用上一块末尾 sample 衔接，输出与一次性整段重采样一致。
 */
export class Resampler {
  /** 源流中的下一读取位置（相对下一块输入起点；负值表示与 tail 跨界插值）。 */
  private pos = 0;
  /** 上一块输入的最后一个 sample（跨界插值锚点）。 */
  private tail = 0;
  private readonly step: number;

  constructor(srcRate: number, dstRate: number = UPLINK_SAMPLE_RATE) {
    if (srcRate <= 0 || dstRate <= 0) throw new Error("sample rates must be positive");
    this.step = srcRate / dstRate;
  }

  push(input: Float32Array): Float32Array {
    const out: number[] = [];
    let pos = this.pos;
    // 源位置 t 读取 input[floor(t)] 与 input[floor(t)+1]；
    // 只有 floor(t)+1 落在本块内才能产出（否则留给下一块）。
    while (pos < input.length - 1) {
      const i = Math.floor(pos);
      const frac = pos - i;
      const a = i < 0 ? this.tail : input[i];
      const b = input[i + 1];
      out.push(a + (b - a) * frac);
      pos += this.step;
    }
    this.pos = pos - input.length;
    if (input.length > 0) this.tail = input[input.length - 1];
    return new Float32Array(out);
  }
}

/**
 * 重采样 + 定长切帧器：push 原生 Float32 chunk，吐出 800-sample 的
 * s16le 完整帧；不足一帧的残余累积在内部，收尾用 flush() 取出。
 */
export class UplinkFramer {
  private readonly resampler: Resampler;
  private buf: Float32Array = new Float32Array(0);

  constructor(srcRate: number) {
    this.resampler = new Resampler(srcRate, UPLINK_SAMPLE_RATE);
  }

  /** 喂入一段原生采样率 Float32，返回 0..n 个完整 50ms 帧（s16le，1600B/帧）。 */
  push(input: Float32Array): Uint8Array[] {
    const resampled = this.resampler.push(input);
    if (resampled.length === 0) return [];
    const merged = new Float32Array(this.buf.length + resampled.length);
    merged.set(this.buf);
    merged.set(resampled, this.buf.length);
    const frames: Uint8Array[] = [];
    const whole = Math.floor(merged.length / FRAME_SAMPLES) * FRAME_SAMPLES;
    for (let off = 0; off < whole; off += FRAME_SAMPLES) {
      frames.push(floatToS16le(merged.subarray(off, off + FRAME_SAMPLES)));
    }
    this.buf = merged.subarray(whole);
    return frames;
  }

  /** 收尾：残余不足一帧的 samples 作为一个短帧返回（s16le）；无残余返回 null。 */
  flush(): Uint8Array | null {
    if (this.buf.length === 0) return null;
    const out = floatToS16le(this.buf);
    this.buf = new Float32Array(0);
    return out;
  }
}
