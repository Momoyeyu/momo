
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 协议名与版本常量（hello/hello.ok 使用）。
pub const PROTOCOL_NAME: &str = "momo-ws";

///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
    #[serde(rename = "hello")]
    Hello(HelloMsg),
    #[serde(rename = "input.text")]
    InputText(InputTextMsg),
    #[serde(rename = "input.vad")]
    InputVad(InputVadMsg),
    #[serde(rename = "input.audio.segment")]
    #[serde(rename = "output.sentence")]
    OutputSentence(OutputSentenceMsg),
    #[serde(rename = "control.barge_in")]
    ControlBargeIn(BargeInMsg),
    #[serde(rename = "control.cancel")]
    ControlCancel(ControlCancelMsg),
    #[serde(rename = "state.turn")]
    StateTurn(StateTurnMsg),
    #[serde(rename = "error")]
    Error(ErrorMsg),
    #[serde(other)]
    Unknown,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloMsg {
    pub protocol: String,
    pub version: String,
    pub client: ClientKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    Orchestrator,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloOkMsg {
    pub version: String,
    pub session_id: String,
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


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub text: String,
    pub lang: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputVadMsg {
    pub event: VadEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VadEvent {
    SpeechStart,
    SpeechEnd,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputSentenceMsg {
    pub turn_id: String,
    pub seq: u32,
    pub text: String,
    pub emotion: EmotionTag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BargeInMsg {
    pub turn_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BargeInReason {
    User,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlCancelMsg {
    pub target: CancelTarget,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTarget {
    Turn,
    Stream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTurnMsg {
    pub turn_id: String,
    pub state: TurnState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Interrupted,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    VersionMismatch,
    BadMessage,
    BackpressureOverflow,
    ModelUnreachable,
    AsrFailed,
    TtsFailed,
    Internal,
}

// ---------- emotion tag ----------

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

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::new())
}
