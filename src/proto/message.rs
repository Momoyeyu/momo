//! `ClientMsg` / `ServerMsg`：serde `tag = "type"` 多态，对齐 protocol v1 §5/§6。
//!
//! 音频不走 JSON：二进制 `audio.chunk` 帧见 [`crate::proto::codec`]。

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 协议名与版本常量（hello/hello.ok 使用）。
pub const PROTOCOL_NAME: &str = "momo-ws";
pub const PROTOCOL_VERSION: &str = "v1";

/// 客户端 → 服务端（UI 链路 / action 链路）消息。
///
/// 未知 `type` 归一为 [`ClientMsg::Unknown`]，由 ws 层回 `error{BAD_MESSAGE}` 且不断连。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    #[serde(rename = "hello")]
    Hello(HelloMsg),
    #[serde(rename = "input.text")]
    InputText(InputTextMsg),
    #[serde(rename = "input.audio.segment")]
    InputAudioSegment(AudioSegmentMsg),
    #[serde(rename = "input.vad")]
    InputVad(InputVadMsg),
    #[serde(rename = "control.barge_in")]
    ControlBargeIn(BargeInMsg),
    #[serde(rename = "action.register")]
    ActionRegister(ActionRegisterMsg),
    #[serde(rename = "action.unregister")]
    ActionUnregister(ActionUnregisterMsg),
    #[serde(rename = "action.result")]
    ActionResult(ActionResultMsg),
    #[serde(other)]
    Unknown,
}

/// 服务端 → 客户端消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    #[serde(rename = "hello.ok")]
    HelloOk(HelloOkMsg),
    /// ASR→orch 的转写镜像给 UI 做字幕。
    #[serde(rename = "input.audio.segment")]
    InputAudioSegment(AudioSegmentMsg),
    /// ASR→orch 的 VAD 边界镜像给 UI。
    #[serde(rename = "input.vad")]
    InputVad(InputVadMsg),
    #[serde(rename = "output.sentence")]
    OutputSentence(OutputSentenceMsg),
    #[serde(rename = "control.barge_in")]
    ControlBargeIn(BargeInMsg),
    #[serde(rename = "control.cancel")]
    ControlCancel(ControlCancelMsg),
    #[serde(rename = "state.turn")]
    StateTurn(StateTurnMsg),
    #[serde(rename = "action.invoke")]
    ActionInvoke(ActionInvokeMsg),
    #[serde(rename = "error")]
    Error(ErrorMsg),
    #[serde(other)]
    Unknown,
}

// ---------- §3 握手 ----------

/// `hello`（client → server），连接建立后第一条消息。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloMsg {
    pub protocol: String,
    pub version: String,
    pub client: ClientKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<HelloAudio>,
}

/// `hello.client`：服务端据此区分对端角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    Action,
    /// 编排器连模型服务（ASR/TTS）时使用。
    Orchestrator,
}

/// `hello.audio`：该连接将使用的二进制音频参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloAudio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uplink: Option<AudioParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downlink: Option<AudioParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioParams {
    /// 如 `pcm_s16le`。
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub chunk_ms: u32,
}

/// `hello.ok`（server → client）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloOkMsg {
    pub version: String,
    /// `sess_` 前缀 ULID。
    pub session_id: String,
    /// 如 `momod` / `momo-svc-tts`。
    pub server: String,
    pub role: ServerRole,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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

// ---------- §6 输入 ----------

/// `input.text`：用户文字输入，触发新回合。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputTextMsg {
    /// `r_` 前缀 id，`state.turn.ref_id` 回填。
    pub id: String,
    /// 上限 4000 字符。
    pub text: String,
    /// `zh`/`en`/`ja`/`auto`，默认 `auto`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
}

/// `input.audio.segment`：ASR 对一个完整 VAD 段的转写；orch 镜像给 UI。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSegmentMsg {
    /// `s_` 前缀段 id。
    pub segment_id: String,
    pub duration_ms: u64,
    /// 可为空串（空段 orch 直接回 idle）。
    pub text: String,
    /// `zh/en/ja/ko/yue`。
    pub lang: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speech_emotion: Option<EmotionTag>,
    #[serde(rename = "final")]
    pub is_final: bool,
}

/// `input.vad`：ASR 侧 VAD 边界事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputVadMsg {
    pub event: VadEvent,
    pub segment_id: String,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}

// ---------- §6 输出 / 控制 ----------

/// `output.sentence`：LLM 句切后的最小播报单元（UI 字幕 + TTS 驱动）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSentenceMsg {
    pub turn_id: String,
    /// 回合内句序号，从 1 递增；下行 `audio.chunk.tag` 引用它。
    pub seq: u32,
    pub text: String,
    pub emotion: EmotionTag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(rename = "final")]
    pub is_final: bool,
}

/// `control.barge_in`：请求 / 通知 / 回报共用（字段按用法子集出现，见 §6）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BargeInMsg {
    pub turn_id: String,
    /// (a) 请求：`vad_speech`/`user`/`system`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<BargeInReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u64>,
    /// (b)(c) 通知与回报：`true`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    /// (c) 回报：已完整播完的最后一句 `seq`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub played_seq: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BargeInReason {
    VadSpeech,
    User,
    System,
}

/// `control.cancel`（orch→TTS/ASR/action 方）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlCancelMsg {
    pub target: CancelTarget,
    /// target 指向的 id：turn_id / segment_id / invoke_id / stream 标识。
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<CancelReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTarget {
    Turn,
    Segment,
    Invoke,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    BargeIn,
    User,
    Timeout,
    Shutdown,
}

/// `state.turn`（orch→UI，广播）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTurnMsg {
    pub turn_id: String,
    pub state: TurnState,
    /// 回合内状态序号。
    pub seq: u32,
    pub at_ms: u64,
    /// 回填触发本回合的 `input.text.id`（语音回合省略）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
}

/// 回合状态机（迁移图见 momo-app.md §session）。wire 值 lower_snake。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Interrupted,
}

// ---------- §6 action（P4 预留） ----------

/// `action.register` 携带的能力声明。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 原样透传给 LLM tool 列表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRegisterMsg {
    pub provider: String,
    pub actions: Vec<ActionSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionUnregisterMsg {
    pub provider: String,
    pub names: Vec<String>,
}

/// `action.invoke`（orch→action 方）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionInvokeMsg {
    /// `a_` 前缀 id。
    pub invoke_id: String,
    pub turn_id: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// `action.result`（action 方→orch）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResultMsg {
    pub invoke_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

/// `action.result.error` / `error` 帧内嵌错误体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

// ---------- §6 error ----------

/// `error`（双向）。`fatal=true` 表示连接即将关闭（配合 WS close code）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub fatal: bool,
}

/// protocol §6 的 error code 表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    VersionMismatch,
    BadMessage,
    BackpressureOverflow,
    ModelUnreachable,
    AsrFailed,
    TtsFailed,
    ActionTimeout,
    Internal,
}

// ---------- emotion tag ----------

/// emotion tag：v1 固定 8 值（protocol §6 `output.sentence`）。
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
    /// 表外取值一律归一为 neutral（protocol §6）。
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

    /// 解析 LLM 内联 `[tag]` / ASR `speech_emotion` 文本。
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

/// 协议 id 生成：`<prefix>_<ULID>`（`t_`/`s_`/`r_`/`a_`/`sess_`）。
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::new())
}
