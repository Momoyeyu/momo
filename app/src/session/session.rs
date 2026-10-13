//! `Session` actor：命令循环、回合状态机接线、管线回传。
//!


use futures_util::StreamExt;
use tracing::{debug, warn};

use crate::{
    audio::sink::AudioSink,
    config::Config,
    error::AppError,
    pipeline::{
        llm::{ChatRequest, LlmClient, OpenAiCompatLlm},
    },
    },
    session::{
        history::{ChatMessage, History, Role},
        turn::{Turn, TurnEvent, TurnId},
    },
    ws::conn::OutboundFrame,
};

/// interruption settlement deadline（protocol §8：500ms）。
const SETTLEMENT_DEADLINE: Duration = Duration::from_millis(500);
/// input 幂等回执缓存上限（protocol §5：256）。
const RECEIPT_CACHE: usize = 256;

/// 汇入 Session actor 的命令。
#[derive(Debug)]
pub enum SessionCmd {
    /// conn_rx 解码出的客户端消息。
    Msg(Msg),
    /// 回合管线回传事件（runtime.md：任务只能回传事件，不得改 actor 状态）。
    Pipeline(PipelineEvent),
    Shutdown,
}

/// 管线任务 / 定时器 → Session 的回传事件。
#[derive(Debug)]
pub enum PipelineEvent {
    /// 句切成形句；`audio` 标记该句已送 TTS 通道（计入 required completed ACK）。
    Sentence {
        turn_id: TurnId,
        sentence: crate::pipeline::splitter::Sentence,
        audio: bool,
    },
    /// generation 正常结束（LLM stream 关闭、残句已 flush）。
    GenerationDone { turn_id: TurnId },
    /// LLM/管线失败：显式异常终态。
    Failed { turn_id: TurnId, error: AppError },
    /// 设备/mute/TTS 失败降级；`sentence_seq` 为 `None` 时移除全部 pending stream。
    Degraded {
        turn_id: TurnId,
        sentence_seq: Option<u32>,
    },
    /// settlement deadline 到：interrupted 无新输入则回 idle。
    Settled { turn_id: TurnId },
}

/// Session 构造依赖（AppState 从 config 注入；测试可换 mock）。
#[derive(Clone)]
pub struct SessionDeps {
    /// `MOMO_HISTORY_TURNS`：注入 LLM 的历史回合上限。
    pub history_turns: usize,
    /// `MOMO_SENT_SOFT_CHARS`：句切弱切阈值。
    pub sent_soft_chars: usize,
    /// `MOMO_SENT_MAX_CHARS`：句切硬上限。
    pub sent_max_chars: usize,
    /// LLM client；`None` 时回合直接判 failed（readiness：LLM 缺失不生成 reply）。
    pub llm: Option<Arc<dyn LlmClient>>,
}

impl SessionDeps {
    pub fn from_config(config: &Config) -> Self {
        Self {
            history_turns: config.history_turns,
            sent_soft_chars: config.sent_soft_chars,
            sent_max_chars: config.sent_max_chars,
            llm: Some(Arc::new(OpenAiCompatLlm::from_config(config))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: String,
    /// 该 session 所属连接的 client 类型（hello.client）。
    pub client: ClientKind,
    pub cmd_tx: mpsc::Sender<SessionCmd>,
    /// conn_tx 出站队列（有界 mpsc 背压）；Session/管线经它向连接推帧。
    pub out_tx: mpsc::Sender<OutboundFrame>,
    pub state_rx: watch::Receiver<TurnState>,
}

/// Session actor 本体；由 [`Session::spawn`] 在 tokio task 中运行。
pub struct Session {
    id: String,
    conn_id: String,
    client: ClientKind,
    /// 服务端 envelope 工厂（`state.turn`/`turn.finished`/`control.*` 的 event_seq 序列）。
    env: ServerEnv,
    state_tx: watch::Sender<TurnState>,
    /// conn_tx 出站队列；回合/管线 domain event 经它按序推帧（有界背压）。
    out_tx: mpsc::Sender<OutboundFrame>,
    /// 自投递句柄（管线 task、settlement timer 回传 `SessionCmd::Pipeline`）。
    cmd_tx: mpsc::Sender<SessionCmd>,
    /// 当前回合（含 listening capture 回合）；`None` = idle。
    /// barge-in 时新回合用新 CancellationToken。
    turn: Option<Turn>,
    /// 对话历史与已播文本记账（`MOMO_HISTORY_TURNS`）。
    history: History,
    llm: Option<Arc<dyn LlmClient>>,
    sent_soft_chars: usize,
    sent_max_chars: usize,
}

impl Session {
    /// 创建 actor task 并返回句柄；`out_tx` 是该连接的 conn_tx 出站队列。
    pub fn spawn(
        id: String,
        conn_id: String,
        client: ClientKind,
        out_tx: mpsc::Sender<OutboundFrame>,
        deps: SessionDeps,
    ) -> SessionHandle {
        // 会话命令队列；容量无协议硬约束，取与 conn_tx 同量级。
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (state_tx, state_rx) = watch::channel(TurnState::Idle);
        let session = Session {
            env: ServerEnv::new(id.clone()),
            id: id.clone(),
            conn_id,
            client,
            state_tx,
            out_tx: out_tx.clone(),
            cmd_tx: cmd_tx.clone(),
            turn: None,
            sink: None,
            history: History::new(deps.history_turns),
            llm: deps.llm,
            sent_soft_chars: deps.sent_soft_chars,
            sent_max_chars: deps.sent_max_chars,
            seen_inputs: VecDeque::new(),
        };
        tokio::spawn(session.run(cmd_rx));
        SessionHandle {
            id,
            client,
            cmd_tx,
            out_tx,
            state_rx,
        }
    }

    /// 命令循环：唯一可变状态持有点。
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<SessionCmd>) {
        debug!(session = %self.id, conn = %self.conn_id, client = ?self.client, "session actor started");
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SessionCmd::Msg(msg) => self.on_client_msg(msg).await,
                SessionCmd::Pipeline(ev) => self.on_pipeline(ev).await,
                    }
                    debug!(session = %self.id, conn = %self.conn_id, "session actor exit");
                    break;
                }
            }
        }
        let _ = self.state_tx.send(TurnState::Idle);
        debug!(session = %self.id, "session actor exit");
    }

    /// 客户端消息分派：逐条映射到迁移表事件。
    async fn on_client_msg(&mut self, msg: Msg) {
        match msg {
            Msg::InputText(input) => self.on_input_text(input).await,
            Msg::InputAudioStart(m) => self.on_audio_start(m).await,
            Msg::InputAudioEnd(m) => {
                // capture finalizing；final segment 由 ASR 侧推进。
                debug!(session = %self.id, capture = %m.capture_id, "input.audio.end");
            }
            Msg::InputAudioCancel(m) => self.on_audio_cancel(m).await,
            Msg::InputAudioSegment(seg) => self.on_audio_segment(seg).await,
            Msg::InputVad(vad) => self.on_vad(vad).await,
            Msg::ControlBargeIn(b) => self.on_barge_in(b).await,
            Msg::PlaybackStarted(p) => self.on_playback_started(p).await,
            Msg::PlaybackProgress(p) => self.on_playback_progress(p).await,
            other => warn!(session = %self.id, ?other, "unsupported client msg"),
        }
    }

    // ---------- 输入 → 回合 ----------

    /// `input.text`：空输入拒绝、幂等去重、busy 拒绝、否则 invalidate 旧回合后接受。
    async fn on_input_text(&mut self, input: InputTextMsg) {
        if input.text.trim().is_empty() {
            self.reject_input(input.id, "empty_input").await;
            return;
        }
        if let Some(turn_id) = self.seen_turn(&input.id) {
            // (session_id,id) 幂等：重复输入回 ACK 已有 turn，绝不新建。
            self.accept_input(input.id, turn_id).await;
            return;
        }
        if self.state() == TurnState::Listening {
            // 语音 capture 进行中：新文字暂不抢占（busy turn）。
            self.reject_input(input.id, "busy_turn").await;
            return;
        }
        let turn_id = self
            .accept_reply(input.id.clone(), input.text, input.lang)
            .await;
        self.remember_input(input.id, turn_id);
    }

    /// `input.audio.start`：idle/interrupted 开 capture 回合；thinking/speaking
    /// 中开麦意图（PTT/VAD opt-in）先打断当前回复，新 capture 独立保留。
    async fn on_audio_start(&mut self, m: InputAudioStartMsg) {
        match self.state() {
            TurnState::Listening => {
            }
            TurnState::Thinking | TurnState::Speaking => {
                self.retire_active(Some(TurnEvent::Interrupt)).await;
                self.adopt_capture(m.capture_id).await;
            }
            TurnState::Interrupted => {
                self.retire_active(None).await;
                self.adopt_capture(m.capture_id).await;
            }
            TurnState::Idle => self.adopt_capture(m.capture_id).await,
        }
    }

    async fn on_audio_cancel(&mut self, m: InputAudioMsg) {
        } else {
            warn!(session = %self.id, capture = %m.capture_id, "audio cancel without active capture");
        }
    }

    /// `input.audio.segment`：final 收敛 capture 为 reply（listening → thinking，
    /// 同体回合续跑）；其余状态按新输入 invalidate→accept。
    async fn on_audio_segment(&mut self, seg: InputAudioSegmentMsg) {
        if !seg.is_final {
            debug!(session = %self.id, capture = %seg.capture_id, "partial segment preview");
            return;
        }
        let same_capture = self.state() == TurnState::Listening
            && self
                .turn
                .as_ref()
                .is_some_and(|t| t.capture_id.as_deref() == Some(seg.capture_id.as_str()));
        if same_capture {
            let turn = self.turn.as_mut().expect("checked");
            match turn.on_event(TurnEvent::SegmentFinal) {
                Ok(next) => {
                    turn.user_text = seg.text.clone();
                    let turn_id = turn.id.clone();
                    self.emit_state(&turn_id, next).await;
                    self.spawn_reply();
                }
                Err(err) => warn!(session = %self.id, %err, "segment final rejected"),
            }
        } else {
                .await;
        }
    }

    async fn on_vad(&mut self, vad: InputVadMsg) {
        match vad.event {
            VadEvent::SpeechStart => match self.state() {
                // 打断用的新 capture 保持活动：旧回合结算，进入 listening。
                TurnState::Interrupted => {
                    self.retire_active(None).await;
                    self.adopt_capture(vad.capture_id).await;
                }
                TurnState::Idle => self.adopt_capture(vad.capture_id).await,
                TurnState::Listening => {
                    debug!(session = %self.id, capture = %vad.capture_id, "speech_start inside capture");
                }
            },
            VadEvent::SpeechEnd => {
                debug!(session = %self.id, capture = %vad.capture_id, "speech_end (finalizing)");
            }
        }
    }

    // ---------- 打断与播放账本 ----------

    async fn on_barge_in(&mut self, b: BargeInMsg) {
        let matched = self.turn.as_ref().is_some_and(|t| t.id == b.turn_id);
        if !matched {
            return;
        }
        self.interrupt_active().await;
    }

    /// Stop/PTT/VAD：立即停播 + 扇出取消 → interrupted，arm settlement 定时器。
    async fn interrupt_active(&mut self) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        match turn.on_event(TurnEvent::Interrupt) {
            Ok(next) => {
                let turn_id = turn.id.clone();
                // stop notice 先行（protocol §8：取消不等 ACK）。
                if let Some(sink) = &self.sink {
                    sink.stop();
                }
                self.emit_state(&turn_id, next).await;
                self.arm_settlement(turn_id);
            }
        }
    }

    /// `playback.started`：唯一进入 speaking 的通路；speaking 中后续句只记账。
    async fn on_playback_started(&mut self, p: PlaybackStartedMsg) {
        let Some(turn) = self.turn.as_mut() else {
            warn!(session = %self.id, stream = %p.stream_id, "playback.started with no active turn");
            return;
        };
        match turn.state() {
            TurnState::Thinking => match turn.on_event(TurnEvent::PlaybackStarted) {
                Ok(next) => {
                    let turn_id = turn.id.clone();
                    turn.mark_played(p.sentence_seq);
                    self.emit_state(&turn_id, next).await;
                }
                Err(err) => warn!(session = %self.id, %err, "playback.started rejected"),
            },
            TurnState::Speaking => turn.mark_played(p.sentence_seq),
            _ => warn!(
                session = %self.id,
                state = ?turn.state(),
                "stale playback.started"
            ),
        }
    }

    /// `playback.progress`：听见账本 —— 记已播句、清 required ACK，再判定完成。
    async fn on_playback_progress(&mut self, p: PlaybackProgressMsg) {
        let Some(turn) = self.turn.as_mut() else {
            warn!(session = %self.id, stream = %p.stream_id, "playback.progress with no active turn");
            return;
        };
        turn.mark_played(p.sentence_seq);
        if p.completed {
            turn.ack_completed(p.sentence_seq);
        }
        self.maybe_finish().await;
    }


    async fn on_pipeline(&mut self, ev: PipelineEvent) {
        match ev {
            PipelineEvent::Sentence {
                turn_id,
                sentence,
                audio,
            } => {
                let Some(turn) = self.live_turn(&turn_id) else {
                    return;
                };
                turn.mark_emitted(sentence.clone());
                if audio {
                    turn.expect_audio(sentence.index);
                }
                let lang = turn.lang.clone();
                self.send(Msg::OutputSentence(OutputSentenceMsg {
                    env: self.env.next(),
                    turn_id,
                    seq: sentence.index,
                    text: sentence.text,
                    emotion: sentence.emotion.unwrap_or_default(),
                    lang,
                }))
                .await;
            }
            PipelineEvent::GenerationDone { turn_id } => {
                let Some(turn) = self.live_turn(&turn_id) else {
                    return;
                };
                turn.mark_generation_done();
                let last = turn.emitted.last().map(|s| s.index).unwrap_or(0);
                self.send(Msg::OutputComplete(OutputCompleteMsg {
                    env: self.env.next(),
                    turn_id,
                    last_sentence_seq: last,
                }))
                .await;
                self.maybe_finish().await;
            }
            PipelineEvent::Failed { turn_id, error } => {
                if self.live_turn(&turn_id).is_none() {
                    return;
                }
                // LLM 失败：invalidate audio + 清 pending + finished(failed)。
                if let Some(t) = self.turn.as_mut() {
                    if let Ok(next) = t.on_event(TurnEvent::LlmFailed) {
                        let id = t.id.clone();
                        self.emit_state(&id, next).await;
                        if let Some(turn) = self.turn.take() {
                            self.finalize(turn, TurnOutcome::Failed).await;
                        }
                    }
                }
            }
            PipelineEvent::Degraded {
                turn_id,
                sentence_seq,
            } => {
                    return;
                };
                match turn.state() {
                    TurnState::Speaking => {
                        // 设备/mute/TTS 失败降级：移除失败 stream，剩余 text-only。
                        if let Ok(next) = turn.on_event(TurnEvent::OutputDegraded) {
                            let id = turn.id.clone();
                            turn.drop_pending_audio(sentence_seq);
                            self.emit_state(&id, next).await;
                        }
                    }
                    TurnState::Thinking => turn.drop_pending_audio(sentence_seq),
                    _ => {}
                }
                self.maybe_finish().await;
            }
            PipelineEvent::Settled { turn_id } => {
                let Some(turn) = self
                    .turn
                    .as_mut()
                    .filter(|t| t.id == turn_id && t.state() == TurnState::Interrupted)
                else {
                    return;
                };
                if let Ok(next) = turn.on_event(TurnEvent::Settled) {
                    let id = turn.id.clone();
                    self.emit_state(&id, next).await;
                }
                if let Some(turn) = self.turn.take() {
                    self.finalize(turn, TurnOutcome::Interrupted).await;
                }
            }
        }
    }

    /// 防 stale stream：id 不符、已取消或已收尾的回合事件一律丢弃。
    fn live_turn(&mut self, turn_id: &str) -> Option<&mut Turn> {
        match self.turn.as_mut() {
            Some(t) if t.id == turn_id && !t.cancel.is_cancelled() && !t.is_finished() => Some(t),
            _ => {
                debug!(session = %self.id, %turn_id, "stale pipeline event dropped");
                None
            }
        }
    }

    /// `output.complete` + required completed ACK 齐备 → 终态回 idle。
    async fn maybe_finish(&mut self) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        if !turn.generation_done() || !turn.acks_settled() {
            return;
        }
        let ev = match turn.state() {
            TurnState::Thinking => TurnEvent::TextComplete,
            TurnState::Speaking => TurnEvent::PlaybackComplete,
            _ => return,
        };
        let Ok(next) = turn.on_event(ev) else {
            return;
        };
        let turn_id = turn.id.clone();
        self.emit_state(&turn_id, next).await;
        if let Some(turn) = self.turn.take() {
            self.finalize(turn, TurnOutcome::Completed).await;
        }
    }

    // ---------- 回合生命周期 ----------

    fn state(&self) -> TurnState {
        self.turn
            .as_ref()
            .map(Turn::state)
            .unwrap_or(TurnState::Idle)
    }

    /// 接管新回合：绑定每回合 `AudioSink`（与 conn_tx + env seq 共享）。
    fn adopt(&mut self, turn: Turn) {
            self.out_tx.clone(),
            turn.id.clone(),
            self.env.clone(),
        self.turn = Some(turn);
    }

    /// 新 reply 回合：原子 invalidate 旧回合 → thinking → 发管线。
    async fn accept_reply(&mut self, ref_id: String, user_text: String, lang: String) -> TurnId {
        match self.state() {
            TurnState::Thinking | TurnState::Speaking => {
                self.retire_active(Some(TurnEvent::NewInput)).await;
            }
            TurnState::Interrupted => self.retire_active(None).await,
            TurnState::Idle => {}
        }
        let turn = Turn::for_reply(user_text, lang);
        let turn_id = turn.id.clone();
        self.adopt(turn);
        self.accept_input(ref_id, turn_id.clone()).await;
        self.emit_state(&turn_id, TurnState::Thinking).await;
        self.spawn_reply();
        turn_id
    }

    /// 新 capture 回合（listening）。
    async fn adopt_capture(&mut self, capture_id: String) {
        let turn = Turn::for_capture(capture_id.clone());
        let turn_id = turn.id.clone();
        self.adopt(turn);
        self.emit_state(&turn_id, TurnState::Listening).await;
    }

    /// invalidate 当前回合并终态收尾：`ev` 驱动旧 turn 走完打断迁移行
    /// （thinking/speaking → interrupted），随后记账 + `turn.finished`。
    async fn retire_active(&mut self, ev: Option<TurnEvent>) {
        let Some(mut old) = self.turn.take() else {
            return;
        };
        if let Some(ev) = ev {
            let prev = old.state();
            if let Ok(next) = old.on_event(ev) {
                if next != prev {
                    self.emit_state(&old.id, next).await;
                }
            }
        }
        self.finalize(old, TurnOutcome::Interrupted).await;
    }

    /// 终态收尾：completed → `drain`（排空 + `audio.end`）；其余 → `stop`
    /// （丢弃未播 + 停播指令）。记账后 `turn.finished` 恰好一次。
    async fn finalize(&mut self, mut turn: Turn, outcome: TurnOutcome) {
        match outcome {
            TurnOutcome::Completed => {
                if let Some(sink) = &self.sink {
                    if let Err(err) = sink.drain().await {
                        warn!(session = %self.id, %err, "audio drain failed");
                    }
                }
            }
            _ => {
                if let Some(sink) = &self.sink {
                    sink.stop();
                }
            }
        }
        self.sink = None;
        match outcome {
            TurnOutcome::Completed => self.history.commit(&turn.user_text, &turn.emitted),
            _ => self
                .history
                .commit_partial(&turn.user_text, &turn.committed),
        }
        if let Some(o) = turn.finish(outcome) {
            self.send(Msg::TurnFinished(TurnFinishedMsg {
                env: self.env.next(),
                turn_id: turn.id.clone(),
                outcome: o,
            }))
            .await;
        }
    }

    /// settlement 定时器：500ms 无新输入 → interrupted 回 idle。
    fn arm_settlement(&self, turn_id: TurnId) {
        let tx = self.cmd_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(SETTLEMENT_DEADLINE).await;
            let _ = tx
                .send(SessionCmd::Pipeline(PipelineEvent::Settled { turn_id }))
                .await;
        });
    }

    // ---------- 回合管线 ----------

    fn spawn_reply(&self) {
        let Some(turn) = &self.turn else {
            return;
        };
        let turn_id = turn.id.clone();
        let Some(llm) = self.llm.clone() else {
            let _ = self
                .cmd_tx
                .try_send(SessionCmd::Pipeline(PipelineEvent::Failed {
                    turn_id,
                    error: AppError::ModelUnreachable("llm client not configured".into()),
                }));
            return;
        };
        let req = ChatRequest {
            model: String::new(),
            messages: self.llm_prompt(&turn.user_text),
            stream: true,
        };
        let soft = self.sent_soft_chars;
        let hard = self.sent_max_chars;
        tokio::spawn(async move {
                Ok(s) => s,
                Err(error) => {
                    return;
                }
            };
            let mut splitter = SentenceSplitter::new(soft, hard);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(delta) => {
                        for sentence in splitter.push(&delta) {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        return;
                    }
                }
            }
            if let Some(sentence) = splitter.flush() {
            }
                .send(SessionCmd::Pipeline(PipelineEvent::GenerationDone {
                }))
                .await;
        });
    }

    fn llm_prompt(&self, user_text: &str) -> Vec<ChatMessage> {
        let mut messages = self.history.messages().to_vec();
        messages.push(ChatMessage {
            role: Role::User,
            content: user_text.to_string(),
            partial: false,
        });
        messages
    }

    // ---------- 出站与幂等 ----------

    async fn emit_state(&mut self, turn_id: &str, state: TurnState) {
        let _ = self.state_tx.send(state);
        self.send(Msg::StateTurn(StateTurnMsg {
            env: self.env.next(),
            turn_id: turn_id.to_string(),
            state,
        }))
        .await;
    }

    async fn send(&mut self, msg: Msg) {
        if self.out_tx.send(OutboundFrame::Json(msg)).await.is_err() {
            debug!(session = %self.id, "out_tx closed, frame dropped");
        }
    }

    async fn accept_input(&mut self, ref_id: String, turn_id: TurnId) {
        self.send(Msg::InputAccepted(InputAcceptedMsg {
            env: self.env.next(),
            ref_id,
            turn_id,
        }))
        .await;
    }

    async fn reject_input(&mut self, ref_id: String, reason: &str) {
        self.send(Msg::InputRejected(InputRejectedMsg {
            env: self.env.next(),
            ref_id,
            reason: reason.to_string(),
        }))
        .await;
    }

    fn seen_turn(&self, input_id: &str) -> Option<TurnId> {
        self.seen_inputs
            .iter()
    }

    fn remember_input(&mut self, input_id: String, turn_id: TurnId) {
        if self.seen_inputs.len() > RECEIPT_CACHE {
            self.seen_inputs.pop_front();
        }
    }
}
