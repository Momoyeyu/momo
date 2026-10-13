//! `Session` actor：命令循环、回合状态机接线、管线回传。
//!
//! 唯一可变状态持有者：所有事件汇进它的 `mpsc`，内部不共享锁
//! （momo-app.md §ws 子系统）；迁移对齐 runtime.md「turn 状态机」。

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use futures_util::StreamExt;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use ulid::Ulid;

use crate::{
    audio::sink::AudioSink,
    config::Config,
    error::AppError,
    pipeline::{
        llm::{ChatRequest, LlmClient, OpenAiCompatLlm},
        splitter::{Sentence, SentenceSplitter},
    },
    proto::{
        codec::{AudioHeader, Direction},
        message::{
            AudioCodec, AudioEndMsg, AudioOpenMsg, BargeInMsg, CancelTarget, ClientKind,
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
/// 上行缓冲帧数：50ms/帧 × 40 = 2s（runtime.md「mic uplink buffer 最多 2s」）。
const UPLINK_BUFFER_FRAMES: usize = 40;

/// 汇入 Session actor 的命令。
#[derive(Debug)]
pub enum SessionCmd {
    /// conn_rx 解码出的客户端消息。
    Msg(Msg),
    /// conn_rx 解码的上行二进制音频帧（按 `audio.open` 绑定路由）。
    Audio(AudioHeader, Bytes),
    /// svc-asr 链路回流事件（input.vad / input.audio.segment / 链路终态）。
    Asr(AsrEvent),
    /// 回合管线回传事件（runtime.md：任务只能回传事件，不得改 actor 状态）。
    Pipeline(PipelineEvent),
    /// WS 断开：取消活动回合、保留会话历史，进入 `MOMO_SESSION_RESUME_TTL_MS`
    /// 保活窗口等 resume（protocol §5）。`conn_id` 校验防 stale conn 误清。
    ConnClosed { conn_id: String },
    /// `hello.resume_session_id` 命中：session 换绑新 conn 的出站队列并下发
    Reattach {
        conn_id: String,
        out_tx: mpsc::Sender<OutboundFrame>,
    },
    /// 保活窗口到点；`seq` 与当前 detached 代次匹配才退出（防 stale 计时器）。
    ResumeExpired { seq: u64 },
    /// 被新 session 顶掉 / 进程关停。
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
    /// `error` 为需通知 UI 的原因（合成失败/断链）；`None` 表示报告方已自行
    /// 出站告警（如 sink 溢出已发 error + control.cancel），不再重复发帧。
    Degraded {
        turn_id: TurnId,
        sentence_seq: Option<u32>,
        error: Option<AppError>,
    },
    /// settlement deadline 到：interrupted 无新输入则回 idle。
    Settled { turn_id: TurnId },
    /// VAD speech_start 防抖计时到点；`seq` 与当前 pending 匹配才生效。
    VadDebounce { seq: u64, capture_id: String },
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
    /// ASR client；`None` 时 `audio.open{uplink}` 拒绝绑定（MODEL_UNREACHABLE）。
    pub asr: Option<Arc<dyn AsrClient>>,
    /// TTS client；barge-in 时扇出 `control.cancel{target:turn}`。
    pub tts: Option<Arc<dyn TtsClient>>,
    /// `MOMO_BARGEIN_ENABLED`：VAD speech_start 是否允许打断。
    pub barge_in_enabled: bool,
    /// `MOMO_BARGEIN_MIN_MS`：speech_start 防抖时长。
    pub barge_in_min: Duration,
    /// `MOMO_SESSION_RESUME_TTL_MS`：WS 断开后 session 保活窗口。
    pub resume_ttl: Duration,
}

impl SessionDeps {
    pub fn from_config(config: &Config) -> Self {
        Self {
            history_turns: config.history_turns,
            sent_soft_chars: config.sent_soft_chars,
            sent_max_chars: config.sent_max_chars,
            llm: Some(Arc::new(OpenAiCompatLlm::from_config(config))),
            barge_in_enabled: config.barge_in_enabled,
            barge_in_min: config.barge_in_min,
            resume_ttl: config.session_resume_ttl,
        }
    }
}

/// 外界持有的会话句柄（momo-app.md §ws 子系统）。
#[derive(Debug, Clone)]
pub struct SessionHandle {
    pub id: String,
    /// 该 session 所属连接的 client 类型（hello.client）。
    pub client: ClientKind,
    pub cmd_tx: mpsc::Sender<SessionCmd>,
    /// conn_tx 出站队列（有界 mpsc 背压）；Session/管线经它向连接推帧。
    pub out_tx: mpsc::Sender<OutboundFrame>,
    pub state_rx: watch::Receiver<TurnState>,
    /// 当前归属 conn_id：resume 换主时变更，actor 退出时 sender 掉落；
    /// conn 据此实现单活跃顶掉（protocol §认证）。
    pub conn_rx: watch::Receiver<String>,
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
    /// conn 归属广播：resume 换主时写入新 conn_id，actor 退出随 self 掉落。
    conn_watch: watch::Sender<String>,
    /// 当前回合（含 listening capture 回合）；`None` = idle。
    /// barge-in 时新回合用新 CancellationToken。
    turn: Option<Turn>,
    /// 当前回合的下行音频下发器（与 turn 同生命周期）；
    /// `Arc` 共享给各句合成任务 push，stop/drain/溢出语义不变。
    sink: Option<Arc<AudioSink>>,
    /// 对话历史与已播文本记账（`MOMO_HISTORY_TURNS`）。
    history: History,
    llm: Option<Arc<dyn LlmClient>>,
    asr: Option<Arc<dyn AsrClient>>,
    tts: Option<Arc<dyn TtsClient>>,
    sent_soft_chars: usize,
    sent_max_chars: usize,
    /// `(input.id → turn_id,status)` 幂等回执缓存（protocol §7：重复输入
    /// ACK 已有 turn；§5：snapshot receipts 收敛凭据）。
    seen_inputs: VecDeque<InputReceipt>,
    /// 活动 uplink stream 绑定（`stream_id` 原始 ULID → 转发通道）；
    /// 独立于 turn 生命周期，capture 不随旧 turn cancel 删除。
    uplinks: HashMap<Ulid, Uplink>,
    /// capture 注册表（`capture_id` → 绑定信息）；独立于 reply turn。
    captures: HashMap<String, CaptureInfo>,
    barge_in_enabled: bool,
    barge_in_min: Duration,
    /// VAD speech_start 防抖：计时中的 pending（seq 代次防 stale 确认）。
    vad_pending: Option<VadPending>,
    vad_seq: u64,
    /// 断线保活窗口（`MOMO_SESSION_RESUME_TTL_MS`）：超时 actor 退出回收。
    resume_ttl: Duration,
    /// `Some(seq)` = detached 等 resume；`seq` 防前一轮计时器误杀新窗口。
    detached: Option<u64>,
    detach_seq: u64,
}

/// 一条活动 uplink stream（`audio.open{uplink}` 建立，`audio.end`/abort 收尾）。
#[derive(Debug)]
struct Uplink {
    capture_id: String,
    /// per-capture 有界缓冲（≤2s）；溢出/seq gap 由 actor 中止 capture。
    tx: mpsc::Sender<UplinkCmd>,
    /// 期望的下一帧 `chunk_seq`/`sample_offset`（重复丢、gap 中止）。
    next_seq: u32,
    next_offset: u32,
    eos_seen: bool,
}

/// uplink task 的消费单元：数据帧与 stream 收尾按序经同一队列（保 wire 序）。
#[derive(Debug)]
enum UplinkCmd {
    Data(AudioHeader, Bytes),
    End { last_chunk_seq: u32 },
}

/// capture 登记信息：独立于 reply turn，打断后保持活动。
#[derive(Debug, Default)]
struct CaptureInfo {
    /// 已绑定 uplink stream（`audio.open{uplink}`）。
    stream: Option<Ulid>,
}

/// 防抖中的 speech_start：到点经 `VadDebounce` 回传，`seq` 匹配才生效。
#[derive(Debug)]
struct VadPending {
    seq: u64,
    capture_id: String,
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
        let (conn_watch, conn_rx) = watch::channel(conn_id.clone());
        let session = Session {
            env: ServerEnv::new(id.clone()),
            id: id.clone(),
            conn_id,
            client,
            state_tx,
            conn_watch,
            out_tx: out_tx.clone(),
            cmd_tx: cmd_tx.clone(),
            turn: None,
            sink: None,
            history: History::new(deps.history_turns),
            llm: deps.llm,
            asr: deps.asr,
            tts: deps.tts,
            sent_soft_chars: deps.sent_soft_chars,
            sent_max_chars: deps.sent_max_chars,
            seen_inputs: VecDeque::new(),
            uplinks: HashMap::new(),
            captures: HashMap::new(),
            barge_in_enabled: deps.barge_in_enabled,
            barge_in_min: deps.barge_in_min,
            vad_pending: None,
            vad_seq: 0,
            resume_ttl: deps.resume_ttl,
            detached: None,
            detach_seq: 0,
        };
        // svc-asr 回流事件 → SessionCmd（单活跃 session：新订阅取代旧）。
        if let Some(asr) = session.asr.clone() {
            let cmd_tx = cmd_tx.clone();
            tokio::spawn(async move {
                let mut events = asr.subscribe().await;
                while let Some(ev) = events.recv().await {
                    if cmd_tx.send(SessionCmd::Asr(ev)).await.is_err() {
                        return;
                    }
                }
            });
        }
        tokio::spawn(session.run(cmd_rx));
        SessionHandle {
            id,
            client,
            cmd_tx,
            out_tx,
            state_rx,
            conn_rx,
        }
    }

    /// 命令循环：唯一可变状态持有点。
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<SessionCmd>) {
        debug!(session = %self.id, conn = %self.conn_id, client = ?self.client, "session actor started");
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SessionCmd::Msg(msg) => self.on_client_msg(msg).await,
                SessionCmd::Audio(header, payload) => self.on_uplink_frame(header, payload).await,
                SessionCmd::Asr(ev) => self.on_asr(ev).await,
                SessionCmd::Pipeline(ev) => self.on_pipeline(ev).await,
                SessionCmd::ConnClosed { conn_id } => {
                    if conn_id == self.conn_id {
                        self.detach().await;
                    } else {
                        // resume 换主后旧 conn 的迟到通知：忽略。
                        debug!(session = %self.id, %conn_id, "stale ConnClosed ignored");
                    }
                }
                SessionCmd::Reattach { conn_id, out_tx } => {
                    self.on_reattach(conn_id, out_tx).await;
                }
                SessionCmd::ResumeExpired { seq } => {
                    if self.detached == Some(seq) {
                        debug!(session = %self.id, "resume window expired, session actor exit");
                        break;
                    }
                }
                SessionCmd::Shutdown => {
                    self.teardown().await;
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
            Msg::AudioOpen(open) => self.on_audio_open(open).await,
            Msg::AudioEnd(end) => self.on_audio_end(end).await,
            Msg::ControlCancel(c) => self.on_control_cancel(c).await,
            Msg::ControlBargeIn(b) => self.on_barge_in(b).await,
            Msg::PlaybackStarted(p) => self.on_playback_started(p).await,
            Msg::PlaybackProgress(p) => self.on_playback_progress(p).await,
            other => warn!(session = %self.id, ?other, "unsupported client msg"),
        }
    }

    // ---------- 连接归属与保活 ----------

    /// 断连/关停公共清理：取消活动回合（上游 token 扇出）、已播部分记账、
    /// receipt 标 interrupted；uplink sender 全部掉落 → task 向 svc-asr 发
    /// `control.cancel{capture}`。
    async fn teardown(&mut self) {
        if let Some(turn) = self.turn.take() {
            turn.cancel.cancel();
            self.history
                .commit_partial(&turn.user_text, &turn.committed);
            self.mark_receipts(&turn.id, ReceiptStatus::Interrupted);
        }
        self.uplinks.clear();
        self.captures.clear();
        self.sink = None;
    }

    /// WS 断开进保活窗口：清理活动对象后 detached，TTL 内等
    /// `hello.resume_session_id` 命中；超时 `ResumeExpired` 退出回收。
    async fn detach(&mut self) {
        self.teardown().await;
        self.detach_seq += 1;
        let seq = self.detach_seq;
        self.detached = Some(seq);
        let tx = self.cmd_tx.clone();
        let ttl = self.resume_ttl;
        tokio::spawn(async move {
            tokio::time::sleep(ttl).await;
            let _ = tx.send(SessionCmd::ResumeExpired { seq }).await;
        });
        debug!(
            session = %self.id,
            conn = %self.conn_id,
            ttl_ms = ttl.as_millis() as u64,
            "connection closed, session detached"
        );
    }

    /// resume 命中：换绑新 conn 出站队列与 conn 归属广播，在途回合的
    /// downlink sink 一并换绑，随后下发 `session.snapshot`。
    async fn on_reattach(&mut self, conn_id: String, out_tx: mpsc::Sender<OutboundFrame>) {
        self.detached = None;
        self.conn_id = conn_id.clone();
        self.out_tx = out_tx;
        // conn 换主：旧 conn 监听发现 id 不符即按 close 1000 退出（单活跃）。
        let _ = self.conn_watch.send(conn_id);
        if let Some(turn_id) = self.turn.as_ref().map(|t| t.id.clone()) {
            self.sink = Some(Arc::new(AudioSink::spawn(
                self.out_tx.clone(),
                turn_id,
                self.env.clone(),
            )));
        }
        self.send_snapshot().await;
        debug!(session = %self.id, conn = %self.conn_id, "session resumed");
    }

    /// `session.snapshot`：无音频重放的状态收敛（protocol §4-5）。
    /// `last_event_seq` 报已分配水位，不携带自身 `event_seq`。
    async fn send_snapshot(&mut self) {
        let snapshot = Msg::SessionSnapshot(SessionSnapshotMsg {
            session_id: self.id.clone(),
            turn: self.turn.as_ref().map(|t| SnapshotTurn {
                id: t.id.clone(),
                state: t.state(),
            }),
            receipts: self.seen_inputs.iter().cloned().collect(),
            capabilities: vec!["barge_in".into(), "emotion_tags".into()],
            last_event_seq: self.env.last_seq(),
        });
        self.send(snapshot).await;
    }

    /// turn 终态回写 receipt status（snapshot receipts 收敛凭据）。
    fn mark_receipts(&mut self, turn_id: &str, status: ReceiptStatus) {
        for receipt in &mut self.seen_inputs {
            if receipt.turn_id == turn_id {
                receipt.status = status;
            }
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
        self.captures.entry(m.capture_id.clone()).or_default();
        if let Some(turn_id) = self.seen_turn(&m.capture_id) {
            // (session_id,capture_id) 幂等：重投的 audio.start ACK 已有 turn，不新开 capture。
            self.accept_input(m.capture_id, turn_id).await;
            return;
        }
        match self.state() {
            TurnState::Listening => {
                // 异 capture 并发开麦：拒绝（单活跃 capture）。
                self.reject_input(m.capture_id, "busy_capture").await;
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

    /// `input.audio.cancel`：capture 取消（listening → idle）+ uplink 收尾。
    async fn on_audio_cancel(&mut self, m: InputAudioMsg) {
        let had_capture = self.captures.contains_key(&m.capture_id);
        if self.state() == TurnState::Listening || had_capture {
            self.close_capture(&m.capture_id, None).await;
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
                    // 语音回执：ref_id = capture_id（protocol §7）。
                    self.accept_input(seg.capture_id, turn_id.clone()).await;
                    self.emit_state(&turn_id, next).await;
                    self.spawn_reply();
                }
                Err(err) => warn!(session = %self.id, %err, "segment final rejected"),
            }
        } else if let Some(turn_id) = self.seen_turn(&seg.capture_id) {
            // (session_id,capture_id) 幂等：重投的 final 回执已有 turn，绝不新建。
            self.accept_input(seg.capture_id, turn_id).await;
        } else {
            let turn_id = self
                .accept_reply(seg.capture_id.clone(), seg.text, "auto".into())
                .await;
            self.remember_input(seg.capture_id, turn_id);
        }
    }

    /// `input.vad`：speech_start 依状态走 capture 开启或防抖打断；
    /// speech_end 取消防抖并标记 capture finalizing（capture 状态机不在 turn FSM 内）。
    async fn on_vad(&mut self, vad: InputVadMsg) {
        match vad.event {
            VadEvent::SpeechStart => match self.state() {
                // barge-in：speech_start 须持续 MOMO_BARGEIN_MIN_MS 才打断。
                TurnState::Thinking | TurnState::Speaking if self.barge_in_enabled => {
                    self.arm_vad_debounce(vad.capture_id);
                }
                TurnState::Thinking | TurnState::Speaking => {
                    debug!(session = %self.id, capture = %vad.capture_id, "barge-in disabled, speech_start ignored");
                }
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
                // 未到防抖时长的 speech 是 false positive：撤销 pending 不打断。
                if self
                    .vad_pending
                    .as_ref()
                    .is_some_and(|p| p.capture_id == vad.capture_id)
                {
                    self.vad_pending = None;
                }
                debug!(session = %self.id, capture = %vad.capture_id, "speech_end (finalizing)");
            }
        }
    }

    // ---------- 打断与播放账本 ----------

    /// `control.barge_in`：打断指定 turn；stale turn_id 显式拒绝（控制消息不静默丢弃）。
    async fn on_barge_in(&mut self, b: BargeInMsg) {
        let matched = self.turn.as_ref().is_some_and(|t| t.id == b.turn_id);
        if !matched {
            warn!(session = %self.id, turn = %b.turn_id, reason = ?b.reason, "barge_in for unknown/stale turn");
            .await;
            return;
        }
        self.interrupt_active().await;
    }

    /// Stop/PTT/VAD：立即停播 + 扇出取消 → interrupted，arm settlement 定时器。
    async fn interrupt_active(&mut self) {
        self.vad_pending = None;
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
                // 向 svc-tts/svc-asr 扇出 control.cancel{target:turn}（不等 ACK）。
                self.cancel_upstreams(&turn_id);
                self.emit_state(&turn_id, next).await;
                self.arm_settlement(turn_id);
            }
            Err(err) => {
                warn!(session = %self.id, %err, "interrupt rejected");
            }
        }
    }

    /// 尽力取消扇出：svc-tts + svc-asr 收 `control.cancel{target:turn}`；
    /// spawn 异步执行不阻塞 actor 循环，不等 ACK。
    fn cancel_upstreams(&self, turn_id: &str) {
        if let Some(tts) = &self.tts {
            let (tts, id) = (tts.clone(), turn_id.to_string());
            tokio::spawn(async move {
                let _ = tts.cancel_turn(&id).await;
            });
        }
        if let Some(asr) = &self.asr {
            let (asr, id) = (asr.clone(), turn_id.to_string());
            tokio::spawn(async move {
                let _ = asr.cancel(CancelTarget::Turn, &id).await;
            });
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

    // ---------- 上行音频：stream ↔ capture ----------

    /// `audio.open`（UI 侧）：仅接受 uplink；校验后建立 stream↔capture 绑定。
    async fn on_audio_open(&mut self, open: AudioOpenMsg) {
        if let Err(err) = open.validate() {
            return;
        }
        match open.direction {
            Direction::Uplink => self.bind_uplink(open).await,
            Direction::Downlink => {
                .await;
            }
        }
    }

    /// uplink 绑定：格式校验 → spawn per-capture uplink task → 登记路由表。
    /// v2 上行固定 pcm_s16le/16kHz/mono（protocol §6）。
    async fn bind_uplink(&mut self, open: AudioOpenMsg) {
        let Some(key) = stream_key(&open.stream_id) else {
            .await;
            return;
        };
        if self.uplinks.contains_key(&key) {
            .await;
            return;
        }
        if open.codec != AudioCodec::PcmS16le
            || open.sample_rate != UPLINK_SAMPLE_RATE
            || open.channels != 1
        {
            .await;
            return;
        }
        let Some(asr) = self.asr.clone() else {
            .await;
            return;
        };
        let capture_id = open.capture_id.clone().expect("validate() checked");
        let (tx, rx) = mpsc::channel(UPLINK_BUFFER_FRAMES);
        tokio::spawn(run_uplink(
            asr,
            open,
            rx,
            self.cmd_tx.clone(),
            capture_id.clone(),
        ));
        self.uplinks.insert(
            key,
            Uplink {
                capture_id: capture_id.clone(),
                tx,
                next_seq: 0,
                next_offset: 0,
                eos_seen: false,
            },
        );
        // UI 未发 input.audio.start 的 capture 也登记（连续麦 opt-in）。
        self.captures.entry(capture_id).or_default().stream = Some(key);
    }

    /// conn_rx 上行帧 → per-capture 缓冲：归属/seq/offset 校验，重复幂等丢、
    /// gap 或缓冲溢出中止 capture（不静默丢帧，protocol §6/runtime.md）。
    async fn on_uplink_frame(&mut self, header: AudioHeader, payload: Bytes) {
        if header.direction != Direction::Uplink {
            .await;
            return;
        }
        let Some(up) = self.uplinks.get_mut(&header.stream_id) else {
            // unknown/stale stream：abort 时已回报，后续帧静默防 error 风暴。
            debug!(stream = %header.stream_id, "dropping frame for unknown/stale uplink");
            return;
        };
        if up.eos_seen {
            debug!(stream = %header.stream_id, "dropping post-EOS uplink frame");
            return;
        }
        if header.chunk_seq < up.next_seq {
            debug!(stream = %header.stream_id, seq = header.chunk_seq, "dropping duplicate uplink frame");
            return;
        }
        if header.chunk_seq > up.next_seq || header.sample_offset != up.next_offset {
            let err = AppError::BadMessage(format!(
                "uplink seq gap: want seq {} offset {}, got seq {} offset {}",
                up.next_seq, up.next_offset, header.chunk_seq, header.sample_offset
            ));
            let capture_id = up.capture_id.clone();
            self.abort_capture(&capture_id, err).await;
            return;
        }
        up.next_seq += 1;
        up.next_offset += (payload.len() / 2) as u32;
        if header.eos() {
            up.eos_seen = true;
        }
        match up.tx.try_send(UplinkCmd::Data(header, payload)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                let err = AppError::BackpressureOverflow(format!(
                    "uplink buffer exceeded {UPLINK_BUFFER_FRAMES} frames"
                ));
                let capture_id = up.capture_id.clone();
                self.abort_capture(&capture_id, err).await;
            }
        }
    }

    /// `audio.end`：校验 `last_chunk_seq` 与最后接收帧一致后按序收尾 stream。
    async fn on_audio_end(&mut self, end: AudioEndMsg) {
        let Some(key) = stream_key(&end.stream_id) else {
            .await;
            return;
        };
        let Some(up) = self.uplinks.get(&key) else {
            debug!(stream = %end.stream_id, "dropping audio.end for unknown/stale uplink");
            return;
        };
        // EOS 帧是最后接收帧：last_chunk_seq 必须等于它（protocol §6）。
        let ok = up.next_seq > 0 && end.last_chunk_seq == up.next_seq - 1;
        let capture_id = up.capture_id.clone();
        if !ok {
            let err = AppError::BadMessage(format!(
                "audio.end last_chunk_seq {} != last received {}",
                end.last_chunk_seq,
                up.next_seq.saturating_sub(1)
            ));
            self.abort_capture(&capture_id, err).await;
            return;
        }
        let Some(up) = self.uplinks.remove(&key) else {
            return;
        };
        if up
            .tx
            .try_send(UplinkCmd::End {
                last_chunk_seq: end.last_chunk_seq,
            })
            .is_err()
        {
            // 通道满/闭 = 数据积压未排空：按溢出语义中止 capture。
            self.abort_capture(
                &capture_id,
                AppError::BackpressureOverflow("uplink end could not drain".into()),
            )
            .await;
            return;
        }
        if let Some(cap) = self.captures.get_mut(&capture_id) {
            cap.stream = None;
        }
    }

    /// `control.cancel`（UI 侧）：`target:capture` 等价 input.audio.cancel。
    async fn on_control_cancel(&mut self, c: ControlCancelMsg) {
        match c.target {
            CancelTarget::Capture => self.close_capture(&c.id, None).await,
            other => {
                debug!(session = %self.id, ?other, id = %c.id, "unsupported control.cancel from ui")
            }
        }
    }

    /// capture 中止（seq gap/溢出/链路失败）：摘 uplink → 通知 UI + svc-asr，
    /// listening 中对应 turn 按 CaptureCancel 收尾。
    async fn abort_capture(&mut self, capture_id: &str, err: AppError) {
        self.close_capture(capture_id, Some(err)).await;
    }

    /// capture 收尾公共路径：`err` 为 Some 时附带 error 帧（异常中止）。
    /// 摘 uplink sender 掉落 → uplink task 向 svc-asr 发 control.cancel{capture}。
    async fn close_capture(&mut self, capture_id: &str, err: Option<AppError>) {
        self.uplinks.retain(|_, up| up.capture_id != capture_id);
        self.captures.remove(capture_id);
        if let Some(err) = err {
        }
        // 通知 UI 释放 mic（control.cancel 向各方向取消同一对象，protocol §2）。
        self.send(Msg::ControlCancel(ControlCancelMsg {
            env: self.env.next(),
            target: CancelTarget::Capture,
            id: capture_id.to_string(),
        }))
        .await;
        if self.state() == TurnState::Listening
            && self
                .turn
                .as_ref()
                .is_some_and(|t| t.capture_id.as_deref() == Some(capture_id))
        {
            self.retire_active(Some(TurnEvent::CaptureCancel)).await;
        }
    }

    /// svc-asr 回流事件：vad/segment 先镜像 UI（restamp 本 session envelope），
    /// 再走与 UI 注入相同的处理；链路终态中止全部活动 capture。
    async fn on_asr(&mut self, ev: AsrEvent) {
        match ev {
            AsrEvent::Vad(vad) => {
                self.send(Msg::InputVad(InputVadMsg {
                    env: self.env.next(),
                    capture_id: vad.capture_id.clone(),
                    event: vad.event,
                }))
                .await;
                self.on_vad(vad).await;
            }
            AsrEvent::Segment(seg) => {
                self.send(Msg::InputAudioSegment(InputAudioSegmentMsg {
                    env: self.env.next(),
                    capture_id: seg.capture_id.clone(),
                    text: seg.text.clone(),
                    is_final: seg.is_final,
                }))
                .await;
                self.on_audio_segment(seg).await;
            }
                warn!(session = %self.id, code = ?err.code, msg = %err.message, "svc-asr error");
                let env = self.env.next();
            }
            AsrEvent::LinkDown(err) => {
                warn!(session = %self.id, %err, "asr link down, aborting captures");
                let captures: Vec<String> = self.captures.keys().cloned().collect();
                for capture_id in captures {
                    self.close_capture(
                        &capture_id,
                        Some(AppError::ModelUnreachable("asr link down".into())),
                    )
                    .await;
                }
            }
        }
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
                        self.cancel_upstreams(&id);
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
                error,
            } => {
                if self.live_turn(&turn_id).is_none() {
                    return;
                }
                // 合成失败/断链通知 UI（runtime.md：text-only badge）；
                // 已自行出站告警的降级（sink 溢出）不重复发帧。
                if let Some(err) = &error {
                    warn!(session = %self.id, %err, seq = ?sentence_seq, "output degraded");
                }
                let Some(turn) = self.turn.as_mut() else {
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
            PipelineEvent::VadDebounce { seq, capture_id } => {
                // seq 不符 = 已被新 speech_start/打断取代的 stale 确认。
                if self.vad_pending.as_ref().map(|p| p.seq) != Some(seq) {
                    return;
                }
                self.vad_pending = None;
                match self.state() {
                    // speech 持续满防抖时长：打断旧回合，新 capture 独立保留。
                    TurnState::Thinking | TurnState::Speaking => {
                        self.retire_active(Some(TurnEvent::Interrupt)).await;
                        self.adopt_capture(capture_id).await;
                    }
                    TurnState::Interrupted => {
                        self.retire_active(None).await;
                        self.adopt_capture(capture_id).await;
                    }
                    TurnState::Idle => self.adopt_capture(capture_id).await,
                    TurnState::Listening => {}
                }
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
        self.sink = Some(Arc::new(AudioSink::spawn(
            self.out_tx.clone(),
            turn.id.clone(),
            self.env.clone(),
        )));
        self.turn = Some(turn);
    }

    /// 新 reply 回合：原子 invalidate 旧回合 → thinking → 发管线。
    async fn accept_reply(&mut self, ref_id: String, user_text: String, lang: String) -> TurnId {
        match self.state() {
            TurnState::Thinking | TurnState::Speaking => {
                self.retire_active(Some(TurnEvent::NewInput)).await;
            }
            TurnState::Interrupted => self.retire_active(None).await,
            TurnState::Listening => {
                // 异 capture 的 voice final：旧 capture 按取消收尾（uplink 一并摘除）。
                let old_capture = self
                    .turn
                    .as_ref()
                    .and_then(|t| t.capture_id.clone())
                    .filter(|c| c != &ref_id);
                self.retire_active(Some(TurnEvent::CaptureCancel)).await;
                if let Some(capture_id) = old_capture {
                    self.close_capture(&capture_id, None).await;
                }
            }
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
        self.captures.entry(capture_id.clone()).or_default();
        let turn = Turn::for_capture(capture_id.clone());
        let turn_id = turn.id.clone();
        self.adopt(turn);
        self.accept_input(capture_id.clone(), turn_id.clone()).await;
        self.remember_input(capture_id, turn_id.clone());
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
                // 打断类迁移：cancel token 已扇出（turn::on_event），同步尽力取消上游服务。
                if matches!(
                    ev,
                    TurnEvent::Interrupt | TurnEvent::NewInput | TurnEvent::LlmFailed
                ) {
                    self.cancel_upstreams(&old.id);
                }
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
        self.mark_receipts(
            &turn.id,
            match outcome {
                TurnOutcome::Completed => ReceiptStatus::Completed,
                TurnOutcome::Interrupted => ReceiptStatus::Interrupted,
                TurnOutcome::Failed => ReceiptStatus::Failed,
            },
        );
        if let Some(o) = turn.finish(outcome) {
            self.send(Msg::TurnFinished(TurnFinishedMsg {
                env: self.env.next(),
                turn_id: turn.id.clone(),
                outcome: o,
            }))
            .await;
        }
    }

    /// VAD 防抖定时器：speech_start 持续 `MOMO_BARGEIN_MIN_MS` 后回传确认；
    /// 期间 speech_end/新 speech_start/打断会撤销或取代 pending（seq 防 stale）。
    fn arm_vad_debounce(&mut self, capture_id: String) {
        self.vad_seq += 1;
        let seq = self.vad_seq;
        self.vad_pending = Some(VadPending {
            seq,
            capture_id: capture_id.clone(),
        });
        let tx = self.cmd_tx.clone();
        let delay = self.barge_in_min;
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = tx
                .send(SessionCmd::Pipeline(PipelineEvent::VadDebounce {
                    seq,
                    capture_id,
                }))
                .await;
        });
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

    /// reply 管线：history 注入 prompt → LLM stream → 句切 → `Sentence` 事件；
    /// 配置了 TTS 时每句 spawn 合成任务，`AudioSink` 逐句下发 conn 下行
    /// （runtime.md「generation、TTS 与播放」）。
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
        let mut ctx = ReplyCtx {
            cmd_tx: self.cmd_tx.clone(),
            tts: self.tts.clone(),
            sink: self.sink.clone(),
            turn_id,
            lang: turn.lang.clone(),
            cancel: turn.cancel.child_token(),
            synth_gate: None,
        };
        tokio::spawn(async move {
            let mut stream = match llm.chat_stream(req, ctx.cancel.clone()).await {
                Ok(s) => s,
                Err(error) => {
                    ctx.failed(error).await;
                    return;
                }
            };
            let mut splitter = SentenceSplitter::new(soft, hard);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(delta) => {
                        for sentence in splitter.push(&delta) {
                            if !ctx.emit_sentence(sentence).await {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        ctx.failed(error).await;
                        return;
                    }
                }
            }
            if let Some(sentence) = splitter.flush() {
                let _ = ctx.emit_sentence(sentence).await;
            }
            let _ = ctx
                .cmd_tx
                .send(SessionCmd::Pipeline(PipelineEvent::GenerationDone {
                    turn_id: ctx.turn_id.clone(),
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

        let env = self.env.next();
            session_id: Some(env.session_id),
            event_id: Some(env.event_id),
            at_ms: Some(env.at_ms),
            event_seq: Some(env.event_seq),
            code: err.code(),
            message: err.to_string(),
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
            .find(|r| r.id == input_id)
            .map(|r| r.turn_id.clone())
    }

    fn remember_input(&mut self, input_id: String, turn_id: TurnId) {
        self.seen_inputs.push_back(InputReceipt {
            id: input_id,
            turn_id,
            status: ReceiptStatus::Accepted,
        });
        if self.seen_inputs.len() > RECEIPT_CACHE {
            self.seen_inputs.pop_front();
        }
    }
}

/// per-capture uplink task：`audio.open` 先行（restamp 链路 envelope）→
/// 二进制帧原样转发 → `audio.end` 收尾。发送端全部掉落（capture 中止/
/// session 退出）时尽力发 `control.cancel{target:capture}`；写失败上报
/// `LinkDown` 由 actor 收尾 capture。
async fn run_uplink(
    asr: Arc<dyn AsrClient>,
    open: AudioOpenMsg,
    mut rx: mpsc::Receiver<UplinkCmd>,
    cmd_tx: mpsc::Sender<SessionCmd>,
    capture_id: String,
) {
    async fn report(cmd_tx: &mpsc::Sender<SessionCmd>, err: AppError) {
        let _ = cmd_tx.send(SessionCmd::Asr(AsrEvent::LinkDown(err))).await;
    }
    if let Err(err) = asr.open_uplink(&open).await {
        report(&cmd_tx, err).await;
        return;
    }
    while let Some(cmd) = rx.recv().await {
        match cmd {
            UplinkCmd::Data(header, payload) => {
                if let Err(err) = asr.send_frame(header, payload).await {
                    report(&cmd_tx, err).await;
                    return;
                }
            }
            UplinkCmd::End { last_chunk_seq } => {
                let _ = asr.end_uplink(&open.stream_id, last_chunk_seq).await;
                return;
            }
        }
    }
    let _ = asr.cancel(CancelTarget::Capture, &capture_id).await;
}

/// reply 管线任务的共享上下文：LLM 流任务持有，逐句 clone 给合成任务。
struct ReplyCtx {
    cmd_tx: mpsc::Sender<SessionCmd>,
    tts: Option<Arc<dyn TtsClient>>,
    sink: Option<Arc<AudioSink>>,
    turn_id: TurnId,
    /// `output.sentence.lang`（跟随输入 lang）。
    lang: String,
    /// turn 取消令牌的子令牌；扇出到 LLM 流与全部合成任务。
    cancel: CancellationToken,
    /// 合成发起闸门：保持 svc-tts 收到的 `output.sentence` 序 = 句序
    /// （`synth_stream` 内 send_mu 只保证单句原子，句间顺序由本闸门传递）。
    synth_gate: Option<oneshot::Receiver<()>>,
}

impl ReplyCtx {
    /// LLM/管线失败回传。
    async fn failed(&self, error: AppError) {
        let _ = self
            .cmd_tx
            .send(SessionCmd::Pipeline(PipelineEvent::Failed {
                turn_id: self.turn_id.clone(),
                error,
            }))
            .await;
    }

    /// 句切成形句的统一出口：`Sentence` 事件先入队（`audio` 标记是否有 TTS
    /// 合成跟随），随后按序 spawn 合成任务。`Sentence` 一定先于该句的
    /// `Degraded` 到达 actor（同一发送者、先 send 后 spawn），pending audio
    /// 注册先于降级摘除，无乱序悬挂。
    async fn emit_sentence(&mut self, sentence: Sentence) -> bool {
        let audio = self.tts.is_some() && self.sink.is_some();
        if self
            .cmd_tx
            .send(SessionCmd::Pipeline(PipelineEvent::Sentence {
                turn_id: self.turn_id.clone(),
                sentence: sentence.clone(),
                audio,
            }))
            .await
            .is_err()
        {
            return false;
        }
        if let (Some(tts), Some(sink)) = (self.tts.clone(), self.sink.clone()) {
            let (gate_tx, gate_rx) = oneshot::channel();
            let synth = SynthCtx {
                tts,
                sink,
                cmd_tx: self.cmd_tx.clone(),
                turn_id: self.turn_id.clone(),
                lang: self.lang.clone(),
                cancel: self.cancel.clone(),
            };
            tokio::spawn(run_synth(
                synth,
                sentence,
                self.synth_gate.replace(gate_rx),
                gate_tx,
            ));
        }
        true
    }
}

/// 单句合成任务的可 clone 句柄集。
struct SynthCtx {
    tts: Arc<dyn TtsClient>,
    sink: Arc<AudioSink>,
    cmd_tx: mpsc::Sender<SessionCmd>,
    turn_id: TurnId,
    lang: String,
    cancel: CancellationToken,
}

/// 单句合成任务：等前一句 `output.sentence` 上线后发起 `synth_stream`（保持
/// wire 序 = 句序），PCM 帧经 `AudioSink::push` 下发 conn 下行。发起失败 /
/// 流内错误 / 下发失败上报 `Degraded{sentence_seq}`（该句转纯文字，不阻塞
/// 后续句）；turn 取消或 sink 停播后静默退出 —— 迟到帧不再下发 UI，也不向
/// 已收尾回合重复上报。
async fn run_synth(
    ctx: SynthCtx,
    sentence: Sentence,
    prev: Option<oneshot::Receiver<()>>,
    gate: oneshot::Sender<()>,
) {
    let seq = sentence.index;
    // 等前一句完成发起；闸门对端掉落视为已通过（前句提前退出）。
    if let Some(prev) = prev {
        let _ = prev.await;
    }
    if ctx.cancel.is_cancelled() {
        // gate 随 drop 释放，后续句同样快速短路。
        return;
    }
    let stream = ctx
        .tts
        .synth_stream(
            SynthRequest {
                turn_id: ctx.turn_id.clone(),
                lang: ctx.lang.clone(),
                sentence,
            },
            ctx.cancel.clone(),
        )
        .await;
    // 发起结果落定（成功 = output.sentence 已上线；失败 = 不再发起）才放行后句。
    let _ = gate.send(());
    let mut stream = match stream {
        Ok(s) => s,
        Err(error) => {
            ctx.degraded(seq, Some(error)).await;
            return;
        }
    };
    while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => {
                if ctx.cancel.is_cancelled() {
                    return;
                }
                if let Err(error) = ctx.sink.push(chunk) {
                    // 溢出已由 sink 自行发 error{BACKPRESSURE_OVERFLOW} +
                    // control.cancel{turn}；其余下发失败带 error 通知 UI。
                    let error =
                        (!matches!(error, AppError::BackpressureOverflow(_))).then_some(error);
                    ctx.degraded(seq, error).await;
                    return;
                }
            }
            Err(error) => {
                ctx.degraded(seq, Some(error)).await;
                return;
            }
        }
    }
}

impl SynthCtx {
    /// 合成失败降级回传：该句转纯文字，`error` 为 `Some` 时 actor 补发
    /// error 帧；turn 已取消则静默（stale 事件本就被丢弃）。
    async fn degraded(&self, sentence_seq: u32, error: Option<AppError>) {
        if self.cancel.is_cancelled() {
            return;
        }
        let _ = self
            .cmd_tx
            .send(SessionCmd::Pipeline(PipelineEvent::Degraded {
                turn_id: self.turn_id.clone(),
                sentence_seq: Some(sentence_seq),
                error,
            }))
            .await;
    }
}
