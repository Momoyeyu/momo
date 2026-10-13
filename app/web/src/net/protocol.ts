// momo-ws v2 消息类型定义与编解码（与 app/src/proto/message.rs、codec.rs 对齐）。
// 字段 schema 唯一权威：docs/01-architecture/protocol.md；
// golden 基准：app/tests/golden/*.json（对拍同组 fixture）。

export const PROTOCOL_NAME = "momo-ws";
export const PROTOCOL_VERSION = "v2";

// ---------- ULID（128-bit ↔ Crockford base32 26 字符） ----------
// 与 app `ulid::Ulid` / models `proto.py` 同一编码：48-bit ms 时间戳 + 80-bit 随机。

const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CROCKFORD_REV = new Map(Array.from(CROCKFORD, (c, i) => [c, i]));
const ULID_LEN = 26;
const ULID_BITS = 1n << 128n;

/** 生成新 ULID（26 字符 Crockford 文本）。 */
export function newUlid(): string {
  const ts = BigInt(Date.now()) & ((1n << 48n) - 1n);
  const rand = crypto.getRandomValues(new Uint8Array(10));
  let randVal = 0n;
  for (const b of rand) randVal = (randVal << 8n) | BigInt(b);
  let value = (ts << 80n) | randVal;
  let out = "";
  for (let i = 0; i < ULID_LEN; i++) {
    out = CROCKFORD[Number(value & 0x1fn)] + out;
    value >>= 5n;
  }
  return out;
}

/** ULID 文本 → 16B 原始值；非法文本返回 null。 */
export function ulidToBytes(text: string): Uint8Array | null {
  if (text.length !== ULID_LEN) return null;
  let value = 0n;
  for (const ch of text.toUpperCase()) {
    const digit = CROCKFORD_REV.get(ch);
    if (digit === undefined) return null;
    value = (value << 5n) | BigInt(digit);
  }
  if (value >= ULID_BITS) return null;
  const out = new Uint8Array(16);
  for (let i = 15; i >= 0; i--) {
    out[i] = Number(value & 0xffn);
    value >>= 8n;
  }
  return out;
}

/** 16B 原始值 → ULID 文本；长度非 16 返回 null。 */
export function ulidFromBytes(raw: Uint8Array): string | null {
  if (raw.length !== 16) return null;
  let value = 0n;
  for (const b of raw) value = (value << 8n) | BigInt(b);
  let out = "";
  for (let i = 0; i < ULID_LEN; i++) {
    out = CROCKFORD[Number(value & 0x1fn)] + out;
    value >>= 5n;
  }
  return out;
}

/** 协议 id：`<prefix>_<ULID>`（`st_`/`cap_`/`r_` 等），对齐 app `new_id`。 */
export function newId(prefix: string): string {
  return `${prefix}_${newUlid()}`;
}

// ---------- 二进制 audio.chunk 帧（protocol §6：固定 32B 头，多字节字段小端） ----------
// 布局：[ver:u8][direction:u8][flags:u16][stream_id:16B ULID][chunk_seq:u32]
//       [sentence_seq:u32][sample_offset:u32]

export const AUDIO_HEADER_LEN = 32;
export const AUDIO_VER = 2;
/** direction：上行（UI→orchestrator→ASR）。 */
export const DIR_UPLINK = 1;
/** direction：下行（TTS→orchestrator→UI）。 */
export const DIR_DOWNLINK = 2;
/** flags bit0：EOS，本 stream 最后一帧；其余 bit 为 reserved，非零即拒绝。 */
export const FLAG_EOS = 0x0001;

export interface AudioHeader {
  direction: number;
  flags: number;
  /** `st_<ULID>` 文本形态（与 audio.open.stream_id 一致）；二进制头内只携带 ULID 的 16B 原始值。 */
  stream_id: string;
  /** 每 stream 从 0 开始严格递增。 */
  chunk_seq: number;
  /** 所属 output.sentence.seq；uplink 固定 0。 */
  sentence_seq: number;
  /** 本帧首 sample 偏移，从 0 按每声道 PCM sample 累计。 */
  sample_offset: number;
}

/** `st_<ULID>` → ULID 文本；无前缀时按整体为 ULID 处理。 */
function streamIdUlid(streamId: string): string {
  const sep = streamId.indexOf("_");
  return sep < 0 ? streamId : streamId.slice(sep + 1);
}

/** 编码为完整二进制帧（32B 头 + payload）；stream_id 非法时抛错（编码侧属本地 bug，不静默）。 */
export function encodeAudioFrame(
  header: AudioHeader,
  payload: Uint8Array,
): Uint8Array<ArrayBuffer> {
  const raw = ulidToBytes(streamIdUlid(header.stream_id));
  if (!raw) throw new Error(`invalid stream_id ULID: ${header.stream_id}`);
  const frame = new Uint8Array(AUDIO_HEADER_LEN + payload.length);
  const view = new DataView(frame.buffer);
  view.setUint8(0, AUDIO_VER);
  view.setUint8(1, header.direction);
  view.setUint16(2, header.flags, true);
  frame.set(raw, 4);
  view.setUint32(20, header.chunk_seq, true);
  view.setUint32(24, header.sentence_seq, true);
  view.setUint32(28, header.sample_offset, true);
  frame.set(payload, AUDIO_HEADER_LEN);
  return frame;
}

/**
 * 解码头 + 载荷；拒绝口径同 codec.rs：长度不足、ver/direction 非法、
 * reserved flags 非零、payload 奇数字节、非 EOS 空载荷、uplink sentence_seq 非 0。
 * 依赖 stream 状态的检查（seq gap、sample_offset 错位）由上层负责。
 */
export function decodeAudioFrame(
  frame: ArrayBuffer | Uint8Array,
): { header: AudioHeader; payload: Uint8Array } | null {
  const bytes = frame instanceof Uint8Array ? frame : new Uint8Array(frame);
  if (bytes.byteLength < AUDIO_HEADER_LEN) return null;
  if (bytes[0] !== AUDIO_VER) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const direction = view.getUint8(1);
  if (direction !== DIR_UPLINK && direction !== DIR_DOWNLINK) return null;
  const flags = view.getUint16(2, true);
  if (flags & ~FLAG_EOS) return null;
  const ulid = ulidFromBytes(bytes.subarray(4, 20));
  if (!ulid) return null;
  const payload = bytes.subarray(AUDIO_HEADER_LEN);
  if (payload.byteLength % 2 !== 0) return null;
  if (payload.byteLength === 0 && (flags & FLAG_EOS) === 0) return null;
  const header: AudioHeader = {
    direction,
    flags,
    // stream id 一律 `st_` 前缀（app/models `new_id("st")`），与 audio.open.stream_id 直接可比。
    stream_id: `st_${ulid}`,
    chunk_seq: view.getUint32(20, true),
    sentence_seq: view.getUint32(24, true),
    sample_offset: view.getUint32(28, true),
  };
  if (direction === DIR_UPLINK && header.sentence_seq !== 0) return null;
  return { header, payload };
}

// ---------- 枚举（protocol §1/§2；serde snake_case / SCREAMING_SNAKE_CASE） ----------

export type ClientKind = "ui" | "orchestrator";
export type ServerRole = "orchestrator" | "asr" | "tts";
export type VadEvent = "speech_start" | "speech_end";
export type CaptureMode = "ptt" | "continuous";
export type BargeInReason = "user" | "vad_speech" | "system";
export type CancelTarget = "turn" | "capture" | "stream" | "unknown";
export type ReceiptStatus = "accepted" | "completed" | "interrupted" | "failed";
export type TurnOutcome = "completed" | "interrupted" | "failed";
export type TurnState = "idle" | "listening" | "thinking" | "speaking" | "interrupted";

/** protocol §2 固定 8 值；表外取值一律按 neutral 处理。 */
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
  | "UNAUTHORIZED"
  | "BAD_MESSAGE"
  | "BACKPRESSURE_OVERFLOW"
  | "MODEL_UNREACHABLE"
  | "ASR_FAILED"
  | "TTS_FAILED"
  | "INTERNAL"
  | "UNKNOWN";

// ---------- envelope（protocol §1 核心类型） ----------

/** 客户端发出消息的公共 envelope；event_id 为无前缀 ULID。 */
export interface Envelope {
  session_id: string;
  event_id: string;
  at_ms: number;
  reply_to?: string;
}

/** 服务端业务消息 envelope：Envelope + 单调递增 event_seq。 */
export interface ServerEnvelope extends Envelope {
  event_seq: number;
}

// ---------- 握手（session 建立前，不带 envelope） ----------

export interface HelloMsg {
  type: "hello";
  protocol: string;
  version: string;
  client: ClientKind;
  /** 本机 ephemeral token；不落 URL/日志/header（protocol §4）。 */
  auth_token: string;
  /** UI 恢复 session 时携带；未知/过期 session 则下发新 session。 */
  resume_session_id?: string;
}

/** hello.ok 只承载 version/session_id/role/capabilities 四字段（#102）。 */
export interface HelloOkMsg {
  type: "hello.ok";
  version: string;
  session_id: string;
  role: ServerRole;
  capabilities: string[];
}

// ---------- session.snapshot（orchestrator→UI，无音频重放的状态收敛） ----------

export interface SnapshotTurn {
  id: string;
  state: TurnState;
}

export interface InputReceipt {
  /** 对应 input.text.id（文字）或 capture_id（语音）。 */
  id: string;
  turn_id: string;
  status: ReceiptStatus;
}

export interface SessionSnapshotMsg {
  type: "session.snapshot";
  session_id: string;
  turn: SnapshotTurn | null;
  receipts: InputReceipt[];
  capabilities: string[];
  last_event_seq: number;
}

// ---------- 输入（UI→orchestrator） ----------

export interface InputTextMsg extends Envelope {
  type: "input.text";
  /** 幂等 id；input.accepted.ref_id 指回它。 */
  id: string;
  text: string;
  lang: string;
}

export interface InputAcceptedMsg extends ServerEnvelope {
  type: "input.accepted";
  /** 文字 = input.text.id；语音 = capture_id。 */
  ref_id: string;
  turn_id: string;
}

export interface InputRejectedMsg extends ServerEnvelope {
  type: "input.rejected";
  ref_id: string;
  reason: string;
}

export interface InputAudioStartMsg extends Envelope {
  type: "input.audio.start";
  capture_id: string;
  mode: CaptureMode;
}

/** input.audio.end / input.audio.cancel 共用结构。 */
export interface InputAudioMsg extends Envelope {
  type: "input.audio.end" | "input.audio.cancel";
  capture_id: string;
}

export interface InputVadMsg extends ServerEnvelope {
  type: "input.vad";
  capture_id: string;
  event: VadEvent;
}

export interface InputAudioSegmentMsg extends ServerEnvelope {
  type: "input.audio.segment";
  capture_id: string;
  text: string;
  /** 仅 final=true 启动 reply turn。 */
  final: boolean;
}

// ---------- 音频流（audio.open 方向约束见 protocol §2） ----------

export interface AudioOpenMsg extends Envelope {
  type: "audio.open";
  /** 服务端发出时携带；客户端发起方向不带。 */
  event_seq?: number;
  /** `st_<ULID>`；二进制帧头携带其 16B 原始值。 */
  stream_id: string;
  direction: "uplink" | "downlink";
  /** uplink 必填（绑定 capture）；downlink 不得带。 */
  capture_id?: string;
  /** downlink 必填；uplink 不得带。 */
  turn_id?: string;
  /** downlink 必填；uplink 不得带。 */
  sentence_seq?: number;
  codec: "pcm_s16le";
  /** 16000（uplink）/ 24000（downlink）。 */
  sample_rate: 16000 | 24000;
  channels: 1;
}

export interface AudioEndMsg extends Envelope {
  type: "audio.end";
  event_seq?: number;
  stream_id: string;
  /** 必须与最后接收帧的 chunk_seq 一致。 */
  last_chunk_seq: number;
}

// ---------- 输出（orchestrator→UI） ----------

export interface OutputSentenceMsg extends ServerEnvelope {
  type: "output.sentence";
  turn_id: string;
  seq: number;
  text: string;
  emotion: EmotionTag;
  lang: string;
}

export interface OutputCompleteMsg extends ServerEnvelope {
  type: "output.complete";
  turn_id: string;
  last_sentence_seq: number;
}

// ---------- 播放账本（UI→orchestrator） ----------

export interface PlaybackStartedMsg extends Envelope {
  type: "playback.started";
  stream_id: string;
  sentence_seq: number;
}

export interface PlaybackProgressMsg extends Envelope {
  type: "playback.progress";
  stream_id: string;
  sentence_seq: number;
  played_samples: number;
  completed: boolean;
}

// ---------- 控制与状态 ----------

export interface BargeInMsg extends Envelope {
  type: "control.barge_in";
  turn_id: string;
  reason: BargeInReason;
}

export interface ControlCancelMsg extends ServerEnvelope {
  type: "control.cancel";
  target: CancelTarget;
  /** target 指向的 id；turn stop 时 id = turn_id。 */
  id: string;
}

export interface TurnFinishedMsg extends ServerEnvelope {
  type: "turn.finished";
  turn_id: string;
  outcome: TurnOutcome;
}

export interface StateTurnMsg extends ServerEnvelope {
  type: "state.turn";
  turn_id: string;
  state: TurnState;
}


export interface ErrorMsg {
  type: "error";
  session_id?: string;
  event_id?: string;
  at_ms?: number;
  event_seq?: number;
  code: ErrorCode;
  message: string;
}

// ---------- type 多态联合（文本帧 JSON） ----------

/** UI 发送的业务消息 body：envelope 字段由 WsClient.send 注入，调用方不填。 */
export type ClientBody =
  | ({ type: "input.text" } & Omit<InputTextMsg, "type" | keyof Envelope>)
  | ({ type: "input.audio.start" } & Omit<InputAudioStartMsg, "type" | keyof Envelope>)
  | ({ type: "input.audio.end" | "input.audio.cancel" } & Omit<InputAudioMsg, "type" | keyof Envelope>)
  | ({ type: "audio.open" } & Omit<AudioOpenMsg, "type" | keyof Envelope | "event_seq">)
  | ({ type: "audio.end" } & Omit<AudioEndMsg, "type" | keyof Envelope | "event_seq">)
  | ({ type: "playback.started" } & Omit<PlaybackStartedMsg, "type" | keyof Envelope>)
  | ({ type: "playback.progress" } & Omit<PlaybackProgressMsg, "type" | keyof Envelope>)
  | ({ type: "control.barge_in" } & Omit<BargeInMsg, "type" | keyof Envelope>);

/** UI → orchestrator 全部消息（hello 不带 envelope；业务消息 = Envelope & body）。 */
export type ClientMsg = HelloMsg | (Envelope & ClientBody);

/** orchestrator → UI。 */
export type ServerMsg =
  | HelloOkMsg
  | SessionSnapshotMsg
  | InputAcceptedMsg
  | InputRejectedMsg
  | InputVadMsg
  | InputAudioSegmentMsg
  | AudioOpenMsg
  | AudioEndMsg
  | OutputSentenceMsg
  | OutputCompleteMsg
  | ControlCancelMsg
  | TurnFinishedMsg
  | StateTurnMsg
  | ErrorMsg;

/** 服务端未知 type → orch 回 BAD_MESSAGE；客户端同理容错解析。 */
export function parseServerMsg(text: string): ServerMsg | null {
  try {
    const msg = JSON.parse(text) as ServerMsg;
    return typeof msg.type === "string" ? msg : null;
  } catch {
    return null;
  }
}

// ---------- close code（protocol §10） ----------

/** 正常；也用于“被新 UI 连接顶掉”（superseded）。 */
export const CLOSE_NORMAL = 1000;
/** 版本不匹配。 */
export const CLOSE_VERSION_MISMATCH = 4001;
/** hello timeout / malformed / 协议违反。 */
export const CLOSE_PROTOCOL_VIOLATION = 4400;
/** token 校验失败。 */
export const CLOSE_UNAUTHORIZED = 4401;
/** 服务端内部错误。 */
export const CLOSE_INTERNAL = 1011;
