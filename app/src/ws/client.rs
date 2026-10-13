//! momo-ws 客户端连接（orchestrator → 模型服务链路）。
//! 与 server 侧 conn.rs 对称：`OutboundFrame` 有界 mpsc 背压 + [`CONN_TX_STALL`]
//! deadline；TLS 走 rustls；握手/帧格式见 protocol.md §4/§6。

use std::fmt::Display;

use bytes::Bytes;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, Message};
use tracing::{debug, warn};
use url::Url;

use crate::{
    error::AppError,
    proto::{
        codec::{self, AudioHeader},
        message::{
            ClientKind, ErrorMsg, HelloMsg, HelloOkMsg, Msg, PROTOCOL_NAME, PROTOCOL_VERSION,
            ServerRole,
        },
    },
    ws::conn::{CONN_TX_CAP, CONN_TX_STALL, HELLO_TIMEOUT, OutboundFrame},
};

/// rx → 业务方的入站单元。
#[derive(Debug)]
pub enum Inbound {
    /// 已解码文本消息（`audio.open` / `audio.end` / `error` 等）。
    Msg(Msg),
    /// 已解码二进制音频帧（头 + 载荷）。
    Audio(AudioHeader, Bytes),
    /// 传输或解码错误；此后通道关闭。
    Failed(AppError),
    /// 对端关闭或流耗尽；此后通道关闭。
    Closed,
}

/// 一条已握手的模型服务链路。
#[derive(Debug)]
pub struct ClientConn {
    /// `hello.ok.session_id`：客户端消息 envelope 复用。
    pub session_id: String,
    /// 出站队列（业务方发 `OutboundFrame`）。
    pub tx: mpsc::Sender<OutboundFrame>,
    /// 入站事件流（rx task 推送）。
    pub rx: mpsc::Receiver<Inbound>,
}

/// 连 `endpoint` 并完成 hello/hello.ok 握手，校验 `hello.ok.role == role`
/// （protocol §2 必需字段表注：连错端口在此暴露）。
pub async fn connect(endpoint: &Url, role: ServerRole) -> Result<ClientConn, AppError> {
    let (socket, _resp) = tokio_tungstenite::connect_async(endpoint.as_str())
        .await
        .map_err(|e| AppError::ModelUnreachable(format!("ws connect {endpoint}: {e}")))?;
    let (sink, mut stream) = socket.split();

    let (tx, out_rx) = mpsc::channel::<OutboundFrame>(CONN_TX_CAP);
    let tx_task = tokio::spawn(tx_loop(sink, out_rx));

    // 模型链路 peer credential 协议未定稿（protocol §4），auth_token 占位空串。
    let hello = Msg::Hello(HelloMsg {
        protocol: PROTOCOL_NAME.to_string(),
        version: PROTOCOL_VERSION.to_string(),
        client: ClientKind::Orchestrator,
        auth_token: String::new(),
        resume_session_id: None,
    });
    let hello_ok = match tx.send(OutboundFrame::Json(hello)).await {
        Ok(()) => match tokio::time::timeout(HELLO_TIMEOUT, read_hello_ok(&mut stream)).await {
            Ok(res) => res,
            Err(_) => Err(AppError::Protocol("hello.ok timeout".into())),
        },
        Err(_) => Err(AppError::ModelUnreachable("ws tx closed".into())),
    };

    let ok = match hello_ok {
        Ok(ok) => ok,
        Err(err) => {
            tx_task.abort();
            return Err(err);
        }
    };
    if ok.version != PROTOCOL_VERSION {
        tx_task.abort();
        return Err(AppError::VersionMismatch(format!(
            "server speaks {:?}, client requires {PROTOCOL_VERSION:?}",
            ok.version
        )));
    }
    if ok.role != role {
        tx_task.abort();
        return Err(AppError::Protocol(format!(
            "hello.ok role {role_expected:?} expected, got {got:?}",
            role_expected = role,
            got = ok.role,
        )));
    }

    let (in_tx, in_rx) = mpsc::channel(CONN_TX_CAP);
    tokio::spawn(rx_loop(stream, tx.clone(), in_tx));
    debug!(session = %ok.session_id, ?role, "ws client handshake done");
    Ok(ClientConn {
        session_id: ok.session_id,
        tx,
        rx: in_rx,
    })
}

/// 握手期读取：首条业务帧必须是 `hello.ok`（或 fatal `error`/close）。
async fn read_hello_ok<St>(stream: &mut St) -> Result<HelloOkMsg, AppError>
where
    St: Stream<Item = Result<Message, tungstenite::Error>> + Unpin,
{
    match stream.next().await {
        Some(Ok(Message::Text(text))) => match codec::decode_msg(&text) {
            Ok(Msg::HelloOk(ok)) => Ok(ok),
            Ok(Msg::Error(err)) => Err(fatal(err)),
            Ok(_) => Err(AppError::Protocol("expected hello.ok".into())),
            Err(err) => Err(err),
        },
        Some(Ok(Message::Close(cf))) => Err(AppError::ModelUnreachable(format!(
            "link closed during handshake: {cf:?}"
        ))),
        Some(Ok(_)) => Err(AppError::Protocol(
            "unexpected non-text frame during handshake".into(),
        )),
        Some(Err(e)) => Err(AppError::ModelUnreachable(format!("ws read: {e}"))),
        None => Err(AppError::ModelUnreachable(
            "link closed during handshake".into(),
        )),
    }
}

/// 握手期 `error` 帧 → 对应 AppError（close code 语义见 protocol §10）。
fn fatal(err: ErrorMsg) -> AppError {
    use crate::proto::message::ErrorCode::*;
    match err.code {
        VersionMismatch => AppError::VersionMismatch(err.message),
        Unauthorized => AppError::Unauthorized(err.message),
        _ => AppError::Protocol(format!("link rejected: {}", err.message)),
    }
}

/// conn_rx 对偶：socket → decode → `Inbound` mpsc。
///
/// 裸 `Ping` 显式回 `Pong`（uvicorn 周期 ping；tungstenite 的自动应答只在
/// 写半被驱动时才会真正落线，不依赖它）。通道满即阻塞形成背压。
async fn rx_loop<St>(
    mut stream: St,
    tx: mpsc::Sender<OutboundFrame>,
    inbound: mpsc::Sender<Inbound>,
) where
    St: Stream<Item = Result<Message, tungstenite::Error>> + Unpin + Send + 'static,
{
    while let Some(item) = stream.next().await {
        let ev = match item {
            Ok(Message::Text(text)) => match codec::decode_msg(&text) {
                Ok(m) => Inbound::Msg(m),
                Err(err) => Inbound::Failed(err),
            },
            Ok(Message::Binary(b)) => match AudioHeader::decode(&b) {
                Ok((header, payload)) => Inbound::Audio(header, Bytes::copy_from_slice(payload)),
                Err(err) => Inbound::Failed(err),
            },
            Ok(Message::Ping(payload)) => {
                if tx.send(OutboundFrame::Pong(payload)).await.is_err() {
                    break;
                }
                continue;
            }
            Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
            Ok(Message::Close(_)) => {
                let _ = inbound.send(Inbound::Closed).await;
                return;
            }
            Err(err) => {
                let _ = inbound
                    .send(Inbound::Failed(AppError::ModelUnreachable(format!(
                        "ws read: {err}"
                    ))))
                    .await;
                return;
            }
        };
        if inbound.send(ev).await.is_err() {
            return;
        }
    }
    let _ = inbound.send(Inbound::Closed).await;
}

/// conn_tx 对偶：`OutboundFrame` → tungstenite 帧 → 写 socket。
///
/// 与 server 侧 `conn::tx_loop` 同构：单队列保序、每帧 stall deadline、
/// 对端消费不动则关连（protocol §10「控制消息不得静默丢弃」）。
async fn tx_loop<Si, E>(mut sink: Si, mut rx: mpsc::Receiver<OutboundFrame>)
where
    Si: Sink<Message, Error = E> + Unpin,
    E: Display,
{
    while let Some(frame) = rx.recv().await {
        let (msg, is_close) = match frame {
            OutboundFrame::Json(m) => match codec::encode_msg(&m) {
                Ok(text) => (Message::Text(text.into()), false),
                Err(err) => {
                    warn!(%err, "failed to encode client msg");
                    continue;
                }
            },
            OutboundFrame::JsonText(text) => (Message::Text(text.into()), false),
            OutboundFrame::Audio(header, payload) => (
                Message::Binary(codec::encode_audio_frame(header, &payload)),
                false,
            ),
            OutboundFrame::Pong(payload) => (Message::Pong(payload), false),
            OutboundFrame::Close(code, reason) => (
                Message::Close(Some(tungstenite::protocol::CloseFrame {
                    code: code.into(),
                    reason: reason.into(),
                })),
                true,
            ),
        };
        match tokio::time::timeout(CONN_TX_STALL, sink.send(msg)).await {
            Ok(Ok(())) if is_close => break,
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(%err, "ws client sink send failed, closing connection");
                break;
            }
            Err(_) => {
                warn!(stall_s = CONN_TX_STALL.as_secs(), "ws client tx stalled");
                break;
            }
        }
    }
}
