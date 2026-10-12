//! LLM SSE client 用例（issue #12 验收）：
//! mock OpenAI-compatible SSE 服务（axum + `text/event-stream`），
//! 断言 token 顺序、请求形状（path/header/body）、`[DONE]` 终止、
//! 错块切分重组、`CancellationToken` 中途取消与 HTTP 错误映射。
//! 全部走 127.0.0.1 回环，无真实外网。

use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use bytes::Bytes;
use futures_util::{StreamExt, stream};
use momo_app::{
    config::SecretString,
    error::AppError,
    pipeline::llm::{ChatRequest, LlmClient, OpenAiCompatLlm},
    session::history::{ChatMessage, Role},
};
use tokio_util::sync::CancellationToken;
use url::Url;

// ---------- mock SSE server ----------

/// 录下的单次请求（header + JSON body）。
#[derive(Debug, Clone)]
struct Recorded {
    headers: HeaderMap,
    body: serde_json::Value,
}

#[derive(Clone)]
struct MockLlm {
    /// 非 2xx 时直接回该状态码。
    status: StatusCode,
    /// 依次写入 response body 的原始字节块（每块一个 TCP write）。
    chunks: Vec<Vec<u8>>,
    /// 每块写入前的间隔；0 = 尽快。
    delay: Duration,
    record: Arc<Mutex<Vec<Recorded>>>,
}

impl MockLlm {
    fn streaming(chunks: Vec<Vec<u8>>, delay: Duration) -> Self {
        Self {
            status: StatusCode::OK,
            chunks,
            delay,
            record: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn failing(status: StatusCode) -> Self {
        Self {
            status,
            chunks: Vec::new(),
            delay: Duration::ZERO,
            record: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

async fn chat_completions(State(mock): State<MockLlm>, req: Request) -> Response {
    let headers = req.headers().clone();
    let body = to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    mock.record.lock().unwrap().push(Recorded {
        headers,
        body: json,
    });

    if !mock.status.is_success() {
        return Response::builder()
            .status(mock.status)
            .body(Body::from("{\"error\":{\"message\":\"boom\"}}"))
            .unwrap();
    }
    let delay = mock.delay;
    let stream = stream::iter(mock.chunks.clone()).then(move |chunk| async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        Ok::<_, Infallible>(Bytes::from(chunk))
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(stream))
        .unwrap()
}

async fn spawn(mock: &MockLlm) -> SocketAddr {
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

// ---------- helpers ----------

fn client_for(addr: SocketAddr, api_key: Option<&str>) -> OpenAiCompatLlm {
    OpenAiCompatLlm::new(
        Url::parse(&format!("http://{addr}/v1")).unwrap(),
        "test-model".into(),
        api_key.map(|k| SecretString::new(k.into())),
        Duration::from_secs(5),
    )
}

fn chat_req() -> ChatRequest {
    ChatRequest {
        model: "test-model".into(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "be brief".into(),
                partial: false,
            },
            ChatMessage {
                role: Role::User,
                content: "hi".into(),
                partial: false,
            },
        ],
        stream: true,
    }
}

/// OpenAI-compatible 的 `data:` 行（content delta chunk）。
fn sse_content(content: &str) -> Vec<u8> {
    format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{content}\"}}}}]}}\n\n").into_bytes()
}

// ---------- tests ----------

#[tokio::test]
async fn tokens_arrive_in_order_and_done_terminates() {
    let mock = MockLlm::streaming(
        vec![
            // role-only 首块无 content，跳过。
            b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n".to_vec(),
            sse_content("你"),
            sse_content("好"),
            sse_content("啊"),
            // finish_reason 块无 content，跳过。
            b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n".to_vec(),
            b"data: [DONE]\n\n".to_vec(),
            // DONE 之后的内容不可达。
            sse_content("不可见"),
        ],
        Duration::ZERO,
    );
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);

    let stream = client
        .chat_stream(chat_req(), CancellationToken::new())
        .await
        .expect("chat_stream");
    let tokens: Vec<String> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()
        .expect("no stream errors");
    assert_eq!(tokens, ["你", "好", "啊"]);
}

#[tokio::test]
async fn sends_auth_header_and_openai_body() {
    let mock = MockLlm::streaming(vec![b"data: [DONE]\n\n".to_vec()], Duration::ZERO);
    let addr = spawn(&mock).await;
    let client = client_for(addr, Some("sk-test-key"));

    let mut stream = client
        .chat_stream(chat_req(), CancellationToken::new())
        .await
        .expect("chat_stream");
    while stream.next().await.is_some() {}

    let recorded = mock.record.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let rec = &recorded[0];
    assert_eq!(
        rec.headers.get("authorization").unwrap(),
        "Bearer sk-test-key"
    );
    assert_eq!(rec.body["model"], "test-model");
    assert_eq!(rec.body["stream"], true);
    assert_eq!(rec.body["messages"][0]["role"], "system");
    assert_eq!(rec.body["messages"][1]["role"], "user");
    // 本地字段（partial）不得下发。
    assert!(rec.body["messages"][0].get("partial").is_none());
    assert!(rec.body["messages"][1].get("partial").is_none());
}

#[tokio::test]
async fn split_data_line_reassembles_mid_utf8() {
    // 一条完整 data 行切成两块，切点落在“你”的多字节序列中间。
    let line = sse_content("你好");
    let split = String::from_utf8_lossy(&line).find('你').unwrap() + 1;
    let mock = MockLlm::streaming(
        vec![
            line[..split].to_vec(),
            line[split..].to_vec(),
            b"data: [DONE]\n\n".to_vec(),
        ],
        Duration::ZERO,
    );
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);

    let stream = client
        .chat_stream(chat_req(), CancellationToken::new())
        .await
        .expect("chat_stream");
    let tokens: Vec<String> = stream
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()
        .expect("no stream errors");
    assert_eq!(tokens, ["你好"]);
}

#[tokio::test]
async fn cancel_mid_stream_ends_stream() {
    // 逐块慢发；客户端读到首 token 后 cancel，后续块不可见。
    let mock = MockLlm::streaming(
        vec![
            sse_content("A"),
            sse_content("B"),
            sse_content("C"),
            b"data: [DONE]\n\n".to_vec(),
        ],
        Duration::from_millis(150),
    );
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);
    let cancel = CancellationToken::new();

    let mut stream = client
        .chat_stream(chat_req(), cancel.clone())
        .await
        .expect("chat_stream");
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        "A",
        "first token arrives"
    );
    cancel.cancel();
    let next = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("cancel must end stream promptly");
    assert!(next.is_none(), "no more items after cancel");
}

#[tokio::test]
async fn pre_cancelled_token_fails_without_request() {
    let mock = MockLlm::streaming(vec![sse_content("x")], Duration::ZERO);
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);
    let cancel = CancellationToken::new();
    cancel.cancel();

    let err = match client.chat_stream(chat_req(), cancel).await {
        Err(e) => e,
        Ok(_) => panic!("pre-cancelled must fail"),
    };
    assert!(matches!(err, AppError::Internal(_)));
    // 不发请求：server 记录为空。
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(mock.record.lock().unwrap().is_empty());
}

#[tokio::test]
async fn http_error_maps_to_model_unreachable() {
    let mock = MockLlm::failing(StatusCode::INTERNAL_SERVER_ERROR);
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);

    let err = match client
        .chat_stream(chat_req(), CancellationToken::new())
        .await
    {
        Err(e) => e,
        Ok(_) => panic!("http 500 must fail"),
    };
    match err {
        AppError::ModelUnreachable(msg) => {
            assert!(msg.contains("500"), "status in message: {msg}");
        }
        other => panic!("expected ModelUnreachable, got {other:?}"),
    }
}

#[tokio::test]
async fn upstream_error_event_surfaces_as_error_item() {
    let mock = MockLlm::streaming(
        vec![
            sse_content("A"),
            b"data: {\"error\":{\"message\":\"rate limited\"}}\n\n".to_vec(),
            sse_content("B"),
        ],
        Duration::ZERO,
    );
    let addr = spawn(&mock).await;
    let client = client_for(addr, None);

    let stream = client
        .chat_stream(chat_req(), CancellationToken::new())
        .await
        .expect("chat_stream");
    let items: Vec<Result<String, AppError>> = stream.collect().await;
    assert_eq!(items.len(), 2, "delta then terminal error");
    assert_eq!(items[0].as_ref().unwrap(), "A");
    match items[1].as_ref() {
        Err(AppError::ModelUnreachable(msg)) => assert!(msg.contains("rate limited")),
        other => panic!("expected ModelUnreachable error item, got {other:?}"),
    }
}
