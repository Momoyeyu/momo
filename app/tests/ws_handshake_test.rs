//! `/ws/ui` 握手对拍：Origin 白名单、hello 校验（4400/4401/4001）、
//! 5s deadline、单活跃 UI 连接顶掉前连（protocol §认证）。

use std::sync::Arc;

use axum::{
    extract::ws::{CloseFrame, Message, Utf8Bytes},
    http::{HeaderMap, HeaderValue, header},
};
use futures_channel::mpsc as fmpsc;
use futures_util::{SinkExt, StreamExt};
use momo_app::{
    config::Config,
    proto::{
        codec::{
            self, CLOSE_NORMAL, CLOSE_PROTOCOL_VIOLATION, CLOSE_UNAUTHORIZED,
            CLOSE_VERSION_MISMATCH,
        },
        message::{ErrorCode, Msg},
    },
    ws::{conn, server},
};
use serde_json::json;
use tokio::task::JoinHandle;

fn test_state(env: &[(&str, &str)]) -> Arc<server::AppState> {
    let config = Config::load(|key| {
        env.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
    })
    .expect("test config");
    Arc::new(server::AppState::new(Arc::new(config)).expect("app state"))
}

struct TestConn {
    inbound: fmpsc::Sender<Message>,
    outbound: fmpsc::Receiver<Message>,
    _task: JoinHandle<()>,
}

fn connect(state: Arc<server::AppState>) -> TestConn {
    let (in_tx, in_rx) = fmpsc::channel::<Message>(8);
    let (out_tx, out_rx) = fmpsc::channel::<Message>(8);
    let stream = in_rx.map(Ok::<Message, axum::Error>);
    let task = tokio::spawn(conn::serve(stream, out_tx, state));
    TestConn {
        inbound: in_tx,
        outbound: out_rx,
        _task: task,
    }
}

fn hello(overrides: serde_json::Value) -> String {
    let mut v = json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v2",
        "client": "ui",
        "auth_token": "ephemeral-token",
    });
    if let (Some(base), Some(patch)) = (v.as_object_mut(), overrides.as_object()) {
        for (k, val) in patch {
            base.insert(k.clone(), val.clone());
        }
    }
    v.to_string()
}

async fn send_text(conn: &mut TestConn, text: impl Into<String>) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(text.into())))
        .await
        .expect("inbound send");
}

async fn recv_frame(conn: &mut TestConn) -> Message {
    tokio::time::timeout(std::time::Duration::from_secs(30), conn.outbound.next())
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

fn expect_close(frame: Message, code: u16) {
    let Message::Close(Some(CloseFrame {
        code: got, reason, ..
    })) = frame
    else {
        panic!("expected close frame, got {frame:?}");
    };
    assert_eq!(got, code, "close reason: {reason}");
}

/// 断言握手被拒：`error` 帧 + close(code)。
async fn expect_fatal(conn: &mut TestConn, code: ErrorCode, close: u16) {
    let Msg::Error(err) = recv_msg(conn).await else {
        panic!("expected error frame");
    };
    assert_eq!(err.code, code);
    expect_close(recv_frame(conn).await, close);
}

#[tokio::test]
async fn first_frame_not_hello_closes_4400() {
    let mut conn = connect(test_state(&[]));
    // 字段合法的 input.text：decode 成功但不是 hello → "first message must be hello"。
    send_text(
        &mut conn,
        json!({
            "type": "input.text",
            "session_id": "sess_01J5M0N8T1RZ8K2W4V7H6X9P3A",
            "event_id": "01J5M0N8T1RZ8K2W4V7H6X9P3B",
            "at_ms": 1730000000000_u64,
            "id": "in_01J5M0N8T1RZ8K2W4V7H6X9P3C",
            "text": "hi",
            "lang": "zh",
        })
        .to_string(),
    )
    .await;
    expect_fatal(&mut conn, ErrorCode::Internal, CLOSE_PROTOCOL_VIOLATION).await;
}

#[tokio::test]
async fn malformed_hello_closes_4400() {
    let mut conn = connect(test_state(&[]));
    send_text(&mut conn, "{not json").await;
    expect_fatal(&mut conn, ErrorCode::BadMessage, CLOSE_PROTOCOL_VIOLATION).await;
}

#[tokio::test]
async fn wrong_protocol_name_closes_4400() {
    let mut conn = connect(test_state(&[]));
    send_text(&mut conn, hello(json!({"protocol": "other-ws"}))).await;
    expect_fatal(&mut conn, ErrorCode::Internal, CLOSE_PROTOCOL_VIOLATION).await;
}

#[tokio::test]
async fn version_mismatch_closes_4001() {
    let mut conn = connect(test_state(&[]));
    send_text(&mut conn, hello(json!({"version": "v1"}))).await;
    expect_fatal(
        &mut conn,
        ErrorCode::VersionMismatch,
        CLOSE_VERSION_MISMATCH,
    )
    .await;
}

#[tokio::test]
async fn non_ui_client_closes_4400() {
    let mut conn = connect(test_state(&[]));
    send_text(&mut conn, hello(json!({"client": "orchestrator"}))).await;
    expect_fatal(&mut conn, ErrorCode::Internal, CLOSE_PROTOCOL_VIOLATION).await;
}

#[tokio::test]
async fn bad_token_closes_4401() {
    let state = test_state(&[("MOMO_UI_TOKEN", "s3cret")]);
    let mut conn = connect(state);
    send_text(&mut conn, hello(json!({"auth_token": "wrong"}))).await;
    expect_fatal(&mut conn, ErrorCode::Unauthorized, CLOSE_UNAUTHORIZED).await;
}

#[tokio::test]
async fn good_token_returns_hello_ok() {
    let state = test_state(&[("MOMO_UI_TOKEN", "s3cret")]);
    let mut conn = connect(state);
    send_text(&mut conn, hello(json!({"auth_token": "s3cret"}))).await;
    let Msg::HelloOk(_) = recv_msg(&mut conn).await else {
        panic!("expected hello.ok");
    };
}

/// 5s 内收不到合法 hello → error + close 4400（暂停时钟自动推进到 deadline）。
#[tokio::test(start_paused = true)]
async fn hello_timeout_closes_4400() {
    let mut conn = connect(test_state(&[]));
    // 不发任何帧；tokio 暂停时钟下 serve 的 HELLO_TIMEOUT 自动触发。
    expect_fatal(&mut conn, ErrorCode::Internal, CLOSE_PROTOCOL_VIOLATION).await;
}

/// 单活跃 UI 连接：第二个连接握手成功 → 第一个连接收 close 1000 被顶掉。
#[tokio::test]
async fn second_ui_conn_supersedes_first() {
    let state = test_state(&[]);
    let mut a = connect(state.clone());
    let mut b = connect(state);

    send_text(&mut a, hello(json!({}))).await;
    assert!(matches!(recv_msg(&mut a).await, Msg::HelloOk(_)));

    send_text(&mut b, hello(json!({}))).await;
    assert!(matches!(recv_msg(&mut b).await, Msg::HelloOk(_)));

    expect_close(recv_frame(&mut a).await, CLOSE_NORMAL);
}

// ---------- /ws/ui upgrade 前 Origin 白名单 ----------

/// `origin_allowed`（server.rs）直接对拍白名单：missing / 未批准 → false。
/// （真 upgrade 断言由 axum extractor 链承担，oneshot 无法构造 OnUpgrade 扩展。）
#[test]
fn origin_allowlist() {
    let config = Config::load(|_| None).expect("default config");
    let mut h = HeaderMap::new();

    // missing Origin 拒绝。
    assert!(!server::origin_allowed(&h, &config));

    h.insert(
        header::ORIGIN,
        HeaderValue::from_static("https://evil.example"),
    );
    assert!(!server::origin_allowed(&h, &config));

    // MOMO_WEB_ORIGIN 默认 http://localhost:5173（browser-dev）。
    h.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:5173"),
    );
    assert!(server::origin_allowed(&h, &config));

    // desktop origin 三种形态。
    for o in [
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    ] {
        h.insert(header::ORIGIN, HeaderValue::from_static(o));
        assert!(server::origin_allowed(&h, &config), "origin {o}");
    }

    // momod 自托管 UI（MOMO_UI_DIR）时浏览器源 = http://{orch_listen}（默认 7800）；
    // loopback 监听下 localhost/[::1] 字面量等价放行。
    for o in [
        "http://127.0.0.1:7800",
        "http://localhost:7800",
        "http://[::1]:7800",
    ] {
        h.insert(header::ORIGIN, HeaderValue::from_static(o));
        assert!(server::origin_allowed(&h, &config), "origin {o}");
    }
    // 其他端口不放行。
    h.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:7801"),
    );
    assert!(!server::origin_allowed(&h, &config));

    // 自定义 MOMO_WEB_ORIGIN 生效，旧默认不再放行。
    let config =
        Config::load(|key| (key == "MOMO_WEB_ORIGIN").then(|| "http://127.0.0.1:5173".to_string()))
            .unwrap();
    h.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://127.0.0.1:5173"),
    );
    assert!(server::origin_allowed(&h, &config));
    h.insert(
        header::ORIGIN,
        HeaderValue::from_static("http://localhost:5173"),
    );
    assert!(!server::origin_allowed(&h, &config));
}
