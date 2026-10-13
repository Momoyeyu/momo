// 上行 PCM 整形单测：重采样 / 50ms 切帧 / s16le 转换（protocol §6）。

import { describe, expect, it } from "vitest";
import { FRAME_SAMPLES, Resampler, UplinkFramer, floatToS16le } from "./pcm";

describe("floatToS16le", () => {
  it("量化与截断正确", () => {
    const out = floatToS16le(new Float32Array([0, 0.5, -0.5, 1, -1, 2]));
    const view = new DataView(out.buffer);
    expect(view.getInt16(0, true)).toBe(0);
    expect(view.getInt16(2, true)).toBe(Math.floor(0.5 * 0x7fff)); // 截断取整
    expect(view.getInt16(4, true)).toBe(-0.5 * 0x8000);
    expect(view.getInt16(6, true)).toBe(0x7fff);
    expect(view.getInt16(8, true)).toBe(-0x8000);
    expect(view.getInt16(10, true)).toBe(0x7fff); // 超幅 clamp
  });
});

describe("Resampler", () => {
  it("同采样率累积透传（末 sample 随下一块衔接，不丢）", () => {
    const r = new Resampler(16000);
    const first = r.push(new Float32Array([0.1, 0.2, 0.3]));
    const second = r.push(new Float32Array([0.4]));
    // Float32 量化后与输入逐点相等
    expect(Array.from(first)).toEqual(Array.from(new Float32Array([0.1, 0.2])));
    expect(Array.from(second)).toEqual(Array.from(new Float32Array([0.3])));
  });

  it("48k→16k 长度 1/3，且分块喂入与整段一致", () => {
    const src = new Float32Array(4800).map((_, i) => Math.sin(i / 10));
    const whole = new Resampler(48000).push(src);
    const chunks = new Resampler(48000);
    const part: number[] = [];
    for (let off = 0; off < src.length; off += 128) {
      part.push(...chunks.push(src.subarray(off, off + 128)));
    }
    expect(whole.length).toBe(1600); // pos: 0,3,…,4797 各产一点
    expect(part.length).toBe(whole.length);
    for (let i = 0; i < whole.length; i++) {
      expect(part[i]).toBeCloseTo(whole[i], 6);
    }
  });
});

describe("UplinkFramer", () => {
  it("按 800-sample 定长切帧；残余 flush 收尾", () => {
    const framer = new UplinkFramer(16000);
    // 2000 samples @16k → 重采样 1999（末 sample 待下一块衔接）→ 2 整帧 + 399 残余
    const frames = framer.push(new Float32Array(2000).fill(0.25));
    expect(frames.length).toBe(2);
    for (const f of frames) expect(f.byteLength).toBe(FRAME_SAMPLES * 2);
    const tail = framer.flush();
    expect(tail).not.toBeNull();
    expect(tail!.byteLength).toBe(399 * 2);
    expect(framer.flush()).toBeNull();
  });

  it("小块累积凑帧（模拟 worklet 128-sample 回调）", () => {
    const framer = new UplinkFramer(48000);
    let emitted = 0;
    let bytes = 0;
    // 100 个 128-sample chunk @48k = 12800 samples → 16k 后 4266 → 5 整帧 + 残余
    for (let i = 0; i < 100; i++) {
      for (const f of framer.push(new Float32Array(128).fill(0.1))) {
        emitted += 1;
        bytes += f.byteLength;
      }
    }
    expect(emitted).toBe(5);
    expect(bytes).toBe(5 * 1600);
    const tail = framer.flush();
    expect(tail).not.toBeNull();
    expect(tail!.byteLength % 2).toBe(0);
  });
});
