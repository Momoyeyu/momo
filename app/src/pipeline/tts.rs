//! `TtsClient` trait + WS 流式实现（momo-ws v1 客户端）。
//!
//! 编排器作为客户端连 momo-svc-tts（`hello.client=orchestrator`）；
//! 逐句并发上限 2（当前句+预取下一句），barge-in 时发 `control.cancel`。
//! PCM 格式：s16le 24kHz mono chunk。

use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{error::AppError, pipeline::splitter::Sentence};

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
        sentence: Sentence,
        cancel: CancellationToken,
    ) -> Result<AudioStream, AppError>;
}

/// momo-ws v1 流式 TTS 客户端（`MOMO_TTS_ENDPOINT`）。
#[derive(Debug)]
pub struct MomoWsTts {
    #[allow(dead_code)] // TODO(P0): ws:// connect + hello(client=orchestrator)
    endpoint: Url,
}

impl MomoWsTts {
    pub fn new(endpoint: Url) -> Self {
        Self { endpoint }
    }
}

#[async_trait]
impl TtsClient for MomoWsTts {
    async fn synth_stream(
        &self,
        _sentence: Sentence,
        _cancel: CancellationToken,
    ) -> Result<AudioStream, AppError> {
        // TODO(P0): 连 TTS ws → hello/hello.ok → output.sentence → audio.chunk 流；
        // cancel 时发 control.cancel{target:turn} 并断开。
        Err(AppError::Internal(
            "TtsClient::synth_stream not implemented yet (P0)".into(),
        ))
    }
}
