//! `TtsClient` trait + WS 流式实现（momo-ws v2 客户端，握手/帧流见 protocol.md §2/§4/§6）。
//! 在途合成经 [`SENT_QUEUE_CAP`] 信号量限流（runtime.md `sentence_q`，容量 8）；
//! `cancel` 发 `control.cancel{target:turn}`，迟到帧按 stale stream 丢弃。
//! PCM：s16le 24kHz mono。

use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, stream};
use serde::Serialize;
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use ulid::Ulid;
use url::Url;

use crate::{
    error::AppError,
    proto::{
        codec::{AudioHeader, Direction},
        message::{CancelTarget, EmotionTag, Msg, ServerRole},
    },
    ws::{
        client::{self, ClientConn, Inbound},
        conn::OutboundFrame,
    },
};

/// `sentence_q` 容量（runtime.md bounded queue）：在途 pending+active 合成上限。
pub const SENT_QUEUE_CAP: usize = 8;

/// 下行音频格式固定值（protocol §6）。
const DOWNLINK_SAMPLE_RATE: u32 = 24000;

/// TTS 音频块：封装进二进制 `audio.chunk` 帧下发。
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// 流内单调递增序号（帧头 `seq`）。
    pub seq: u32,
    /// 所属 `output.sentence.seq`（帧头 `tag`）。
    pub tag: u32,
    /// `pcm_s16le` 载荷。
    pub payload: Bytes,
    /// 该句最后一帧（帧头 `flags` EOS）。
    pub eos: bool,
}

pub type AudioStream = Pin<Box<dyn Stream<Item = Result<AudioChunk, AppError>> + Send>>;

/// 合成请求：`output.sentence` 必需字段的全部来源（protocol §2）。
#[derive(Debug, Clone)]
pub struct SynthRequest {
    /// 所属回合 id（`t_` 前缀）；`control.cancel{target:turn,id}` 指回它。
    pub turn_id: String,
    /// 合成语言（`zh`/`en`/`ja`/`auto`）。
    pub lang: String,
    pub sentence: Sentence,
}

#[async_trait]
pub trait TtsClient: Send + Sync {
    async fn synth_stream(
        &self,
        req: SynthRequest,
        cancel: CancellationToken,
    ) -> Result<AudioStream, AppError>;

    /// barge-in 扇出：`control.cancel{target:turn,id}`（不等 ACK）；
    /// 无活链路时无可取消对象，默认无操作。
    async fn cancel_turn(&self, _turn_id: &str) -> Result<(), AppError> {
        Ok(())
    }
}

/// momo-ws v2 流式 TTS 客户端（`MOMO_TTS_ENDPOINT`）。
///
/// 链路懒建立、断开后下一次 `synth_stream` 自动重连；多路复用同一条
/// 连接，`audio.open.stream_id` 路由二进制帧到对应合成（protocol §6）。
pub struct MomoWsTts {
    endpoint: Url,
}

impl fmt::Debug for MomoWsTts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    }
}

impl MomoWsTts {
    pub fn new(endpoint: Url) -> Self {
        Self {
        }
    }

    /// 提前建连 + 握手（bring-up 场景先暴露连通性/角色错误）。
    pub async fn connect(&self) -> Result<(), AppError> {
        self.link().await.map(|_| ())
    }

    /// 取活链路；无或已死则重连（并发调用由 mutex 串行化，后者复用新链路）。
    async fn link(&self) -> Result<Arc<TtsLink>, AppError> {
            if link.alive.load(Ordering::Acquire) {
                return Ok(link.clone());
            }
        }
        let link = TtsLink::spawn(conn);
        Ok(link)
    }
}

#[async_trait]
impl TtsClient for MomoWsTts {
    async fn synth_stream(
        &self,
        req: SynthRequest,
        cancel: CancellationToken,
    ) -> Result<AudioStream, AppError> {
        if cancel.is_cancelled() {
            return Err(AppError::Internal("tts synth cancelled".into()));
        }
        let link = self.link().await?;
        // `sentence_q` 有界：许可耗尽即阻塞，等效暂停上游拉取（runtime.md）。
        let permit = link
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Internal("tts scheduler closed".into()))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();
        let entry = StreamEntry {
            turn_id: req.turn_id.clone(),
            seq: req.sentence.index,
            tx,
            _permit: permit,
            _done: done_tx,
            next_seq: 0,
            next_offset: 0,
            eos_seen: false,
        };

        // 「登记 pending → 发 output.sentence」串行化，保证挂起表序 = wire 序
        // （服务端对 BAD_MESSAGE 类拒绝按序回 error，路由据此归因）。
        let wire = wire_sentence(&link.session_id, &req);
        {
            let _guard = link.send_mu.lock().await;
            link.router.lock().await.pending.push(entry);
            if let Err(err) = link.tx.send(OutboundFrame::JsonText(wire)).await {
                link.router
                    .lock()
                    .await
                    .remove(&req.turn_id, req.sentence.index);
                link.alive.store(false, Ordering::Release);
                return Err(AppError::ModelUnreachable(format!("tts link tx: {err}")));
            }
        }

        // cancel → control.cancel{target:turn,id} + 摘条目（迟到帧 stale 丢弃）；
        // 条目被路由正常移除时 done 触发 watcher 退出，任务不泄漏。
        {
            let link = link.clone();
            let turn_id = req.turn_id.clone();
            let seq = req.sentence.index;
            tokio::spawn(async move {
                tokio::select! {
                    () = cancel.cancelled() => {
                        let _ = link
                            .tx
                            .send(OutboundFrame::JsonText(wire_cancel(
                                &link.session_id,
                                &turn_id,
                            )))
                            .await;
                        link.router.lock().await.cancel(&turn_id, seq);
                    }
                    _ = done_rx => {}
                }
            });
        }

        Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })))
    }

    /// barge-in 扇出：活链路上发 `control.cancel{target:turn}`；尽力而为。
    async fn cancel_turn(&self, turn_id: &str) -> Result<(), AppError> {
            return Ok(());
        };
        let _ = link
            .tx
            .send(OutboundFrame::JsonText(wire_cancel(
                &link.session_id,
                turn_id,
            )))
            .await;
        Ok(())
    }
}

/// 单条 svc-tts 链路：出站队列 + 流路由表 + `sentence_q` 信号量。
struct TtsLink {
    session_id: String,
    tx: mpsc::Sender<OutboundFrame>,
    router: Mutex<Router>,
    /// `sentence_q` 许可；随条目移除（audio.end/失败/取消/断连）释放。
    permits: Arc<Semaphore>,
    /// 串行化「登记 pending → 发 output.sentence」（见 `synth_stream`）。
    send_mu: Mutex<()>,
    /// rx loop 退出/终态事件置 false；`synth_stream` 据此重连。
    alive: AtomicBool,
}

impl TtsLink {
    fn spawn(conn: ClientConn) -> Arc<Self> {
        let link = Arc::new(TtsLink {
            session_id: conn.session_id,
            tx: conn.tx,
            router: Mutex::new(Router::default()),
            permits: Arc::new(Semaphore::new(SENT_QUEUE_CAP)),
            send_mu: Mutex::new(()),
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
            // 通道耗尽 = rx task 已退出，链路判死并终止全部在途合成。
            if let Some(link) = weak.upgrade() {
                link.died(AppError::ModelUnreachable("tts link closed".into()))
                    .await;
            }
        });
        link
    }

    /// 链路终态：标记死链 + 所有在途合成收到错误项后关闭。
    async fn died(&self, err: AppError) {
        self.alive.store(false, Ordering::Release);
        self.router.lock().await.fail_all(err);
    }

    async fn route(&self, item: Inbound) {
        match item {
            Inbound::Msg(Msg::AudioOpen(open)) => self.router.lock().await.bind(&open),
            Inbound::Msg(Msg::AudioEnd(end)) => self.router.lock().await.end(&end),
            Inbound::Msg(Msg::Error(err)) => {
                warn!(code = ?err.code, msg = %err.message, "svc-tts error frame");
            }
            Inbound::Msg(other) => {
                debug!(?other, "unhandled msg on tts link (dropped)");
            }
            Inbound::Audio(header, payload) => self.router.lock().await.deliver(header, payload),
            Inbound::Failed(err) => self.died(err).await,
            Inbound::Closed => {
                self.died(AppError::ModelUnreachable("tts link closed".into()))
                    .await;
            }
        }
    }
}

/// 服务端 `error` 帧 → AppError（保留 code 语义；`message` 给调用方）。
fn server_err(err: &crate::proto::message::ErrorMsg) -> AppError {
    use crate::proto::message::ErrorCode::*;
    match err.code {
        TtsFailed => AppError::TtsFailed(err.message.clone()),
        BadMessage => AppError::BadMessage(err.message.clone()),
        ModelUnreachable => AppError::ModelUnreachable(err.message.clone()),
        _ => AppError::Internal(format!("svc-tts error: {}", err.message)),
    }
}

/// 一条在途合成的下行接收端；条目被移除时 `tx`/`done`/`permit` 一并释放。
struct StreamEntry {
    turn_id: String,
    /// 所属 `output.sentence.seq`。
    seq: u32,
    tx: mpsc::UnboundedSender<Result<AudioChunk, AppError>>,
    /// `sentence_q` 许可（字段持有即占坑，Drop 自动释放）。
    _permit: tokio::sync::OwnedSemaphorePermit,
    /// 通知 cancel watcher 条目已收尾（仅 Drop 语义）。
    _done: oneshot::Sender<()>,
    /// 期望的下一帧 `chunk_seq`（stale stream 规则：重复丢、gap 中止）。
    next_seq: u32,
    /// 期望的下一帧 `sample_offset`。
    next_offset: u32,
    eos_seen: bool,
}

impl StreamEntry {
    /// 注入错误项后关闭流（`tx` 随后随条目移除而 drop）。
    fn fail(&self, err: AppError) {
        let _ = self.tx.send(Err(err));
    }
}

/// 路由表：pending 按发送序（`Vec`）；活动流按 `stream_id` 原始 ULID 索引。
#[derive(Default)]
struct Router {
    /// 已发 `output.sentence`、待 `audio.open` 绑定的合成。
    pending: Vec<StreamEntry>,
    /// `audio.open` 已绑定的下行流（key = 帧头 `stream_id`）。
    streams: HashMap<Ulid, StreamEntry>,
}

impl Router {
    /// `audio.open{downlink}` → pending 提升为活动流。
    fn bind(&mut self, open: &crate::proto::message::AudioOpenMsg) {
        let Some(entry) = self.take_pending(open) else {
            warn!(stream = %open.stream_id, "dropping audio.open for unknown/cancelled sentence");
            return;
        };
        // v2 固定下行格式（protocol §6）；不符视为协议违反。
        if open.codec != crate::proto::message::AudioCodec::PcmS16le
            || open.sample_rate != DOWNLINK_SAMPLE_RATE
            || open.channels != 1
        {
            entry.fail(AppError::Protocol(format!(
                "audio.open unsupported format: {:?} {}Hz {}ch",
                open.codec, open.sample_rate, open.channels
            )));
            return;
        }
        let Some(key) = stream_key(&open.stream_id) else {
            entry.fail(AppError::BadMessage(format!(
                "audio.open bad stream_id {:?}",
                open.stream_id
            )));
            return;
        };
        self.streams.insert(key, entry);
    }

    /// 摘出 `audio.open` 指向的 pending 条目；非法/迟到 open 记日志丢弃。
    fn take_pending(&mut self, open: &crate::proto::message::AudioOpenMsg) -> Option<StreamEntry> {
        if let Err(err) = open.validate() {
            warn!(%err, "invalid audio.open on tts link");
            return None;
        }
        if open.direction != Direction::Downlink {
            warn!("audio.open on tts link must be downlink");
            return None;
        }
        let (turn_id, seq) = (open.turn_id.as_deref().unwrap_or(""), open.sentence_seq?);
        let pos = self
            .pending
            .iter()
            .position(|p| p.turn_id == turn_id && p.seq == seq)?;
        Some(self.pending.remove(pos))
    }

    /// 二进制帧 → `AudioChunk`；stale/重复丢，seq/offset gap 中止该流。
    fn deliver(&mut self, header: AudioHeader, payload: Bytes) {
        let Some(st) = self.streams.get_mut(&header.stream_id) else {
            debug!(stream = %header.stream_id, "dropping frame for unknown/stale stream");
            return;
        };
        if header.direction != Direction::Downlink || header.sentence_seq != st.seq {
            let key = header.stream_id;
            if let Some(st) = self.streams.remove(&key) {
                st.fail(AppError::Protocol(format!(
                    "frame binding mismatch: direction {:?}, sentence_seq {}",
                    header.direction, header.sentence_seq
                )));
            }
            return;
        }
        if st.eos_seen {
            debug!(stream = %header.stream_id, "dropping post-EOS frame");
            return;
        }
        if header.chunk_seq < st.next_seq {
            debug!(stream = %header.stream_id, seq = header.chunk_seq, "dropping duplicate frame");
            return;
        }
        if header.chunk_seq > st.next_seq || header.sample_offset != st.next_offset {
            let key = header.stream_id;
            if let Some(st) = self.streams.remove(&key) {
                st.fail(AppError::Protocol(format!(
                    "audio seq gap: want seq {} offset {}, got seq {} offset {}",
                    st.next_seq, st.next_offset, header.chunk_seq, header.sample_offset
                )));
            }
            return;
        }
        st.next_seq += 1;
        st.next_offset += (payload.len() / 2) as u32;
        if header.eos() {
            st.eos_seen = true;
        }
        if st
            .tx
            .send(Ok(AudioChunk {
                seq: header.chunk_seq,
                tag: header.sentence_seq,
                payload,
                eos: header.eos(),
            }))
            .is_err()
        {
            // 消费端已放弃：移除条目，迟到帧按 stale 丢弃。
            self.streams.remove(&header.stream_id);
        }
    }

    /// `audio.end`：校验 `last_chunk_seq` 与最后接收帧一致后正常收尾。
    fn end(&mut self, end: &crate::proto::message::AudioEndMsg) {
        let Some(key) = stream_key(&end.stream_id) else {
            warn!(stream = %end.stream_id, "audio.end with bad stream_id");
            return;
        };
        let Some(st) = self.streams.remove(&key) else {
            debug!(stream = %end.stream_id, "dropping audio.end for unknown/stale stream");
            return;
        };
        // EOS 帧是最后接收帧：last_chunk_seq 必须等于它（protocol §6）。
        let ok = st.next_seq > 0 && end.last_chunk_seq == st.next_seq - 1;
        if !ok {
            st.fail(AppError::Protocol(format!(
                "audio.end last_chunk_seq {} != last received {}",
                end.last_chunk_seq,
                st.next_seq.saturating_sub(1)
            )));
        }
        drop(st);
    }

    /// `control.cancel` 后摘下该句条目：pending 直接丢，活动流关闭。
    fn cancel(&mut self, turn_id: &str, seq: u32) {
        if let Some(pos) = self
            .pending
            .iter()
            .position(|p| p.turn_id == turn_id && p.seq == seq)
        {
            self.pending.remove(pos);
            return;
        }
        let key = self
            .streams
            .iter()
            .find(|(_, st)| st.turn_id == turn_id && st.seq == seq)
            .map(|(k, _)| *k);
        if let Some(key) = key {
            self.streams.remove(&key);
        }
    }

    /// 服务端 `error` 帧归因 pending：BAD_MESSAGE 按序对应最新发送；
    /// 其余（TTS_FAILED 等异步失败）对应最早未绑定者。无 pending 只记日志。
    fn fail_pending(&mut self, err: AppError) {
        if self.pending.is_empty() {
            return;
        }
        let pos = if matches!(err, AppError::BadMessage(_)) {
            self.pending.len() - 1
        } else {
            0
        };
        self.pending.remove(pos).fail(err);
    }

    /// 摘出指定句的 pending 条目（发送失败回滚用）。
    fn remove(&mut self, turn_id: &str, seq: u32) {
        if let Some(pos) = self
            .pending
            .iter()
            .position(|p| p.turn_id == turn_id && p.seq == seq)
        {
            self.pending.remove(pos);
        }
    }

    /// 链路终态：所有 pending/活动流收到错误项后清空。
    fn fail_all(&mut self, err: AppError) {
        for st in self.pending.drain(..) {
            st.fail(clone_err(&err));
        }
        for (_, st) in self.streams.drain() {
            st.fail(clone_err(&err));
        }
    }
}

/// 同一终态错误分发给多个流时的轻量复制（保留 message 内容与 code 映射）。
fn clone_err(err: &AppError) -> AppError {
    match err {
        AppError::BadMessage(m) => AppError::BadMessage(m.clone()),
        AppError::TtsFailed(m) => AppError::TtsFailed(m.clone()),
        AppError::ModelUnreachable(m) => AppError::ModelUnreachable(m.clone()),
        other => AppError::Internal(other.to_string()),
    }
}

/// `st_ULID` 文本 → 帧头 16B 原始 ULID（protocol §6）。
fn stream_key(stream_id: &str) -> Option<Ulid> {
    Ulid::from_string(stream_id.strip_prefix("st_")?).ok()
}

/// 客户端 envelope（protocol §1 `Envelope`；`event_seq` 仅服务端携带）。
#[derive(Serialize)]
struct ClientEnv<'a> {
    session_id: &'a str,
    event_id: String,
    at_ms: u64,
}

/// `output.sentence` 客户端出线形态（envelope 无 `event_seq`）。
#[derive(Serialize)]
struct WireSentence<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    env: ClientEnv<'a>,
    turn_id: &'a str,
    seq: u32,
    text: &'a str,
    emotion: EmotionTag,
    lang: &'a str,
}

/// `control.cancel{target:turn}` 客户端出线形态。
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

fn wire_sentence(session_id: &str, req: &SynthRequest) -> String {
    serde_json::to_string(&WireSentence {
        kind: "output.sentence",
        env: client_env(session_id),
        turn_id: &req.turn_id,
        seq: req.sentence.index,
        text: &req.sentence.text,
        emotion: req.sentence.emotion.unwrap_or_default().normalized(),
        lang: &req.lang,
    })
    .expect("wire sentence serialize")
}

fn wire_cancel(session_id: &str, turn_id: &str) -> String {
    serde_json::to_string(&WireCancel {
        kind: "control.cancel",
        env: client_env(session_id),
        target: CancelTarget::Turn,
        id: turn_id,
    })
    .expect("wire cancel serialize")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
