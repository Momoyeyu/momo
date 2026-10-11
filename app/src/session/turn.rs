//!

use tokio_util::sync::CancellationToken;


/// `t_` 前缀回合 id。
pub type TurnId = String;

/// 一个进行中/已结束的回合。
#[derive(Debug)]
pub struct Turn {
    pub id: TurnId,
    /// 取消令牌，扇出给 llm/tts/audio 任务；barge-in 后新回合换新 token。
    pub cancel: CancellationToken,
    pub committed: Vec<Sentence>,
}

impl Turn {
    pub fn new() -> Self {
        Self::with_id(new_id("t"))
    }

    pub fn with_id(id: TurnId) -> Self {
        Self {
            id,
            cancel: CancellationToken::new(),
            committed: Vec::new(),
        }
    }

    }
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}
