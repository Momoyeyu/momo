//! `AsrClient` trait：上行音频转发 + `input.audio.segment` 接收。
//!
//! 编排器作为客户端连 momo-svc-asr（momo-ws v1）：UI 上行 `audio.chunk`
//! 原样转发，ASR 回报 `input.vad` / `input.audio.segment`（protocol §6）。

use async_trait::async_trait;
use bytes::Bytes;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{error::AppError, proto::message::EmotionTag};

/// 待转写的 VAD 音频段（PCM s16le 16kHz mono，协商见 `hello.audio.uplink`）。
#[derive(Debug)]
pub struct AudioSegment {
    /// `s_` 前缀段 id，与 `control.cancel{target:segment}` 对账。
    pub id: String,
    pub pcm: Bytes,
    pub duration_ms: u64,
}

/// ASR 转写结果（对应 `input.audio.segment`）。
#[derive(Debug, Clone)]
pub struct Transcript {
    pub segment_id: String,
    pub text: String,
    /// `zh/en/ja/ko/yue`。
    pub lang: String,
    pub speech_emotion: Option<EmotionTag>,
    pub is_final: bool,
}

#[async_trait]
pub trait AsrClient: Send + Sync {
    async fn transcribe(&self, seg: AudioSegment) -> Result<Transcript, AppError>;
}

/// momo-ws v1 ASR 客户端（`MOMO_ASR_ENDPOINT`）。
#[derive(Debug)]
pub struct MomoWsAsr {
    #[allow(dead_code)] // TODO(P2): ws:// connect + hello(client=orchestrator)
    endpoint: Url,
}

impl MomoWsAsr {
    pub fn new(endpoint: Url) -> Self {
        Self { endpoint }
    }

    /// 上行音频转发（2s 缓冲丢旧帧，protocol §7）。P2 实装。
    pub fn uplink(&self, _cancel: CancellationToken) {
        // TODO(P2): audio.chunk 上行转发 + input.vad/input.audio.segment 回流。
    }
}

#[async_trait]
impl AsrClient for MomoWsAsr {
    async fn transcribe(&self, seg: AudioSegment) -> Result<Transcript, AppError> {
        // TODO(P2): 经 uplink 通道送段，等 input.audio.segment{final}。
        let _ = seg;
        Err(AppError::Internal(
            "AsrClient::transcribe not implemented yet (P2)".into(),
        ))
    }
}
