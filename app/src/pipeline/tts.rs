
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use tokio_util::sync::CancellationToken;
use url::Url;


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

#[async_trait]
pub trait TtsClient: Send + Sync {
    async fn synth_stream(
        &self,
        cancel: CancellationToken,
    ) -> Result<AudioStream, AppError>;
}

pub struct MomoWsTts {
    endpoint: Url,
}

impl MomoWsTts {
    pub fn new(endpoint: Url) -> Self {
    }
}

#[async_trait]
impl TtsClient for MomoWsTts {
    async fn synth_stream(
        &self,
    ) -> Result<AudioStream, AppError> {
    }
}
