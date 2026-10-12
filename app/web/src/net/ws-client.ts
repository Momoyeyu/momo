// momo-ws 客户端：自动重连、二进制帧分发、状态事件（momo-app.md §前端）。
//
// 重连退避：指数 0.5s→8s，±20% 抖动（protocol §5）。
// 握手：open 后第一帧必须 hello{protocol:"momo-ws",version:"v1",client:"ui"}。

import {
  PROTOCOL_NAME,
  PROTOCOL_VERSION,
  decodeAudioFrame,
  parseServerMsg,
} from "./protocol";
import type {
  AudioHeader,
  ClientMsg,
  HelloOkMsg,
  ServerMsg,
} from "./protocol";

export type WsConnState = "connecting" | "open" | "closed";

const BACKOFF_MIN_MS = 500;
const BACKOFF_MAX_MS = 8000;
const BACKOFF_JITTER = 0.2;

/** 指数退避：0.5s→8s，±20% 抖动（protocol §5）。纯函数便于测试。 */
export function backoffDelay(attempt: number, rand: () => number = Math.random): number {
  const base = Math.min(BACKOFF_MAX_MS, BACKOFF_MIN_MS * 2 ** attempt);
  const jitter = base * BACKOFF_JITTER * (rand() * 2 - 1);
  return Math.max(0, Math.round(base + jitter));
}

export class WsClient {
  /** 收到服务端 JSON 消息（hello.ok/state.turn/output.sentence/...）。 */
  onServerMsg: ((msg: ServerMsg) => void) | null = null;
  /** 收到二进制 audio.chunk（下行 TTS 音频）。 */
  onAudioChunk: ((header: AudioHeader, payload: Uint8Array) => void) | null = null;
  /** 连接状态变化（驱动 ui/status）。 */
  onState: ((state: WsConnState, helloOk?: HelloOkMsg) => void) | null = null;

  private ws: WebSocket | null = null;
  private attempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private manualClose = false;

  private readonly url: string;

  constructor(url: string) {
    this.url = url;
  }

  connect(): void {
    this.manualClose = false;
    this.open();
  }

  close(): void {
    this.manualClose = true;
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    this.setState("closed");
  }

  /** 发送 JSON 文本帧；未连接时丢弃并告警（P0 可加离线队列）。 */
  send(msg: ClientMsg): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    } else {
      console.warn("ws not open, drop msg", msg.type);
    }
  }

  /** 发送上行 audio.chunk 二进制帧（P2 麦克风上送）。 */
  sendAudio(header: AudioHeader, payload: Uint8Array): void {
    if (this.ws?.readyState !== WebSocket.OPEN) return;
    const frame = new Uint8Array(12 + payload.length);
    const view = new DataView(frame.buffer);
    view.setUint8(0, 1);
    view.setUint8(1, header.stream);
    view.setUint16(2, header.flags);
    view.setUint32(4, header.seq);
    view.setUint32(8, header.tag);
    frame.set(payload, 12);
    this.ws.send(frame);
  }

  private open(): void {
    this.setState("connecting");
    const ws = new WebSocket(this.url);
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
      // 第一帧必须是 hello（protocol §4）。
      this.send({
        type: "hello",
        protocol: PROTOCOL_NAME,
        version: PROTOCOL_VERSION,
        client: "ui",
        audio: {
          uplink: { codec: "pcm_s16le", sample_rate: 16000, channels: 1, chunk_ms: 50 },
          downlink: { codec: "pcm_s16le", sample_rate: 24000, channels: 1, chunk_ms: 50 },
        },
      });
    };

    ws.onmessage = (ev: MessageEvent) => {
      if (typeof ev.data === "string") {
        const msg = parseServerMsg(ev.data);
        if (!msg) {
          console.warn("unparseable server frame", ev.data);
          return;
        }
        if (msg.type === "hello.ok") {
          this.attempts = 0;
          this.setState("open", msg);
        }
        this.onServerMsg?.(msg);
      } else if (ev.data instanceof ArrayBuffer) {
        const decoded = decodeAudioFrame(ev.data);
        if (decoded) this.onAudioChunk?.(decoded.header, decoded.payload);
      }
    };

    ws.onclose = () => {
      this.ws = null;
      this.setState("closed");
      if (!this.manualClose) {
        const delay = backoffDelay(this.attempts++);
        this.reconnectTimer = setTimeout(() => this.open(), delay);
      }
    };

    ws.onerror = () => {
      // onclose 统一处理重连。
    };
  }

  private setState(state: WsConnState, helloOk?: HelloOkMsg): void {
    this.onState?.(state, helloOk);
  }
}
