// momo-ws v1 消息类型定义（与 src/proto/message.rs、src/proto/codec.rs 对齐）。
// 字段 schema 唯一权威：momo-docs 01-architecture/protocol.md。

export const PROTOCOL_NAME = "momo-ws";
export const PROTOCOL_VERSION = "v1";

// ---------- 二进制 audio.chunk 帧（protocol §4） ----------
// 固定 12B 头 [ver:u8][stream:u8][flags:u16][seq:u32][tag:u32]，多字节字段大端。

export const AUDIO_HEADER_LEN = 12;
export const AUDIO_VER = 1;
/** 上行（麦→ASR）。 */
export const STREAM_UPLINK = 0x01;
/** 下行（TTS→播放）。 */
export const STREAM_DOWNLINK = 0x02;
/** bit0：EOS，本段/本句最后一帧。 */
export const FLAG_EOS = 0x0001;

export interface AudioHeader {
  stream: number;
  flags: number;
  seq: number;
  tag: number;
}

export function encodeAudioFrame(header: AudioHeader, payload: Uint8Array): Uint8Array {
  const frame = new Uint8Array(AUDIO_HEADER_LEN + payload.length);
  const view = new DataView(frame.buffer);
  view.setUint8(0, AUDIO_VER);
  view.setUint8(1, header.stream);
  view.setUint16(2, header.flags);
  view.setUint32(4, header.seq);
  view.setUint32(8, header.tag);
  frame.set(payload, AUDIO_HEADER_LEN);
  return frame;
}

/** 解码头 + 载荷；长度不足或 ver 不符返回 null。 */
export function decodeAudioFrame(
  frame: ArrayBuffer | Uint8Array,
): { header: AudioHeader; payload: Uint8Array } | null {
  const bytes = frame instanceof Uint8Array ? frame : new Uint8Array(frame);
  if (bytes.byteLength < AUDIO_HEADER_LEN || bytes[0] !== AUDIO_VER) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    header: {
      stream: view.getUint8(1),
      flags: view.getUint16(2),
      seq: view.getUint32(4),
      tag: view.getUint32(8),
    },
    payload: bytes.subarray(AUDIO_HEADER_LEN),
  };
}

// ---------- 枚举（protocol §2 lower_snake / §6 SCREAMING error code） ----------

export type ClientKind = "ui" | "action" | "orchestrator";
export type ServerRole = "orchestrator" | "asr" | "tts";
export type VadEvent = "speech_start" | "speech_end";
export type BargeInReason = "vad_speech" | "user" | "system";
export type CancelTarget = "turn" | "segment" | "invoke" | "stream";
export type CancelReason = "barge_in" | "user" | "timeout" | "shutdown";
export type TurnState = "idle" | "listening" | "thinking" | "speaking" | "interrupted";

/** v1 固定 8 值；表外取值一律按 neutral 处理（protocol §6）。 */
export type EmotionTag =
  | "neutral"
  | "happy"
  | "sad"
  | "angry"
  | "surprised"
  | "thinking"
  | "shy"
  | "sleepy";

export function normalizeEmotion(tag: string): EmotionTag {
  switch (tag) {
    case "happy":
    case "sad":
    case "angry":
    case "surprised":
    case "thinking":
    case "shy":
    case "sleepy":
      return tag;
    default:
      return "neutral";
  }
}

export type ErrorCode =
  | "VERSION_MISMATCH"
  | "BAD_MESSAGE"
  | "BACKPRESSURE_OVERFLOW"
  | "MODEL_UNREACHABLE"
  | "ASR_FAILED"
  | "TTS_FAILED"
  | "ACTION_TIMEOUT"
  | "INTERNAL";

// ---------- 消息载荷（payload 不含 type，UI/模型链路复用） ----------

export interface AudioParams {
  codec: string;
  sample_rate: number;
  channels: number;
  chunk_ms: number;
}

export interface HelloAudio {
  uplink?: AudioParams;
  downlink?: AudioParams;
}

export interface HelloMsg {
  protocol: string;
  version: string;
  client: ClientKind;
  audio?: HelloAudio;
}

export interface HelloOkMsg {
  version: string;
  session_id: string;
  server: string;
  role: ServerRole;
  capabilities?: string[];
}

export interface InputTextMsg {
  id: string;
  text: string;
  lang?: string;
  at_ms?: number;
}

export interface AudioSegmentMsg {
  segment_id: string;
  duration_ms: number;
  text: string;
  lang: string;
  speech_emotion?: EmotionTag;
  final: boolean;
}

export interface InputVadMsg {
  event: VadEvent;
  segment_id: string;
  at_ms: number;
}

export interface OutputSentenceMsg {
  turn_id: string;
  seq: number;
  text: string;
  emotion: EmotionTag;
  lang?: string;
  final: boolean;
}

export interface BargeInMsg {
  turn_id: string;
  reason?: BargeInReason;
  at_ms?: number;
  applied?: boolean;
  played_seq?: number;
}

export interface ControlCancelMsg {
  target: CancelTarget;
  id: string;
  reason?: CancelReason;
}

export interface StateTurnMsg {
  turn_id: string;
  state: TurnState;
  seq: number;
  at_ms: number;
  ref_id?: string;
}

export interface ActionSpec {
  name: string;
  description?: string;
  json_schema?: unknown;
}

export interface ActionRegisterMsg {
  provider: string;
  actions: ActionSpec[];
}

export interface ActionUnregisterMsg {
  provider: string;
  names: string[];
}

export interface ActionInvokeMsg {
  invoke_id: string;
  turn_id: string;
  action: string;
  args?: unknown;
  timeout_ms?: number;
}

export interface ErrorBody {
  code: string;
  message: string;
}

export interface ActionResultMsg {
  invoke_id: string;
  ok: boolean;
  data?: unknown;
  error?: ErrorBody;
}

export interface ErrorMsg {
  code: ErrorCode;
  message: string;
  ref_id?: string;
  fatal?: boolean;
}

// ---------- type 多态联合（文本帧 JSON） ----------

/** UI → orch（前端实际只发 hello/input.text/control.barge_in；其余为协议全集）。 */
export type ClientMsg =
  | ({ type: "hello" } & HelloMsg)
  | ({ type: "input.text" } & InputTextMsg)
  | ({ type: "input.audio.segment" } & AudioSegmentMsg)
  | ({ type: "input.vad" } & InputVadMsg)
  | ({ type: "control.barge_in" } & BargeInMsg)
  | ({ type: "action.register" } & ActionRegisterMsg)
  | ({ type: "action.unregister" } & ActionUnregisterMsg)
  | ({ type: "action.result" } & ActionResultMsg);

/** orch → UI。 */
export type ServerMsg =
  | ({ type: "hello.ok" } & HelloOkMsg)
  | ({ type: "input.audio.segment" } & AudioSegmentMsg)
  | ({ type: "input.vad" } & InputVadMsg)
  | ({ type: "output.sentence" } & OutputSentenceMsg)
  | ({ type: "control.barge_in" } & BargeInMsg)
  | ({ type: "control.cancel" } & ControlCancelMsg)
  | ({ type: "state.turn" } & StateTurnMsg)
  | ({ type: "action.invoke" } & ActionInvokeMsg)
  | ({ type: "error" } & ErrorMsg);

/** 服务端未知 type → orch 回 BAD_MESSAGE；客户端同理容错解析。 */
export function parseServerMsg(text: string): ServerMsg | null {
  try {
    const msg = JSON.parse(text) as ServerMsg;
    return typeof msg.type === "string" ? msg : null;
  } catch {
    return null;
  }
}

// ---------- 协议 id / close code ----------

/** 简易 `<prefix>_<26字符>` id（ULID 形，无依赖实现）。 */
export function newId(prefix: string): string {
  const rand = crypto.getRandomValues(new Uint8Array(16));
  const body = Array.from(rand, (b) => b.toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase()
    .slice(0, 26);
  return `${prefix}_${body}`;
}

export const CLOSE_NORMAL = 1000;
export const CLOSE_VERSION_MISMATCH = 4001;
export const CLOSE_PROTOCOL_VIOLATION = 4400;
export const CLOSE_INTERNAL = 1011;
