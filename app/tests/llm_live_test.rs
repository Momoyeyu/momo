//! 真实 LLM smoke：默认 #[ignore]，需 MOMO_LLM_API_KEY + MOMO_LLM_ENDPOINT。
//! 运行：MOMO_LLM_ENDPOINT=https://api.deepseek.com/v1 MOMO_LLM_API_KEY=sk-... \
//!   MOMO_LLM_MODEL=deepseek-chat cargo test --test llm_live_test -- --ignored --nocapture

use futures_util::StreamExt;
use momo_app::config::SecretString;
use momo_app::pipeline::llm::{ChatRequest, LlmClient, OpenAiCompatLlm};
use momo_app::session::history::{ChatMessage, Role};
use std::env;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use url::Url;

fn live_llm() -> Option<OpenAiCompatLlm> {
    let key = env::var("MOMO_LLM_API_KEY").ok().filter(|k| !k.is_empty())?;
    let endpoint =
        env::var("MOMO_LLM_ENDPOINT").unwrap_or_else(|_| "https://api.deepseek.com/v1".into());
    let model = env::var("MOMO_LLM_MODEL").unwrap_or_else(|_| "deepseek-chat".into());
    Some(OpenAiCompatLlm::new(
        Url::parse(&endpoint).unwrap(),
        model,
        Some(SecretString::new(key)),
        Duration::from_secs(30),
    ))
}

fn req() -> ChatRequest {
    ChatRequest {
        model: String::new(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "从1数到10，每个数字单独一行，不要别的内容".into(),
            partial: false,
        }],
        stream: true,
    }
}

#[tokio::test]
#[ignore = "requires live LLM endpoint + MOMO_LLM_API_KEY"]
async fn live_token_stream_ordered() {
    let Some(llm) = live_llm() else {
        eprintln!("skip: MOMO_LLM_API_KEY unset");
        return;
    };
    let t0 = Instant::now();
    let mut stream = llm.chat_stream(req(), CancellationToken::new()).await.unwrap();
    let mut tokens = Vec::new();
    let mut first = None;
    while let Some(item) = stream.next().await {
        let tok = item.unwrap();
        if first.is_none() {
            first = Some(t0.elapsed());
        }
        tokens.push(tok);
    }
    let text = tokens.join("");
    eprintln!(
        "first_token={:?} total={:?} chunks={} text={:?}",
        first.unwrap(),
        t0.elapsed(),
        tokens.len(),
        text
    );
    assert!(text.contains('1') && text.contains('0'), "expected digits: {text}");
}

#[tokio::test]
#[ignore = "requires live LLM endpoint + MOMO_LLM_API_KEY"]
async fn live_cancel_mid_stream() {
    let Some(llm) = live_llm() else {
        eprintln!("skip: MOMO_LLM_API_KEY unset");
        return;
    };
    let cancel = CancellationToken::new();
    let mut stream = llm.chat_stream(req(), cancel.clone()).await.unwrap();
    let first = stream.next().await.unwrap().unwrap();
    eprintln!("first chunk: {first:?}");
    cancel.cancel();
    let rest: Vec<_> = stream.collect().await;
    eprintln!("chunks after cancel: {}", rest.len());
    assert!(rest.is_empty(), "stream should end promptly on cancel");
}
