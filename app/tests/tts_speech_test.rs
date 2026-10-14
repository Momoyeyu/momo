//! `/v1/audio/speech` client 集成测试：`OpenAiSpeechTts`/`TtsProvider` → axum mock svc-tts。
//! 覆盖 protocol.md §11.3：POST JSON 字段（input/response_format/lang/emotion）、
//! `Authorization: Bearer`、chunked PCM → `AudioChunk` 流（seq 递增、末块 EOS）、
//! OpenAI 错误信封 → AppError 映射、空音频降级、`set_endpoint` 热切换。

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
    routing::post,
};
use bytes::Bytes;
use futures_util::StreamExt;
use momo_app::{
    config::SecretString,
    error::AppError,
    pipeline::{
        splitter::Sentence,
        tts::{SynthRequest, TtsClient, TtsProvider},
        tts_speech::OpenAiSpeechTts,
    },
    proto::message::EmotionTag,
};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

// ---------- mock svc-tts（/v1/audio/speech 形态） ----------

/// 测试体控制响应：request 回报（auth + JSON body）后等回包指令。
struct Mock {
    url: url::Url,
    reqs: mpsc::Receiver<(Option<String>, Value, oneshot::Sender<Response>)>,
}

async fn spawn_mock() -> Mock {
    let (tx, reqs) = mpsc::channel(8);
    let app = Router::new()
        .route(
            "/v1/audio/speech",
            post(
                |headers: HeaderMap, State(tx): State<ReqTx>, body: Bytes| async move {
                    let auth = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let (rtx, rrx) = oneshot::channel();
                    let _ = tx.0.send((auth, json, rtx)).await;
                    rrx.await.unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .unwrap()
                    })
                },
            ),
        )
        .with_state(ReqTx(tx));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Mock {
        url: url::Url::parse(&format!("http://{addr}")).unwrap(),
        reqs,
    }
}

#[derive(Clone)]
struct ReqTx(mpsc::Sender<(Option<String>, Value, oneshot::Sender<Response>)>);

/// PCM 流式响应（content-type audio/pcm）。
fn pcm_response(chunks: Vec<Bytes>) -> Response {
    let stream = futures_util::stream::iter(chunks.into_iter().map(Ok::<Bytes, std::io::Error>));
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "audio/pcm")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// OpenAI 错误信封响应。
fn err_response(status: StatusCode, message: &str, type_: &str) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"error": {"message": message, "type": type_, "param": null, "code": null}})
                .to_string(),
        ))
        .unwrap()
}

fn synth_req(text: &str) -> SynthRequest {
    SynthRequest {
        turn_id: "t_1".into(),
        lang: "zh".into(),
        sentence: Sentence {
            index: 1,
            text: text.into(),
            emotion: Some(EmotionTag::Happy),
        },
    }
}

// ---------- 测试 ----------

/// 正常流式：请求字段/鉴权头断言 + chunk 序/EOS 断言。
#[tokio::test]
async fn speech_streams_pcm_chunks() {
    let mut mock = spawn_mock().await;
    // endpoint 不带 path → client 自动补 /v1/audio/speech。
    let tts = OpenAiSpeechTts::new(mock.url.clone(), Some(SecretString::new("k2".into())));

    let cancel = CancellationToken::new();
    let req = synth_req("你好，世界。");
    let stream_task = tokio::spawn(async move {
        let mut s = tts.synth_stream(req, cancel).await.unwrap();
        let mut out = Vec::new();
        while let Some(item) = s.next().await {
            out.push(item);
        }
        out
    });

    let (auth, body, rtx) = mock.reqs.recv().await.unwrap();
    assert_eq!(auth.as_deref(), Some("Bearer k2"));
    assert_eq!(body["input"], "你好，世界。");
    assert_eq!(body["response_format"], "pcm");
    assert_eq!(body["lang"], "zh");
    assert_eq!(body["emotion"], "happy");
    // 未写字段不下发。
    assert!(body.get("voice").is_none());
    assert!(body.get("speed").is_none());

    rtx.send(pcm_response(vec![
        Bytes::from_static(&[0x11; 2400]),
        Bytes::from_static(&[0x22; 1200]),
        Bytes::from_static(&[0x33; 600]),
    ]))
    .unwrap();

    let chunks = stream_task.await.unwrap();
    assert_eq!(chunks.len(), 3);
    let seqs: Vec<u32> = chunks.iter().map(|c| c.as_ref().unwrap().seq).collect();
    let total: usize = chunks
        .iter()
        .map(|c| c.as_ref().unwrap().payload.len())
        .sum();
    assert_eq!(seqs, vec![0, 1, 2]);
    assert_eq!(total, 4200);
    for (i, c) in chunks.iter().enumerate() {
        let c = c.as_ref().unwrap();
        assert_eq!(c.tag, 1);
        assert_eq!(c.eos, i == 2, "chunk {i} eos");
    }
}

/// OpenAI 错误信封 → AppError 映射：400→BadMessage，401→Unauthorized，503→ModelUnreachable。
#[tokio::test]
async fn speech_error_envelope_maps_to_app_error() {
    type Check = fn(&AppError) -> bool;
    let cases: [(StatusCode, &str, Check); 3] = [
        (StatusCode::BAD_REQUEST, "invalid_request_error", |e| {
            matches!(e, AppError::BadMessage(_))
        }),
        (StatusCode::UNAUTHORIZED, "authentication_error", |e| {
            matches!(e, AppError::Unauthorized(_))
        }),
        (StatusCode::SERVICE_UNAVAILABLE, "server_error", |e| {
            matches!(e, AppError::ModelUnreachable(_))
        }),
    ];
    for (status, type_, check) in cases {
        let mut mock = spawn_mock().await;
        let tts = OpenAiSpeechTts::new(mock.url.clone(), None);
        let cancel = CancellationToken::new();
        let req = synth_req("坏");
        let task = tokio::spawn(async move { tts.synth_stream(req, cancel).await });
        let (_auth, _body, rtx) = mock.reqs.recv().await.unwrap();
        rtx.send(err_response(status, "mock failure", type_))
            .unwrap();
        let err = task.await.unwrap().err().expect("synth_stream must fail");
        assert!(check(&err), "{status} mapped to {err:?}");
        assert!(err.to_string().contains("mock failure"));
    }
}

/// 200 但空 body → 流首项即 TtsFailed（该句转纯文字，不悬挂 pending-audio）。
#[tokio::test]
async fn speech_empty_audio_fails() {
    let mut mock = spawn_mock().await;
    let tts = OpenAiSpeechTts::new(mock.url.clone(), None);
    let cancel = CancellationToken::new();
    let req = synth_req("空");
    let task = tokio::spawn(async move {
        let mut s = tts.synth_stream(req, cancel).await.unwrap();
        s.next().await
    });
    let (_auth, _body, rtx) = mock.reqs.recv().await.unwrap();
    rtx.send(pcm_response(vec![])).unwrap();
    let first = task.await.unwrap().expect("stream item");
    let err = first.unwrap_err();
    assert!(matches!(err, AppError::TtsFailed(_)), "got {err:?}");
}

/// 取消令牌触发即断开在途请求（断连即取消语义）。
#[tokio::test]
async fn speech_cancel_aborts_in_flight() {
    let mut mock = spawn_mock().await;
    let tts = OpenAiSpeechTts::new(mock.url.clone(), None);
    let cancel = CancellationToken::new();
    let req = synth_req("长句");
    let cc = cancel.clone();
    let task = tokio::spawn(async move { tts.synth_stream(req, cc).await });
    let (_auth, _body, rtx) = mock.reqs.recv().await.unwrap();
    // 响应一直不发；取消后 client 应以 cancelled 错返回而不是干等。
    cancel.cancel();
    let err = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .err()
        .expect("cancelled synth must fail");
    assert!(matches!(err, AppError::Internal(_)), "got {err:?}");
    drop(rtx);
}

/// `TtsProvider` 分发：http(s) 落到 /v1/audio/speech 实装；`set_endpoint`
/// 换址后下一请求落到新服务端。
#[tokio::test]
async fn provider_dispatches_speech_and_hot_swaps() {
    let mut mock_a = spawn_mock().await;
    let mut mock_b = spawn_mock().await;

    let provider = Arc::new(TtsProvider::new(mock_a.url.clone(), None));
    let cancel = CancellationToken::new();

    // 第一次合成打到 A。
    let t1 = tokio::spawn({
        let p = provider.clone();
        let cc = cancel.clone();
        async move {
            let mut s = p.synth_stream(synth_req("甲"), cc).await.unwrap();
            let mut n = 0;
            while let Some(i) = s.next().await {
                i.unwrap();
                n += 1;
            }
            n
        }
    });
    let (_a, _b, rtx) = mock_a.reqs.recv().await.unwrap();
    rtx.send(pcm_response(vec![Bytes::from_static(&[0x01; 480])]))
        .unwrap();
    assert_eq!(t1.await.unwrap(), 1);

    // 热切换到 B：下一请求落到新服务端。
    provider.set_endpoint(mock_b.url.clone()).await;
    let t2 = tokio::spawn({
        let p = provider.clone();
        let cc = cancel.clone();
        async move {
            let mut s = p.synth_stream(synth_req("乙"), cc).await.unwrap();
            let mut n = 0;
            while let Some(i) = s.next().await {
                i.unwrap();
                n += 1;
            }
            n
        }
    });
    let (_a, _b, rtx) = mock_b.reqs.recv().await.unwrap();
    rtx.send(pcm_response(vec![Bytes::from_static(&[0x02; 240])]))
        .unwrap();
    assert_eq!(t2.await.unwrap(), 1);

    // mock_a 无第二请求。
    assert!(
        tokio::time::timeout(Duration::from_millis(100), mock_a.reqs.recv())
            .await
            .is_err()
    );
}
