// 协议编解码单测（与 tests/proto_roundtrip.rs 对拍同组 golden）。

import { describe, expect, it } from "vitest";
import {
  AUDIO_HEADER_LEN,
  FLAG_EOS,
  STREAM_DOWNLINK,
  decodeAudioFrame,
  encodeAudioFrame,
  newId,
  parseServerMsg,
} from "./protocol";

describe("audio.chunk 12B 头", () => {
  it("编码/解码 roundtrip", () => {
    const header = { stream: STREAM_DOWNLINK, flags: FLAG_EOS, seq: 42, tag: 3 };
    const payload = new Uint8Array([1, 2, 3, 4]);
    const frame = encodeAudioFrame(header, payload);
    expect(frame.length).toBe(AUDIO_HEADER_LEN + payload.length);
    expect(frame[0]).toBe(1); // ver
    const decoded = decodeAudioFrame(frame);
    expect(decoded?.header).toEqual(header);
    expect(Array.from(decoded!.payload)).toEqual([1, 2, 3, 4]);
  });

  it("短帧与错误 ver 解码失败", () => {
    expect(decodeAudioFrame(new Uint8Array(8))).toBeNull();
    const frame = encodeAudioFrame({ stream: 2, flags: 0, seq: 0, tag: 0 }, new Uint8Array());
    frame[0] = 9;
    expect(decodeAudioFrame(frame)).toBeNull();
  });
});

describe("server msg 解析", () => {
  it("hello.ok golden（protocol §3）", () => {
    const msg = parseServerMsg(
      JSON.stringify({
        type: "hello.ok",
        version: "v1",
        session_id: "sess_01JXXX",
        server: "momod",
        role: "orchestrator",
        capabilities: ["barge_in", "actions", "emotion_tags"],
      }),
    );
    expect(msg?.type).toBe("hello.ok");
    if (msg?.type === "hello.ok") {
      expect(msg.role).toBe("orchestrator");
      expect(msg.session_id).toMatch(/^sess_/);
    }
  });

  it("非法 JSON 返回 null", () => {
    expect(parseServerMsg("not json")).toBeNull();
  });
});

describe("id 生成", () => {
  it("带前缀且全局唯一", () => {
    const a = newId("r");
    const b = newId("r");
    expect(a).toMatch(/^r_[0-9A-F]{26}$/);
    expect(a).not.toBe(b);
  });
});
