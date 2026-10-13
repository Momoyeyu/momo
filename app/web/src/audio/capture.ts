//
// 链路：getUserMedia({audio}) → AudioWorklet 回传 Float32 chunk →
// 重采样 16kHz mono → s16le → 50ms/帧切片 → v2 32B 二进制帧上行
// （direction=1 uplink、stream_id=audio.open 绑定的 ULID、chunk_seq/
// sample_offset 从 0 累计、sentence_seq=0、EOS 仅标尾帧）。
//
// capture 生命周期（protocol §2）：
//   start:  input.audio.start{capture_id,mode} → audio.open{uplink,stream_id,capture_id}
//   end:    残余 PCM 尾帧 → 空 payload EOS 帧 → audio.end{last_chunk_seq} → input.audio.end
//   cancel: input.audio.cancel（不发 EOS/audio.end；stream 直接作废）
// 前端不做 VAD：断句在 svc-asr 侧 silero-vad，转写以 input.audio.segment 回流。
// 权限拒绝/设备缺失：释放 mic、start() 抛错，调用方回退文字路径。

import { DIR_UPLINK, FLAG_EOS, newId } from "../net/protocol";
import type { AudioHeader, CaptureMode, ClientBody } from "../net/protocol";
import { UplinkFramer } from "./pcm";

/** 上层（ws-client）注入的发送面；envelope 由 WsClient.send 注入。 */
export interface MicCaptureSink {
  /** 业务 JSON 消息（input.audio.* / audio.open / audio.end）。 */
  send(msg: ClientBody): void;
  /** 上行 audio.chunk 二进制帧。 */
  sendAudio(header: AudioHeader, payload: Uint8Array): void;
}

export interface MicCaptureOptions {
  /** PTT 按住说话 / 连续麦（前端不做 VAD，两种模式上行行为一致）。 */
  mode: CaptureMode;
  /** capture 结束后异步错误回调（设备断开、worklet 失败等）。 */
  onError?: (err: unknown) => void;
}

// AudioWorklet 处理器源码：把 input channel 0 的 Float32 chunk 原样回传主线程。
// 以 Blob URL 加载，dev/build/Tauri 三态一致、不经 bundler。
const WORKLET_NAME = "momo-pcm-capture";
const WORKLET_SOURCE = `class MomoPcmCapture extends AudioWorkletProcessor {
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (ch) this.port.postMessage(ch);
    return true;
  }
}
registerProcessor("${WORKLET_NAME}", MomoPcmCapture);
`;

/** 麦克风采集器：一次活动 capture；start/end/cancel 生命周期幂等。 */
export class MicCapture {
  private stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private node: AudioWorkletNode | null = null;
  private workletUrl: string | null = null;

  private sink: MicCaptureSink | null = null;
  private framer: UplinkFramer | null = null;
  private onError: ((err: unknown) => void) | null = null;

  private captureId = "";
  private streamId = "";
  private chunkSeq = 0;
  private sampleOffset = 0;
  private active = false;

  /** 当前 capture id（start 前为空串）。 */
  get currentCaptureId(): string {
    return this.captureId;
  }

  get capturing(): boolean {
    return this.active;
  }

  /**
   * 请求麦克风并开始上行推送。
   * 权限拒绝/设备缺失/Worklet 加载失败：释放已占用资源并抛错（保留文字路径）。
   */
  async start(sink: MicCaptureSink, opts: MicCaptureOptions): Promise<void> {
    if (this.active) this.cancel();
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    try {
      this.stream = stream;
      this.sink = sink;
      this.onError = opts.onError ?? null;
      this.captureId = newId("cap");
      this.streamId = newId("st");
      this.chunkSeq = 0;
      this.sampleOffset = 0;

      // 开流信令先行（protocol §2：stream 先由 audio.open 绑定再发二进制帧）。
      sink.send({ type: "input.audio.start", capture_id: this.captureId, mode: opts.mode });
      sink.send({
        type: "audio.open",
        stream_id: this.streamId,
        direction: "uplink",
        capture_id: this.captureId,
        codec: "pcm_s16le",
        sample_rate: 16000,
        channels: 1,
      });

      // AudioContext 用设备原生采样率，重采样在 UplinkFramer 内完成
      // （强制 sampleRate:16000 的构造在部分 webview 上不受支持）。
      this.ctx = new AudioContext();
      if (!this.workletUrl) {
        this.workletUrl = URL.createObjectURL(
          new Blob([WORKLET_SOURCE], { type: "text/javascript" }),
        );
      }
      await this.ctx.audioWorklet.addModule(this.workletUrl);
      await this.ctx.resume();

      // end/cancel 在异步间隙被调用（sink/stream 已被 teardown 清空）→ 直接退出。
      if (!this.sink || this.stream !== stream) return;

      this.framer = new UplinkFramer(this.ctx.sampleRate);
      this.source = this.ctx.createMediaStreamSource(stream);
      this.node = new AudioWorkletNode(this.ctx, WORKLET_NAME);
      this.node.port.onmessage = (ev: MessageEvent<Float32Array>) => this.onPcm(ev.data);
      this.source.connect(this.node);
      // 处理器不产出声音，接 destination 仅为驱动渲染线程拉流。
      this.node.connect(this.ctx.destination);

      // 设备拔除/系统回收 → 取消 capture（不发 EOS），错误回给上层提示。
      const track = stream.getAudioTracks()[0];
      if (track) {
        track.addEventListener("ended", () => {
          if (!this.active) return;
          this.cancel();
          this.onError?.(new Error("microphone track ended"));
        });
      }
      this.active = true;
    } catch (err) {
      // 信令已发出（input.audio.start/audio.open）→ 取消 capture 生命周期，
      // 避免服务端悬挂一个永不来数据的 stream。
      if (this.sink) {
        this.sink.send({ type: "input.audio.cancel", capture_id: this.captureId });
      }
      this.teardown();
      throw err;
    }
  }

  /**
   * 正常结束（PTT 松开 / speech_end 收尾）：
   * 残余 PCM 尾帧 → 空 EOS 帧 → audio.end → input.audio.end → 释放 mic。
   */
  end(): void {
    // 在 start() 异步段内被调用也照常收尾：audio.open 已发出，stream 需要正常关闭。
    const sink = this.sink;
    if (!sink) return;
    // flush 残余：不足 50ms 的尾帧按普通数据帧送出（payload 非空、偶数字节）。
    const tail = this.framer?.flush() ?? null;
    if (tail && tail.byteLength > 0) this.pushFrame(sink, tail, 0);
    // EOS：空 payload 收尾帧（对齐 TTS 下行同款形态）；audio.end.last_chunk_seq 与之对齐。
    this.pushFrame(sink, new Uint8Array(0), FLAG_EOS);
    sink.send({ type: "audio.end", stream_id: this.streamId, last_chunk_seq: this.chunkSeq - 1 });
    sink.send({ type: "input.audio.end", capture_id: this.captureId });
    this.teardown();
  }

  /** 取消（窗口失焦/主动取消）：input.audio.cancel + 释放 mic，不发 EOS/audio.end。 */
  cancel(): void {
    if (!this.sink) return;
    this.sink.send({ type: "input.audio.cancel", capture_id: this.captureId });
    this.teardown();
  }

  /** worklet 回传的 Float32 chunk → 重采样 → 50ms 帧 → 上送。 */
  private onPcm(chunk: Float32Array): void {
    if (!this.active || !this.sink || !this.framer) return;
    for (const frame of this.framer.push(chunk)) {
      this.pushFrame(this.sink, frame, 0);
    }
  }

  private pushFrame(sink: MicCaptureSink, payload: Uint8Array, flags: number): void {
    sink.sendAudio(
      {
        direction: DIR_UPLINK,
        flags,
        stream_id: this.streamId,
        chunk_seq: this.chunkSeq,
        sentence_seq: 0, // uplink 固定 0（protocol §6）
        sample_offset: this.sampleOffset,
      },
      payload,
    );
    this.chunkSeq += 1;
    this.sampleOffset += payload.byteLength / 2; // mono：字节数/2 = 每声道 sample 数
  }

  private teardown(): void {
    this.active = false;
    this.sink = null;
    this.framer = null;
    this.onError = null;
    this.captureId = "";
    this.streamId = "";
    this.release();
  }

  /** 释放音频图与 mic；幂等（start 失败路径也走这里）。 */
  private release(): void {
    this.node?.disconnect();
    this.source?.disconnect();
    this.node = null;
    this.source = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    // AudioContext 随 capture 生灭，close 异步不阻塞。
    if (this.ctx && this.ctx.state !== "closed") {
      void this.ctx.close().catch(() => {});
    }
    this.ctx = null;
  }
}
