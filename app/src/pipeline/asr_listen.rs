//! `AsrClient` 的 Deepgram 对齐实装：provider 接入面 `ws(s)://…/v1/listen`
//! （protocol.md §11.2；线格式权威定义见 momo-infra provider-protocol.md §1）。
//!
//! 与 [`MomoWsAsr`]（momo-ws 一连接多 stream 复用）不同：一连接 = 一音频流，
//! capture 生命周期 = 连接生命周期，无应用层握手帧。`open_uplink` 触发建连
//! （缺省补齐 `encoding/sample_rate/channels/vad_events` query，鉴权走
//! `Authorization: Token` 头）；上行 binary 为裸 PCM s16le 16kHz mono
//! （momo 32B 帧头不随行）；服务端 JSON 事件映射回流为 [`AsrEvent`]。
//! `end_uplink` 发 `CloseStream` 等服务端回声 + close 1000；capture 取消
//! 直接断连（协议语义：断连即取消）。

use std::fmt;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{
    self, Message, client::IntoClientRequest, http::header::AUTHORIZATION,
};
use tracing::{debug, warn};
use ulid::Ulid;
use url::Url;

use crate::{
    config::SecretString,
    error::AppError,
    pipeline::asr::{ASR_EVENT_CAP, AsrClient, AsrEvent, now_ms},
    proto::{
        codec::AudioHeader,
        message::{
            AudioOpenMsg, CancelTarget, ErrorCode, ErrorMsg, InputAudioSegmentMsg, InputVadMsg,
            ServerEnvelope, VadEvent,
        },
    },
    ws::conn::{CONN_TX_CAP, CONN_TX_STALL},
};

/// `/v1/listen` 建连 query 默认值：endpoint 自带同名参数不覆盖
/// （`?token=`/`?access_token=`/`language`/`model` 等用户参数原样透传）。
const LISTEN_DEFAULTS: &[(&str, &str)] = &[
    ("encoding", "linear16"),
    ("sample_rate", "16000"),
    ("channels", "1"),
    ("vad_events", "true"),
];

/// 出站单元：裸 PCM 帧 / 收尾 CloseStream / 立即关闭 / Ping 应答。
enum ListenOut {
    Pcm(Bytes),
    /// `{"type":"CloseStream"}`：Finalize 语义，等服务端回声 + close 1000。
    CloseStream,
    /// ws close 帧（cancel capture：断连即取消）。
    Close,
    Pong(Bytes),
}

/// `/v1/listen` ASR 客户端（endpoint 形态非 `…/ws` 时由 `AsrProvider` 选用）。
///
/// 链路懒建立：首个 `open_uplink` 触发 WS 升级；一连接只服务一个 capture，
/// 新 `open_uplink` 顶掉旧链（session 语义保证同时只有一个活动 capture）。
pub struct DeepgramListenAsr {
    /// 链路与目标端点同槽：endpoint 变更新链自然落到新地址。
    slot: Mutex<ListenSlot>,
    /// 当前事件订阅者的发送端；`subscribe` 替换（与 MomoWsAsr 同口径）。
    events: Arc<Mutex<mpsc::Sender<AsrEvent>>>,
}

struct ListenSlot {
    endpoint: Url,
    api_key: Option<SecretString>,
    link: Option<Arc<ListenLink>>,
}

impl fmt::Debug for DeepgramListenAsr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepgramListenAsr").finish_non_exhaustive()
    }
}

impl DeepgramListenAsr {
    pub fn new(endpoint: Url, api_key: Option<SecretString>) -> Self {
        // 订阅前事件无处可去：先建一个立即丢弃的接收端。
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        Self {
            slot: Mutex::new(ListenSlot {
                endpoint,
                api_key,
                link: None,
            }),
            events: Arc::new(Mutex::new(tx)),
        }
    }

    /// 活链路快照（不重连）。
    async fn live_link(&self) -> Option<Arc<ListenLink>> {
        let guard = self.slot.lock().await;
        guard
            .link
            .as_ref()
            .filter(|l| l.alive.load(Ordering::Acquire))
            .cloned()
    }
}

#[async_trait]
impl AsrClient for DeepgramListenAsr {
    /// `audio.open{uplink}` → 建连（一连接一流）；已有活链先 graceful 顶掉。
    async fn open_uplink(&self, open: &AudioOpenMsg) -> Result<(), AppError> {
        let mut guard = self.slot.lock().await;
        if let Some(old) = guard.link.take() {
            old.retire();
        }
        let link = ListenLink::connect(
            &guard.endpoint,
            guard.api_key.as_ref(),
            open.capture_id.clone().unwrap_or_default(),
            self.events.clone(),
        )
        .await?;
        debug!(endpoint = %guard.endpoint, capture = %link.capture_id, "asr listen link established");
        guard.link = Some(link);
        Ok(())
    }

    /// 上行 binary = 裸 PCM 载荷（momo 帧头不下行到本协议；空帧不上行）。
    async fn send_frame(&self, _header: AudioHeader, payload: Bytes) -> Result<(), AppError> {
        if payload.is_empty() {
            return Ok(());
        }
        let Some(link) = self.live_link().await else {
            return Err(AppError::ModelUnreachable("asr link down".into()));
        };
        link.tx.send(ListenOut::Pcm(payload)).await.map_err(|e| {
            link.alive.store(false, Ordering::Release);
            AppError::ModelUnreachable(format!("asr link tx: {e}"))
        })
    }

    /// `audio.end` → `{"type":"CloseStream"}`；链路等服务端回声后由对端关闭。
    async fn end_uplink(&self, _stream_id: &str, _last_chunk_seq: u32) -> Result<(), AppError> {
        let Some(link) = self.live_link().await else {
            return Ok(());
        };
        link.graceful.store(true, Ordering::Release);
        let _ = link.tx.send(ListenOut::CloseStream).await;
        Ok(())
    }

    /// `cancel{capture}` → 断连即取消；`cancel{turn}` 在本协议无对应物，尽力而为。
    async fn cancel(&self, target: CancelTarget, id: &str) -> Result<(), AppError> {
        if target != CancelTarget::Capture {
            return Ok(());
        }
        let Some(link) = self.live_link().await else {
            return Ok(());
        };
        if link.capture_id == id {
            link.retire();
        }
        Ok(())
    }

    /// 订阅回流事件；单活跃 session 语义，新订阅取代旧订阅者。
    async fn subscribe(&self) -> mpsc::Receiver<AsrEvent> {
        let (tx, rx) = mpsc::channel(ASR_EVENT_CAP);
        *self.events.lock().await = tx;
        rx
    }
}

/// 单条 `/v1/listen` 链路：出站队列 + 回流事件路由 + 存活标记。
struct ListenLink {
    /// 本连接绑定的 capture（VAD/Results 事件回填用）。
    capture_id: String,
    tx: mpsc::Sender<ListenOut>,
    /// 事件订阅者发送端（与 `DeepgramListenAsr::events` 共享）。
    events: Arc<Mutex<mpsc::Sender<AsrEvent>>>,
    /// rx task 退出/终态置 false。
    alive: AtomicBool,
    /// 我们发了 CloseStream / cancel，或对端回声 CloseStream：
    /// 之后的终态属正常收尾，不再上报 LinkDown。
    graceful: AtomicBool,
    /// rx/tx task 句柄（retire 且出站队列不可写时 abort）。持锁期间无 await，
    /// 用 std Mutex（`blocking_lock` 在 async 上下文会 panic）。
    tasks: std::sync::Mutex<Vec<JoinHandle<()>>>,
}

impl ListenLink {
    /// 建连：query 缺省补齐 + `Authorization: Token` 头；返回已就位链路。
    async fn connect(
        endpoint: &Url,
        api_key: Option<&SecretString>,
        capture_id: String,
        events: Arc<Mutex<mpsc::Sender<AsrEvent>>>,
    ) -> Result<Arc<Self>, AppError> {
        let url = listen_url(endpoint);
        // tungstenite `url` feature 未开：走 &str impl 构造带 header 的请求。
        let mut req = url
            .as_str()
            .into_client_request()
            .map_err(|e| AppError::ModelUnreachable(format!("asr listen url {url}: {e}")))?;
        if let Some(key) = api_key {
            let value = format!("Token {}", key.expose());
            req.headers_mut().insert(
                AUTHORIZATION,
                value.parse().map_err(|_| {
                    AppError::Config(crate::config::ConfigError::Invalid(vec![
                        "MOMO_ASR_API_KEY: not a valid header value".into(),
                    ]))
                })?,
            );
        }
        let (socket, _resp) = tokio_tungstenite::connect_async(req)
            .await
            .map_err(|e| listen_connect_err(endpoint, e))?;
        let (sink, stream) = socket.split();

        let (tx, out_rx) = mpsc::channel::<ListenOut>(CONN_TX_CAP);
        let link = Arc::new(ListenLink {
            capture_id,
            tx,
            events,
            alive: AtomicBool::new(true),
            graceful: AtomicBool::new(false),
            tasks: std::sync::Mutex::new(Vec::new()),
        });
        let tx_task = tokio::spawn(listen_tx_loop(sink, out_rx));
        let rx_task = tokio::spawn(listen_rx_loop(stream, Arc::downgrade(&link)));
        link.tasks
            .lock()
            .expect("listen link tasks")
            .extend([tx_task, rx_task]);
        Ok(link)
    }

    /// 主动收尾：标记 graceful + 判死 + 发 close 帧（cancel/顶链路径）。
    /// 出站队列已满/已死时直接 abort 双 task，socket 随任务退出断开。
    fn retire(&self) {
        self.graceful.store(true, Ordering::Release);
        self.alive.store(false, Ordering::Release);
        if self.tx.try_send(ListenOut::Close).is_err() {
            for t in self.tasks.lock().expect("listen link tasks").iter() {
                t.abort();
            }
        }
    }

    /// 链路终态：判死；非 graceful 收尾上报 LinkDown（session 据此中止 capture）。
    async fn died(&self, err: AppError) {
        self.alive.store(false, Ordering::Release);
        if self.graceful.load(Ordering::Acquire) {
            return;
        }
        let _ = self.events.lock().await.send(AsrEvent::LinkDown(err)).await;
    }

    async fn emit(&self, ev: AsrEvent) {
        if self.events.lock().await.send(ev).await.is_err() {
            debug!("asr event dropped: no subscriber");
        }
    }

    /// 服务端 JSON 事件 → AsrEvent 映射（protocol.md §11.2）。
    async fn route_text(&self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            warn!("asr listen: non-json text frame (dropped)");
            return;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("Metadata") => {
                debug!(
                    request_id = ?v.get("request_id").and_then(|x| x.as_str()),
                    model = ?v.pointer("/model_info/name").and_then(|x| x.as_str()),
                    "asr listen metadata"
                );
            }
            Some("SpeechStarted") => {
                self.emit(AsrEvent::Vad(self.vad(VadEvent::SpeechStart)))
                    .await;
            }
            Some("UtteranceEnd") => {
                self.emit(AsrEvent::Vad(self.vad(VadEvent::SpeechEnd)))
                    .await;
            }
            Some("Results") => {
                let transcript = v
                    .pointer("/channel/alternatives/0/transcript")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let is_final = v.get("is_final").and_then(Value::as_bool).unwrap_or(true);
                if !transcript.is_empty() {
                    self.emit(AsrEvent::Segment(self.segment(transcript, is_final)))
                        .await;
                }
            }
            Some("Error") => {
                let message = v
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let code = v.get("code").and_then(Value::as_str).unwrap_or("");
                warn!(%code, %message, "asr listen error event");
                self.emit(AsrEvent::Error(self.error(code, message))).await;
            }
            // 服务端回声：正常收尾路径标记（终态不再上报 LinkDown）。
            Some("CloseStream") => self.graceful.store(true, Ordering::Release),
            other => debug!(?other, "unhandled asr listen event (dropped)"),
        }
    }

    fn env(&self) -> ServerEnvelope {
        // session 侧对回流消息一律 restamp 自身 envelope，这里只需占位。
        ServerEnvelope {
            session_id: String::new(),
            event_id: Ulid::new().to_string(),
            at_ms: now_ms(),
            reply_to: None,
            event_seq: 0,
        }
    }

    fn vad(&self, event: VadEvent) -> InputVadMsg {
        InputVadMsg {
            env: self.env(),
            capture_id: self.capture_id.clone(),
            event,
        }
    }

    fn segment(&self, text: String, is_final: bool) -> InputAudioSegmentMsg {
        InputAudioSegmentMsg {
            env: self.env(),
            capture_id: self.capture_id.clone(),
            text,
            is_final,
        }
    }

    fn error(&self, code: &str, message: String) -> ErrorMsg {
        ErrorMsg {
            session_id: None,
            event_id: None,
            at_ms: None,
            event_seq: None,
            code: match code {
                "BAD_MESSAGE" => ErrorCode::BadMessage,
                "ASR_FAILED" => ErrorCode::AsrFailed,
                "UNAUTHORIZED" => ErrorCode::Unauthorized,
                _ => ErrorCode::Unknown,
            },
            message,
            ref_id: None,
            turn_id: None,
            capture_id: Some(self.capture_id.clone()),
            stream_id: None,
            sentence_seq: None,
        }
    }
}

/// `/v1/listen` URL：endpoint + 缺省 query 补齐（已有参数不覆盖）。
fn listen_url(endpoint: &Url) -> Url {
    let mut url = endpoint.clone();
    let existing: Vec<String> = url.query_pairs().map(|(k, _)| k.into_owned()).collect();
    {
        let mut qp = url.query_pairs_mut();
        for (k, v) in LISTEN_DEFAULTS {
            if !existing.iter().any(|e| e == k) {
                qp.append_pair(k, v);
            }
        }
    }
    url
}

/// 升级失败 → AppError：401/403（4401 鉴权拒绝的 HTTP 表现）映射 Unauthorized。
fn listen_connect_err(endpoint: &Url, err: tungstenite::Error) -> AppError {
    match err {
        tungstenite::Error::Http(resp) => match resp.status().as_u16() {
            401 | 403 => AppError::Unauthorized(format!(
                "asr listen auth rejected by {endpoint}: {}",
                resp.status()
            )),
            status => AppError::ModelUnreachable(format!("asr listen http {status} at {endpoint}")),
        },
        other => AppError::ModelUnreachable(format!("ws connect {endpoint}: {other}")),
    }
}

/// rx 循环：文本事件 → `route_text`；Ping 经出站队列回 Pong；
/// close/错误/流尽 → 终态（graceful 与否决定报不报 LinkDown）。
async fn listen_rx_loop<St>(mut stream: St, link: Weak<ListenLink>)
where
    St: Stream<Item = Result<Message, tungstenite::Error>> + Unpin + Send + 'static,
{
    while let Some(item) = stream.next().await {
        let Some(link) = link.upgrade() else { return };
        match item {
            Ok(Message::Text(text)) => link.route_text(&text).await,
            Ok(Message::Binary(_)) => {
                warn!("unexpected binary frame on asr listen link (dropped)");
            }
            Ok(Message::Ping(payload)) => {
                if link.tx.send(ListenOut::Pong(payload)).await.is_err() {
                    return;
                }
            }
            Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
            Ok(Message::Close(cf)) => {
                let err = AppError::ModelUnreachable(format!("asr listen closed: {cf:?}"));
                link.died(err).await;
                return;
            }
            Err(err) => {
                link.died(AppError::ModelUnreachable(format!("ws read: {err}")))
                    .await;
                return;
            }
        }
    }
    if let Some(link) = link.upgrade() {
        link.died(AppError::ModelUnreachable("asr listen closed".into()))
            .await;
    }
}

/// tx 循环：`ListenOut` → tungstenite 帧；单帧 stall deadline 与
/// momo-ws client 同口径（CONN_TX_STALL）。队列耗尽（link 全丢）即关连。
async fn listen_tx_loop<Si, E>(mut sink: Si, mut rx: mpsc::Receiver<ListenOut>)
where
    Si: Sink<Message, Error = E> + Unpin,
    E: Display,
{
    while let Some(out) = rx.recv().await {
        let (msg, is_close) = match out {
            ListenOut::Pcm(payload) => (Message::Binary(payload), false),
            ListenOut::CloseStream => (Message::Text(r#"{"type":"CloseStream"}"#.into()), false),
            ListenOut::Close => (Message::Close(None), true),
            ListenOut::Pong(payload) => (Message::Pong(payload), false),
        };
        match tokio::time::timeout(CONN_TX_STALL, sink.send(msg)).await {
            Ok(Ok(())) if is_close => break,
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(%err, "asr listen sink send failed, closing connection");
                break;
            }
            Err(_) => {
                warn!(stall_s = CONN_TX_STALL.as_secs(), "asr listen tx stalled");
                break;
            }
        }
    }
    // 发送端全掉落（link retire/drop）：发 close 帧收尾后退出。
    let _ = sink.send(Message::Close(None)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_url_defaults_fill_missing_params_only() {
        let url = listen_url(&Url::parse("ws://127.0.0.1:7810/v1/listen").unwrap());
        assert_eq!(url.path(), "/v1/listen");
        let q: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        for (k, v) in LISTEN_DEFAULTS {
            assert!(q.contains(&(k.to_string(), v.to_string())), "missing {k}");
        }
    }

    #[test]
    fn listen_url_keeps_user_params_and_does_not_override() {
        let url = listen_url(
            &Url::parse("wss://api.example.com/v1/listen?token=k1&language=zh&sample_rate=8000")
                .unwrap(),
        );
        let q: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert!(q.contains(&("token".into(), "k1".into())));
        assert!(q.contains(&("language".into(), "zh".into())));
        // 用户显式值不被默认覆盖。
        assert!(q.contains(&("sample_rate".into(), "8000".into())));
        assert!(q.contains(&("encoding".into(), "linear16".into())));
    }
}
