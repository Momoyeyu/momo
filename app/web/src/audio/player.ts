// 音频 chunk 调度播放：PCM s16le 24kHz mono → AudioWorklet 环形缓冲逐帧排播
// （momo-app.md §前端）。骨架阶段：采样解码与排队已实现，Worklet 排播为 TODO。

import { FLAG_EOS } from "../net/protocol";
import type { AudioHeader } from "../net/protocol";

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
  tag: number;
  samples: Float32Array;
  eos: boolean;
}

/** 下行音频排播器；打断时 stop() 清空播放队列（与后端 control.cancel 对齐）。 */
export class AudioPlayer {
  private ctx: AudioContext | null = null;
  private queue: PlayedChunk[] = [];

  /** 按需创建 AudioContext（浏览器要求用户手势后启动）。 */
  ensureContext(): AudioContext {
    this.ctx ??= new AudioContext({ sampleRate: 24000 });
    return this.ctx;
  }

  /** 收到一个 audio.chunk：解码并入队。 */
  enqueue(header: AudioHeader, payload: Uint8Array): void {
    this.queue.push({
      tag: header.tag,
      samples: s16leToFloat32(payload),
      eos: (header.flags & FLAG_EOS) !== 0,
    });
    // TODO(P0): AudioWorklet 环形缓冲逐帧排播 + worklet 内算 RMS → lipsync。
  }

  /** barge-in：清空播放队列。 */
  stop(): void {
    this.queue = [];
    // TODO(P0): 停掉 worklet 输出。
  }

  /** 待播帧数（测试/调试用）。 */
  get pending(): number {
    return this.queue.length;
  }
}
