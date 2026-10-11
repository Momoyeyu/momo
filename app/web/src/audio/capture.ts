// 麦克风采集 + 分段上送（P2，momo-app.md §前端）。
//
// 链路：getUserMedia → 重采样 16k mono → 按协议上送二进制 audio.chunk 上行帧
// （STREAM_UPLINK，50ms/帧，tag=当前 VAD 段计数由 ASR 回写）。
// 前端侧不做 VAD：断句在 ASR 服务侧（silero-vad），转写以 input.audio.segment 回流。

import { STREAM_UPLINK } from "../net/protocol";
import type { AudioHeader } from "../net/protocol";

export class MicCapture {
  private stream: MediaStream | null = null;
  private seq = 0;

  /** 请求麦克风并开始上行推送。P2 实装。 */
  async start(_send: (header: AudioHeader, payload: Uint8Array) => void): Promise<void> {
    // TODO(P2): navigator.mediaDevices.getUserMedia({audio}) → AudioWorklet
    // 重采样 16kHz mono → s16le → send({stream: STREAM_UPLINK, seq: this.seq++, ...}, pcm)
    void STREAM_UPLINK;
    void this.seq;
    throw new Error("MicCapture.start not implemented yet (P2)");
  }

  stop(): void {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
  }
}
