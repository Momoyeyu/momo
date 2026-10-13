//! `Msg`：serde `tag = "type"` 多态，对齐 protocol.md 核心类型与必需字段表。
//! 音频走二进制帧（[`crate::proto::codec`]）；`turn_id` 必需性落在各消息自身
//! 字段上，[`Envelope`] 只含恒定字段与可选 `reply_to`（hello 前无 envelope）。

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{error::AppError, proto::codec::Direction};

/// 协议名与版本常量（hello/hello.ok 使用）。
pub const PROTOCOL_NAME: &str = "momo-ws";
pub const PROTOCOL_VERSION: &str = "v2";

/// momo-ws 全部消息类型（protocol.md 必需字段表）。
///
/// 方向不在类型上区分（同一 `type` 在 UI 链路与模型链路上流动）；
/// 未知 `type` 归一为 [`Msg::Unknown`]，由 ws 层回 non-fatal `error` 且不断连。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Msg {
    #[serde(rename = "hello")]
    Hello(HelloMsg),
    #[serde(rename = "hello.ok")]
    HelloOk(HelloOkMsg),
    #[serde(rename = "session.snapshot")]
    SessionSnapshot(SessionSnapshotMsg),
    #[serde(rename = "input.text")]
    InputText(InputTextMsg),
    #[serde(rename = "input.accepted")]
    InputAccepted(InputAcceptedMsg),
    #[serde(rename = "input.rejected")]
    InputRejected(InputRejectedMsg),
    #[serde(rename = "input.audio.start")]
    InputAudioStart(InputAudioStartMsg),
    #[serde(rename = "input.audio.end")]
    InputAudioEnd(InputAudioMsg),
    #[serde(rename = "input.audio.cancel")]
    InputAudioCancel(InputAudioMsg),
    #[serde(rename = "input.vad")]
    InputVad(InputVadMsg),
    #[serde(rename = "input.audio.segment")]
    InputAudioSegment(InputAudioSegmentMsg),
    #[serde(rename = "audio.open")]
    AudioOpen(AudioOpenMsg),
    #[serde(rename = "audio.end")]
    AudioEnd(AudioEndMsg),
    #[serde(rename = "output.sentence")]
    OutputSentence(OutputSentenceMsg),
    #[serde(rename = "output.complete")]
    OutputComplete(OutputCompleteMsg),
    #[serde(rename = "playback.started")]
    PlaybackStarted(PlaybackStartedMsg),
    #[serde(rename = "playback.progress")]
    PlaybackProgress(PlaybackProgressMsg),
    #[serde(rename = "control.barge_in")]
    ControlBargeIn(BargeInMsg),
    #[serde(rename = "control.cancel")]
    ControlCancel(ControlCancelMsg),
    #[serde(rename = "turn.finished")]
    TurnFinished(TurnFinishedMsg),
    #[serde(rename = "state.turn")]
    StateTurn(StateTurnMsg),
    #[serde(rename = "error")]
    Error(ErrorMsg),
    #[serde(other)]
    Unknown,
}

// ---------- envelope ----------

/// 客户端发出消息的公共 envelope（protocol 核心类型 `Envelope`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub session_id: String,
    /// ULID 事件 id。
    pub event_id: String,
    pub at_ms: u64,
    /// 关联的上游 `event_id`（receipt/cancel 等指回触发事件）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// 服务端业务消息 envelope：`Envelope` + 单调递增 `event_seq`。
///
/// 客户端保存 `last_event_seq`，忽略 `event_seq <= last_event_seq` 的重复事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub session_id: String,
    pub event_id: String,
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub event_seq: u64,
}

/// 服务端 envelope 工厂：每 stamp 分配新 ULID `event_id`、unix `at_ms`
/// 与单调 `event_seq`；`Clone` 句柄共享同一 seq 计数器（session 与
/// audio sink 出站消息共用一条 seq 序列）。
#[derive(Debug, Clone)]
pub struct ServerEnv {
    session_id: String,
    event_seq: Arc<AtomicU64>,
}

impl ServerEnv {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            event_seq: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 分配一帧 envelope 元数据：`(event_id, at_ms, event_seq)`。
    fn stamp(&self) -> (String, u64, u64) {
        let at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        (
            ulid::Ulid::new().to_string(),
            at_ms,
            self.event_seq.fetch_add(1, Ordering::Relaxed) + 1,
        )
    }

    /// 服务端业务消息 envelope（`event_seq` 单调递增）。
    pub fn next(&self) -> ServerEnvelope {
        let (event_id, at_ms, event_seq) = self.stamp();
        ServerEnvelope {
            session_id: self.session_id.clone(),
            event_id,
            at_ms,
            reply_to: None,
            event_seq,
        }
    }

    /// `audio.open`/`audio.end` 形态的 envelope + 本次 seq
    /// （其 `event_seq` 为可选字段，由调用方落位）。
    pub fn envelope(&self) -> (Envelope, u64) {
        let (event_id, at_ms, event_seq) = self.stamp();
        (
            Envelope {
                session_id: self.session_id.clone(),
                event_id,
                at_ms,
                reply_to: None,
            },
            event_seq,
        )
    }
}

// ---------- 握手（session 建立前，不带 envelope） ----------

/// `hello`（client → server）：upgrade 后 5s 内第一帧。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloMsg {
    /// 恒为 [`PROTOCOL_NAME`]。
    pub protocol: String,
    /// 恒为 [`PROTOCOL_VERSION`]。
    pub version: String,
    pub client: ClientKind,
    /// 本机 ephemeral token；服务端 constant-time 比较，不落 URL/日志/header。
    pub auth_token: String,
    /// UI 恢复 session 时携带；未知/过期 session 则下发新 session。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_session_id: Option<String>,
}

/// `hello.client`：UI 连接或编排器连模型服务。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    Orchestrator,
}

/// `hello.ok`（server → client）：确认 server role 与能力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloOkMsg {
    pub version: String,
    pub session_id: String,
    /// `orchestrator`；ASR/TTS 服务端分别回 `asr`/`tts`。
    pub role: ServerRole,
    pub capabilities: Vec<String>,
}

/// `hello.ok.role`：服务端自报角色，客户端用于自检连错端口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerRole {
    Orchestrator,
    Asr,
    Tts,
}

// ---------- session.snapshot ----------

/// `session.snapshot`（orchestrator→UI）：无音频重放的状态收敛。
///
/// 用 `last_event_seq` 标记水位而非携带自身 `event_seq`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshotMsg {
    pub session_id: String,
    /// 当前活动 turn；无则 `null`。
    pub turn: Option<SnapshotTurn>,
    /// 服务端 receipt cache 中保留的最近 ≤256 条。
    pub receipts: Vec<InputReceipt>,
    pub capabilities: Vec<String>,
    pub last_event_seq: u64,
}

/// `session.snapshot.turn`：活动 turn 引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotTurn {
    pub id: String,
    pub state: TurnState,
}

/// receipt cache 条目（`session.snapshot.receipts`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputReceipt {
    /// 对应 `input.text.id`（文字）或 `capture_id`（语音）。
    pub id: String,
    pub turn_id: String,
    pub status: ReceiptStatus,
}

/// receipt 终态/在途状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Accepted,
    Completed,
    Interrupted,
    Failed,
}

// ---------- 输入 ----------

/// `input.text`（UI→orchestrator）：幂等文字输入，`(session_id, id)` 去重。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTextMsg {
    #[serde(flatten)]
    pub env: Envelope,
    /// 幂等 id；`input.accepted.ref_id` 指回它。
    pub id: String,
    /// 上限 4000 字符；空输入忽略。
    pub text: String,
    /// 如 `zh`/`en`/`ja`/`auto`。
    pub lang: String,
}

/// `input.accepted`（orchestrator→UI）：receipt cache 回执。
///
/// 文字输入 `ref_id = input.text.id`；语音 `ref_id = capture_id`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAcceptedMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub ref_id: String,
    pub turn_id: String,
}

/// `input.rejected`（orchestrator→UI）：输入拒绝。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputRejectedMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub ref_id: String,
    pub reason: String,
}

/// `input.audio.start`（UI→orchestrator）：开 capture。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioStartMsg {
    #[serde(flatten)]
    pub env: Envelope,
    pub capture_id: String,
    pub mode: CaptureMode,
}

/// `input.audio.end` / `input.audio.cancel` 共用结构（UI→orchestrator）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioMsg {
    #[serde(flatten)]
    pub env: Envelope,
    pub capture_id: String,
}

/// `input.audio.start.mode`：PTT 按住说话 / 连续麦 VAD。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Ptt,
    Continuous,
}

/// `input.vad`（ASR→orchestrator→UI）：VAD 边界事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputVadMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub capture_id: String,
    pub event: VadEvent,
}

/// `input.vad.event`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}

/// `input.audio.segment`（ASR→orchestrator→UI）：partial 预览或 final 输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputAudioSegmentMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub capture_id: String,
    pub text: String,
    /// 仅 `final=true` 启动 reply turn。
    #[serde(rename = "final")]
    pub is_final: bool,
}

// ---------- 音频流 ----------

/// `audio.open`（双方向）：stream 绑定 capture（uplink）或 turn/sentence（downlink）。
///
/// uplink：UI→orchestrator→ASR，必须带 `capture_id`，不得带 `sentence_seq`；
/// downlink：TTS→orchestrator→UI，必须带 `turn_id`+`sentence_seq`，不得带 `capture_id`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioOpenMsg {
    #[serde(flatten)]
    pub env: Envelope,
    /// 服务端发出时携带；客户端发起方向不带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    /// ULID stream id；二进制帧头携带其 16B 原始值。
    pub stream_id: String,
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentence_seq: Option<u32>,
    pub codec: AudioCodec,
    /// `16000`（uplink）或 `24000`（downlink）。
    pub sample_rate: u32,
    /// 固定 `1`（mono）。
    pub channels: u8,
}

impl AudioOpenMsg {
    /// 方向约束校验（protocol `audio.open` 行）。
    pub fn validate(&self) -> Result<(), AppError> {
        let ok = match self.direction {
            Direction::Uplink => self.capture_id.is_some() && self.sentence_seq.is_none(),
            Direction::Downlink => {
                self.capture_id.is_none() && self.turn_id.is_some() && self.sentence_seq.is_some()
            }
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::BadMessage(format!(
                "audio.open {:?} violates direction binding rules",
                self.direction
            )))
        }
    }
}

/// `audio.open.codec`：v2 只定义 `pcm_s16le`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioCodec {
    #[serde(rename = "pcm_s16le")]
    PcmS16le,
}

/// `audio.end`（stream sender→receiver）：显式结束 stream 生命周期。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioEndMsg {
    #[serde(flatten)]
    pub env: Envelope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    /// 必须与已打开 stream 匹配。
    pub stream_id: String,
    /// 必须与最后接收帧的 `chunk_seq` 一致。
    pub last_chunk_seq: u32,
}

// ---------- 输出 ----------

/// `output.sentence`（orchestrator→UI/TTS）：字幕与逐句合成最小单元。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSentenceMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub turn_id: String,
    /// 回合内句序号，从 1 递增；下行帧 `sentence_seq` 引用它。
    pub seq: u32,
    pub text: String,
    pub emotion: EmotionTag,
    pub lang: String,
}

/// `output.complete`（orchestrator→UI）：generation 结束，不依赖末尾标点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputCompleteMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub turn_id: String,
    pub last_sentence_seq: u32,
}

// ---------- 播放账本 ----------

/// `playback.started`（UI→orchestrator）：实际开播并进入 speaking。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackStartedMsg {
    #[serde(flatten)]
    pub env: Envelope,
    pub stream_id: String,
    pub sentence_seq: u32,
}

/// `playback.progress`（UI→orchestrator）：听见账本。
///
/// `played_samples` 单调且不得超过 stream total samples；`completed=true`
/// 仅在 total samples 与 EOS/`audio.end` 一致时有效。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackProgressMsg {
    #[serde(flatten)]
    pub env: Envelope,
    pub stream_id: String,
    pub sentence_seq: u32,
    pub played_samples: u64,
    pub completed: bool,
}

// ---------- 控制与状态 ----------

/// `control.barge_in`（UI→orchestrator）：请求立即打断指定 turn。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BargeInMsg {
    #[serde(flatten)]
    pub env: Envelope,
    pub turn_id: String,
    pub reason: BargeInReason,
}

/// `control.barge_in.reason`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BargeInReason {
    /// 用户显式 Stop/PTT。
    User,
    /// orchestrator 内部 VAD speech_start 触发。
    VadSpeech,
    System,
}

/// `control.cancel`（orchestrator→UI/models）：取消指定 turn/object。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlCancelMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub target: CancelTarget,
    /// target 指向的 id；turn stop 时 `id = turn_id`。
    pub id: String,
}

/// `control.cancel.target`：被取消对象的类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTarget {
    Turn,
    Capture,
    Stream,
    /// 表外取值；解码不失败。
    #[serde(other)]
    Unknown,
}

/// `turn.finished`（orchestrator→UI）：每 turn 恰好一次终态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnFinishedMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub turn_id: String,
    pub outcome: TurnOutcome,
}

/// `turn.finished.outcome`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed,
}

/// `state.turn`（orchestrator→UI）：turn domain event。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTurnMsg {
    #[serde(flatten)]
    pub env: ServerEnvelope,
    pub turn_id: String,
    pub state: TurnState,
}

/// 回合状态机（`idle→listening→thinking→speaking→interrupted`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Interrupted,
}

// ---------- error ----------

/// `error`（双向）：`code,message` 必需。
///
/// 握手期错误发生在 session 建立前，envelope 字段可不出现；
/// 服务端发出时可附 `event_seq`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorMsg {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    pub code: ErrorCode,
    pub message: String,
}

/// `error.code`；表外取值解码为 [`ErrorCode::Unknown`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    VersionMismatch,
    Unauthorized,
    BadMessage,
    BackpressureOverflow,
    ModelUnreachable,
    AsrFailed,
    TtsFailed,
    Internal,
    #[serde(other)]
    Unknown,
}

// ---------- emotion tag ----------

/// emotion tag：protocol 固定 8 值（`output.sentence.emotion`）。
///
/// 表外取值反序列化为 [`EmotionTag::Unknown`]，语义上按 `neutral` 处理；
/// 序列化 Unknown 一律写成 `neutral`，永不产生表外值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmotionTag {
    #[default]
    Neutral,
    Happy,
    Sad,
    Angry,
    Surprised,
    Thinking,
    Shy,
    Sleepy,
    /// 表外取值；[`EmotionTag::normalized`] 归一为 `Neutral`。
    Unknown,
}

impl EmotionTag {
    /// 表外取值一律归一为 neutral。
    pub fn normalized(self) -> Self {
        match self {
            EmotionTag::Unknown => EmotionTag::Neutral,
            tag => tag,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EmotionTag::Neutral | EmotionTag::Unknown => "neutral",
            EmotionTag::Happy => "happy",
            EmotionTag::Sad => "sad",
            EmotionTag::Angry => "angry",
            EmotionTag::Surprised => "surprised",
            EmotionTag::Thinking => "thinking",
            EmotionTag::Shy => "shy",
            EmotionTag::Sleepy => "sleepy",
        }
    }

    /// 解析 LLM 内联 `[tag]` 文本。
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "neutral" => EmotionTag::Neutral,
            "happy" => EmotionTag::Happy,
            "sad" => EmotionTag::Sad,
            "angry" => EmotionTag::Angry,
            "surprised" => EmotionTag::Surprised,
            "thinking" => EmotionTag::Thinking,
            "shy" => EmotionTag::Shy,
            "sleepy" => EmotionTag::Sleepy,
            _ => EmotionTag::Unknown,
        }
    }
}

impl Serialize for EmotionTag {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EmotionTag {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(EmotionTag::from_tag(&s))
    }
}

/// 协议 id 生成：`<prefix>_<ULID>`（`t_`/`sess_`/`conn_` 等）。
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::new())
}
