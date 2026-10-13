// 端到端对拍：连真实 momod 的 /ws/ui 走 v2 hello + 二进制音频帧。
// 默认跳过；设 MOMO_WS_E2E_URL=ws://127.0.0.1:7800/ws/ui（可选
// MOMO_WS_E2E_ORIGIN，默认 http://localhost:5173）后 `pnpm test` 生效。
// node ≥22 的全局 WebSocket（undici）支持 headers 注入 Origin。

import { describe, expect, it } from "vitest";
import { DIR_UPLINK, encodeAudioFrame, newId, newUlid } from "./protocol";

// node 侧 env 读取（tsconfig types 只含 vite/client，这里补最小声明）。
declare const process: { env: Record<string, string | undefined> };

const URL_ENV = process.env.MOMO_WS_E2E_URL;
const ORIGIN = process.env.MOMO_WS_E2E_ORIGIN ?? "http://localhost:5173";

interface RawMsg {
  type: "text" | "binary" | "close";
  text?: string;
  bytes?: Uint8Array;
  code?: number;
  reason?: string;
}

/** 开一条 ws 并把收到的事件推入队列；await ready 后方可 send，await take() 逐条消费。 */
function connect(): {
  ready: Promise<void>;
  send: (data: string | ArrayBufferView<ArrayBuffer>) => void;
  take: () => Promise<RawMsg>;
  close: () => void;
} {
  const queue: RawMsg[] = [];
  let wake: (() => void) | null = null;
  // undici 扩展签名：第二参 {headers}（DOM 类型只声明 protocols，此处断言）。
  const ws = new WebSocket(URL_ENV!, {
    headers: { Origin: ORIGIN },
  } as unknown as string[]);
  ws.binaryType = "arraybuffer";
  const push = (m: RawMsg) => {
    queue.push(m);
    wake?.();
    wake = null;
  };
  const ready = new Promise<void>((resolve, reject) => {
    ws.onopen = () => resolve();
    ws.onerror = () => reject(new Error("ws e2e: connect failed"));
  });
  ws.onmessage = (ev) => {
    if (typeof ev.data === "string") push({ type: "text", text: ev.data });
    else push({ type: "binary", bytes: new Uint8Array(ev.data as ArrayBuffer) });
  };
  ws.onclose = (ev) => push({ type: "close", code: ev.code, reason: ev.reason });
  return {
    ready,
    send: (d) => ws.send(d),
    close: () => ws.close(),
    take: () =>
      new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("ws e2e: message timeout")), 5000);
        const poll = () => {
          const m = queue.shift();
          if (m) {
            clearTimeout(timer);
            return resolve(m);
          }
          wake = poll;
        };
        poll();
      }),
  };
}

describe.skipIf(!URL_ENV)("ws e2e（需运行 momod）", () => {
  it("v2 hello → hello.ok，二进制帧被接受，非法帧回 error", async () => {
    const c = connect();
    await c.ready;
    c.send(
      JSON.stringify({
        type: "hello",
        protocol: "momo-ws",
        version: "v2",
        client: "ui",
        auth_token: "dev",
      }),
    );
    const helloOk = await c.take();
    expect(helloOk.type).toBe("text");
    const ok = JSON.parse(helloOk.text!);
    expect(ok).toMatchObject({ type: "hello.ok", version: "v2", role: "orchestrator" });
    const sessionId = ok.session_id as string;
    expect(sessionId).toMatch(/^sess_/);

    // capture 生命周期信令 + 上行音频帧（协议字段同 capture.ts 发出的形态）。
    const captureId = newId("cap");
    const streamId = newId("st");
    const env = () => ({ session_id: sessionId, event_id: newUlid(), at_ms: Date.now() });
    c.send(JSON.stringify({ ...env(), type: "input.audio.start", capture_id: captureId, mode: "ptt" }));
    c.send(
      JSON.stringify({
        ...env(),
        type: "audio.open",
        stream_id: streamId,
        direction: "uplink",
        capture_id: captureId,
        codec: "pcm_s16le",
        sample_rate: 16000,
        channels: 1,
      }),
    );
    c.send(
      encodeAudioFrame(
        {
          direction: DIR_UPLINK,
          flags: 0,
          stream_id: streamId,
          chunk_seq: 0,
          sentence_seq: 0,
          sample_offset: 0,
        },
        new Uint8Array(1600), // 50ms @16k s16le
      ),
    );
    // 非法帧（reserved flag）→ error BAD_MESSAGE，连接保持。
    const bad = encodeAudioFrame(
      {
        direction: DIR_UPLINK,
        flags: 0,
        stream_id: streamId,
        chunk_seq: 1,
        sentence_seq: 0,
        sample_offset: 800,
      },
      new Uint8Array([0, 0]),
    );
    bad[3] = 0x02;
    c.send(bad);
    // input.audio.start 已先回 input.accepted（ref_id=capture_id）排在 error 之前，
    // 逐条消费直到取到 BAD_MESSAGE；顺带校验 accepted 回执确实到达。
    let sawBadMessage = false;
    let sawAccepted = false;
    // audio.open 会懒连 svc-asr：无该服务时 MODEL_UNREACHABLE 属预期先发 error，
    // 不能断言「第一条 error」；BAD_MESSAGE 走 conn_rx decode 直发，
    // 与 accepted/session 侧帧序不保证——drain 到两者都见（或上限）再断言。
    for (let i = 0; i < 8 && !(sawBadMessage && sawAccepted); i++) {
      const m = await c.take();
      if (m.type !== "text") continue;
      const j = JSON.parse(m.text!);
      if (j.type === "input.accepted" && j.ref_id === captureId) sawAccepted = true;
      if (j.type === "error" && j.code === "BAD_MESSAGE") sawBadMessage = true;
    }
    expect(sawAccepted).toBe(true);
    expect(sawBadMessage).toBe(true);
    c.close();
  });

  it("version v1 hello → error + close 4001", async () => {
    const c = connect();
    await c.ready;
    c.send(
      JSON.stringify({
        type: "hello",
        protocol: "momo-ws",
        version: "v1",
        client: "ui",
        auth_token: "dev",
      }),
    );
    const err = await c.take();
    expect(JSON.parse(err.text!)).toMatchObject({ type: "error", code: "VERSION_MISMATCH" });
    const close = await c.take();
    expect(close.type).toBe("close");
    expect(close.code).toBe(4001);
  });

  it("hello.client != ui → close 4400", async () => {
    const c = connect();
    await c.ready;
    c.send(
      JSON.stringify({
        type: "hello",
        protocol: "momo-ws",
        version: "v2",
        client: "orchestrator",
        auth_token: "dev",
      }),
    );
    const close = await c.take();
    // 服务端先回 error 帧再 close（顺序以实现为准），消费到 close 为止。
    const msgs = [close];
    while (msgs[msgs.length - 1].type !== "close") msgs.push(await c.take());
    expect(msgs[msgs.length - 1].code).toBe(4400);
  });

  it("新连接完成握手顶掉旧连接：旧连接收 close 1000", async () => {
    const a = connect();
    const b = connect();
    await Promise.all([a.ready, b.ready]);
    const hello = JSON.stringify({
      type: "hello",
      protocol: "momo-ws",
      version: "v2",
      client: "ui",
      auth_token: "dev",
    });
    a.send(hello);
    expect(JSON.parse((await a.take()).text!)).toMatchObject({ type: "hello.ok" });
    b.send(hello);
    expect(JSON.parse((await b.take()).text!)).toMatchObject({ type: "hello.ok" });
    const last = await a.take();
    expect(last.type).toBe("close");
    expect(last.code).toBe(1000);
    b.close();
  });
});
