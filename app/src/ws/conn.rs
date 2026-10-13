//! 单连接 actor：rx/tx task、帧编解码、背压（拓扑见 momo-app.md §ws 子系统）。
//!
//! `conn_tx` 有界 mpsc 保序不丢帧；JSON 控制帧永不丢弃，对端消费不动
//! （stall 超 [`CONN_TX_STALL`]）则关连（protocol §10）。

use std::{fmt::Display, sync::Arc, time::Duration};

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket};
use bytes::Bytes;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    error::AppError,
    proto::{
        codec::{
            self, CLOSE_INTERNAL, CLOSE_NORMAL, CLOSE_PROTOCOL_VIOLATION, CLOSE_UNAUTHORIZED,
            CLOSE_VERSION_MISMATCH,
        },
        message::{ClientKind, ErrorMsg, HelloOkMsg, Msg, ServerRole, new_id},
    },
    session::{SessionCmd, SessionHandle},
    ws::server::AppState,
};

/// conn_tx 有界队列上限（momo-app.md §ws 子系统）。
pub const CONN_TX_CAP: usize = 256;

/// hello deadline：upgrade 后 5s 内必须收到合法 hello（protocol §认证）。
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// conn_tx 单帧写 socket 的 stall deadline：超时说明对端消费不动，直接关连。
pub const CONN_TX_STALL: Duration = Duration::from_secs(5);

/// conn_tx 的出站单元：JSON 文本帧 / 二进制音频帧 / Pong / 关闭帧。
#[derive(Debug)]
pub enum OutboundFrame {
    Json(Msg),
    /// 已序列化的 JSON 文本帧（客户端方向消息等非 `Msg` 类型，如模型链路）。
    JsonText(String),
    /// `audio.chunk`：头 + PCM 载荷。
    Audio(codec::AudioHeader, Bytes),
    /// 传输层 keepalive 应答（ws::client 收到对端 Ping 时回写）。
    Pong(Bytes),
    Close(u16, &'static str),
}

/// 一条 `/ws/ui` 连接的完整生命周期。
pub async fn run(socket: WebSocket, state: Arc<AppState>) {
    let (sink, stream) = socket.split();
    serve(stream, sink, state).await;
}

/// 连接驱动本体，与 transport 解耦：`stream`/`sink` 可以是真 socket，
/// 也可以是测试注入的帧通道（`Message` 为 axum ws 帧类型）。
pub async fn serve<St, Si, StErr, SiErr>(mut stream: St, sink: Si, state: Arc<AppState>)
where
    St: Stream<Item = Result<Message, StErr>> + Unpin + Send + 'static,
    Si: Sink<Message, Error = SiErr> + Unpin + Send + 'static,
    StErr: Display,
    SiErr: Display + Send + 'static,
{
    let conn_id = new_id("conn");
    debug!(%conn_id, "ws connection opened");

    let (tx, rx) = mpsc::channel::<OutboundFrame>(CONN_TX_CAP);
    let tx_task = tokio::spawn(tx_loop(sink, rx));

    // 首帧必须是 hello（protocol §4）；握手失败也要把 error/close 帧排空再退出。
    let Some(session) = handshake(&conn_id, &mut stream, &tx, &state).await else {
        drop(tx);
        let _ = tx_task.await;
        return;
    };

    // conn_rx 循环：帧 → Msg → Session。
    let cmd_tx = session.cmd_tx.clone();
    loop {
        let msg = tokio::select! {
            next = stream.next() => next,
                    let _ = tx
                        .send(OutboundFrame::Close(
                            CLOSE_NORMAL,
                            "superseded by new ui connection",
                        ))
                        .await;
                    break;
                }
                continue;
            }
        };
        let Some(msg) = msg else { break };
        match msg {
            Ok(Message::Text(text)) => match codec::decode_msg(text.as_str()) {
                Ok(Msg::Unknown) => {
                    let _ = tx
                        .await;
                }
                Ok(client_msg) => {
                    if cmd_tx.send(SessionCmd::Msg(client_msg)).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                }
            },
            Ok(Message::Binary(bytes)) => {
                match codec::AudioHeader::decode(&bytes) {
                    }
                    Err(err) => {
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            // Ping/Pong 由 axum 自动应答。
            Ok(_) => {}
        }
    }

    let _ = tx_task.await;
    debug!(%conn_id, "ws connection closed");
}

/// 握手：首消息必须是 `hello{protocol:"momo-ws", version:"v2"}`，
/// 不匹配回 `error` + 按 close code 表断连。
async fn handshake<St, StErr>(
    conn_id: &str,
    stream: &mut St,
    tx: &mpsc::Sender<OutboundFrame>,
    state: &Arc<AppState>,
) -> Option<SessionHandle>
where
    St: Stream<Item = Result<Message, StErr>> + Unpin,
    StErr: Display,
{
    // upgrade 后 5s 内必须收到 hello（protocol §认证），超时 close 4400。
    let first = match tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(Message::Text(text)))) => text,
        // 静默断连（如对端直接关闭）不视为协议违反。
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return None,
        Ok(_) => {
            send_fatal(
                tx,
                &AppError::Protocol("first frame must be a text hello".into()),
                CLOSE_PROTOCOL_VIOLATION,
            )
            .await;
            return None;
        }
        Err(_) => {
            send_fatal(
                tx,
                &AppError::Protocol("hello timeout".into()),
                CLOSE_PROTOCOL_VIOLATION,
            )
            .await;
            return None;
        }
    };

    let hello = match codec::decode_msg(first.as_str()) {
        Ok(Msg::Hello(hello)) => hello,
        other => {
            let err = match other {
                Err(e) => e,
                Ok(_) => AppError::Protocol("first message must be hello".into()),
            };
            send_fatal(tx, &err, CLOSE_PROTOCOL_VIOLATION).await;
            return None;
        }
    };

    if let Err(err) = codec::check_hello(&hello) {
        let code = match err {
            AppError::VersionMismatch(_) => CLOSE_VERSION_MISMATCH,
            _ => CLOSE_PROTOCOL_VIOLATION,
        };
        send_fatal(tx, &err, code).await;
        return None;
    }

    // /ws/ui 只接受 UI 角色（protocol §必需字段表注）。
    if hello.client != ClientKind::Ui {
        send_fatal(
            tx,
            &AppError::Protocol("/ws/ui requires hello.client='ui'".into()),
            CLOSE_PROTOCOL_VIOLATION,
        )
        .await;
        return None;
    }

    // token 恒时比较；`MOMO_UI_TOKEN` 未配置（standalone dev）不比对，
    // `hello.auth_token` 字段本身已由 serde 保证必填（protocol §认证）。
    if let Some(expected) = state.config.ui_token.as_ref() {
        if !token_eq(&hello.auth_token, expected.expose()) {
            send_fatal(
                tx,
                &AppError::Unauthorized("hello.auth_token mismatch".into()),
                CLOSE_UNAUTHORIZED,
            )
            .await;
            return None;
        }
    }

    let hello_ok = Msg::HelloOk(HelloOkMsg {
        version: crate::proto::message::PROTOCOL_VERSION.to_string(),
        session_id: session.id.clone(),
        role: ServerRole::Orchestrator,
        capabilities: vec!["barge_in".into(), "emotion_tags".into()],
    });
    if tx.send(OutboundFrame::Json(hello_ok)).await.is_err() {
        return None;
    }
    debug!(%conn_id, client = ?hello.client, session = %session.id, "hello.ok sent");
    Some(session)
}

/// conn_tx：Session/管线 → encode → 写帧。
///
/// `rx.recv()` 逐帧消费保证顺序；`sink.send` 套 [`CONN_TX_STALL`] deadline，
/// 对端消费不动时停止写并关连，不静默丢控制帧。
async fn tx_loop<Si, E>(mut sink: Si, mut rx: mpsc::Receiver<OutboundFrame>)
where
    Si: Sink<Message, Error = E> + Unpin,
    E: Display,
{
    while let Some(frame) = rx.recv().await {
        let (msg, is_close) = match frame {
            OutboundFrame::Json(m) => match codec::encode_msg(&m) {
                Ok(text) => (Message::Text(Utf8Bytes::from(text)), false),
                Err(err) => {
                    warn!(%err, "failed to encode server msg");
                    continue;
                }
            },
            OutboundFrame::JsonText(text) => (Message::Text(Utf8Bytes::from(text)), false),
            OutboundFrame::Pong(payload) => (Message::Pong(payload), false),
            OutboundFrame::Audio(header, payload) => (
                Message::Binary(codec::encode_audio_frame(header, &payload)),
                false,
            ),
            OutboundFrame::Close(code, reason) => (
                Message::Close(Some(CloseFrame {
                    code,
                    reason: Utf8Bytes::from(reason),
                })),
                true,
            ),
        };
        match tokio::time::timeout(CONN_TX_STALL, sink.send(msg)).await {
            Ok(Ok(())) if is_close => break,
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(%err, "ws sink send failed, closing connection");
                break;
            }
            Err(_) => {
                warn!(
                    stall_s = CONN_TX_STALL.as_secs(),
                    "conn_tx stalled, closing connection"
                );
                break;
            }
        }
    }
}

/// 回 `error` 帧后按协议 close code 断连。
async fn send_fatal(tx: &mpsc::Sender<OutboundFrame>, err: &AppError, code: u16) {
    let reason: &'static str = match code {
        CLOSE_VERSION_MISMATCH => "version mismatch",
        CLOSE_PROTOCOL_VIOLATION => "protocol violation",
        CLOSE_UNAUTHORIZED => "unauthorized",
        CLOSE_INTERNAL => "internal error",
        _ => "closing",
    };
    let _ = tx.send(OutboundFrame::Close(code, reason)).await;
}

/// constant-time 字符串比较（protocol §认证：token 不落日志、不用早退比较）。
fn token_eq(got: &str, expected: &str) -> bool {
    let (a, b) = (got.as_bytes(), expected.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `AppError` → 协议 `error` 帧。
///
    Msg::Error(ErrorMsg {
        event_seq: None,
        code: err.code(),
        message: err.to_string(),
    })
}
