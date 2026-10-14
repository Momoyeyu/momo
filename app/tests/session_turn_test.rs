//! Session actor 回合接线：input/capture → turn 生命周期、`state.turn`/
//! `turn.finished` 帧、已播记账回灌下轮 LLM prompt。
//! 不经真 socket：`Session::spawn` + `cmd_tx` 投递，从 `out_tx` 断言出站序列。

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use futures_util::stream;
use momo_app::{
    error::AppError,
    persona::Persona,
    pipeline::{
        llm::{ChatRequest, ChatStream, LlmClient},
        splitter::Sentence,
    },
    proto::message::{
        BargeInMsg, BargeInReason, ClientKind, Envelope, ErrorCode, Msg, PlaybackProgressMsg,
        PlaybackStartedMsg, TurnOutcome, TurnState, new_id,
    },
    session::{
        ChatMessage, PipelineEvent, Role, Session, SessionCmd, SessionDeps, SessionHandle, TurnId,
    },
    ws::conn::OutboundFrame,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 手动喂 delta 的 mock LLM：记录每次请求的 messages，供断言 history 注入。
#[derive(Default)]
struct MockLlm {
    requests: Mutex<Vec<Vec<ChatMessage>>>,
    /// 每次 `chat_stream` 一个 feed；测试经它手动喂 delta / drop 收尾。
    feeds: Mutex<Vec<mpsc::Sender<Result<String, AppError>>>>,
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        _cancel: CancellationToken,
    ) -> Result<ChatStream, AppError> {
        self.requests.lock().expect("mock").push(req.messages);
        let (tx, rx) = mpsc::channel(8);
        self.feeds.lock().expect("mock").push(tx);
        Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })))
    }
}

type Feed = mpsc::Sender<Result<String, AppError>>;

async fn take_feed(llm: &MockLlm) -> Feed {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let feed = llm.feeds.lock().expect("mock").pop();
            if let Some(tx) = feed {
                return tx;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("llm request feed")
}

fn spawn_session(llm: Option<Arc<MockLlm>>) -> (SessionHandle, mpsc::Receiver<OutboundFrame>) {
    let (out_tx, out_rx) = mpsc::channel(64);
    let deps = SessionDeps {
        history_turns: 20,
        sent_soft_chars: 25,
        sent_max_chars: 60,
        llm: llm.map(|l| {
            let client: Arc<dyn LlmClient> = l;
            client
        }),
        asr: None,
        tts: None,
        barge_in_enabled: true,
        barge_in_min: Duration::from_millis(250),
        resume_ttl: Duration::from_secs(60),
        persona: Persona::new(std::path::PathBuf::from("persona/momo.yaml")),
        memory: None,
    };
    let handle = Session::spawn(
        "sess_test".into(),
        "conn_test".into(),
        ClientKind::Ui,
        out_tx,
        deps,
    );
    (handle, out_rx)
}

fn client_env() -> Envelope {
    Envelope {
        session_id: "sess_test".into(),
        event_id: new_id("e"),
        at_ms: 1,
        reply_to: None,
    }
}

/// ASR→orchestrator 方向消息（input.vad/input.audio.segment）带服务端 envelope。
fn server_env() -> momo_app::proto::message::ServerEnvelope {
    momo_app::proto::message::ServerEnvelope {
        session_id: "sess_test".into(),
        event_id: new_id("e"),
        at_ms: 1,
        reply_to: None,
        event_seq: 0,
    }
}

fn input_text(id: &str, text: &str) -> Msg {
    Msg::InputText(momo_app::proto::message::InputTextMsg {
        env: client_env(),
        id: id.into(),
        text: text.into(),
        lang: "zh".into(),
    })
}

fn barge_in(turn_id: &str) -> Msg {
    Msg::ControlBargeIn(BargeInMsg {
        env: client_env(),
        turn_id: turn_id.into(),
        reason: BargeInReason::User,
    })
}

async fn send(handle: &SessionHandle, msg: Msg) {
    handle
        .cmd_tx
        .send(SessionCmd::Msg(msg))
        .await
        .expect("cmd send");
}

async fn signal(handle: &SessionHandle, ev: PipelineEvent) {
    handle
        .cmd_tx
        .send(SessionCmd::Pipeline(ev))
        .await
        .expect("cmd send");
}

/// 收一帧服务端 JSON 消息（音频帧跳过）。
async fn recv(rx: &mut mpsc::Receiver<OutboundFrame>) -> Msg {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("server frame timeout")
            .expect("out_tx closed");
        if let OutboundFrame::Json(msg) = frame {
            return msg;
        }
    }
}

/// 收到满足 `pred` 的消息为止；返回沿途全部消息（含命中帧）。
async fn recv_until(
    rx: &mut mpsc::Receiver<OutboundFrame>,
    pred: impl Fn(&Msg) -> bool,
) -> Vec<Msg> {
    let mut seen = Vec::new();
    loop {
        let msg = recv(rx).await;
        let hit = pred(&msg);
        seen.push(msg);
        if hit {
            return seen;
        }
    }
}

/// 开回合并取回 `turn_id`（`input.accepted` + `state.turn{thinking}`）。
async fn open_text_turn(
    handle: &SessionHandle,
    rx: &mut mpsc::Receiver<OutboundFrame>,
    input_id: &str,
    text: &str,
) -> TurnId {
    send(handle, input_text(input_id, text)).await;
    let msgs = recv_until(rx, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Msg::InputAccepted(acc)) = msgs.last() else {
        panic!("expected input.accepted");
    };
    assert_eq!(acc.ref_id, input_id);
    let turn_id = acc.turn_id.clone();
    let msgs = recv_until(rx, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn");
    };
    assert_eq!(st.turn_id, turn_id);
    assert_eq!(st.state, TurnState::Thinking);
    turn_id
}

fn sentence(index: u32, text: &str) -> Sentence {
    Sentence {
        index,
        text: text.into(),
        emotion: None,
    }
}

#[tokio::test]
async fn text_input_accepts_and_enters_thinking() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));

    let turn_id = open_text_turn(&handle, &mut rx, "in_1", "你好").await;
    assert!(turn_id.starts_with("t_"));

    // 管线已 spawn：LLM 请求带出本轮 user 消息。
    let _feed = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[0].clone();
    assert_eq!(req.last().expect("prompt").content, "你好");
    assert_eq!(req.last().expect("prompt").role, Role::User);
}

#[tokio::test]
async fn empty_input_rejected() {
    let (handle, mut rx) = spawn_session(None);
    send(&handle, input_text("in_e", "   ")).await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::InputRejected(_))).await;
    let Some(Msg::InputRejected(rej)) = msgs.last() else {
        panic!("expected input.rejected");
    };
    assert_eq!(rej.ref_id, "in_e");
    assert_eq!(rej.reason, "empty_input");
}

#[tokio::test]
async fn duplicate_input_reacks_existing_turn() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let turn_id = open_text_turn(&handle, &mut rx, "in_1", "你好").await;

    // 重复 (session_id,id)：ACK 已有 turn，不新建。
    send(&handle, input_text("in_1", "你好")).await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Msg::InputAccepted(acc)) = msgs.last() else {
        panic!("expected input.accepted");
    };
    assert_eq!(acc.turn_id, turn_id);
    assert_eq!(llm.feeds.lock().expect("mock").len(), 1, "no second turn");
}

#[tokio::test]
async fn no_llm_fails_turn_with_finished() {
    let (handle, mut rx) = spawn_session(None);
    send(&handle, input_text("in_1", "你好")).await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;

    // error{MODEL_UNREACHABLE} 先行且归因到失败回合（#132），
    // turn.finished(failed) 恰好一次。
    let finished: Vec<_> = msgs
        .iter()
        .filter(|m| matches!(m, Msg::TurnFinished(_)))
        .collect();
    assert_eq!(finished.len(), 1);
    let Some(Msg::TurnFinished(f)) = msgs.last() else {
        panic!("expected turn.finished");
    };
    assert_eq!(f.outcome, TurnOutcome::Failed);
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::Error(e) if e.code == ErrorCode::ModelUnreachable
            && e.turn_id.as_deref() == Some(f.turn_id.as_str())
    )));
}

#[tokio::test]
async fn completed_turn_commits_into_next_prompt() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问一").await;

    // LLM 回复一句 → output.sentence → output.complete → turn.finished(completed)。
    let feed = take_feed(&llm).await;
    feed.send(Ok("答一。".into())).await.expect("delta");
    drop(feed);
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Msg::OutputSentence(s) if s.turn_id == t1 && s.text == "答一。"))
    );
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1 && f.outcome == TurnOutcome::Completed
    )));
    let finished = msgs
        .iter()
        .filter(|m| matches!(m, Msg::TurnFinished(_)))
        .count();
    assert_eq!(finished, 1, "turn.finished exactly once");

    // 下轮 input → LLM prompt 带上一轮完整记账（MOMO_HISTORY_TURNS）。
    let _t2 = open_text_turn(&handle, &mut rx, "in_2", "问二").await;
    let _feed2 = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[1].clone();
    assert_eq!(
        req.len(),
        4,
        "system + history(user+assistant) + current user"
    );
    assert_eq!(req[0].role, Role::System, "persona system prompt first");
    assert_eq!(req[1].content, "问一");
    assert_eq!(req[1].role, Role::User);
    assert_eq!(req[2].content, "答一。");
    assert_eq!(req[2].role, Role::Assistant);
    assert!(!req[2].partial);
    assert_eq!(req[3].content, "问二");
}

#[tokio::test]
async fn speaking_needs_playback_started_then_waits_acks() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问").await;

    // 句已下发且带音频 → 仍是 thinking（不由 output.sentence 推导 speaking）。
    signal(
        &handle,
        PipelineEvent::Sentence {
            turn_id: t1.clone(),
            sentence: sentence(1, "第一句。"),
            audio: true,
        },
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::OutputSentence(_))).await;
    assert!(msgs.iter().all(|m| !matches!(
        m,
        Msg::StateTurn(s) if s.state == TurnState::Speaking
    )));

    // playback.started → speaking。
    send(
        &handle,
        Msg::PlaybackStarted(PlaybackStartedMsg {
            env: client_env(),
            stream_id: new_id("st"),
            sentence_seq: 1,
        }),
    )
    .await;
    let msgs = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn speaking");
    };
    assert_eq!(st.turn_id, t1);

    // generation 结束但 required ACK 未齐 → 不收尾。
    signal(
        &handle,
        PipelineEvent::GenerationDone {
            turn_id: t1.clone(),
        },
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::OutputComplete(_))).await;
    assert!(msgs.iter().all(|m| !matches!(m, Msg::TurnFinished(_))));

    // completed ACK 齐 → finished(completed) + idle。
    send(
        &handle,
        Msg::PlaybackProgress(PlaybackProgressMsg {
            env: client_env(),
            stream_id: new_id("st"),
            sentence_seq: 1,
            played_samples: 1200,
            completed: true,
        }),
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Msg::TurnFinished(f) if f.outcome == TurnOutcome::Completed))
    );
}

#[tokio::test]
async fn barge_in_stops_audio_and_commits_played_prefix() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问").await;

    // 两句：seq1 开播（已播），seq2 已下发未播。
    signal(
        &handle,
        PipelineEvent::Sentence {
            turn_id: t1.clone(),
            sentence: sentence(1, "第一句。"),
            audio: true,
        },
    )
    .await;
    let _ = recv_until(&mut rx, |m| matches!(m, Msg::OutputSentence(_))).await;
    send(
        &handle,
        Msg::PlaybackStarted(PlaybackStartedMsg {
            env: client_env(),
            stream_id: new_id("st"),
            sentence_seq: 1,
        }),
    )
    .await;
    let _ = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;
    signal(
        &handle,
        PipelineEvent::Sentence {
            turn_id: t1.clone(),
            sentence: sentence(2, "第二句未播。"),
            audio: true,
        },
    )
    .await;
    let _ = recv_until(
        &mut rx,
        |m| matches!(m, Msg::OutputSentence(s) if s.seq == 2),
    )
    .await;

    // barge-in：停播指令（control.cancel{turn}）+ state.turn{interrupted}。
    send(&handle, barge_in(&t1)).await;
    let msgs = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Interrupted),
    )
    .await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::ControlCancel(c) if c.id == t1
    )));

    // settlement 结算：turn.finished(interrupted) 恰好一次。
    send(
        &handle,
        Msg::PlaybackProgress(PlaybackProgressMsg {
            env: client_env(),
            stream_id: new_id("st"),
            sentence_seq: 1,
            played_samples: 1200,
            completed: true,
        }),
    )
    .await;
    signal(
        &handle,
        PipelineEvent::Settled {
            turn_id: t1.clone(),
        },
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;
    let finished = msgs
        .iter()
        .filter(|m| matches!(m, Msg::TurnFinished(_)))
        .count();
    assert_eq!(finished, 1);
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1 && f.outcome == TurnOutcome::Interrupted
    )));

    // 下一轮 LLM prompt：只有已播 seq1 进入 history（partial），seq2 不出现。
    let _t2 = open_text_turn(&handle, &mut rx, "in_2", "问二").await;
    let _feed2 = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[1].clone();
    assert_eq!(req.len(), 4);
    assert_eq!(req[0].role, Role::System);
    assert_eq!(req[1].content, "问");
    assert_eq!(req[2].content, "第一句。");
    assert!(req[2].partial, "interrupted assistant msg is partial");
    assert!(!req.iter().any(|m| m.content.contains("第二句")));
    assert_eq!(req[3].content, "问二");
}

#[tokio::test]
async fn new_text_during_speaking_replaces_turn() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问一").await;
    signal(
        &handle,
        PipelineEvent::Sentence {
            turn_id: t1.clone(),
            sentence: sentence(1, "第一句。"),
            audio: true,
        },
    )
    .await;
    let _ = recv_until(&mut rx, |m| matches!(m, Msg::OutputSentence(_))).await;
    send(
        &handle,
        Msg::PlaybackStarted(PlaybackStartedMsg {
            env: client_env(),
            stream_id: new_id("st"),
            sentence_seq: 1,
        }),
    )
    .await;
    let _ = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;

    // speaking 中新 input.text：旧 turn interrupted 收尾，新 turn thinking。
    send(&handle, input_text("in_2", "问二")).await;
    let msgs = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::StateTurn(s) if s.turn_id == t1 && s.state == TurnState::Interrupted
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1 && f.outcome == TurnOutcome::Interrupted
    )));
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn thinking");
    };
    assert_ne!(st.turn_id, t1, "new turn id expected");
}

#[tokio::test]
async fn settlement_timer_finishes_interrupted_turn() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问").await;

    send(&handle, barge_in(&t1)).await;
    let _ = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Interrupted),
    )
    .await;

    // 不注入 Settled：500ms settlement deadline 自动收尾 → idle。
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1 && f.outcome == TurnOutcome::Interrupted
    )));
    assert!(
        msgs.iter().any(
            |m| matches!(m, Msg::StateTurn(s) if s.turn_id == t1 && s.state == TurnState::Idle)
        )
    );
}

#[tokio::test]
async fn stale_barge_in_is_rejected() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let _t1 = open_text_turn(&handle, &mut rx, "in_1", "问").await;
    // 未知 turn_id：不崩、不迁移，actor 仍活着。
    send(&handle, barge_in("t_stale")).await;
    send(&handle, input_text("in_2", "问二")).await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Msg::InputAccepted(acc)) = msgs.last() else {
        panic!("expected input.accepted");
    };
    assert!(acc.turn_id.starts_with("t_"));
}

#[tokio::test]
async fn interrupted_turn_late_llm_events_are_dropped() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));
    let t1 = open_text_turn(&handle, &mut rx, "in_1", "问").await;

    send(&handle, barge_in(&t1)).await;
    let _ = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Interrupted),
    )
    .await;

    // 已取消 turn 的迟到管线事件不可见（stale stream 防线）。
    signal(
        &handle,
        PipelineEvent::Sentence {
            turn_id: t1.clone(),
            sentence: sentence(1, "迟到句。"),
            audio: false,
        },
    )
    .await;
    signal(
        &handle,
        PipelineEvent::Settled {
            turn_id: t1.clone(),
        },
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(msgs.iter().all(|m| !matches!(m, Msg::OutputSentence(_))));
}

#[tokio::test]
async fn capture_turn_listens_then_final_segment_replies() {
    let llm = Arc::new(MockLlm::default());
    let (handle, mut rx) = spawn_session(Some(llm.clone()));

    // input.audio.start → listening。
    send(
        &handle,
        Msg::InputAudioStart(momo_app::proto::message::InputAudioStartMsg {
            env: client_env(),
            capture_id: "cap_1".into(),
            mode: momo_app::proto::message::CaptureMode::Ptt,
        }),
    )
    .await;
    let msgs = recv_until(&mut rx, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn");
    };
    assert_eq!(st.state, TurnState::Listening);
    let turn_id = st.turn_id.clone();

    // final segment → thinking（同体回合），voice 转写开 reply。
    send(
        &handle,
        Msg::InputAudioSegment(momo_app::proto::message::InputAudioSegmentMsg {
            env: server_env(),
            capture_id: "cap_1".into(),
            text: "语音问一".into(),
            is_final: true,
        }),
    )
    .await;
    let msgs = recv_until(
        &mut rx,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn thinking");
    };
    assert_eq!(st.turn_id, turn_id, "capture turn continues as reply turn");

    // 语音转写作为 user 进入 LLM prompt。
    let _feed = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[0].clone();
    assert_eq!(req.last().expect("prompt").content, "语音问一");
}
