//! `Turn`：回合状态机 + barge-in 取消扇出 + 已播账本。
//!
//! 迁移表对齐 runtime.md「turn 状态机」；非法 state×event 组合由
//! [`transition`]/[`Turn::on_event`] 显式拒绝（`IllegalTransition`）。

use std::{collections::BTreeSet, fmt};

use tokio_util::sync::CancellationToken;

use crate::{
    pipeline::splitter::Sentence,
    proto::message::{TurnOutcome, TurnState, new_id},
};

/// `t_` 前缀回合 id。
pub type TurnId = String;

/// 迁移事件（runtime.md 迁移表的事件列）。
///
/// `NewInput` 与 `Interrupt` 的区别：前者是"新 input 到达，先 invalidate
/// 再接受"，后者是"Stop/PTT/VAD 打断但尚无新输入"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEvent {
    /// `input.audio.start` / VAD `speech_start`：开 capture。
    CaptureStart,
    /// `input.text` accepted / voice final：开 reply turn。
    ReplyAccepted,
    /// `input.audio.segment{final}`：capture 收敛为 reply。
    SegmentFinal,
    /// capture cancel / 空段 / false positive。
    CaptureCancel,
    /// 活动/结算中回合收到新 `input.text`。
    NewInput,
    /// Stop / PTT / VAD `speech_start`：打断活动回合。
    Interrupt,
    /// `playback.started`：唯一进入 `Speaking` 的事件。
    PlaybackStarted,
    /// speaking 中设备切换 / mute / TTS 失败：降级 text-only，移除失败 stream。
    OutputDegraded,
    /// LLM 失败：显式异常终态。
    LlmFailed,
    /// 纯文字/静音回合的 `output.complete`。
    TextComplete,
    /// `output.complete` + required completed ACK 齐备。
    PlaybackComplete,
    /// interruption settlement 结束且无新输入。
    Settled,
}

/// 非法迁移：`state × event` 不在 runtime.md 迁移表内。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IllegalTransition {
    pub state: TurnState,
    pub event: TurnEvent,
}

impl fmt::Display for IllegalTransition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "illegal turn transition: {:?} + {:?}",
            self.state, self.event
        )
    }
}

impl std::error::Error for IllegalTransition {}

/// 纯迁移表：`state × event` → 新状态；非法组合 `Err(IllegalTransition)`。
///
/// 逐行对齐 runtime.md「turn 状态机」mermaid（18 行合法迁移）。
pub fn transition(state: TurnState, event: TurnEvent) -> Result<TurnState, IllegalTransition> {
    use TurnEvent as E;
    use TurnState as S;
    let next = match (state, event) {
        (S::Idle, E::CaptureStart) => S::Listening,
        (S::Idle, E::ReplyAccepted) => S::Thinking,
        (S::Listening, E::SegmentFinal) => S::Thinking,
        (S::Listening, E::CaptureCancel) => S::Idle,
        (S::Thinking, E::PlaybackStarted) => S::Speaking,
        (S::Thinking, E::NewInput) => S::Thinking,
        (S::Thinking, E::Interrupt) => S::Interrupted,
        (S::Thinking, E::LlmFailed) => S::Idle,
        (S::Thinking, E::TextComplete) => S::Idle,
        (S::Speaking, E::Interrupt) => S::Interrupted,
        (S::Speaking, E::NewInput) => S::Interrupted,
        (S::Speaking, E::OutputDegraded) => S::Thinking,
        (S::Speaking, E::PlaybackComplete) => S::Idle,
        (S::Speaking, E::LlmFailed) => S::Idle,
        (S::Interrupted, E::CaptureStart) => S::Listening,
        (S::Interrupted, E::ReplyAccepted) => S::Thinking,
        (S::Interrupted, E::NewInput) => S::Thinking,
        (S::Interrupted, E::Settled) => S::Idle,
        _ => return Err(IllegalTransition { state, event }),
    };
    Ok(next)
}

/// 取消扇出事件：触发这些迁移时终止本 turn 的全部上游任务
/// （LLM stream / splitter / TTS / audio，挂 `cancel` 的 child token）。
fn invalidates(event: TurnEvent) -> bool {
    matches!(
        event,
        TurnEvent::Interrupt
            | TurnEvent::NewInput
            | TurnEvent::LlmFailed
            | TurnEvent::CaptureCancel
    )
}

/// 一个进行中/已结束的回合。
/// `for_reply` 出生即 `Thinking`，`for_capture` 出生即 `Listening`（可在
/// `SegmentFinal` 后同体转 reply）；状态仅经 [`Turn::on_event`] 迁移。
#[derive(Debug)]
pub struct Turn {
    pub id: TurnId,
    /// 取消令牌，扇出给 llm/tts/audio 任务；barge-in 后新回合换新 token。
    pub cancel: CancellationToken,
    /// 已播出句（playback 账本确认），打断记账取 seq 前缀。
    pub committed: Vec<Sentence>,
    /// 已生成句（`output.sentence` 全量），正常完成记账用。
    pub emitted: Vec<Sentence>,
    /// 本回合的用户输入（input.text 原文或 voice final 转写）。
    pub user_text: String,
    /// 绑定的 capture（voice 回合）。
    pub capture_id: Option<String>,
    /// `output.sentence.lang`（跟随输入 lang，默认 `auto`）。
    pub lang: String,
    /// 当前状态；仅经 `on_event` 迁移。
    state: TurnState,
    /// `turn.finished` 幂等闸门：每 turn 恰好一次终态。
    finished: Option<TurnOutcome>,
    /// 等待 completed ACK 的音频句 seq（required ACK 集）。
    pending_audio: BTreeSet<u32>,
    /// `output.complete` 已下发。
    generation_done: bool,
}

impl Turn {
    /// 开新回合（分配 `t_` id 与新取消令牌），`Idle` 起步供 `on_event` 全驱动。
    pub fn new() -> Self {
        Self::with_id(new_id("t"))
    }

    pub fn with_id(id: TurnId) -> Self {
        Self {
            id,
            cancel: CancellationToken::new(),
            committed: Vec::new(),
            emitted: Vec::new(),
            user_text: String::new(),
            capture_id: None,
            lang: "auto".into(),
            state: TurnState::Idle,
            finished: None,
            pending_audio: BTreeSet::new(),
            generation_done: false,
        }
    }

    /// 新 reply 回合：出生即 `Thinking`（runtime.md：input.text accepted → thinking）。
    pub fn for_reply(user_text: impl Into<String>, lang: impl Into<String>) -> Self {
        let mut turn = Self::new();
        turn.state = TurnState::Thinking;
        turn.user_text = user_text.into();
        turn.lang = lang.into();
        turn
    }

    /// 新 capture 回合：出生即 `Listening`。
    pub fn for_capture(capture_id: impl Into<String>) -> Self {
        let mut turn = Self::new();
        turn.state = TurnState::Listening;
        turn.capture_id = Some(capture_id.into());
        turn
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    /// 应用一条迁移：合法则更新状态并执行副作用（中断类事件扇出 `cancel`）；
    /// 非法组合返回 `Err(IllegalTransition)`，状态与令牌保持不变。
    pub fn on_event(&mut self, event: TurnEvent) -> Result<TurnState, IllegalTransition> {
        let next = transition(self.state, event)?;
        if invalidates(event) {
            self.cancel.cancel();
        }
        self.state = next;
        Ok(next)
    }

    /// 幂等 finalize：首个终态返回 `Some(outcome)`（据此发 `turn.finished`），
    /// 之后一律 `None`，保证每 turn 恰好一次。
    pub fn finish(&mut self, outcome: TurnOutcome) -> Option<TurnOutcome> {
        if self.finished.is_some() {
            return None;
        }
        self.finished = Some(outcome);
        Some(outcome)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.is_some()
    }

    pub fn outcome(&self) -> Option<TurnOutcome> {
        self.finished
    }

    /// `output.sentence` 已下发：记入全量生成句。
    pub fn mark_emitted(&mut self, sentence: Sentence) {
        self.emitted.push(sentence);
    }

    /// 句 `seq` 已送 TTS 通道：计入 required completed ACK。
    pub fn expect_audio(&mut self, seq: u32) {
        self.pending_audio.insert(seq);
    }

    /// playback 账本：`sentence_seq` 已开始/继续播出。
    ///
    /// 记 `committed`（打断记账的已播前缀）；按 `index` 保序插入，重复
    /// ACK 幂等。required ACK 只由 `completed=true` 清，见 `ack_completed`。
    pub fn mark_played(&mut self, seq: u32) {
        if self.committed.iter().any(|s| s.index == seq) {
            return;
        }
        if let Some(s) = self.emitted.iter().find(|s| s.index == seq).cloned() {
            self.committed.push(s);
            self.committed.sort_by_key(|s| s.index);
        }
    }

    /// `playback.progress{completed:true}`：required completed ACK 到位。
    pub fn ack_completed(&mut self, seq: u32) {
        self.pending_audio.remove(&seq);
    }

    /// 设备/mute/TTS 失败：移除失败 stream 的 pending（`None` = 全部）。
    pub fn drop_pending_audio(&mut self, seq: Option<u32>) {
        match seq {
            Some(s) => {
                self.pending_audio.remove(&s);
            }
            None => self.pending_audio.clear(),
        }
    }

    /// required completed ACK 是否已清零。
    pub fn acks_settled(&self) -> bool {
        self.pending_audio.is_empty()
    }

    /// `output.complete` 已下发。
    pub fn mark_generation_done(&mut self) {
        self.generation_done = true;
    }

    pub fn generation_done(&self) -> bool {
        self.generation_done
    }
}

impl Default for Turn {
    fn default() -> Self {
        Self::new()
    }
}
