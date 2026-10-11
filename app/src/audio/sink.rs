
use bytes::Bytes;

use crate::{
    error::AppError,
    pipeline::tts::AudioChunk,
    ws::conn::OutboundFrame,
};

pub const DOWNLINK_BUFFER_FRAMES: usize = 96;

#[derive(Debug)]
pub struct AudioSink {
}

impl AudioSink {
    }

    ///
        let header = AudioHeader {
            flags: if chunk.eos { FLAG_EOS } else { 0 },
        };
            .send(OutboundFrame::Audio(header, chunk.payload))
            .await
            .map_err(|_| AppError::Internal("conn_tx closed".into()))?;
        self.sent_frames += 1;
        Ok(())
    }

        }
    }
}
