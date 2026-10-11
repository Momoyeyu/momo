//! 单连接 actor：rx/tx task、帧编解码、背压。
//!
//! 拓扑（momo-app.md §tokio 任务拓扑）：
//! `conn_rx`（读帧→decode→`mpsc` 给 Session）、`conn_tx`（Session→encode→写帧）。
//! tx 用有界 mpsc 做背压；JSON 控制帧永不丢弃，对端消费不动则关连（protocol §7）。

use std::sync::Arc;

use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    error::AppError,
    proto::{
        codec::{self, CLOSE_INTERNAL, CLOSE_PROTOCOL_VIOLATION, CLOSE_VERSION_MISMATCH},
        message::{ClientMsg, ErrorMsg, HelloOkMsg, ServerMsg, ServerRole, new_id},
    },
    session::{SessionCmd, SessionHandle},
    ws::server::AppState,
};

/// conn_tx 有界队列上限（momo-app.md §ws）。
pub const CONN_TX_CAP: usize = 256;

/// conn_tx 的出站单元：JSON 文本帧 / 二进制音频帧 / 关闭帧。
#[derive(Debug)]
pub enum OutboundFrame {
    Json(ServerMsg),
    /// `audio.chunk`：头 + PCM 载荷。
    Audio(codec::AudioHeader, Bytes),
    Close(u16, &'static str),
}

/// 一条 `/ws/ui` 连接的完整生命周期。
pub async fn run(socket: WebSocket, state: Arc<AppState>) {
    let conn_id = new_id("conn");
    debug!(%conn_id, "ws connection opened");

    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel::<OutboundFrame>(CONN_TX_CAP);
    let tx_task = tokio::spawn(tx_loop(sink, rx));

    // 首帧必须是 hello（protocol §3）。
    let Some(session) = handshake(&conn_id, &mut stream, &tx, &state).await else {
        tx_task.abort();
        return;
    };

    // conn_rx 循环：帧 → ClientMsg → Session。
    let cmd_tx = session.cmd_tx.clone();
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Text(text)) => match codec::decode_client_msg(text.as_str()) {
                Ok(ClientMsg::Unknown) => {
                    let _ = tx
                        .send(OutboundFrame::Json(error_frame(
                            &AppError::BadMessage("unknown message type".into()),
                            None,
                            false,
                        )))
                        .await;
                }
                Ok(client_msg) => {
                    if cmd_tx
                        .send(SessionCmd::ClientMsg(client_msg))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx
                        .send(OutboundFrame::Json(error_frame(&err, None, false)))
                        .await;
                }
            },
            Ok(Message::Binary(bytes)) => {
                match codec::AudioHeader::decode(&bytes) {
                    Some((header, _payload)) => {
                        // TODO(P2): 上行 audio.chunk 转发 ASR uplink task。
                        debug!(%conn_id, seq = header.seq, stream = header.stream, "uplink audio chunk dropped (P2 stub)");
                    }
                    None => {
                        let _ = tx
                            .send(OutboundFrame::Json(error_frame(
                                &AppError::BadMessage("malformed audio frame".into()),
                                None,
                                false,
                            )))
                            .await;
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            // Ping/Pong 由 axum 自动应答。
            Ok(_) => {}
        }
    }

    let _ = cmd_tx.send(SessionCmd::ConnClosed).await;
    drop(tx); // 关闭 conn_tx：tx_loop 排空后退出。
    let _ = tx_task.await;
    debug!(%conn_id, "ws connection closed");
}

/// 握手：首消息必须是 `hello{protocol:"momo-ws", version:"v1"}`，
/// 不匹配回 `error` + 按 §8 close code 断连。
async fn handshake(
    conn_id: &str,
    stream: &mut (impl StreamExt<Item = Result<Message, axum::Error>> + Unpin),
    tx: &mpsc::Sender<OutboundFrame>,
    state: &Arc<AppState>,
) -> Option<SessionHandle> {
    let first = match stream.next().await {
        Some(Ok(Message::Text(text))) => text,
        // 静默断连（如对端直接关闭）不视为协议违反。
        Some(Ok(Message::Close(_))) | None => return None,
        _ => {
            send_fatal(
                tx,
                &AppError::Protocol("first frame must be a text hello".into()),
                CLOSE_PROTOCOL_VIOLATION,
            )
            .await;
            return None;
        }
    };

    let hello = match codec::decode_client_msg(first.as_str()) {
        Ok(ClientMsg::Hello(hello)) => hello,
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

    // 单活跃 session 策略：后连顶掉前连（SessionManager::attach）。
    let session = state.sessions.attach(conn_id, hello.client);
    let hello_ok = ServerMsg::HelloOk(HelloOkMsg {
        version: crate::proto::message::PROTOCOL_VERSION.to_string(),
        session_id: session.id.clone(),
        server: "momod".into(),
        role: ServerRole::Orchestrator,
        capabilities: vec!["barge_in".into(), "actions".into(), "emotion_tags".into()],
    });
    if tx.send(OutboundFrame::Json(hello_ok)).await.is_err() {
        return None;
    }
    debug!(%conn_id, client = ?hello.client, session = %session.id, "hello.ok sent");
    Some(session)
}

/// conn_tx：Session/管线 → encode → 写帧。
async fn tx_loop(
    mut sink: impl SinkExt<Message, Error = axum::Error> + Unpin,
    mut rx: mpsc::Receiver<OutboundFrame>,
) {
    while let Some(frame) = rx.recv().await {
        let (msg, is_close) = match frame {
            OutboundFrame::Json(m) => match codec::encode_server_msg(&m) {
                Ok(text) => (Message::Text(Utf8Bytes::from(text)), false),
                Err(err) => {
                    warn!(%err, "failed to encode server msg");
                    continue;
                }
            },
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
        if sink.send(msg).await.is_err() || is_close {
            break;
        }
    }
}

/// 回 `error{fatal:true}` 后按协议 close code 断连。
async fn send_fatal(tx: &mpsc::Sender<OutboundFrame>, err: &AppError, code: u16) {
    let _ = tx
        .send(OutboundFrame::Json(error_frame(err, None, true)))
        .await;
    let reason: &'static str = match code {
        CLOSE_VERSION_MISMATCH => "version mismatch",
        CLOSE_PROTOCOL_VIOLATION => "protocol violation",
        CLOSE_INTERNAL => "internal error",
        _ => "closing",
    };
    let _ = tx.send(OutboundFrame::Close(code, reason)).await;
}

/// `AppError` → 协议 `error` 帧。
fn error_frame(err: &AppError, ref_id: Option<String>, fatal: bool) -> ServerMsg {
    ServerMsg::Error(ErrorMsg {
        code: err.code(),
        message: err.to_string(),
        ref_id,
        fatal,
    })
}
