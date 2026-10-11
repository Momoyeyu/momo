
use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use url::Url;


/// OpenAI chat/completions 请求（骨架仅保留必要字段）。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
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

    endpoint: Url,
    model: String,
    api_key: Option<Arc<SecretString>>,
    timeout: Duration,
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
            timeout,
            http: reqwest::Client::new(),
        }
    }

    }
}

#[async_trait]
impl LlmClient for OpenAiCompatLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<ChatStream, AppError> {
    }
}
