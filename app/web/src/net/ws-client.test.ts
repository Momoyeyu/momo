// WsClient 单测：fake WebSocket 注入，验证 v2 握手、envelope 注入、
// close code 重连策略与二进制帧上行（protocol §4/§5/§10）。

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WsClient } from "./ws-client";
import { AUDIO_HEADER_LEN, AUDIO_VER, decodeAudioFrame } from "./protocol";
import type { AudioHeader } from "./protocol";

/** 最小 fake socket：记录 send 帧、可手动触发事件。 */
class FakeSocket {
  static instances: FakeSocket[] = [];
  binaryType = "";
  readyState = 0; // CONNECTING
  onopen: (() => void) | null = null;
  onclose: ((ev: { code: number; reason: string }) => void) | null = null;
  onmessage: ((ev: { data: string | ArrayBuffer }) => void) | null = null;
  onerror: (() => void) | null = null;
  sent: (string | Uint8Array)[] = [];
  closed = false;

  constructor() {
    FakeSocket.instances.push(this);
  }

  send(data: string | Uint8Array): void {
    this.sent.push(data);
  }

  close(): void {
    this.closed = true;
  }

  // ---- 测试驱动 ----
  open(): void {
    this.readyState = 1;
    this.onopen?.();
  }

  recvText(text: string): void {
    this.onmessage?.({ data: text });
  }

  drop(code: number, reason = ""): void {
    this.onclose?.({ code, reason });
  }

  lastJson(): Record<string, unknown> {
    const last = this.sent[this.sent.length - 1];
    expect(typeof last).toBe("string");
    return JSON.parse(last as string) as Record<string, unknown>;
  }
}

const factory = () => new FakeSocket() as unknown as WebSocket;

function makeClient(token = "tok"): { client: WsClient; sock: () => FakeSocket } {
  const client = new WsClient("ws://127.0.0.1:7800/ws/ui", token, factory);
  return { client, sock: () => FakeSocket.instances[FakeSocket.instances.length - 1] };
}

describe("WsClient v2 握手", () => {
  beforeEach(() => {
    FakeSocket.instances = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("open 后首帧为 v2 hello（auth_token 必填）", () => {
    const { client, sock } = makeClient("secret-token");
    client.connect();
    sock().open();
    const hello = sock().lastJson();
    expect(hello).toMatchObject({
      type: "hello",
      protocol: "momo-ws",
      version: "v2",
      client: "ui",
      auth_token: "secret-token",
    });
    expect("resume_session_id" in hello).toBe(false);
    expect("session_id" in hello).toBe(false); // 握手无 envelope
  });

  it("hello.ok 后 send() 注入 envelope", () => {
    const { client, sock } = makeClient();
    client.connect();
    sock().open();
    sock().recvText(
      '{"type":"hello.ok","version":"v2","session_id":"sess_01J5M0N8T1RZ8K2W4V7H6X9P3A","role":"orchestrator","capabilities":[]}',
    );
    client.send({ type: "input.text", id: "r_x", text: "hi", lang: "auto" });
    const msg = sock().lastJson();
    expect(msg).toMatchObject({ type: "input.text", id: "r_x", text: "hi", lang: "auto" });
    expect(msg.session_id).toBe("sess_01J5M0N8T1RZ8K2W4V7H6X9P3A");
    expect(msg.event_id).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/);
    expect(typeof msg.at_ms).toBe("number");
  });

  it("sendAudio 产出 32B LE v2 二进制帧", () => {
    const { client, sock } = makeClient();
    client.connect();
    sock().open();
    const header: AudioHeader = {
      direction: 1,
      flags: 0,
      stream_id: "st_01J5M0N8T1RZ8K2W4V7H6X9P3E",
      chunk_seq: 0,
      sentence_seq: 0,
      sample_offset: 0,
    };
    client.sendAudio(header, new Uint8Array([1, 2]));
    // sent[0] 是 hello 文本帧，二进制帧在其后。
    const frame = sock().sent[1] as Uint8Array;
    expect(frame.byteLength).toBe(AUDIO_HEADER_LEN + 2);
    expect(frame[0]).toBe(AUDIO_VER);
    const decoded = decodeAudioFrame(frame);
    expect(decoded?.header.stream_id).toBe(header.stream_id);
  });
});

describe("close code 重连策略（protocol §10）", () => {
  beforeEach(() => {
    FakeSocket.instances = [];
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  function handshakeThen(code: number): number {
    const { client, sock } = makeClient();
    client.connect();
    sock().open();
    sock().drop(code);
    vi.runAllTimers();
    return FakeSocket.instances.length;
  }

  it("close 1000（superseded）不重连", () => {
    expect(handshakeThen(1000)).toBe(1);
  });

  it("close 4001 / 4401 终态不重连", () => {
    expect(handshakeThen(4001)).toBe(1);
    FakeSocket.instances = [];
    expect(handshakeThen(4401)).toBe(1);
  });

  it("异常断线退避重连，hello 带 resume_session_id", () => {
    FakeSocket.instances = [];
    const { client, sock } = makeClient();
    client.connect();
    sock().open();
    sock().recvText(
      '{"type":"hello.ok","version":"v2","session_id":"sess_01J5M0N8T1RZ8K2W4V7H6X9P3A","role":"orchestrator","capabilities":[]}',
    );
    sock().drop(1006); // abnormal
    vi.runAllTimers();
    expect(FakeSocket.instances.length).toBe(2);
    const second = FakeSocket.instances[1];
    second.open();
    expect(second.lastJson().resume_session_id).toBe("sess_01J5M0N8T1RZ8K2W4V7H6X9P3A");
  });
});
