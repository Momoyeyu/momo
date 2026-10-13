//! `MomoWsTts` 集成测试：axum mock svc-tts 走真实 ws 链路对拍。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::get,
};
use futures_util::StreamExt;
use momo_app::{
    error::AppError,
    pipeline::{
        splitter::Sentence,
        tts::{MomoWsTts, SynthRequest, TtsClient},
    },
    proto::codec::{AudioHeader, Direction, FLAG_EOS, encode_audio_frame},
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ulid::Ulid;
use url::Url;

const STREAM_ULID: &str = "01J5M0N8T1RZ8K2W4V7H6X9P3E";

fn stream_id() -> String {
    format!("st_{STREAM_ULID}")
}

fn stream_ulid() -> Ulid {
    Ulid::from_string(STREAM_ULID).unwrap()
}

/// 起 mock svc-tts 并返回 ws:// 地址 + report 接收端；`handler` 驱动剧本，
/// 服务端收到的文本消息经 `report` 回流给测试断言（mock 侧 panic 不会传导，
/// 断言全放测试体）。
async fn spawn_mock<F, Fut>(handler: F) -> (Url, mpsc::Receiver<Value>)
where
    F: Fn(WebSocket, mpsc::Sender<Value>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (report, rx) = mpsc::channel::<Value>(64);
    let handler = Arc::new(handler);
    let app = Router::new().route(
        "/ws",
        get(move |ws: WebSocketUpgrade| {
            let handler = handler.clone();
            let report = report.clone();
            async move { ws.on_upgrade(move |sock| handler(sock, report)) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (Url::parse(&format!("ws://{addr}/ws")).unwrap(), rx)
}

/// 服务端视角收一条文本 JSON。
async fn recv_json(sock: &mut WebSocket) -> Value {
    loop {
        match sock.recv().await {
            Some(Ok(Message::Text(t))) => return serde_json::from_str(t.as_str()).unwrap(),
            Some(Ok(_)) => continue,
            other => panic!("mock expected text frame, got {other:?}"),
        }
    }
}

fn hello_ok(role: &str) -> Message {
    Message::Text(
        json!({
            "type": "hello.ok",
            "version": "v2",
            "session_id": "sess_mock",
            "role": role,
            "capabilities": [],
        })
        .to_string()
        .into(),
    )
}

fn audio_open(turn_id: &str, seq: u32) -> Message {
    Message::Text(
        json!({
            "type": "audio.open",
            "session_id": "sess_mock",
            "event_id": STREAM_ULID,
            "at_ms": 0,
            "event_seq": 1,
            "stream_id": stream_id(),
            "direction": "downlink",
            "turn_id": turn_id,
            "sentence_seq": seq,
            "codec": "pcm_s16le",
            "sample_rate": 24000,
            "channels": 1,
        })
        .to_string()
        .into(),
    )
}

fn audio_end(last_chunk_seq: u32) -> Message {
    Message::Text(
        json!({
            "type": "audio.end",
            "session_id": "sess_mock",
            "event_id": STREAM_ULID,
            "at_ms": 0,
            "event_seq": 2,
            "stream_id": stream_id(),
            "last_chunk_seq": last_chunk_seq,
        })
        .to_string()
        .into(),
    )
}

fn data_frame(seq: u32, sentence_seq: u32, sample_offset: u32, payload: &[u8]) -> Message {
    let header = AudioHeader {
        direction: Direction::Downlink,
        flags: 0,
        stream_id: stream_ulid(),
        chunk_seq: seq,
        sentence_seq,
        sample_offset,
    };
    Message::Binary(encode_audio_frame(header, payload))
}

fn eos_frame(seq: u32, sentence_seq: u32, sample_offset: u32) -> Message {
    let header = AudioHeader {
        direction: Direction::Downlink,
        flags: FLAG_EOS,
        stream_id: stream_ulid(),
        chunk_seq: seq,
        sentence_seq,
        sample_offset,
    };
    Message::Binary(encode_audio_frame(header, b""))
}

fn req(turn_id: &str, seq: u32, text: &str) -> SynthRequest {
    SynthRequest {
        turn_id: turn_id.into(),
        lang: "zh".into(),
        sentence: Sentence {
            index: seq,
            text: text.into(),
            emotion: None,
        },
    }
}

/// 握手 + 收一句 output.sentence，返回其 JSON。
async fn mock_handshake_and_sentence(sock: &mut WebSocket) -> Value {
    let hello = recv_json(sock).await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["client"], "orchestrator");
    sock.send(hello_ok("tts")).await.unwrap();
    recv_json(sock).await
}

#[tokio::test]
async fn synth_streams_ordered_chunks_and_eos() {
    let (url, mut reports) = spawn_mock(|mut sock, report| async move {
        let sent = mock_handshake_and_sentence(&mut sock).await;
        report.send(sent).await.unwrap();
        sock.send(audio_open("t_a", 1)).await.unwrap();
        sock.send(data_frame(0, 1, 0, &[0x11; 2400])).await.unwrap();
        sock.send(data_frame(1, 1, 1200, &[0x22; 800]))
            .await
            .unwrap();
        sock.send(eos_frame(2, 1, 1600)).await.unwrap();
        sock.send(audio_end(2)).await.unwrap();
        // 保持连接直到客户端读完（服务端侧 close 不干扰断言）。
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let tts = MomoWsTts::new(url);
    let mut stream = tts
        .synth_stream(req("t_a", 1, "你好。"), CancellationToken::new())
        .await
        .unwrap();

    let mut seqs = Vec::new();
    let mut payload = Vec::new();
    let mut saw_eos = false;
    while let Some(item) = stream.next().await {
        let chunk = item.unwrap();
        assert_eq!(chunk.tag, 1);
        seqs.push(chunk.seq);
        saw_eos |= chunk.eos;
        payload.extend_from_slice(&chunk.payload);
    }
    assert_eq!(seqs, [0, 1, 2], "chunk order + EOS frame");
    assert!(saw_eos);
    assert_eq!(payload.len(), 3200);

    // 出线 output.sentence 形态（protocol §1 Envelope：无 event_seq）。
    let sent = reports.recv().await.unwrap();
    assert_eq!(sent["type"], "output.sentence");
    assert_eq!(sent["turn_id"], "t_a");
    assert_eq!(sent["seq"], 1);
    assert_eq!(sent["text"], "你好。");
    assert_eq!(sent["emotion"], "neutral");
    assert_eq!(sent["lang"], "zh");
    assert_eq!(sent["session_id"], "sess_mock");
    assert!(sent["event_id"].is_string());
    assert!(sent.get("event_seq").is_none());
}

#[tokio::test]
async fn hello_ok_role_mismatch_rejected() {
    let (url, _reports) = spawn_mock(|mut sock, _report| async move {
        let _hello = recv_json(&mut sock).await;
        // 连错端口：role=asr 必须被拒（protocol §2 必需字段表注）。
        sock.send(hello_ok("asr")).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    })
    .await;

    let tts = MomoWsTts::new(url);
    let err = tts
        .synth_stream(req("t_a", 1, "hi"), CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert!(matches!(err, AppError::Protocol(_)), "got {err:?}");
}

#[tokio::test]
async fn cancel_sends_control_cancel_and_ends_stream() {
    let (url, mut reports) = spawn_mock(|mut sock, report| async move {
        let _sent = mock_handshake_and_sentence(&mut sock).await;
        sock.send(audio_open("t_c", 1)).await.unwrap();
        sock.send(data_frame(0, 1, 0, &[0x33; 2400])).await.unwrap();
        // 等 control.cancel 回流给测试（之后流已被客户端摘除）。
        let msg = recv_json(&mut sock).await;
        report.send(msg).await.unwrap();
    })
    .await;

    let tts = MomoWsTts::new(url);
    let cancel = CancellationToken::new();
    let mut stream = tts
        .synth_stream(req("t_c", 1, "cancel me"), cancel.clone())
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.seq, 0);
    cancel.cancel();

    // 条目摘除后流干净收尾；迟到帧按 stale 丢弃。
    while let Some(item) = stream.next().await {
        item.unwrap();
    }
    let msg = tokio::time::timeout(Duration::from_secs(2), reports.recv())
        .await
        .expect("control.cancel never reached svc-tts")
        .unwrap();
    assert_eq!(msg["type"], "control.cancel");
    assert_eq!(msg["target"], "turn");
    assert_eq!(msg["id"], "t_c");
    assert!(msg.get("event_seq").is_none());
}

#[tokio::test]
async fn chunk_seq_gap_aborts_stream() {
    let (url, _reports) = spawn_mock(|mut sock, _report| async move {
        let _sent = mock_handshake_and_sentence(&mut sock).await;
        sock.send(audio_open("t_g", 1)).await.unwrap();
        sock.send(data_frame(0, 1, 0, &[0x44; 2400])).await.unwrap();
        // 跳过 seq 1：gap 必须中止该流（protocol §6 stale 规则）。
        sock.send(data_frame(2, 1, 1200, &[0x55; 2400]))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let tts = MomoWsTts::new(url);
    let mut stream = tts
        .synth_stream(req("t_g", 1, "gap"), CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(stream.next().await.unwrap().unwrap().seq, 0);
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(err, AppError::Protocol(_)), "got {err:?}");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn audio_end_mismatch_aborts_stream() {
    let (url, _reports) = spawn_mock(|mut sock, _report| async move {
        let _sent = mock_handshake_and_sentence(&mut sock).await;
        sock.send(audio_open("t_m", 1)).await.unwrap();
        sock.send(data_frame(0, 1, 0, &[0x66; 2400])).await.unwrap();
        sock.send(eos_frame(1, 1, 1200)).await.unwrap();
        // last_chunk_seq 与最后接收帧不一致 → 协议违反。
        sock.send(audio_end(7)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let tts = MomoWsTts::new(url);
    let mut stream = tts
        .synth_stream(req("t_m", 1, "mismatch"), CancellationToken::new())
        .await
        .unwrap();

    assert!(stream.next().await.unwrap().is_ok()); // data
    assert!(stream.next().await.unwrap().unwrap().eos); // EOS
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(err, AppError::Protocol(_)), "got {err:?}");
}

#[tokio::test]
async fn tts_failed_error_fails_pending_synth() {
    let (url, _reports) = spawn_mock(|mut sock, _report| async move {
        let _sent = mock_handshake_and_sentence(&mut sock).await;
        // 首块前就失败：服务端只发 error，不开 stream。
        sock.send(Message::Text(
            json!({
                "type": "error",
                "session_id": "sess_mock",
                "event_id": STREAM_ULID,
                "at_ms": 0,
                "event_seq": 1,
                "code": "TTS_FAILED",
                "message": "provider produced no audio",
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    })
    .await;

    let tts = MomoWsTts::new(url);
    let mut stream = tts
        .synth_stream(req("t_e", 1, "fail"), CancellationToken::new())
        .await
        .unwrap();
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(err, AppError::TtsFailed(_)), "got {err:?}");
    assert!(stream.next().await.is_none());
}
