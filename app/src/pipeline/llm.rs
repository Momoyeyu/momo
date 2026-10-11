//! `LlmClient` trait + OpenAI-compatible 实现。
//!
//! LLM 走 OpenAI-compatible HTTP SSE 直连大脑端点（ADR-006；
//! P0 手写 reqwest SSE，可控优先）。取消语义统一由 `CancellationToken` 承担。

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use futures_util::Stream;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{config::SecretString, error::AppError, session::history::ChatMessage};

/// OpenAI chat/completions 请求（骨架仅保留必要字段）。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// 温度等采样参数后续按需扩展。
    pub stream: bool,
}

/// token 流：增量文本 delta。
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ChatStream, AppError>;
}

/// OpenAI-compatible 客户端（`MOMO_LLM_*` 端点）。
#[derive(Debug)]
pub struct OpenAiCompatLlm {
    #[allow(dead_code)] // TODO(P0): POST {endpoint}/chat/completions SSE
    endpoint: Url,
    model: String,
    #[allow(dead_code)] // TODO(P0): Authorization: Bearer（有 key 时）
    api_key: Option<Arc<SecretString>>,
    #[allow(dead_code)] // TODO(P0): 请求总超时
    timeout: Duration,
    #[allow(dead_code)] // TODO(P0): rustls reqwest client
    http: reqwest::Client,
}

impl OpenAiCompatLlm {
    pub fn new(
        endpoint: Url,
        model: String,
        api_key: Option<SecretString>,
        timeout: Duration,
    ) -> Self {
        Self {
            endpoint,
            model,
            api_key: api_key.map(Arc::new),
            timeout,
            http: reqwest::Client::new(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        _cancel: CancellationToken,
    ) -> Result<ChatStream, AppError> {
        // TODO(P0): POST {endpoint}/chat/completions {"stream":true}；
        // 逐行解析 `data: {..delta.content}`；cancel 时 abort 请求。
        let _ = req;
        Err(AppError::Internal(
            "LlmClient::chat_stream not implemented yet (P0)".into(),
        ))
    }
}
