
use bytes::Bytes;
use ulid::Ulid;

use crate::{
    error::AppError,
    pipeline::tts::AudioChunk,
    ws::conn::OutboundFrame,
};

/// 每回合下行音频缓冲上限（帧）。
pub const DOWNLINK_BUFFER_FRAMES: usize = 96;

///
#[derive(Debug)]
pub struct AudioSink {
}

impl AudioSink {
        Self {
        }
    }

    ///
        if chunk.tag != self.sentence_seq {
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
            .send(OutboundFrame::Audio(header, chunk.payload))
            .await
            .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        self.sent_frames += 1;
        self.chunk_seq += 1;
        self.sample_offset += samples;
        Ok(())
    }

        }
    }
}
