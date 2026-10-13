// momo-ws v2 客户端：握手、自动重连 + resume、二进制帧分发（protocol §4/§5/§10）。
//
// 握手：open 后首帧 hello{protocol:"momo-ws", version:"v2", client:"ui",
// auth_token}；已持有 session 的重连带 resume_session_id。
// 重连退避：指数 0.5s→8s，±20% 抖动；close 1000（superseded）/4001/4401
// 为终态不重连——superseded 连接回连会把新活跃 UI 顶掉，auth/版本错误重试无意义。

import {
  CLOSE_NORMAL,
  CLOSE_UNAUTHORIZED,
  CLOSE_VERSION_MISMATCH,
  PROTOCOL_NAME,
  PROTOCOL_VERSION,
  decodeAudioFrame,
  encodeAudioFrame,
  newUlid,
  parseServerMsg,
} from "./protocol";
import type {
  AudioHeader,
  ClientBody,
  ClientMsg,
  Envelope,
  HelloOkMsg,
  ServerMsg,
} from "./protocol";

export type WsConnState = "connecting" | "open" | "closed";

/** 测试可注入的 socket 工厂；默认走 DOM WebSocket。 */
export type WsFactory = (url: string) => WebSocket;

const BACKOFF_MIN_MS = 500;
const BACKOFF_MAX_MS = 8000;
const BACKOFF_JITTER = 0.2;

/** 收到这些 close code 视为终态，不自动重连（protocol §10）。 */
const FATAL_CLOSE_CODES = new Set([
  CLOSE_NORMAL, // 1000：正常关闭 / 被新 UI 连接顶掉（superseded）
  CLOSE_VERSION_MISMATCH, // 4001：版本不匹配
  CLOSE_UNAUTHORIZED, // 4401：auth_token 校验失败
]);

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
  /** 当前 session id；重连 hello 带 resume_session_id（protocol §4）。 */
  private sessionId: string | null = null;
  /** 服务端事件水位：忽略 event_seq <= 水位的 duplicate/stale event。 */
  private lastEventSeq = 0;

  private readonly url: string;
  private readonly token: string;
  private readonly wsFactory: WsFactory;

  constructor(url: string, token: string, wsFactory?: WsFactory) {
    this.url = url;
    this.token = token;
    this.wsFactory = wsFactory ?? ((u) => new WebSocket(u));
  }

  /** 当前 session id（hello.ok 后可用）。 */
  get session(): string | null {
    return this.sessionId;
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

  /**
   * 发送业务 JSON 消息：自动注入 envelope
   * （session_id/event_id/at_ms，protocol §1）。
   * 未建立 session（hello.ok 前）或 socket 未 OPEN 时丢弃并告警。
   */
  send(msg: ClientBody): void {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      console.warn("ws not open, drop msg", msg.type);
      return;
    }
    if (!this.sessionId) {
      console.warn("ws session not ready, drop msg", msg.type);
      return;
    }
    const env: Envelope = {
      session_id: this.sessionId,
      event_id: newUlid(),
      at_ms: Date.now(),
    };
    this.sendJson({ ...env, ...msg });
  }

  /** 发送上行 audio.chunk 二进制帧（32B 头 + PCM s16le）。 */
  sendAudio(header: AudioHeader, payload: Uint8Array): void {
    if (this.ws?.readyState !== WebSocket.OPEN) return;
    this.ws.send(encodeAudioFrame(header, payload));
  }

  private sendJson(msg: ClientMsg): void {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      console.warn("ws not open, drop msg", msg.type);
      return;
    }
    this.ws.send(JSON.stringify(msg));
  }

  private open(): void {
    this.setState("connecting");
    const ws = this.wsFactory(this.url);
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
      // 首帧必须是 hello（protocol §4）；token 不放 query/日志/header。
      const hello: ClientMsg = {
        type: "hello",
        protocol: PROTOCOL_NAME,
        version: PROTOCOL_VERSION,
        client: "ui",
        auth_token: this.token,
        ...(this.sessionId ? { resume_session_id: this.sessionId } : {}),
      };
      this.sendJson(hello);
    };

    ws.onmessage = (ev: MessageEvent) => {
      if (typeof ev.data === "string") {
        const msg = parseServerMsg(ev.data);
        if (!msg) {
          console.warn("unparseable server frame", ev.data);
          return;
        }
        if (msg.type === "hello.ok") {
          this.onHelloOk(msg);
        } else if (msg.type === "session.snapshot") {
          // snapshot 用 last_event_seq 标记水位而非自身 event_seq（protocol §5）。
          this.lastEventSeq = Math.max(this.lastEventSeq, msg.last_event_seq);
        } else if ("event_seq" in msg && typeof msg.event_seq === "number") {
          // 忽略 duplicate/stale event（protocol §1）。
          if (msg.event_seq <= this.lastEventSeq) return;
          this.lastEventSeq = msg.event_seq;
        }
        this.onServerMsg?.(msg);
      } else if (ev.data instanceof ArrayBuffer) {
        const decoded = decodeAudioFrame(ev.data);
        if (decoded) this.onAudioChunk?.(decoded.header, decoded.payload);
      }
    };

    ws.onclose = (ev: CloseEvent) => {
      this.ws = null;
      this.setState("closed");
      if (this.manualClose || FATAL_CLOSE_CODES.has(ev.code)) {
        // 1000 superseded / 4001 / 4401 不重连；sessionId 保留供下一次手动 connect resume。
        return;
      }
      const delay = backoffDelay(this.attempts++);
      this.reconnectTimer = setTimeout(() => this.open(), delay);
    };

    ws.onerror = () => {
      // onclose 统一处理重连。
    };
  }

  private onHelloOk(msg: HelloOkMsg): void {
    // 换 session（resume 被拒绝下发新 session）→ 事件水位归零。
    if (this.sessionId !== msg.session_id) {
      this.sessionId = msg.session_id;
      this.lastEventSeq = 0;
    }
    this.attempts = 0;
    this.setState("open", msg);
  }

  private setState(state: WsConnState, helloOk?: HelloOkMsg): void {
    this.onState?.(state, helloOk);
  }
}
