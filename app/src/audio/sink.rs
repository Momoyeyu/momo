//! `AudioSink`：按 `sentence.index` 严格保序把 `AudioChunk` 封装为二进制
//! 音频帧推给 `conn_tx`。
//!
//! 播放时机在端侧（前端调度），sink 只管按序下发与统计"已播出文本"——
//! 已播出界限以 `playback.progress` 回执为准（protocol 必需字段表）。
//! 下行音频永不静默丢帧；每回合缓冲上限 96 帧（≈4.8s），溢出
//! `error{BACKPRESSURE_OVERFLOW}` + `control.cancel{turn}`。

use bytes::Bytes;
use tokio::sync::mpsc;
use tracing::warn;
use ulid::Ulid;

use crate::{
    error::AppError,
    pipeline::tts::AudioChunk,
    proto::codec::{AudioHeader, Direction, FLAG_EOS},
    ws::conn::OutboundFrame,
};

/// 每回合下行音频缓冲上限（帧）。
pub const DOWNLINK_BUFFER_FRAMES: usize = 96;

/// 下行音频排队/下发器。
///
/// 每条 downlink stream 绑定一个 sentence（`audio.open{direction:downlink,
/// sentence_seq}`）：`tag` 变化即开新 stream，`chunk_seq`/`sample_offset`
/// 随之归零（protocol 二进制帧一节）。
#[derive(Debug)]
pub struct AudioSink {
    tx: mpsc::Sender<OutboundFrame>,
    /// 本回合已下发帧数（溢出检测用）。
    #[allow(dead_code)] // TODO(P0): BACKPRESSURE_OVERFLOW 判定
    sent_frames: usize,
    /// 当前 stream；`sentence_seq == 0` 表示尚未开播。
    stream_id: Ulid,
    sentence_seq: u32,
    chunk_seq: u32,
    sample_offset: u32,
}

impl AudioSink {
    pub fn new(tx: mpsc::Sender<OutboundFrame>) -> Self {
        Self {
            tx,
            sent_frames: 0,
            stream_id: Ulid::new(),
            sentence_seq: 0,
            chunk_seq: 0,
            sample_offset: 0,
        }
    }

    /// 把一个 TTS chunk 封装为二进制音频帧下发。
    ///
    /// `tag` = 所属 `output.sentence.seq`，映射到帧头 `sentence_seq`；
    /// `eos` 帧由句边界置位。保序由调用方按 `sentence.index` 串行 push 保证。
    pub async fn push(&mut self, chunk: AudioChunk) -> Result<(), AppError> {
        if chunk.tag != self.sentence_seq {
            // 新 sentence → 新 stream（TODO(P0): 先随 audio.open 绑定）。
            self.stream_id = Ulid::new();
            self.sentence_seq = chunk.tag;
            self.chunk_seq = 0;
            self.sample_offset = 0;
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
        self.tx
            .send(OutboundFrame::Audio(header, chunk.payload))
            .await
            .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        self.sent_frames += 1;
        self.chunk_seq += 1;
        self.sample_offset += samples;
        Ok(())
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

    /// barge-in：停播——清空待播队列并通知 UI（`control.barge_in{applied}`）。
    ///
    /// TODO(P0): 排空 conn_tx 中未写的音频帧；真正清播放缓冲在 UI 侧。
    pub fn stop(&self) {
        warn!("audio sink stop() stub: pending queue drain not implemented (P0)");
    }

    /// 等待已下发数据排空（句边界/回合收尾）。P0 实装。
    pub fn drain(&self) {
        // TODO(P0): drain 句柄——跟踪 conn_tx 内音频帧消费完成。
    }
}
