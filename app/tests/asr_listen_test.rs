//! `/v1/listen` client 集成测试：`DeepgramListenAsr`/`AsrProvider` → axum mock svc-asr。
//! 覆盖 protocol.md §11.2：建连 query 缺省补齐 + `Authorization: Token` 头、
//! 裸 PCM 上行、JSON 事件（SpeechStarted/UtteranceEnd/Results/Error）→ `AsrEvent`
//! 映射、CloseStream 正常收尾无 LinkDown、异常断链报 LinkDown、鉴权拒绝映射、
//! capture 取消即断连、`set_endpoint` 热切换落到新地址。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Router,
    extract::{
        Query, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use bytes::Bytes;
use momo_app::{
    config::SecretString,
    error::AppError,
    pipeline::{
        asr::{AsrClient, AsrEvent, AsrProvider},
        asr_listen::DeepgramListenAsr,
    },
    proto::{
        codec::{AudioHeader, Direction},
        message::{AudioCodec, AudioOpenMsg, CancelTarget, Envelope, VadEvent, new_id},
    },
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use ulid::Ulid;

// ---------- mock svc-asr（/v1/listen 形态） ----------

/// mock 服务端 → 测试体的回报单元。
#[derive(Debug)]
enum Item {
    /// 建连信息：Authorization 头 + query 参数表（原始序）。
    Req {
        auth: Option<String>,
        query: Vec<(String, String)>,
    },
    Text(Value),
    Binary(Bytes),
    /// 对端 close 帧。
    Closed,
}

/// 起 mock svc-asr：`expect_key` 非空时 Authorization != `Token <key>` → 403。
/// 返回 ws:// 地址 + 回报通道；handler 驱动 socket 剧本。
async fn spawn_mock<F, Fut>(
    expect_key: Option<&'static str>,
    handler: F,
) -> (url::Url, mpsc::Receiver<Item>)
where
    F: Fn(WebSocket, mpsc::Sender<Item>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (report, rx) = mpsc::channel::<Item>(64);
    let handler = Arc::new(handler);
    let app = Router::new().route(
        "/v1/listen",
        get(
            move |ws: WebSocketUpgrade,
                  headers: HeaderMap,
                  Query(q): Query<Vec<(String, String)>>| {
                let handler = handler.clone();
                let report = report.clone();
                async move {
                    let auth = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    if let Some(key) = expect_key {
                        if auth.as_deref() != Some(format!("Token {key}").as_str()) {
                            return StatusCode::FORBIDDEN.into_response();
                        }
                    }
                    let _ = report.try_send(Item::Req { auth, query: q });
                    ws.on_upgrade(move |sock| handler(sock, report))
                        .into_response()
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (
        url::Url::parse(&format!("ws://{addr}/v1/listen")).unwrap(),
        rx,
    )
}

/// 服务端视角收一帧：文本 → JSON，二进制 → 裸 PCM，close → Closed。
async fn recv_item(sock: &mut WebSocket) -> Item {
    match tokio::time::timeout(Duration::from_secs(5), sock.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) => Item::Text(serde_json::from_str(t.as_str()).unwrap()),
        Ok(Some(Ok(Message::Binary(b)))) => Item::Binary(b),
        Ok(Some(Ok(Message::Close(_)))) => Item::Closed,
        other => panic!("mock expected data frame, got {other:?}"),
    }
}

async fn send_json(sock: &mut WebSocket, v: Value) {
    sock.send(Message::Text(v.to_string().into()))
        .await
        .unwrap();
}

// ---------- 测试侧 client 辅助 ----------

fn open_msg(capture_id: &str) -> AudioOpenMsg {
    AudioOpenMsg {
        env: Envelope {
            session_id: "sess_test".into(),
            event_id: new_id("e"),
            at_ms: 1,
            reply_to: None,
        },
        event_seq: None,
        stream_id: format!("st_{}", Ulid::new()),
        direction: Direction::Uplink,
        capture_id: Some(capture_id.into()),
        turn_id: None,
        sentence_seq: None,
        codec: AudioCodec::PcmS16le,
        sample_rate: 16000,
        channels: 1,
    }
}

fn pcm_header(seq: u32) -> AudioHeader {
    AudioHeader {
        direction: Direction::Uplink,
        flags: 0,
        stream_id: Ulid::new(),
        chunk_seq: seq,
        sentence_seq: 0,
        sample_offset: seq * 800,
    }
}

async fn recv_event(rx: &mut mpsc::Receiver<AsrEvent>) -> AsrEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("asr event timeout")
        .expect("asr event channel closed")
}

/// 窗口期内断言无新事件（graceful 收尾不上报 LinkDown 的判定）。
async fn expect_no_event(rx: &mut mpsc::Receiver<AsrEvent>, window: Duration) {
    match tokio::time::timeout(window, rx.recv()).await {
        Err(_) => {}
        Ok(ev) => panic!("unexpected asr event: {ev:?}"),
    }
}

// ---------- 测试 ----------

/// 完整会话：建连参数/鉴权头 → 裸 PCM 上行 → Metadata/VAD/Results 回流映射 →
/// `end_uplink` 发 CloseStream，服务端回声 + close 1000 → 无 LinkDown。
#[tokio::test]
async fn listen_pcm_events_and_clean_close() {
    let (url, mut items) = spawn_mock(Some("k1"), |mut sock, report| async move {
        // 建连信息 + 两帧 PCM + CloseStream，逐项回报。
        for _ in 0..3 {
            report.send(recv_item(&mut sock).await).await.unwrap();
        }
        // 回流事件：Metadata → SpeechStarted → UtteranceEnd → Results。
        send_json(
            &mut sock,
            json!({"type":"Metadata","request_id":"01J","model_info":{"name":"sensevoice"},"sample_rate":16000}),
        )
        .await;
        send_json(&mut sock, json!({"type":"SpeechStarted","timestamp":0.5})).await;
        send_json(
            &mut sock,
            json!({"type":"UtteranceEnd","last_word_end":1.4,"channel":[0,0]}),
        )
        .await;
        send_json(
            &mut sock,
            json!({"type":"Results","channel_index":[0,1],"duration":0.9,"start":0.5,
                   "is_final":true,"speech_final":true,
                   "channel":{"alternatives":[{"transcript":"你好，世界。","confidence":1.0,"words":[]}]}}),
        )
        .await;
        // 回声 CloseStream + 正常关闭。
        send_json(&mut sock, json!({"type":"CloseStream"})).await;
        let _ = sock
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1000,
                reason: "done".into(),
            })))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    })
    .await;

    let asr = DeepgramListenAsr::new(url, Some(SecretString::new("k1".into())));
    let mut events = asr.subscribe().await;
    asr.open_uplink(&open_msg("cap_1")).await.unwrap();

    // 建连信息：Token 头 + query 缺省补齐。
    let Item::Req { auth, query } = items.recv().await.unwrap() else {
        panic!("expected handshake report")
    };
    assert_eq!(auth.as_deref(), Some("Token k1"));
    for (k, v) in [
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("vad_events", "true"),
    ] {
        assert!(
            query.contains(&(k.to_string(), v.to_string())),
            "missing query {k}={v} in {query:?}"
        );
    }

    // 上行帧：momo 32B 头不随行，裸 PCM 字节。
    asr.send_frame(pcm_header(0), Bytes::from_static(&[0x11; 1600]))
        .await
        .unwrap();
    asr.send_frame(pcm_header(1), Bytes::from_static(&[0x22; 800]))
        .await
        .unwrap();
    asr.end_uplink("st_x", 1).await.unwrap();

    let Item::Binary(pcm) = items.recv().await.unwrap() else {
        panic!("expected pcm frame")
    };
    assert_eq!(pcm.len(), 1600);
    assert_eq!(pcm[0], 0x11);
    let Item::Binary(pcm) = items.recv().await.unwrap() else {
        panic!("expected pcm frame")
    };
    assert_eq!(pcm.len(), 800);
    assert_eq!(pcm[0], 0x22);
    let Item::Text(close) = items.recv().await.unwrap() else {
        panic!("expected CloseStream")
    };
    assert_eq!(close["type"], "CloseStream");

    // 事件映射：VAD 边界 → input.vad；Results → final segment。
    let AsrEvent::Vad(v) = recv_event(&mut events).await else {
        panic!("expected vad")
    };
    assert_eq!(v.capture_id, "cap_1");
    assert_eq!(v.event, VadEvent::SpeechStart);

    let AsrEvent::Vad(v) = recv_event(&mut events).await else {
        panic!("expected vad")
    };
    assert_eq!(v.event, VadEvent::SpeechEnd);

    let AsrEvent::Segment(s) = recv_event(&mut events).await else {
        panic!("expected segment")
    };
    assert_eq!(s.capture_id, "cap_1");
    assert_eq!(s.text, "你好，世界。");
    assert!(s.is_final);

    // 服务端回声 CloseStream + close 1000 = 正常收尾：无 LinkDown。
    expect_no_event(&mut events, Duration::from_millis(300)).await;
}

/// 服务端中途异常关闭（非回声收尾）→ LinkDown 上报。
#[tokio::test]
async fn listen_abnormal_close_reports_link_down() {
    let (url, _items) = spawn_mock(None, |mut sock, _report| async move {
        let _ = recv_item(&mut sock).await;
        let _ = sock
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1011,
                reason: "boom".into(),
            })))
            .await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    })
    .await;

    let asr = DeepgramListenAsr::new(url, None);
    let mut events = asr.subscribe().await;
    asr.open_uplink(&open_msg("cap_2")).await.unwrap();
    asr.send_frame(pcm_header(0), Bytes::from_static(&[0x33; 320]))
        .await
        .unwrap();

    let AsrEvent::LinkDown(err) = recv_event(&mut events).await else {
        panic!("expected LinkDown")
    };
    assert!(matches!(err, AppError::ModelUnreachable(_)));
}

/// 鉴权失败（服务端 403 拒绝升级）→ Unauthorized。
#[tokio::test]
async fn listen_auth_rejected_maps_unauthorized() {
    let (url, _items) = spawn_mock(Some("good"), |_sock, _report| async move {}).await;
    let asr = DeepgramListenAsr::new(url, Some(SecretString::new("bad".into())));
    let err = asr.open_uplink(&open_msg("cap_3")).await.unwrap_err();
    assert!(matches!(err, AppError::Unauthorized(_)), "got {err:?}");
}

/// `control.cancel{capture}` = 断连即取消：服务端见到 close，无 LinkDown。
#[tokio::test]
async fn listen_capture_cancel_disconnects() {
    let (url, mut items) = spawn_mock(None, |mut sock, report| async move {
        // 一帧 PCM 后等对端 close。
        report.send(recv_item(&mut sock).await).await.unwrap();
        report.send(recv_item(&mut sock).await).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    })
    .await;

    let asr = DeepgramListenAsr::new(url, None);
    let mut events = asr.subscribe().await;
    asr.open_uplink(&open_msg("cap_4")).await.unwrap();
    let Item::Req { .. } = items.recv().await.unwrap() else {
        panic!("expected handshake")
    };
    asr.send_frame(pcm_header(0), Bytes::from_static(&[0x44; 320]))
        .await
        .unwrap();
    let Item::Binary(_) = items.recv().await.unwrap() else {
        panic!("expected pcm")
    };

    asr.cancel(CancelTarget::Capture, "cap_4").await.unwrap();
    let Item::Closed = items.recv().await.unwrap() else {
        panic!("expected ws close")
    };
    expect_no_event(&mut events, Duration::from_millis(200)).await;
}

/// 持续回报 socket 帧直到对端断开（handler 过早退出会 RST 触发非 graceful 终态）。
async fn pump_items(sock: &mut WebSocket, report: mpsc::Sender<Item>) {
    loop {
        match sock.recv().await {
            Some(Ok(Message::Text(t))) => {
                let v = serde_json::from_str(t.as_str()).unwrap();
                let _ = report.send(Item::Text(v)).await;
            }
            Some(Ok(Message::Binary(b))) => {
                let _ = report.send(Item::Binary(b)).await;
            }
            Some(Ok(Message::Close(_))) | None => {
                let _ = report.send(Item::Closed).await;
                return;
            }
            _ => {}
        }
    }
}

/// `AsrProvider` 分发：非 `/ws` 路径落到 /v1/listen 实装；`set_endpoint`
/// 换址后新 capture 落到新服务端。
#[tokio::test]
async fn provider_dispatches_listen_and_hot_swaps() {
    let (url_a, mut items_a) = spawn_mock(None, |mut sock, report| async move {
        pump_items(&mut sock, report).await;
    })
    .await;
    let (url_b, mut items_b) = spawn_mock(None, |mut sock, report| async move {
        pump_items(&mut sock, report).await;
    })
    .await;

    let provider = AsrProvider::new(url_a, None);
    let mut events = provider.subscribe().await;

    provider.open_uplink(&open_msg("cap_a")).await.unwrap();
    let Item::Req { .. } = items_a.recv().await.unwrap() else {
        panic!("expected handshake on mock A")
    };
    provider
        .send_frame(pcm_header(0), Bytes::from_static(&[0x55; 160]))
        .await
        .unwrap();
    let Item::Binary(_) = items_a.recv().await.unwrap() else {
        panic!("expected pcm on mock A")
    };
    provider.end_uplink("st_a", 0).await.unwrap();

    // 热切换到第二个 mock：新 capture 落到 B。
    provider.set_endpoint(url_b).await;
    provider.open_uplink(&open_msg("cap_b")).await.unwrap();
    let Item::Req { .. } = items_b.recv().await.unwrap() else {
        panic!("expected handshake on mock B")
    };
    provider
        .send_frame(pcm_header(0), Bytes::from_static(&[0x66; 160]))
        .await
        .unwrap();
    let Item::Binary(_) = items_b.recv().await.unwrap() else {
        panic!("expected pcm on mock B")
    };

    // 订阅面在实装切换后仍有效（泵转发），无 LinkDown。
    expect_no_event(&mut events, Duration::from_millis(150)).await;
}
