// 协议编解码单测（v2：与 app/tests/golden/*.json、codec.rs 对拍同组口径）。

import { describe, expect, it } from "vitest";
import {
  AUDIO_HEADER_LEN,
  AUDIO_VER,
  DIR_DOWNLINK,
  DIR_UPLINK,
  FLAG_EOS,
  decodeAudioFrame,
  encodeAudioFrame,
  newId,
  newUlid,
  normalizeEmotion,
  parseServerMsg,
  ulidFromBytes,
  ulidToBytes,
} from "./protocol";

// golden 固定 ULID（audio.open.uplink.json 的 stream_id 主体）。
const GOLDEN_ULID = "01J5M0N8T1RZ8K2W4V7H6X9P3E";
const GOLDEN_STREAM = `st_${GOLDEN_ULID}`;

describe("ULID", () => {
  it("newUlid 生成 26 字符 Crockford 文本", () => {
    expect(newUlid()).toMatch(/^[0-9A-HJKMNP-TV-Z]{26}$/);
    expect(newUlid()).not.toBe(newUlid());
  });

  it("高 48 位为 ms 时间戳（对拍真实 ULID 布局）", () => {
    const raw = ulidToBytes(newUlid())!;
    const view = new DataView(raw.buffer);
    const ts = view.getUint16(0) * 2 ** 32 + view.getUint32(2);
    expect(Math.abs(ts - Date.now())).toBeLessThan(60_000);
  });

  it("bytes 往返（golden ULID 定值对拍）", () => {
    const raw = ulidToBytes(GOLDEN_ULID);
    expect(raw).not.toBeNull();
    expect(raw!.length).toBe(16);
    // 时间戳前缀：01J5M0N8T1 → 48-bit ms，大端落在前 6B。
    expect(raw![0]).toBe(0x01);
    expect(ulidFromBytes(raw!)).toBe(GOLDEN_ULID);
  });

  it("非法文本返回 null", () => {
    expect(ulidToBytes("short")).toBeNull();
    expect(ulidToBytes("01J5M0N8T1RZ8K2W4V7H6X9P3L")).toBeNull(); // 'L' 不在 Crockford 表
    expect(ulidFromBytes(new Uint8Array(8))).toBeNull();
  });

  it("newId 为 <prefix>_<ULID>", () => {
    expect(newId("r")).toMatch(/^r_[0-9A-HJKMNP-TV-Z]{26}$/);
  });
});

describe("audio.chunk 32B 头（小端）", () => {
  const header = {
    direction: DIR_UPLINK,
    flags: 0,
    stream_id: GOLDEN_STREAM,
    chunk_seq: 7,
    sentence_seq: 0,
    sample_offset: 1600,
  };

  it("编码/解码 roundtrip", () => {
    const payload = new Uint8Array([1, 2, 3, 4]);
    const frame = encodeAudioFrame({ ...header, flags: FLAG_EOS }, payload);
    expect(frame.length).toBe(AUDIO_HEADER_LEN + payload.length);
    expect(frame[0]).toBe(AUDIO_VER); // ver
    expect(frame[1]).toBe(DIR_UPLINK);
    expect(frame[2]).toBe(FLAG_EOS & 0xff); // flags LE low byte
    expect(frame[3]).toBe(0);
    const decoded = decodeAudioFrame(frame);
    expect(decoded?.header).toEqual({ ...header, flags: FLAG_EOS });
    expect(Array.from(decoded!.payload)).toEqual([1, 2, 3, 4]);
  });

  it("字节级布局与 codec.rs 对拍", () => {
    const frame = encodeAudioFrame(header, new Uint8Array([0xaa, 0xbb]));
    const ulid = ulidToBytes(GOLDEN_ULID)!;
    expect(Array.from(frame.subarray(4, 20))).toEqual(Array.from(ulid));
    // chunk_seq=7 LE：07 00 00 00
    expect(Array.from(frame.subarray(20, 24))).toEqual([7, 0, 0, 0]);
    // sample_offset=1600 LE：40 06 00 00
    expect(Array.from(frame.subarray(28, 32))).toEqual([0x40, 0x06, 0, 0]);
    expect(Array.from(frame.subarray(32))).toEqual([0xaa, 0xbb]);
  });

  it("空 payload + EOS 合法（零长度 capture 收尾）", () => {
    const frame = encodeAudioFrame({ ...header, flags: FLAG_EOS }, new Uint8Array(0));
    const decoded = decodeAudioFrame(frame);
    expect(decoded?.header.flags).toBe(FLAG_EOS);
    expect(decoded?.payload.byteLength).toBe(0);
  });

  it("拒绝口径同 codec.rs", () => {
    // 短帧
    expect(decodeAudioFrame(new Uint8Array(31))).toBeNull();
    // 错误 ver
    const badVer = encodeAudioFrame(header, new Uint8Array([1, 2]));
    badVer[0] = 1;
    expect(decodeAudioFrame(badVer)).toBeNull();
    // 非法 direction
    const badDir = encodeAudioFrame(header, new Uint8Array([1, 2]));
    badDir[1] = 3;
    expect(decodeAudioFrame(badDir)).toBeNull();
    // reserved flags 非零
    const badFlags = encodeAudioFrame(header, new Uint8Array([1, 2]));
    badFlags[3] = 0x02;
    expect(decodeAudioFrame(badFlags)).toBeNull();
    // 奇数 payload
    expect(decodeAudioFrame(encodeAudioFrame(header, new Uint8Array([1])))).toBeNull();
    // 非 EOS 空 payload
    expect(decodeAudioFrame(encodeAudioFrame(header, new Uint8Array(0)))).toBeNull();
    // uplink sentence_seq != 0
    expect(
      decodeAudioFrame(
        encodeAudioFrame({ ...header, sentence_seq: 2 }, new Uint8Array([1, 2])),
      ),
    ).toBeNull();
  });

  it("downlink 帧带非零 sentence_seq", () => {
    const frame = encodeAudioFrame(
      { ...header, direction: DIR_DOWNLINK, sentence_seq: 2 },
      new Uint8Array([1, 2]),
    );
    expect(decodeAudioFrame(frame)?.header.sentence_seq).toBe(2);
  });
});

describe("server msg 解析（golden 对拍）", () => {
  it("hello.ok golden（四字段，不承载 auth_token/event_seq）", () => {
    const msg = parseServerMsg(
      JSON.stringify({
        type: "hello.ok",
        version: "v2",
        session_id: "sess_01J5M0N8T1RZ8K2W4V7H6X9P3A",
        role: "orchestrator",
        capabilities: ["barge_in", "emotion_tags"],
      }),
    );
    expect(msg?.type).toBe("hello.ok");
    if (msg?.type === "hello.ok") {
      expect(msg.role).toBe("orchestrator");
      expect(msg.session_id).toMatch(/^sess_/);
      expect("auth_token" in msg).toBe(false);
      expect("event_seq" in msg).toBe(false);
    }
  });

  it("error golden（ServerEnvelope + code/message + 归因）", () => {
    const msg = parseServerMsg(
      '{"type":"error","session_id":"sess_01J5M0N8T1RZ8K2W4V7H6X9P3A","event_id":"01J5M0N8T1RZ8K2W4V7H6X9P3B","at_ms":1730000000000,"event_seq":42,"code":"TTS_FAILED","message":"synthesis failed","ref_id":"r_01J5M0N8T1RZ8K2W4V7H6X9P3F","turn_id":"t_01J5M0N8T1RZ8K2W4V7H6X9P3C","capture_id":"cap_01J5M0N8T1RZ8K2W4V7H6X9P3D","stream_id":"st_01J5M0N8T1RZ8K2W4V7H6X9P3E","sentence_seq":2}',
    );
    expect(msg?.type).toBe("error");
    if (msg?.type === "error") {
      expect(msg.code).toBe("TTS_FAILED");
      expect(msg.event_seq).toBe(42);
      expect(msg.turn_id).toBe("t_01J5M0N8T1RZ8K2W4V7H6X9P3C");
      expect(msg.capture_id).toBe("cap_01J5M0N8T1RZ8K2W4V7H6X9P3D");
      expect(msg.stream_id).toBe("st_01J5M0N8T1RZ8K2W4V7H6X9P3E");
      expect(msg.sentence_seq).toBe(2);
    }
  });

  it("非法 JSON 返回 null", () => {
    expect(parseServerMsg("not json")).toBeNull();
  });
});
describe("emotion tag", () => {
  it("表内值原样返回，表外值归一 neutral", () => {
    expect(normalizeEmotion("happy")).toBe("happy");
    expect(normalizeEmotion("sleepy")).toBe("sleepy");
    expect(normalizeEmotion("ecstatic")).toBe("neutral");
    expect(normalizeEmotion("")).toBe("neutral");
  });
});

describe("id 生成", () => {
  it("带前缀且全局唯一", () => {
    const a = newId("r");
    const b = newId("r");
    expect(a).toMatch(/^r_[0-9A-HJKMNP-TV-Z]{26}$/);
    expect(a).not.toBe(b);
  });
});
