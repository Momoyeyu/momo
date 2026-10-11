//!


use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket};
use bytes::Bytes;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    error::AppError,
    proto::{
    },
    session::{SessionCmd, SessionHandle},
    ws::server::AppState,
};

pub const CONN_TX_CAP: usize = 256;

#[derive(Debug)]
pub enum OutboundFrame {
    /// `audio.chunk`：头 + PCM 载荷。
    Audio(codec::AudioHeader, Bytes),
    Close(u16, &'static str),
}

/// 一条 `/ws/ui` 连接的完整生命周期。
pub async fn run(socket: WebSocket, state: Arc<AppState>) {
    let conn_id = new_id("conn");
    debug!(%conn_id, "ws connection opened");

    let (tx, rx) = mpsc::channel::<OutboundFrame>(CONN_TX_CAP);
    let tx_task = tokio::spawn(tx_loop(sink, rx));

    let Some(session) = handshake(&conn_id, &mut stream, &tx, &state).await else {
        return;
    };

    let cmd_tx = session.cmd_tx.clone();
        match msg {
                    let _ = tx
                        .await;
                }
                Ok(client_msg) => {
                        break;
                    }
                }
                Err(err) => {
                }
            },
            Ok(Message::Binary(bytes)) => {
                match codec::AudioHeader::decode(&bytes) {
                    }
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

    conn_id: &str,
    tx: &mpsc::Sender<OutboundFrame>,
    state: &Arc<AppState>,
        // 静默断连（如对端直接关闭）不视为协议违反。
            send_fatal(
                tx,
                &AppError::Protocol("first frame must be a text hello".into()),
                CLOSE_PROTOCOL_VIOLATION,
            )
            .await;
            return None;
        }
    };

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

        version: crate::proto::message::PROTOCOL_VERSION.to_string(),
        session_id: session.id.clone(),
        role: ServerRole::Orchestrator,
    });
    if tx.send(OutboundFrame::Json(hello_ok)).await.is_err() {
        return None;
    }
    debug!(%conn_id, client = ?hello.client, session = %session.id, "hello.ok sent");
    Some(session)
}

/// conn_tx：Session/管线 → encode → 写帧。
    while let Some(frame) = rx.recv().await {
        let (msg, is_close) = match frame {
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
        }
    }
}

async fn send_fatal(tx: &mpsc::Sender<OutboundFrame>, err: &AppError, code: u16) {
    let reason: &'static str = match code {
        CLOSE_VERSION_MISMATCH => "version mismatch",
        CLOSE_PROTOCOL_VIOLATION => "protocol violation",
        CLOSE_INTERNAL => "internal error",
        _ => "closing",
    };
    let _ = tx.send(OutboundFrame::Close(code, reason)).await;
}

/// `AppError` → 协议 `error` 帧。
        code: err.code(),
        message: err.to_string(),
    })
}
