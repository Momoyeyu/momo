//! conn actor 行为对拍：握手 happy path、异常→error 帧、有界 mpsc 背压不丢序。
//!
//! 不经真 socket：`conn::serve` 与 transport 解耦，测试注入 axum ws `Message`
//! 帧流/帧汇（momo-app.md §ws 子系统）。

use std::sync::Arc;

use axum::extract::ws::{Message, Utf8Bytes};
use bytes::Bytes;
use futures_channel::mpsc as fmpsc;
use futures_util::{SinkExt, StreamExt};
use momo_app::{
    config::Config,
    proto::{
        codec,
        message::{ErrorCode, Msg, ServerEnvelope, StateTurnMsg, TurnState},
    },
    ws::{
        conn::{self, CONN_TX_CAP, OutboundFrame},
        server::AppState,
    },
};
use serde_json::json;
use tokio::task::JoinHandle;

fn test_state() -> Arc<AppState> {
    let config = Config::load(|_| None).expect("default config");
    Arc::new(AppState::new(Arc::new(config)))
}

/// 测试侧连接：inbound 注入客户端帧，outbound 收服务端帧。
struct TestConn {
    inbound: fmpsc::Sender<Message>,
    outbound: fmpsc::Receiver<Message>,
    _task: JoinHandle<()>,
}

fn connect(state: Arc<AppState>, sink_cap: usize) -> TestConn {
    let (in_tx, in_rx) = fmpsc::channel::<Message>(8);
    let (out_tx, out_rx) = fmpsc::channel::<Message>(sink_cap);
    let stream = in_rx.map(Ok::<Message, axum::Error>);
    let task = tokio::spawn(conn::serve(stream, out_tx, state));
    TestConn {
        inbound: in_tx,
        outbound: out_rx,
        _task: task,
    }
}

fn hello_json() -> String {
    json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v2",
        "client": "ui",
        "auth_token": "ephemeral-token",
    })
    .to_string()
}

async fn send_text(conn: &mut TestConn, text: impl Into<String>) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(text.into())))
        .await
        .expect("inbound send");
}

/// 收一条服务端文本帧并解码为 `Msg`。
async fn recv_msg(conn: &mut TestConn) -> Msg {
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), conn.outbound.next())
        .await
        .expect("server frame timeout")
        .expect("connection closed");
    match msg {
        Message::Text(text) => codec::decode_msg(text.as_str()).expect("server frame is valid Msg"),
        other => panic!("expected text frame, got {other:?}"),
    }
}

async fn handshake(conn: &mut TestConn) {
    send_text(conn, hello_json()).await;
    let Msg::HelloOk(ok) = recv_msg(conn).await else {
        panic!("expected hello.ok");
    };
    assert!(ok.session_id.starts_with("sess_"));
}

#[tokio::test]
async fn hello_returns_hello_ok() {
    let mut conn = connect(test_state(), 8);
    send_text(&mut conn, hello_json()).await;
    let Msg::HelloOk(ok) = recv_msg(&mut conn).await else {
        panic!("expected hello.ok");
    };
    assert_eq!(ok.version, "v2");
    assert!(ok.session_id.starts_with("sess_"));
}

#[tokio::test]
async fn malformed_json_yields_error_frame() {
    let mut conn = connect(test_state(), 8);
    handshake(&mut conn).await;

    send_text(&mut conn, "{not json").await;
    let Msg::Error(err) = recv_msg(&mut conn).await else {
        panic!("expected error frame");
    };
    assert_eq!(err.code, ErrorCode::BadMessage);
    // 握手后 conn 校验失败带 session_id 定位会话（#132）。
    assert!(err.session_id.is_some());
}

#[tokio::test]
async fn unknown_type_yields_error_frame() {
    let mut conn = connect(test_state(), 8);
    handshake(&mut conn).await;

    send_text(&mut conn, json!({"type": "wat"}).to_string()).await;
    let Msg::Error(err) = recv_msg(&mut conn).await else {
        panic!("expected error frame");
    };
    assert_eq!(err.code, ErrorCode::BadMessage);
}

#[tokio::test]
async fn bad_audio_frame_yields_error_frame() {
    let mut conn = connect(test_state(), 8);
    handshake(&mut conn).await;

    // 不足 32B 的二进制帧 → BAD_MESSAGE；连接保持。
    conn.inbound
        .send(Message::Binary(Bytes::from_static(&[0u8; 10])))
        .await
        .unwrap();
    let Msg::Error(err) = recv_msg(&mut conn).await else {
        panic!("expected error frame");
    };
    assert_eq!(err.code, ErrorCode::BadMessage);

    // 连接仍存活：再发一个坏帧还能收到 error。
    conn.inbound
        .send(Message::Binary(Bytes::from_static(&[0u8; 10])))
        .await
        .unwrap();
    let Msg::Error(err) = recv_msg(&mut conn).await else {
        panic!("expected error frame");
    };
    assert_eq!(err.code, ErrorCode::BadMessage);
}

/// 背压不丢序：向 conn_tx 推入超过队列容量的帧，sink 慢消费，
/// 全部帧仍按入队顺序到达，无一丢弃。
#[tokio::test]
async fn backpressure_preserves_order() {
    let state = test_state();
    let mut conn = connect(state.clone(), 1);
    handshake(&mut conn).await;

    let session = state.sessions.active().expect("session attached");
    const N: u64 = (CONN_TX_CAP + 64) as u64;

    // conn_tx 容量 256：推 320 帧必然在 queue 满时阻塞生产者 = 背压生效。
    let out_tx = session.out_tx.clone();
    let producer = tokio::spawn(async move {
        for i in 1..=N {
            let frame = OutboundFrame::Json(Msg::StateTurn(StateTurnMsg {
                env: ServerEnvelope {
                    session_id: "sess_test".into(),
                    event_id: format!("{i:026}"),
                    at_ms: i,
                    reply_to: None,
                    event_seq: i,
                },
                turn_id: "t_test".into(),
                state: TurnState::Idle,
            }));
            out_tx.send(frame).await.expect("outbound send");
        }
    });

    for i in 1..=N {
        let Msg::StateTurn(m) = recv_msg(&mut conn).await else {
            panic!("expected state.turn at seq {i}");
        };
        assert_eq!(m.env.event_seq, i, "frame order violated");
    }
    producer.await.expect("producer task");
}
