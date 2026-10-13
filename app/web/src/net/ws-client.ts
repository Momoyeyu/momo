//

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

      console.warn("ws not open, drop msg", msg.type);
    }
  }

  sendAudio(header: AudioHeader, payload: Uint8Array): void {
    if (this.ws?.readyState !== WebSocket.OPEN) return;
  }

  private open(): void {
    this.setState("connecting");
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
        type: "hello",
        protocol: PROTOCOL_NAME,
        version: PROTOCOL_VERSION,
        client: "ui",
    };

    ws.onmessage = (ev: MessageEvent) => {
      if (typeof ev.data === "string") {
        const msg = parseServerMsg(ev.data);
        if (!msg) {
          console.warn("unparseable server frame", ev.data);
          return;
        }
        if (msg.type === "hello.ok") {
        }
        this.onServerMsg?.(msg);
      } else if (ev.data instanceof ArrayBuffer) {
        const decoded = decodeAudioFrame(ev.data);
        if (decoded) this.onAudioChunk?.(decoded.header, decoded.payload);
      }
    };

      this.ws = null;
      this.setState("closed");
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
