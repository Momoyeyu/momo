//! `AsrClient` trait + WS 实装（momo-ws v2 客户端，握手/帧流见 protocol.md §2/§4/§6）。
//!
//! 编排器作为客户端连 momo-svc-asr（`MOMO_ASR_ENDPOINT`）：per-capture uplink
//! task 把 UI 上行 `audio.chunk` 二进制帧原样转发（≤2s 有界缓冲在 session 侧，
//! 溢出/seq gap 由 session 中止 capture，不静默丢帧）；svc-asr 回流的
//! `input.vad` / `input.audio.segment` 经 [`AsrClient::subscribe`] 汇入
//! Session actor。上行 PCM：s16le 16kHz mono。

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};
use ulid::Ulid;
use url::Url;

use crate::{
    error::AppError,
    proto::{
        codec::{AudioHeader, Direction},
        message::{
            AudioCodec, AudioOpenMsg, CancelTarget, ErrorMsg, InputAudioSegmentMsg, InputVadMsg,
            Msg, ServerRole,
        },
    },
    ws::{
        client::{self, ClientConn, Inbound},
        conn::OutboundFrame,
    },
};

/// 上行音频格式固定值（protocol §6）。
pub(crate) const UPLINK_SAMPLE_RATE: u32 = 16000;

/// 回流事件队列容量（VAD/segment/error 是低频控制消息）。
const ASR_EVENT_CAP: usize = 64;

/// svc-asr → Session 的回流事件（`input.vad`/`input.audio.segment` 镜像 UI）。
#[derive(Debug)]
pub enum AsrEvent {
    /// VAD 边界事件（speech_start / speech_end）。
    Vad(InputVadMsg),
    /// partial 预览或 final 转写段。
    Segment(InputAudioSegmentMsg),
    /// 服务端 `error` 帧（透传 code/message）。
    Error(ErrorMsg),
    /// 链路失败或对端关闭；进行中的 capture 由 session 收尾。
    LinkDown(AppError),
}

#[async_trait]
pub trait AsrClient: Send + Sync {
    /// `audio.open{uplink}` 转发：stream ↔ capture 绑定声明（restamp 链路 envelope）。
    async fn open_uplink(&self, open: &AudioOpenMsg) -> Result<(), AppError>;

    /// 上行二进制帧原样转发（header/payload 不改写）。
    async fn send_frame(&self, header: AudioHeader, payload: Bytes) -> Result<(), AppError>;

    /// `audio.end` 显式结束 uplink stream（`last_chunk_seq` 对账）。
    async fn end_uplink(&self, stream_id: &str, last_chunk_seq: u32) -> Result<(), AppError>;

    /// `control.cancel{target,id}`：capture 中止 / barge-in turn 取消扇出。
    /// 尽力而为：无活链路时无操作，不等 ACK。
    async fn cancel(&self, target: CancelTarget, id: &str) -> Result<(), AppError>;

    /// 订阅回流事件；单活跃 session 语义，新订阅取代旧订阅者。
    async fn subscribe(&self) -> mpsc::Receiver<AsrEvent>;
}

/// momo-ws v2 ASR 客户端（`MOMO_ASR_ENDPOINT`）。
///
/// 链路懒建立：首个 `open_uplink` 触发连接 + hello 握手（校验 `role=asr`）；
/// 断链后下一次 `open_uplink` 自动重连。一连接多 capture 复用，
/// `audio.open.stream_id` 路由二进制帧。
pub struct MomoWsAsr {
    /// 链路与目标端点同槽：endpoint 变更新链自然落到新地址。
    slot: Mutex<AsrSlot>,
    /// 当前事件订阅者的发送端；`subscribe` 替换（旧订阅者随 channel 关闭退出）。
    events: Arc<Mutex<mpsc::Sender<AsrEvent>>>,
}

struct AsrSlot {
    endpoint: Url,
    link: Option<Arc<AsrLink>>,
}

impl fmt::Debug for MomoWsAsr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MomoWsAsr").finish_non_exhaustive()
    }
}

impl MomoWsAsr {
    pub fn new(endpoint: Url) -> Self {
        // 订阅前事件无处可去：先建一个立即丢弃的接收端。
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        Self {
            slot: Mutex::new(AsrSlot { endpoint, link: None }),
            events: Arc::new(Mutex::new(tx)),
        }
    }

    /// 设置面热更新端点：写新端点并丢旧链，下一次 `link()` 连到新版。
    pub async fn set_endpoint(&self, endpoint: Url) {
        let mut guard = self.slot.lock().await;
        guard.endpoint = endpoint;
        guard.link = None;
    }

    /// 提前建连 + 握手（bring-up 场景先暴露连通性/角色错误）。
    pub async fn connect(&self) -> Result<(), AppError> {
        self.link().await.map(|_| ())
    }

    /// 取活链路；无或已死则重连（并发调用由 mutex 串行化，后者复用新链路）。
    async fn link(&self) -> Result<Arc<AsrLink>, AppError> {
        let mut guard = self.slot.lock().await;
        if let Some(link) = guard.link.as_ref() {
            if link.alive.load(Ordering::Acquire) {
                return Ok(link.clone());
            }
        }
        let conn = client::connect(&guard.endpoint, ServerRole::Asr).await?;
        let link = AsrLink::spawn(conn, self.events.clone());
        debug!(endpoint = %guard.endpoint, session = %link.session_id, "asr link established");
        guard.link = Some(link.clone());
        Ok(link)
    }

    /// 活链路快照（不重连）：帧/cancel 只对已建链路有意义。
    async fn live_link(&self) -> Option<Arc<AsrLink>> {
        let guard = self.slot.lock().await;
        guard
            .link
            .as_ref()
            .filter(|l| l.alive.load(Ordering::Acquire))
            .cloned()
    }
}

#[async_trait]
impl AsrClient for MomoWsAsr {
    async fn open_uplink(&self, open: &AudioOpenMsg) -> Result<(), AppError> {
        let link = self.link().await?;
        let wire = wire_audio_open(&link.session_id, open);
        link.tx
            .send(OutboundFrame::JsonText(wire))
            .await
            .map_err(|e| {
                link.alive.store(false, Ordering::Release);
                AppError::ModelUnreachable(format!("asr link tx: {e}"))
            })
    }

    async fn send_frame(&self, header: AudioHeader, payload: Bytes) -> Result<(), AppError> {
        let Some(link) = self.live_link().await else {
            return Err(AppError::ModelUnreachable("asr link down".into()));
        };
        link.tx
            .send(OutboundFrame::Audio(header, payload))
            .await
            .map_err(|e| {
                link.alive.store(false, Ordering::Release);
                AppError::ModelUnreachable(format!("asr link tx: {e}"))
            })
    }

    async fn end_uplink(&self, stream_id: &str, last_chunk_seq: u32) -> Result<(), AppError> {
        let Some(link) = self.live_link().await else {
            return Ok(()); // 链路已死：stream 随连接消失，无需收尾。
        };
        let wire = wire_audio_end(&link.session_id, stream_id, last_chunk_seq);
        let _ = link.tx.send(OutboundFrame::JsonText(wire)).await;
        Ok(())
    }

    async fn cancel(&self, target: CancelTarget, id: &str) -> Result<(), AppError> {
        let Some(link) = self.live_link().await else {
            return Ok(()); // 未建链：无可取消对象（不等 ACK）。
        };
        let wire = wire_cancel(&link.session_id, target, id);
        let _ = link.tx.send(OutboundFrame::JsonText(wire)).await;
        Ok(())
    }

    async fn subscribe(&self) -> mpsc::Receiver<AsrEvent> {
        let (tx, rx) = mpsc::channel(ASR_EVENT_CAP);
        // 替换发送端：旧 channel 关闭，旧订阅者 recv 得 None 退出。
        *self.events.lock().await = tx;
        rx
    }
}

/// 单条 svc-asr 链路：出站队列 + 回流事件路由 + 存活标记。
struct AsrLink {
    session_id: String,
    tx: mpsc::Sender<OutboundFrame>,
    /// 事件订阅者发送端（与 `MomoWsAsr::events` 共享）。
    events: Arc<Mutex<mpsc::Sender<AsrEvent>>>,
    /// rx loop 退出/终态事件置 false；`open_uplink` 据此重连。
    alive: AtomicBool,
}

impl AsrLink {
    fn spawn(conn: ClientConn, events: Arc<Mutex<mpsc::Sender<AsrEvent>>>) -> Arc<Self> {
        let link = Arc::new(AsrLink {
            session_id: conn.session_id,
            tx: conn.tx,
            events,
            alive: AtomicBool::new(true),
        });
        let weak = Arc::downgrade(&link);
        tokio::spawn(async move {
            let mut rx = conn.rx;
            while let Some(item) = rx.recv().await {
                let Some(link) = weak.upgrade() else { return };
                let terminal = matches!(item, Inbound::Failed(_) | Inbound::Closed);
                link.route(item).await;
                if terminal {
                    return;
                }
            }
            // 通道耗尽 = rx task 已退出，链路判死并通知 capture 收尾。
            if let Some(link) = weak.upgrade() {
                link.died(AppError::ModelUnreachable("asr link closed".into()))
                    .await;
            }
        });
        link
    }

    /// 链路终态：标记死链 + 上报 LinkDown（session 据此中止活动 capture）。
    async fn died(&self, err: AppError) {
        self.alive.store(false, Ordering::Release);
        let _ = self.events.lock().await.send(AsrEvent::LinkDown(err)).await;
    }

    async fn emit(&self, ev: AsrEvent) {
        if self.events.lock().await.send(ev).await.is_err() {
            debug!("asr event dropped: no subscriber");
        }
    }

    async fn route(&self, item: Inbound) {
        match item {
            Inbound::Msg(Msg::InputVad(vad)) => self.emit(AsrEvent::Vad(vad)).await,
            Inbound::Msg(Msg::InputAudioSegment(seg)) => self.emit(AsrEvent::Segment(seg)).await,
            Inbound::Msg(Msg::Error(err)) => {
                warn!(code = ?err.code, msg = %err.message, "svc-asr error frame");
                self.emit(AsrEvent::Error(err)).await;
            }
            Inbound::Msg(other) => {
                debug!(?other, "unhandled msg on asr link (dropped)");
            }
            Inbound::Audio(header, _) => {
                warn!(stream = %header.stream_id, "unexpected audio frame on uplink (dropped)");
            }
            Inbound::Failed(err) => self.died(err).await,
            Inbound::Closed => {
                self.died(AppError::ModelUnreachable("asr link closed".into()))
                    .await;
            }
        }
    }
}

/// `st_ULID` 文本 → 帧头 16B 原始 ULID（protocol §6）。
pub(crate) fn stream_key(stream_id: &str) -> Option<Ulid> {
    Ulid::from_string(stream_id.strip_prefix("st_")?).ok()
}

/// 客户端 envelope（protocol §1 `Envelope`；`event_seq` 仅服务端携带）。
#[derive(Serialize)]
struct ClientEnv<'a> {
    session_id: &'a str,
    event_id: String,
    at_ms: u64,
}

/// `audio.open{uplink}` 客户端出线形态（不含 downlink 专属字段）。
#[derive(Serialize)]
struct WireAudioOpen<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    env: ClientEnv<'a>,
    stream_id: &'a str,
    direction: Direction,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_id: Option<&'a str>,
    codec: AudioCodec,
    sample_rate: u32,
    channels: u8,
}

/// `audio.end` 客户端出线形态。
#[derive(Serialize)]
struct WireAudioEnd<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    env: ClientEnv<'a>,
    stream_id: &'a str,
    last_chunk_seq: u32,
}

/// `control.cancel` 客户端出线形态。
#[derive(Serialize)]
struct WireCancel<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    env: ClientEnv<'a>,
    target: CancelTarget,
    id: &'a str,
}

fn client_env(session_id: &str) -> ClientEnv<'_> {
    ClientEnv {
        session_id,
        event_id: Ulid::new().to_string(),
        at_ms: now_ms(),
    }
}

fn wire_audio_open(session_id: &str, open: &AudioOpenMsg) -> String {
    serde_json::to_string(&WireAudioOpen {
        kind: "audio.open",
        env: client_env(session_id),
        stream_id: &open.stream_id,
        direction: open.direction,
        capture_id: open.capture_id.as_deref(),
        codec: open.codec,
        sample_rate: open.sample_rate,
        channels: open.channels,
    })
    .expect("wire audio.open serialize")
}

fn wire_audio_end(session_id: &str, stream_id: &str, last_chunk_seq: u32) -> String {
    serde_json::to_string(&WireAudioEnd {
        kind: "audio.end",
        env: client_env(session_id),
        stream_id,
        last_chunk_seq,
    })
    .expect("wire audio.end serialize")
}

fn wire_cancel(session_id: &str, target: CancelTarget, id: &str) -> String {
    serde_json::to_string(&WireCancel {
        kind: "control.cancel",
        env: client_env(session_id),
        target,
        id,
    })
    .expect("wire cancel serialize")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
