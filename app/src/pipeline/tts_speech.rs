//! `TtsClient` 的 OpenAI 对齐实装：provider 接入面 `POST /v1/audio/speech`
//! （protocol.md §11.3；线格式权威定义见 momo-infra provider-protocol.md §2）。
//!
//! 请求-响应式逐句合成（与 [`MomoWsTts`] 的一连接多流复用不同）：一句 =
//! 一次 HTTP POST，chunked `audio/pcm`（s16le 24kHz mono）回流映射为
//! [`AudioChunk`] 流，末块标记 EOS。取消 = abort 在途请求（断连即取消，
//! 无 `control.cancel` 等价物）；`sentence_q` 限流沿用 `SENT_QUEUE_CAP`。

use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt, stream};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{Mutex, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::debug;
use url::Url;

use crate::{
    config::SecretString,
    error::AppError,
    pipeline::tts::{AudioChunk, AudioStream, SENT_QUEUE_CAP, SynthRequest, TtsClient},
    proto::message::EmotionTag,
};

/// OpenAI 对齐 TTS 客户端（endpoint 为 http(s) 时由 `TtsProvider` 选用）。
///
/// 无连接态：endpoint/key 写槽即换连接面；`sentence_q` 信号量限制在途合成
/// 并发（与 `MomoWsTts` 同口径）。
pub struct OpenAiSpeechTts {
    slot: Mutex<SpeechSlot>,
    http: reqwest::Client,
    /// `sentence_q` 许可：每句合成持有至流结束，容量见 runtime.md。
    permits: Arc<Semaphore>,
}

struct SpeechSlot {
    endpoint: Url,
    api_key: Option<SecretString>,
}

impl fmt::Debug for OpenAiSpeechTts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiSpeechTts").finish_non_exhaustive()
    }
}

impl OpenAiSpeechTts {
    pub fn new(endpoint: Url, api_key: Option<SecretString>) -> Self {
        Self {
            slot: Mutex::new(SpeechSlot { endpoint, api_key }),
            http: reqwest::Client::new(),
            permits: Arc::new(Semaphore::new(SENT_QUEUE_CAP)),
        }
    }

    /// 设置面热更新端点：写新端点，下一请求生效（无连接态可丢）。
    pub async fn set_endpoint(&self, endpoint: Url) {
        self.slot.lock().await.endpoint = endpoint;
    }

    /// 当前连接面快照。
    async fn conf(&self) -> (Url, Option<SecretString>) {
        let guard = self.slot.lock().await;
        (guard.endpoint.clone(), guard.api_key.clone())
    }
}

#[async_trait]
impl TtsClient for OpenAiSpeechTts {
    async fn synth_stream(
        &self,
        req: SynthRequest,
        cancel: CancellationToken,
    ) -> Result<AudioStream, AppError> {
        if cancel.is_cancelled() {
            return Err(AppError::Internal("tts synth cancelled".into()));
        }
        // `sentence_q` 有界：许可耗尽即阻塞，等效暂停上游拉取（runtime.md）。
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Internal("tts scheduler closed".into()))?;

        let (endpoint, api_key) = self.conf().await;
        let body = SpeechBody {
            input: req.sentence.text.trim(),
            response_format: "pcm",
            lang: &req.lang,
            emotion: req.sentence.emotion.unwrap_or_default().normalized(),
        };
        let url = speech_url(&endpoint);
        let mut rb = self.http.post(url).json(&body);
        if let Some(key) = &api_key {
            rb = rb.bearer_auth(key.expose());
        }
        debug!(endpoint = %endpoint, turn = %req.turn_id, seq = req.sentence.index, "tts speech request");
        let resp = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                return Err(AppError::Internal("tts synth cancelled".into()));
            }
            resp = rb.send() => resp,
        }
        .map_err(|e| AppError::ModelUnreachable(format!("tts speech request: {e}")))?;

        if !resp.status().is_success() {
            return Err(speech_err(resp).await);
        }
        Ok(Box::pin(pcm_stream(
            resp,
            req.sentence.index,
            cancel,
            permit,
        )))
    }

    // `cancel_turn` 用 trait 默认无操作：本协议取消 = 各句请求随 turn 取消
    // 令牌断开，无跨请求的 turn 级控制通道。
}

/// `POST /v1/audio/speech` 请求体；只发 app 侧确定的字段
/// （`model`/`voice`/`speed` 省略 = 服务端默认，`lang`/`emotion` 为 momo 扩展）。
#[derive(Serialize)]
struct SpeechBody<'a> {
    input: &'a str,
    response_format: &'static str,
    lang: &'a str,
    emotion: EmotionTag,
}

/// speech URL：endpoint path 为 `/`（或空）时补 `/v1/audio/speech`；
/// 已带具体 path 的原样使用（兼容指到完整接口地址的写法）。
fn speech_url(endpoint: &Url) -> String {
    let base = endpoint.as_str().trim_end_matches('/');
    if endpoint.path() == "/" || endpoint.path().is_empty() {
        format!("{base}/v1/audio/speech")
    } else {
        base.to_string()
    }
}

/// 非 2xx 响应 → AppError；取 OpenAI 信封 `error.message`，解析失败回原文截断。
async fn speech_err(resp: reqwest::Response) -> AppError {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let msg = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| truncate(&text, 256).to_string());
    match status.as_u16() {
        400 => AppError::BadMessage(format!("tts speech: {msg}")),
        401 | 403 => AppError::Unauthorized(format!("tts speech: {msg}")),
        503 => AppError::ModelUnreachable(format!("tts speech: {msg}")),
        _ => AppError::TtsFailed(format!("tts speech http {status}: {msg}")),
    }
}

/// 截断错误响应体（char 边界安全）供诊断日志。
fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

/// `audio/pcm` 字节流 → `AudioChunk` 流：lookahead 一块判 EOS（末块
/// `eos=true`，无独立结束帧）；取消/空流/中途错误各有终态。
fn pcm_stream(
    resp: reqwest::Response,
    tag: u32,
    cancel: CancellationToken,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> impl Stream<Item = Result<AudioChunk, AppError>> + Send {
    let state = SpeechState {
        body: Box::pin(resp.bytes_stream()),
        tag,
        next_seq: 0,
        seen_any: false,
        pending: None,
        pending_err: None,
        finished: false,
        cancel,
        _permit: permit,
    };
    stream::unfold(state, |mut st| async move {
        if st.finished {
            return None;
        }
        // 上一轮取「下一块」时遇到流错误：当前块已先产出，本轮报错误后终止。
        if let Some(err) = st.pending_err.take() {
            st.finished = true;
            return Some((Err(err), st));
        }
        // lookahead：拉下一块判当前块是否末块（EOF → eos）。
        if st.pending.is_none() {
            match st.fetch().await {
                Fetch::Chunk(b) => st.pending = Some(b),
                Fetch::Eof => {
                    st.finished = true;
                    // 200 但零音频：该句按合成失败降级（否则 pending-audio 悬挂）。
                    if !st.seen_any {
                        let err = AppError::TtsFailed("tts speech: empty audio".into());
                        return Some((Err(err), st));
                    }
                    return None;
                }
                Fetch::Fail(e) => {
                    st.finished = true;
                    return Some((Err(e), st));
                }
                Fetch::Cancelled => return None,
            }
        }
        let cur = st.pending.take().expect("pending filled");
        st.seen_any = true;
        let eos = match st.fetch().await {
            Fetch::Chunk(b) => {
                st.pending = Some(b);
                false
            }
            Fetch::Eof => true,
            // 当前块仍有效先产出（不带 eos——流被截断非正常收尾），错误下轮报。
            Fetch::Fail(e) => {
                st.pending_err = Some(e);
                false
            }
            Fetch::Cancelled => return None,
        };
        let chunk = AudioChunk {
            seq: st.next_seq,
            tag: st.tag,
            payload: cur,
            eos,
        };
        st.next_seq += 1;
        Some((Ok(chunk), st))
    })
}

/// 拉取下一块 body 的结果（cancel 优先于读）。
enum Fetch {
    Chunk(Bytes),
    Eof,
    Fail(AppError),
    Cancelled,
}

struct SpeechState {
    body: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    /// 所属 `output.sentence.seq`（chunk.tag）。
    tag: u32,
    next_seq: u32,
    /// 已产出过音频块（区分「200 但空响应」）。
    seen_any: bool,
    /// lookahead 缓冲的当前块。
    pending: Option<Bytes>,
    /// 取「下一块」时的流错误（当前块先产出，错误下一轮再报）。
    pending_err: Option<AppError>,
    finished: bool,
    cancel: CancellationToken,
    /// `sentence_q` 许可：字段持有至流结束（Drop 释放）。
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl SpeechState {
    /// 拉一块非空 body chunk（空 chunk 跳过）；取消优先。
    async fn fetch(&mut self) -> Fetch {
        loop {
            tokio::select! {
                biased;
                () = self.cancel.cancelled() => return Fetch::Cancelled,
                item = self.body.next() => match item {
                    Some(Ok(b)) if !b.is_empty() => return Fetch::Chunk(b),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => {
                        return Fetch::Fail(AppError::ModelUnreachable(format!(
                            "tts speech stream: {e}"
                        )));
                    }
                    None => return Fetch::Eof,
                },
            }
        }
    }
}
