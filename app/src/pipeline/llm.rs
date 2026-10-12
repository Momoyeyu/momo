//! `LlmClient` trait + OpenAI-compatible 实现。
//!
//! LLM 走 OpenAI-compatible HTTP SSE 直连大脑端点（ADR-006）：
//! `POST {endpoint}/chat/completions`（`stream:true`），逐 `data:` 行
//! 解析 `choices[0].delta.content` 增量，`data: [DONE]` / EOF 终止。
//! 取消语义统一由 `CancellationToken` 承担：token 触发后流立即结束
//! （不产生错误项），HTTP 连接随 response drop 断开；是否取消由调用方
//! 查 token 判定（barge-in 见 runtime.md §barge-in）。

use std::{pin::Pin, sync::Arc, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    config::{Config, SecretString},
    error::AppError,
    session::history::{ChatMessage, Role},
};

/// OpenAI chat/completions 请求（骨架仅保留必要字段）。
#[derive(Debug, Clone)]
pub struct ChatRequest {
    /// 空串回退到 client 配置的 `MOMO_LLM_MODEL`。
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// 保留字段；`chat_stream` 恒以 `stream:true` 发送。
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

/// OpenAI-compatible 客户端（`MOMO_LLM_*` 端点；DeepSeek 等同协议端点通用）。
#[derive(Debug)]
pub struct OpenAiCompatLlm {
    endpoint: Url,
    model: String,
    api_key: Option<Arc<SecretString>>,
    /// `MOMO_LLM_TIMEOUT_MS`：请求总超时，覆盖 connect/headers/整个流。
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
            endpoint,
            model,
            api_key: api_key.map(Arc::new),
            timeout,
            http: reqwest::Client::new(),
        }
    }

    /// 从生效配置构造（config.md §2.2 `MOMO_LLM_*`）。
    pub fn from_config(config: &Config) -> Self {
        Self::new(
            config.llm_endpoint.clone(),
            config.llm_model.clone(),
            config.llm_api_key.clone(),
            config.llm_timeout,
        )
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// `{endpoint}/chat/completions`；endpoint 尾 `/` 有无均可（`Url::join`
    /// 会把末段当文件覆盖，故手工拼接）。
    fn chat_completions_url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.endpoint.as_str().trim_end_matches('/')
        )
    }
}

#[async_trait]
impl LlmClient for OpenAiCompatLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<ChatStream, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled_err());
        }

        let model = if req.model.is_empty() {
            &self.model
        } else {
            &req.model
        };
        let body = RequestBody {
            model,
            messages: req
                .messages
                .iter()
                .map(|m| WireMessage {
                    role: &m.role,
                    content: &m.content,
                })
                .collect(),
            stream: true,
        };

        let mut rb = self
            .http
            .post(self.chat_completions_url())
            .header("accept", "text/event-stream")
            .timeout(self.timeout)
            .json(&body);
        if let Some(key) = &self.api_key {
            rb = rb.bearer_auth(key.expose());
        }

        let resp = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled_err()),
            resp = rb.send() => resp,
        }
        .map_err(|e| AppError::ModelUnreachable(format!("llm request: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::ModelUnreachable(format!(
                "llm http {status}: {}",
                truncate(&text, 512)
            )));
        }

        Ok(Box::pin(sse_stream(resp, cancel, self.timeout)))
    }
}

fn cancelled_err() -> AppError {
    AppError::Internal("llm request cancelled".into())
}

/// 截断错误响应体（char 边界安全）供诊断日志。
fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// POST /chat/completions 请求体；`partial` 等本地字段不下发。
#[derive(Serialize)]
struct RequestBody<'a> {
    model: &'a str,
    messages: Vec<WireMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'a Role,
    content: &'a str,
}

/// SSE `data:` 行的 JSON chunk（usage/finish_reason 等未知字段忽略）。
#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    error: Option<StreamError>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamError {
    message: Option<String>,
}

/// response body → delta content 流。
///
/// 逐字节缓冲，只在 `\n` 处切行（UTF-8 多字节序列不含 `0x0A`，行边界切割
/// 不会截断字符）。`[DONE]`/EOF/取消正常终止（`None`）；JSON 解析失败、
/// 上游 error 事件、body 错误、总超时各产出一个 `Err` 项后终止。
fn sse_stream(
    resp: reqwest::Response,
    cancel: CancellationToken,
    timeout: Duration,
) -> impl Stream<Item = Result<String, AppError>> + Send {
    let state = SseState {
        body: Box::pin(resp.bytes_stream()),
        buf: Vec::with_capacity(4096),
        cancel,
        deadline: tokio::time::Instant::now() + timeout,
        eof: false,
        finished: false,
    };
    stream::unfold(state, |mut st| async move {
        if st.finished {
            return None;
        }
        loop {
            match st.poll_buf() {
                Step::Delta(d) => return Some((Ok(d), st)),
                Step::Fail(e) => {
                    st.finished = true;
                    return Some((Err(e), st));
                }
                Step::End => return None,
                Step::NeedMore => {}
            }
            tokio::select! {
                biased;
                // 返回 None 即终止，无需再置 finished。
                () = st.cancel.cancelled() => return None,
                () = tokio::time::sleep_until(st.deadline) => {
                    st.finished = true;
                    return Some((
                        Err(AppError::ModelUnreachable("llm stream timed out".into())),
                        st,
                    ));
                }
                chunk = st.body.next(), if !st.eof => match chunk {
                    Some(Ok(bytes)) => st.buf.extend_from_slice(&bytes),
                    Some(Err(e)) => {
                        st.finished = true;
                        return Some((
                            Err(AppError::ModelUnreachable(format!("llm stream: {e}"))),
                            st,
                        ));
                    }
                    None => st.eof = true,
                }
            }
        }
    })
}

struct SseState {
    body: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buf: Vec<u8>,
    cancel: CancellationToken,
    deadline: tokio::time::Instant,
    /// body 已 EOF；`buf` 余量按最后一行处理一次后排空即终止。
    eof: bool,
    /// 流已终止（`[DONE]`/EOF 排空/取消/错误项已产出）。
    finished: bool,
}

/// `poll_buf` 结果：缓冲区内能推进到的下一步。
enum Step {
    /// 产出一个 content delta。
    Delta(String),
    /// 产出错误并终止。
    Fail(AppError),
    /// 正常终止（`[DONE]` / EOF 排空）。
    End,
    /// buf 内无完整行，需要更多字节。
    NeedMore,
}

/// 单行处理结果。
enum Line {
    /// 非 `data:` 行、空 payload 或无 content 的 chunk，跳过。
    Skip,
    Delta(String),
    /// `data: [DONE]`。
    Done,
    Fail(AppError),
}

impl SseState {
    /// 从 buf 取一个完整行（去 `\n`/`\r\n`）；EOF 后余量作为末行返回一次。
    fn next_line(&mut self) -> Option<Vec<u8>> {
        if let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            debug_assert_eq!(line.pop(), Some(b'\n'));
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Some(line);
        }
        if self.eof && !self.buf.is_empty() {
            return Some(std::mem::take(&mut self.buf));
        }
        None
    }

    /// 排空 buf 内所有完整行，直到产出 delta/错误/终止或需要更多字节。
    fn poll_buf(&mut self) -> Step {
        loop {
            let Some(line) = self.next_line() else {
                return if self.eof { Step::End } else { Step::NeedMore };
            };
            match self.handle_line(&line) {
                Line::Skip => continue,
                Line::Delta(d) => return Step::Delta(d),
                Line::Done => return Step::End,
                Line::Fail(e) => return Step::Fail(e),
            }
        }
    }

    fn handle_line(&mut self, line: &[u8]) -> Line {
        let Ok(text) = std::str::from_utf8(line) else {
            return Line::Fail(AppError::Internal("llm stream: invalid utf-8".into()));
        };
        // event:/id:/retry:/注释/空行一律忽略。
        let Some(payload) = text.trim_start().strip_prefix("data:") else {
            return Line::Skip;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            return Line::Skip;
        }
        if payload == "[DONE]" {
            return Line::Done;
        }
        let chunk: StreamChunk = match serde_json::from_str(payload) {
            Ok(c) => c,
            Err(e) => {
                return Line::Fail(AppError::Internal(format!(
                    "llm stream: bad chunk json: {e}"
                )));
            }
        };
        if let Some(err) = chunk.error {
            let msg = err.message.unwrap_or_else(|| "unknown".into());
            return Line::Fail(AppError::ModelUnreachable(format!(
                "llm stream error: {msg}"
            )));
        }
        // role-only / finish_reason / usage chunk 无 content，跳过。
        match chunk
            .choices
            .into_iter()
            .find_map(|c| c.delta.content.filter(|s| !s.is_empty()))
        {
            Some(d) => Line::Delta(d),
            None => Line::Skip,
        }
    }
}
