//! `AudioSink`：按 `sentence.index` 保序把 `AudioChunk` 封装为二进制帧推 `conn_tx`。
//! 有界命令队列 + worker 串行走完 `audio.open` → 帧 → `audio.end`；`stop` 丢弃
//! 未下发部分，`drain` 等排空；上限 96 帧 ≈ 4.8s，溢出回 `BACKPRESSURE_OVERFLOW`
//! + `control.cancel{turn}`。

use bytes::Bytes;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use ulid::Ulid;

use crate::{
    error::AppError,
    pipeline::tts::AudioChunk,
    proto::{
        codec::{AudioHeader, Direction, FLAG_EOS},
        message::{
            AudioCodec, AudioEndMsg, AudioOpenMsg, CancelTarget, ControlCancelMsg, ErrorCode,
            ErrorMsg, Msg, ServerEnv,
        },
    },
    ws::conn::OutboundFrame,
};

/// 每回合下行音频缓冲上限（帧）。
pub const DOWNLINK_BUFFER_FRAMES: usize = 96;
/// 下行采样率（protocol：24kHz mono pcm_s16le）。
const DOWNLINK_SAMPLE_RATE: u32 = 24000;

/// worker 内部命令；有界 mpsc 本身就是积压队列。
enum SinkCmd {
    /// 一个待封装下发的 TTS chunk。
    Chunk(AudioChunk),
    /// 排空哨兵：worker 处理完此前全部 chunk 后回 ack。
    Drain(oneshot::Sender<()>),
}

/// 下行音频排队/下发器（每回合一个，绑定 `turn_id`）。
///
/// 每条 downlink stream 绑定一个 sentence：`chunk.tag` 变化即关闭旧 stream
/// （`audio.end`）并以新 `stream_id` 发 `audio.open`。
#[derive(Debug)]
pub struct AudioSink {
    /// conn_tx 出站；停播指令/error 帧直接经它下发，不排内部数据队列。
    out_tx: mpsc::Sender<OutboundFrame>,
    /// 内部下发队列，容量 = [`DOWNLINK_BUFFER_FRAMES`]。
    cmd_tx: mpsc::Sender<SinkCmd>,
    /// 停播信号：worker 退出后未播 chunk 随 channel 丢弃。
    stop: CancellationToken,
    turn_id: String,
    env: ServerEnv,
    /// worker task 句柄（`stop`/`Drop` 不负责 join，task 自行退出）。
    worker: JoinHandle<()>,
}

impl AudioSink {
    /// 开每回合下发器：spawn worker 串行消费内部队列。
    pub fn spawn(out_tx: mpsc::Sender<OutboundFrame>, turn_id: String, env: ServerEnv) -> Self {
        let (cmd_tx, rx) = mpsc::channel(DOWNLINK_BUFFER_FRAMES);
        let stop = CancellationToken::new();
        let worker = tokio::spawn(run_sink(
            rx,
            out_tx.clone(),
            stop.clone(),
            turn_id.clone(),
            env.clone(),
        ));
        Self {
            out_tx,
            cmd_tx,
            stop,
            turn_id,
            env,
            worker,
        }
    }

    /// 把 TTS chunk 排入下发队列（`tag` = 所属 `output.sentence.seq`）。
    /// 积压达 [`DOWNLINK_BUFFER_FRAMES`] → `error{BACKPRESSURE_OVERFLOW}`
    /// + `control.cancel{turn}` + `Err`（不静默丢帧）。
    pub fn push(&self, chunk: AudioChunk) -> Result<(), AppError> {
        if self.stop.is_cancelled() {
            return Err(AppError::Internal("audio sink stopped".into()));
        }
        match self.cmd_tx.try_send(SinkCmd::Chunk(chunk)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow();
                Err(AppError::BackpressureOverflow(format!(
                    "downlink buffer exceeded {DOWNLINK_BUFFER_FRAMES} frames"
                )))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(AppError::Internal("audio sink closed".into()))
            }
        }
    }

    /// 便捷构造一个下行 chunk。
    pub fn chunk(seq: u32, tag: u32, payload: Bytes, eos: bool) -> AudioChunk {
        AudioChunk {
            seq,
            tag,
            payload,
            eos,
        }
    }

    /// 积压溢出：`error{BACKPRESSURE_OVERFLOW}` + `control.cancel{turn}`。
    fn overflow(&self) {
        let env = self.env.next();
        self.emit(Msg::Error(ErrorMsg {
            session_id: Some(env.session_id.clone()),
            event_id: Some(env.event_id.clone()),
            at_ms: Some(env.at_ms),
            event_seq: Some(env.event_seq),
            code: ErrorCode::BackpressureOverflow,
            message: format!("downlink buffer exceeded {DOWNLINK_BUFFER_FRAMES} frames"),
        }));
        self.emit(Msg::ControlCancel(ControlCancelMsg {
            env: self.env.next(),
            target: CancelTarget::Turn,
            id: self.turn_id.clone(),
        }));
    }

    /// 控制帧投递：`conn_tx` 满时由独立 task 等容量送达 ——
    /// 控制消息不静默丢弃，也不阻塞生产者（数据积压不能卡住 Stop/cancel）。
    fn emit(&self, msg: Msg) {
        match self.out_tx.try_send(OutboundFrame::Json(msg)) {
            Ok(()) | Err(mpsc::error::TrySendError::Closed(_)) => {}
            Err(mpsc::error::TrySendError::Full(frame)) => {
                let tx = self.out_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(frame).await;
                });
            }
        }
    }

    /// barge-in：立即停播。
    ///
    /// worker 退出使未播 chunk 随队列丢弃；`control.cancel{target:turn}`
    /// 作为停播指令直接下发（控制通道不排在数据队列后）。幂等。
    pub fn stop(&self) {
        if self.stop.is_cancelled() {
            return;
        }
        self.stop.cancel();
        self.emit(Msg::ControlCancel(ControlCancelMsg {
            env: self.env.next(),
            target: CancelTarget::Turn,
            id: self.turn_id.clone(),
        }));
    }

    /// worker 是否已退出（stop 后未播队列随之丢弃）。
    pub fn worker_done(&self) -> bool {
        self.worker.is_finished()
    }

    /// 回合收尾：等已下发 chunk 全部进 `conn_tx`，并按协议关闭
    /// stream（`audio.end`）后才算播完。
    pub async fn drain(&self) -> Result<(), AppError> {
        if self.stop.is_cancelled() {
            return Ok(());
        }
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SinkCmd::Drain(tx))
            .await
            .map_err(|_| AppError::Internal("audio sink closed".into()))?;
        rx.await
            .map_err(|_| AppError::Internal("audio sink worker gone".into()))
    }
}

/// worker 的 stream 态：一 sentence 一 stream。
#[derive(Debug, Default)]
struct StreamState {
    /// `sentence_seq == 0` 表示当前无 open stream。
    sentence_seq: u32,
    stream_id: Ulid,
    chunk_seq: u32,
    sample_offset: u32,
    /// 本回合已下发帧数（统计/日志）。
    sent_frames: usize,
}

/// 下发 worker：`biased` select 让 `stop` 优先于数据，立即生效。
async fn run_sink(
    mut rx: mpsc::Receiver<SinkCmd>,
    out_tx: mpsc::Sender<OutboundFrame>,
    stop: CancellationToken,
    turn_id: String,
    env: ServerEnv,
) {
    let mut st = StreamState::default();
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => break,
            cmd = rx.recv() => match cmd {
                Some(SinkCmd::Chunk(chunk)) => {
                    if let Err(err) = st.push_chunk(chunk, &out_tx, &env, &turn_id).await {
                        warn!(%turn_id, %err, "audio sink send failed, worker exit");
                        return;
                    }
                }
                Some(SinkCmd::Drain(ack)) => {
                    st.close(&out_tx, &env).await;
                    let _ = ack.send(());
                }
                // 全部 sender 掉落（sink 被 drop）：按协议收尾 open stream。
                None => {
                    st.close(&out_tx, &env).await;
                    return;
                }
            },
        }
    }
    debug!(%turn_id, sent = st.sent_frames, "audio sink stopped");
}

impl StreamState {
    /// chunk → 二进制帧；`tag` 变化先 `audio.end` 旧 stream 再 `audio.open`。
    async fn push_chunk(
        &mut self,
        chunk: AudioChunk,
        out_tx: &mpsc::Sender<OutboundFrame>,
        env: &ServerEnv,
        turn_id: &str,
    ) -> Result<(), AppError> {
        if chunk.tag != self.sentence_seq {
            self.close(out_tx, env).await;
            self.stream_id = Ulid::new();
            self.sentence_seq = chunk.tag;
            self.chunk_seq = 0;
            self.sample_offset = 0;
            let (env, event_seq) = env.envelope();
            let open = Msg::AudioOpen(AudioOpenMsg {
                env,
                event_seq: Some(event_seq),
                direction: Direction::Downlink,
                capture_id: None,
                turn_id: Some(turn_id.to_string()),
                sentence_seq: Some(chunk.tag),
                codec: AudioCodec::PcmS16le,
                sample_rate: DOWNLINK_SAMPLE_RATE,
                channels: 1,
            });
            out_tx
                .send(OutboundFrame::Json(open))
                .await
                .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        }
        let header = AudioHeader {
            direction: Direction::Downlink,
            flags: if chunk.eos { FLAG_EOS } else { 0 },
            stream_id: self.stream_id,
            chunk_seq: self.chunk_seq,
            sentence_seq: chunk.tag,
            sample_offset: self.sample_offset,
        };
        let samples = (chunk.payload.len() / 2) as u32; // mono pcm_s16le
        out_tx
            .send(OutboundFrame::Audio(header, chunk.payload))
            .await
            .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        self.sent_frames += 1;
        self.chunk_seq += 1;
        self.sample_offset += samples;
        Ok(())
    }

    /// 按协议显式结束当前 stream（`audio.end`），无 open stream 时空操作。
    async fn close(&mut self, out_tx: &mpsc::Sender<OutboundFrame>, env: &ServerEnv) {
        if self.sentence_seq == 0 {
            return;
        }
        let (env, event_seq) = env.envelope();
        let end = Msg::AudioEnd(AudioEndMsg {
            env,
            event_seq: Some(event_seq),
            last_chunk_seq: self.chunk_seq.saturating_sub(1),
        });
        let _ = out_tx.send(OutboundFrame::Json(end)).await;
        self.sentence_seq = 0;
    }
}
