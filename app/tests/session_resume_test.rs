//! session resume e2e（#142）：WS 断开后 session actor 进
//! `MOMO_SESSION_RESUME_TTL_MS` 保活窗口；`hello.resume_session_id` 命中 →
//! `hello.ok{同 session_id}` + `session.snapshot` 收敛 turn/receipts/history；
//! 未知或过期 id → fallback 新 session。单活跃语义保留：resume 同样顶掉旧 conn。
//!
//! 基建同 asr_uplink_test：真 `conn::serve` + mock LLM，不经真 socket。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes};
use futures_channel::mpsc as fmpsc;
use futures_util::{SinkExt, StreamExt};
use momo_app::{
    config::Config,
    error::AppError,
    persona::Persona,
    pipeline::llm::{ChatRequest, ChatStream, LlmClient},
    proto::{
        codec::{self, CLOSE_NORMAL},
        message::{Msg, ReceiptStatus, TurnState, new_id},
    },
    session::{ChatMessage, Role, SessionDeps},
    ws::{conn, server::AppState},
};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

// ---------- mock LLM / 测试侧 session（同 session_turn_test 模式） ----------

/// 手动喂 delta 的 mock LLM：记录每次请求的 messages，供断言 history 注入。
#[derive(Default)]
struct MockLlm {
    requests: Mutex<Vec<Vec<ChatMessage>>>,
    feeds: Mutex<Vec<tokio::sync::mpsc::Sender<Result<String, AppError>>>>,
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        _cancel: CancellationToken,
    ) -> Result<ChatStream, AppError> {
        self.requests.lock().expect("mock").push(req.messages);
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        self.feeds.lock().expect("mock").push(tx);
        Ok(Box::pin(futures_util::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|item| (item, rx)) },
        )))
    }
}

async fn take_feed(llm: &MockLlm) -> tokio::sync::mpsc::Sender<Result<String, AppError>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(tx) = llm.feeds.lock().expect("mock").pop() {
                return tx;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("llm request feed")
}

fn test_state(llm: Option<Arc<MockLlm>>, resume_ttl: Duration) -> Arc<AppState> {
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
        resume_ttl,
        persona: Persona::new(std::path::PathBuf::from("persona/momo.yaml")),
    };
    Arc::new(
        AppState::from_deps(
            Arc::new(Config::load(|_| None).expect("default config")),
            deps,
        )
        .expect("app state"),
    )
}

// ---------- 测试侧 UI 连接 ----------

struct TestConn {
    inbound: fmpsc::Sender<Message>,
    outbound: fmpsc::Receiver<Message>,
    task: JoinHandle<()>,
}

fn connect(state: Arc<AppState>) -> TestConn {
    let (in_tx, in_rx) = fmpsc::channel::<Message>(64);
    let (out_tx, out_rx) = fmpsc::channel::<Message>(64);
    let stream = in_rx.map(Ok::<Message, axum::Error>);
    let task = tokio::spawn(conn::serve(stream, out_tx, state));
    TestConn {
        inbound: in_tx,
        outbound: out_rx,
        task,
    }
}

fn hello_json(resume_session_id: Option<&str>) -> Value {
    let mut v = json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v2",
        "client": "ui",
        "auth_token": "ephemeral-token",
    });
    if let Some(id) = resume_session_id {
        v["resume_session_id"] = json!(id);
    }
    v
}

fn input_text(id: &str, text: &str) -> Value {
    json!({
        "type": "input.text",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "id": id,
        "text": text,
        "lang": "zh",
    })
}

async fn send_text(conn: &mut TestConn, v: Value) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(v.to_string())))
        .await
        .expect("inbound send");
}

/// 收一帧（含 Close）；超时/断流 panic。
async fn recv_frame(conn: &mut TestConn) -> Message {
    tokio::time::timeout(Duration::from_secs(5), conn.outbound.next())
        .await
        .expect("server frame timeout")
        .expect("connection closed")
}

async fn recv_msg(conn: &mut TestConn) -> Msg {
    match recv_frame(conn).await {
        Message::Text(text) => codec::decode_msg(text.as_str()).expect("server frame is valid Msg"),
        other => panic!("expected text frame, got {other:?}"),
    }
}

async fn recv_until(conn: &mut TestConn, pred: impl Fn(&Msg) -> bool) -> Vec<Msg> {
    let mut seen = Vec::new();
    loop {
        let msg = recv_msg(conn).await;
        let hit = pred(&msg);
        seen.push(msg);
        if hit {
            return seen;
        }
    }
}

/// 窗口期内排空 outbound（"不应出现某帧"断言用）。
async fn drain_for(conn: &mut TestConn, window: Duration) -> Vec<Msg> {
    let deadline = Instant::now() + window;
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return seen;
        }
        match tokio::time::timeout(left, conn.outbound.next()).await {
            Ok(Some(Message::Text(t))) => {
                seen.push(codec::decode_msg(t.as_str()).expect("valid Msg"));
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return seen,
        }
    }
}

fn expect_close(frame: Message, code: u16) {
    let Message::Close(Some(CloseFrame {
        code: got, reason, ..
    })) = frame
    else {
        panic!("expected close frame, got {frame:?}");
    };
    assert_eq!(got, code, "close reason: {reason}");
}

/// 握手并返回 hello.ok 的 session_id。
async fn handshake(conn: &mut TestConn, resume_session_id: Option<&str>) -> String {
    send_text(conn, hello_json(resume_session_id)).await;
    let Msg::HelloOk(ok) = recv_msg(conn).await else {
        panic!("expected hello.ok");
    };
    ok.session_id
}

/// 模拟断线并等 conn 任务退出（保证 ConnClosed 已进 session 命令队列）。
async fn disconnect(conn: TestConn) {
    drop(conn.inbound);
    conn.task.await.expect("conn task");
}

// ---------- 测试 ----------

/// 断线→重连带 resume_session_id：同 session_id + snapshot 收敛
/// （turn=null、receipt=completed、last_event_seq 水位），history 保留至下轮 prompt。
#[tokio::test]
async fn resume_recovers_session_and_sends_snapshot() {
    let llm = Arc::new(MockLlm::default());
    let state = test_state(Some(llm.clone()), Duration::from_secs(60));

    // conn A：完成一个文字回合后断开。
    let mut a = connect(state.clone());
    let session_id = handshake(&mut a, None).await;

    send_text(&mut a, input_text("in_1", "你好")).await;
    let msgs = recv_until(
        &mut a,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    let turn_id = msgs
        .iter()
        .find_map(|m| match m {
            Msg::InputAccepted(acc) => Some(acc.turn_id.clone()),
            _ => None,
        })
        .expect("input.accepted");
    let feed = take_feed(&llm).await;
    feed.send(Ok("第一句。".into())).await.expect("delta");
    drop(feed); // stream 收尾 → output.complete → turn.finished(completed)
    let _ = recv_until(&mut a, |m| matches!(m, Msg::TurnFinished(_))).await;

    disconnect(a).await;

    // conn B：resume 命中 → 同 session_id + session.snapshot。
    let mut b = connect(state.clone());
    let resumed = handshake(&mut b, Some(&session_id)).await;
    assert_eq!(resumed, session_id, "resume keeps session id");
    let Msg::SessionSnapshot(snap) = recv_msg(&mut b).await else {
        panic!("expected session.snapshot");
    };
    assert_eq!(snap.session_id, session_id);
    assert_eq!(snap.turn, None, "finished turn converges to null");
    assert!(
        snap.receipts.iter().any(|r| r.id == "in_1"
            && r.turn_id == turn_id
            && r.status == ReceiptStatus::Completed),
        "receipt cache keeps completed status: {:?}",
        snap.receipts
    );
    assert!(snap.capabilities.iter().any(|c| c == "barge_in"));
    assert!(snap.last_event_seq > 0, "event_seq watermark reported");

    // history 保留：新输入的 LLM prompt 含断线前已记账回合。
    send_text(&mut b, input_text("in_2", "再问")).await;
    let _ = recv_until(&mut b, |m| matches!(m, Msg::InputAccepted(_))).await;
    let _feed2 = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[1].clone();
    assert_eq!(req.len(), 4);
    assert_eq!(req[0].role, Role::System);
    assert!(req[0].content.contains("momo"), "system prompt = persona");
    assert_eq!(req[1].role, Role::User);
    assert_eq!(req[1].content, "你好");
    assert_eq!(req[2].role, Role::Assistant);
    assert_eq!(req[2].content, "第一句。");
    assert_eq!(req[3].role, Role::User);
    assert_eq!(req[3].content, "再问");
}

/// 回合在途时断线：turn 取消、receipt 标 interrupted、已播记账保留；
/// resume 后 snapshot.turn=null，history 含断线前 user 输入。
#[tokio::test]
async fn resume_after_mid_turn_disconnect_reports_interrupted_receipt() {
    let llm = Arc::new(MockLlm::default());
    let state = test_state(Some(llm.clone()), Duration::from_secs(60));

    let mut a = connect(state.clone());
    let session_id = handshake(&mut a, None).await;
    send_text(&mut a, input_text("in_1", "问")).await;
    let msgs = recv_until(
        &mut a,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    let turn_id = msgs
        .iter()
        .find_map(|m| match m {
            Msg::InputAccepted(acc) => Some(acc.turn_id.clone()),
            _ => None,
        })
        .expect("input.accepted");
    let _feed = take_feed(&llm).await; // generation 在途不断流

    disconnect(a).await;

    let mut b = connect(state.clone());
    let resumed = handshake(&mut b, Some(&session_id)).await;
    assert_eq!(resumed, session_id);
    let Msg::SessionSnapshot(snap) = recv_msg(&mut b).await else {
        panic!("expected session.snapshot");
    };
    assert_eq!(snap.turn, None, "interrupted turn converges to null");
    assert!(
        snap.receipts.iter().any(|r| r.id == "in_1"
            && r.turn_id == turn_id
            && r.status == ReceiptStatus::Interrupted),
        "in-flight receipt marked interrupted: {:?}",
        snap.receipts
    );

    // history 保留断线前 user 输入（无已播句 → 无 assistant 记账）。
    send_text(&mut b, input_text("in_2", "再问")).await;
    let _ = recv_until(&mut b, |m| matches!(m, Msg::InputAccepted(_))).await;
    let _feed2 = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[1].clone();
    assert_eq!(req.len(), 3);
    assert_eq!(req[0].role, Role::System);
    assert_eq!(req[1].content, "问");
    assert_eq!(req[2].content, "再问");
}

/// 单活跃语义：旧 conn 仍连着时新 conn 带 resume_session_id →
/// 旧 conn 收 close 1000，新 conn 拿同 session_id + snapshot。
#[tokio::test]
async fn resume_supersedes_live_conn() {
    let state = test_state(None, Duration::from_secs(60));
    let mut a = connect(state.clone());
    let session_id = handshake(&mut a, None).await;

    let mut b = connect(state);
    let resumed = handshake(&mut b, Some(&session_id)).await;
    assert_eq!(resumed, session_id);
    let Msg::SessionSnapshot(_) = recv_msg(&mut b).await else {
        panic!("expected session.snapshot");
    };

    expect_close(recv_frame(&mut a).await, CLOSE_NORMAL);
}

/// 未知 resume id → fallback 新 session（新 session_id、无 snapshot），
/// 且按单活跃规则顶掉旧 conn。
#[tokio::test]
async fn resume_unknown_session_falls_back_to_new_session() {
    let state = test_state(None, Duration::from_secs(60));
    let mut a = connect(state.clone());
    let old_id = handshake(&mut a, None).await;

    let mut b = connect(state);
    let new_id = handshake(&mut b, Some("sess_nonexistent")).await;
    assert_ne!(new_id, "sess_nonexistent");
    assert_ne!(new_id, old_id);
    assert!(new_id.starts_with("sess_"));

    let seen = drain_for(&mut b, Duration::from_millis(150)).await;
    assert!(
        seen.iter().all(|m| !matches!(m, Msg::SessionSnapshot(_))),
        "fallback must not emit snapshot: {seen:?}"
    );

    expect_close(recv_frame(&mut a).await, CLOSE_NORMAL);
}

/// 保活窗口过期：TTL 后 resume 同一 id → actor 已回收 → fallback 新 session。
#[tokio::test]
async fn resume_expired_session_falls_back_to_new_session() {
    let state = test_state(None, Duration::from_millis(40));
    let mut a = connect(state.clone());
    let session_id = handshake(&mut a, None).await;
    disconnect(a).await;

    tokio::time::sleep(Duration::from_millis(150)).await; // TTL(40ms) 已过

    let mut b = connect(state);
    let new_id = handshake(&mut b, Some(&session_id)).await;
    assert_ne!(new_id, session_id, "expired session must not resume");
    assert!(new_id.starts_with("sess_"));

    let seen = drain_for(&mut b, Duration::from_millis(150)).await;
    assert!(
        seen.iter().all(|m| !matches!(m, Msg::SessionSnapshot(_))),
        "expired fallback must not emit snapshot: {seen:?}"
    );
}
