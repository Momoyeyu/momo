//! `AudioSink`：按 `sentence.index` 严格保序把 `AudioChunk` 封装为二进制
//! `audio.chunk` 帧推给 `conn_tx`。
//!
//! 播放时机在端侧（前端调度），sink 只管按序下发与统计"已播出文本"——
//! 已播出界限以前端回执 `played_seq` 或已发送帧数为准（protocol §6/§7）。
//! 下行音频永不静默丢帧；每回合缓冲上限 96 帧（≈4.8s），溢出
//! `error{BACKPRESSURE_OVERFLOW}` + `control.cancel{turn}`。

use bytes::Bytes;
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
    error::AppError,
    pipeline::tts::AudioChunk,
    proto::codec::{AudioHeader, FLAG_EOS, STREAM_DOWNLINK},
    ws::conn::OutboundFrame,
};

/// 每回合下行音频缓冲上限（帧，protocol §7）。
pub const DOWNLINK_BUFFER_FRAMES: usize = 96;

/// 下行音频排队/下发器。
#[derive(Debug)]
pub struct AudioSink {
    tx: mpsc::Sender<OutboundFrame>,
    /// 本回合已下发帧数（溢出检测用）。
    #[allow(dead_code)] // TODO(P0): BACKPRESSURE_OVERFLOW 判定
    sent_frames: usize,
}

impl AudioSink {
    pub fn new(tx: mpsc::Sender<OutboundFrame>) -> Self {
        Self { tx, sent_frames: 0 }
    }

    /// 把一个 TTS chunk 封装为 `audio.chunk` 帧下发。
    ///
    /// `tag` = 所属 `output.sentence.seq`；`eos` 帧由句边界置位。
    /// 保序由调用方按 `sentence.index` 串行 push 保证。
    pub async fn push(&mut self, chunk: AudioChunk) -> Result<(), AppError> {
        let header = AudioHeader {
            stream: STREAM_DOWNLINK,
            flags: if chunk.eos { FLAG_EOS } else { 0 },
            seq: chunk.seq,
            tag: chunk.tag,
        };
        self.tx
            .send(OutboundFrame::Audio(header, chunk.payload))
            .await
            .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        self.sent_frames += 1;
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
