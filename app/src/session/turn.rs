//! `Turn`：回合状态机 + barge-in 三动作。
//!
//! 状态迁移（momo-app.md §session）：
//! `Idle → Listening → Thinking → Speaking → (Interrupted → Listening) | Idle`。
//! wire 状态枚举用 [`crate::proto::message::TurnState`]。

use tokio_util::sync::CancellationToken;

use crate::{pipeline::splitter::Sentence, proto::message::new_id};

/// `t_` 前缀回合 id。
pub type TurnId = String;

/// 一个进行中/已结束的回合。
#[derive(Debug)]
pub struct Turn {
    pub id: TurnId,
    /// 取消令牌，扇出给 llm/tts/audio 任务；barge-in 后新回合换新 token。
    pub cancel: CancellationToken,
    /// 已下发/已播出的句，用于打断记账（按 `output.sentence.seq` 序）。
    pub committed: Vec<Sentence>,
}

impl Turn {
    /// 开新回合（分配 `t_` id 与新取消令牌）。
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

    /// barge-in 三动作（protocol §6 `control.barge_in`，时序约束见 §9）：
    /// 1. `AudioSink::stop` + 广播 `control.barge_in{applied:true}`，等 UI 回报 `played_seq`；
    /// 2. `self.cancel.cancel()` 扇出终止 LLM stream，并向 TTS/ASR 发 `control.cancel`；
    /// 3. `committed[..=played_seq]` 拼一条 assistant(partial) 写入 history。
    pub fn barge_in(&self) {
        todo!("P0: barge-in 三动作（停播 → 取消上游 → partial 记账）")
    }
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}
