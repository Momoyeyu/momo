// 音频 chunk 调度播放：PCM s16le 24kHz mono → AudioWorklet 环形缓冲逐帧排播
// （momo-app.md §模块结构）。worklet 内算输出 RMS → mouthOpenY 平滑 →
// onMouthOpen 回调写 Live2D ParamMouthOpenY；stop/drain 口型复位。

import { FLAG_EOS } from "../net/protocol";
import type { AudioHeader } from "../net/protocol";
import { assetBase } from "../config";
import { mouthOpenY } from "../live2d/lipsync";

/** s16le 字节流 → Float32 采样（-1..1）。纯函数，单测覆盖。 */
export function s16leToFloat32(pcm: Uint8Array): Float32Array {
  const view = new DataView(pcm.buffer, pcm.byteOffset, pcm.byteLength - (pcm.byteLength % 2));
  const out = new Float32Array(Math.floor(pcm.byteLength / 2));
  for (let i = 0; i < out.length; i++) {
    out[i] = view.getInt16(i * 2, true) / 0x8000;
  }
  return out;
}

export interface PlayedChunk {
  /** 所属 output.sentence.seq（帧头 sentence_seq，protocol §6）。 */
  sentence_seq: number;
  samples: Float32Array;
  eos: boolean;
  /** 已推入 worklet 的标记（playback ledger 骨架，playback ACK 复用）。 */
  sent: boolean;
}

/** 下行音频排播器；打断时 stop() 清空播放队列（与后端 control.cancel 对齐）。 */
export class AudioPlayer {
  /** 嘴型输出（0..1）；main.ts 接到 stage.setMouthOpen。 */
  onMouthOpen: ((value: number) => void) | null = null;

  private ctx: AudioContext | null = null;
  private node: AudioWorkletNode | null = null;
  private initPromise: Promise<void> | null = null;
  private queue: PlayedChunk[] = [];
  private mouth = 0;

  /** 按需创建 AudioContext（浏览器要求用户手势后启动）。 */
  ensureContext(): AudioContext {
    this.ctx ??= new AudioContext({ sampleRate: 24000 });
    return this.ctx;
  }

  /** 加载 playback worklet 并接线（幂等）；首个 audio.chunk 时调用。 */
  init(): Promise<void> {
    this.initPromise ??= this.setup();
    return this.initPromise;
  }

  private async setup(): Promise<void> {
    const ctx = this.ensureContext();
    await ctx.audioWorklet.addModule(`${assetBase()}worklets/pcm-player.js`);
    const node = new AudioWorkletNode(ctx, "pcm-player", {
      numberOfInputs: 0,
      outputChannelCount: [1],
    });
    node.port.onmessage = (ev: MessageEvent) => {
      const msg = ev.data as { type?: string; rms?: number } | null;
      if (msg?.type === "rms" && typeof msg.rms === "number") {
        this.feedRms(msg.rms);
      } else if (msg?.type === "drain") {
        // 播完最后一段：chunk ledger 清账，口型归零。
        this.queue = this.queue.filter((c) => !c.sent);
        this.resetMouth();
      }
    };
    node.connect(ctx.destination);
    this.node = node;
    this.flushPending();
  }

  /** 收到一个 audio.chunk：解码入队；worklet 就绪后推送采样。 */
  enqueue(header: AudioHeader, payload: Uint8Array): void {
    const chunk: PlayedChunk = {
      sentence_seq: header.sentence_seq,
      samples: s16leToFloat32(payload),
      eos: (header.flags & FLAG_EOS) !== 0,
      sent: false,
    };
    this.queue.push(chunk);
    if (this.node) this.push(chunk);
  }

  /** barge-in：清空播放队列并让 worklet 立即静音，口型复位。 */
  stop(): void {
    this.queue = [];
    this.node?.port.postMessage({ type: "clear" });
    this.resetMouth();
  }

  /** 待播帧数（测试/调试用；含已推入 worklet 未播完的）。 */
  get pending(): number {
    return this.queue.length;
  }

  /** worklet 建立后补推积压的 chunk（保持到达顺序）。 */
  private flushPending(): void {
    for (const chunk of this.queue) {
      if (!chunk.sent) this.push(chunk);
    }
  }

  private push(chunk: PlayedChunk): void {
    chunk.sent = true;
    this.node?.port.postMessage({ type: "push", samples: chunk.samples }, [chunk.samples.buffer]);
  }

  /** worklet RMS → attack/release 平滑 → 0..1 张嘴幅度。 */
  private feedRms(rms: number): void {
    this.mouth = mouthOpenY(rms, this.mouth);
    this.onMouthOpen?.(this.mouth);
  }

  private resetMouth(): void {
    this.mouth = 0;
    this.onMouthOpen?.(0);
  }
}
