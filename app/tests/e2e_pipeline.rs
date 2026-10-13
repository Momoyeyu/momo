//! 协议全链路 e2e（#16）：进程内起 mock OpenAI SSE LLM + momo-ws svc-asr /
//! svc-tts，真 TCP listener + 真 ws client（tokio-tungstenite）驱动
//! `ws::server::router`，覆盖文本/语音全链、barge-in 打断与断线 resume 收敛。
//!
//! 断言纪律：`event_seq` 跨生产者（session actor vs audio sink worker）不保证
//! wire 序严格单调（共享计数器但交错出站），仅对同生产者断言递增；
//! 所有 recv 均带 timeout 防 CI hang。

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    Router,
    body::Body,
    extract::{
        Json,
        ws::{Message as AxumMsg, WebSocket, WebSocketUpgrade},
    },
    http::{Request, header},
    response::Response,
    routing::{get, post},
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, stream};
use momo_app::{
    config::Config,
    proto::{
        codec::{self, AUDIO_HEADER_LEN, AudioHeader, Direction, FLAG_EOS, encode_audio_frame},
        message::{
            BargeInMsg, BargeInReason, CancelTarget, Envelope, Msg, ReceiptStatus, TurnOutcome,
            TurnState, new_id,
        },
    },
};
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::mpsc};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message as WsMsg, client::IntoClientRequest, http::HeaderValue},
};
use tower::ServiceExt;
use ulid::Ulid;

const RECV_TIMEOUT: Duration = Duration::from_secs(5);
/// `MOMO_WEB_ORIGIN` 默认值；`/ws/ui` upgrade 校验 Origin 白名单。
const WEB_ORIGIN: &str = "http://localhost:5173";
/// mock 模型服务链路在 hello.ok 里回报的 session id。
const MOCK_SESSION: &str = "sess_mock";

// ---------- mock momo-ws 服务（svc-asr / svc-tts） ----------

/// mock 服务端 → 测试体的回报单元（JSON 业务帧 / 二进制音频帧）。
#[derive(Debug, Clone)]
enum Item {
    Json(Value),
    Audio(AudioHeader, Bytes),
}

/// 起一条 mock momo-ws 服务链路并返回 ws:// 地址 + 入站项回流通道。
async fn spawn_ws_mock<F, Fut>(handler: F) -> (url::Url, mpsc::Receiver<Item>)
where
    F: Fn(WebSocket, mpsc::Sender<Item>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let (report, rx) = mpsc::channel::<Item>(128);
    let handler = Arc::new(handler);
    let app = Router::new().route(
        "/ws",
        get(move |ws: WebSocketUpgrade| {
            let handler = handler.clone();
            let report = report.clone();
            async move { ws.on_upgrade(move |sock| handler(sock, report)) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url::Url::parse(&format!("ws://{addr}/ws")).unwrap(), rx)
}

/// 服务端视角收一条入站项；跳过 ping/pong，断流或超时回 None。
async fn recv_item(sock: &mut WebSocket) -> Option<Item> {
    loop {
        match tokio::time::timeout(RECV_TIMEOUT, sock.recv()).await {
            Ok(Some(Ok(AxumMsg::Text(t)))) => {
                return Some(Item::Json(serde_json::from_str(t.as_str()).unwrap()));
            }
            Ok(Some(Ok(AxumMsg::Binary(b)))) => {
                let (header, payload) = AudioHeader::decode(&b).unwrap();
                return Some(Item::Audio(header, Bytes::copy_from_slice(payload)));
            }
            Ok(Some(Ok(_))) => continue,
            _ => return None,
        }
    }
}

async fn send_json(sock: &mut WebSocket, v: Value) {
    sock.send(AxumMsg::Text(v.to_string().into()))
        .await
        .unwrap();
}

/// 服务端视角 hello 握手（断言 client=orchestrator 后回 role=expected）。
async fn mock_handshake(sock: &mut WebSocket, role: &str) {
    let Some(Item::Json(hello)) = recv_item(sock).await else {
        panic!("expected hello");
    };
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["client"], "orchestrator");
    send_json(
        sock,
        json!({
            "type": "hello.ok",
            "version": "v2",
            "session_id": MOCK_SESSION,
            "role": role,
            "capabilities": [],
        }),
    )
    .await;
}

/// 对一句 `output.sentence` 回一个完整下行 stream：
/// `audio.open` → 2×2400B 数据帧 + EOS 空帧 → `audio.end`。
async fn respond_audio(sock: &mut WebSocket, turn_id: &str, sentence_seq: u32) {
    let stream = Ulid::new();
    send_json(
        sock,
        json!({
            "type": "audio.open",
            "session_id": MOCK_SESSION,
            "event_id": new_id("e"),
            "at_ms": 1,
            "event_seq": 1,
            "stream_id": format!("st_{stream}"),
            "direction": "downlink",
            "turn_id": turn_id,
            "sentence_seq": sentence_seq,
            "codec": "pcm_s16le",
            "sample_rate": 24000,
            "channels": 1,
        }),
    )
    .await;
    for seq in 0..3u32 {
        let last = seq == 2;
        let header = AudioHeader {
            direction: Direction::Downlink,
            flags: if last { FLAG_EOS } else { 0 },
            stream_id: stream,
            chunk_seq: seq,
            sentence_seq,
            sample_offset: seq * 1200,
        };
        let payload = if last { Vec::new() } else { vec![0x55; 2400] };
        sock.send(AxumMsg::Binary(encode_audio_frame(header, &payload)))
            .await
            .unwrap();
    }
    send_json(
        sock,
        json!({
            "type": "audio.end",
            "session_id": MOCK_SESSION,
            "event_id": new_id("e"),
            "at_ms": 1,
            "event_seq": 2,
            "stream_id": format!("st_{stream}"),
            "last_chunk_seq": 2,
        }),
    )
    .await;
}

/// mock svc-tts：hello 后逐条回流入站消息；每收一句 `output.sentence`
/// 自动回一个完整下行 stream（barge-in 场景的 `control.cancel` 只回流不断言）。
async fn spawn_mock_tts() -> (url::Url, mpsc::Receiver<Item>) {
    spawn_ws_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "tts").await;
        while let Some(item) = recv_item(&mut sock).await {
            let synth = match &item {
                Item::Json(v) if v["type"] == "output.sentence" => Some((
                    v["turn_id"].as_str().unwrap().to_string(),
                    v["seq"].as_u64().unwrap() as u32,
                )),
                _ => None,
            };
            report.send(item).await.unwrap();
            if let Some((turn_id, seq)) = synth {
                respond_audio(&mut sock, &turn_id, seq).await;
            }
        }
    })
    .await
}

fn segment_json(capture_id: &str, text: &str, is_final: bool) -> Value {
    json!({
        "type": "input.audio.segment",
        "session_id": MOCK_SESSION,
        "event_id": new_id("e"),
        "at_ms": 1,
        "event_seq": 1,
        "capture_id": capture_id,
        "text": text,
        "final": is_final,
    })
}

/// mock svc-asr：hello 后逐条回流入站消息；首个 `audio.end` 后回
/// `input.audio.segment` partial + final（capture_id 取自 audio.open）。
async fn spawn_mock_asr(final_text: &'static str) -> (url::Url, mpsc::Receiver<Item>) {
    spawn_ws_mock(move |mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        let mut capture_id = String::new();
        let mut replied = false;
        while let Some(item) = recv_item(&mut sock).await {
            if let Item::Json(v) = &item {
                if v["type"] == "audio.open" {
                    capture_id = v["capture_id"].as_str().unwrap_or_default().to_string();
                }
            }
            let is_end = matches!(&item, Item::Json(v) if v["type"] == "audio.end");
            report.send(item).await.unwrap();
            if is_end && !replied {
                replied = true;
                send_json(&mut sock, segment_json(&capture_id, final_text, false)).await;
                send_json(&mut sock, segment_json(&capture_id, final_text, true)).await;
            }
        }
    })
    .await
}

// ---------- mock OpenAI SSE LLM ----------

/// 一次 `/v1/chat/completions` 调用：`body` 供断言，`feed` 供测试喂 delta
/// 文本（drop feed → 自动补 `data: [DONE]` 收尾）。
struct LlmCall {
    body: Value,
    feed: mpsc::Sender<String>,
}

/// 起 mock LLM（OpenAI-compatible SSE）并返回 base URL + 调用回流通道。
async fn spawn_mock_llm() -> (String, mpsc::Receiver<LlmCall>) {
    let (report, rx) = mpsc::channel::<LlmCall>(8);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let report = report.clone();
            async move {
                let (feed, feed_rx) = mpsc::channel::<String>(16);
                report.send(LlmCall { body, feed }).await.unwrap();
                sse_response(feed_rx)
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/v1"), rx)
}

/// `text/event-stream` 响应：feed 每个 delta → 一个 OpenAI chunk；
/// feed 关闭 → `data: [DONE]` 后结束 body。
fn sse_response(feed_rx: mpsc::Receiver<String>) -> Response {
    let chunks = stream::unfold((feed_rx, false), |(mut rx, done)| async move {
        if done {
            return None;
        }
        match rx.recv().await {
            Some(delta) => {
                let chunk = json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {"content": delta}}],
                });
                Some((
                    Ok::<Bytes, Infallible>(format!("data: {chunk}\n\n").into()),
                    (rx, false),
                ))
            }
            None => Some((Ok(Bytes::from_static(b"data: [DONE]\n\n")), (rx, true))),
        }
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(chunks))
        .unwrap()
}

async fn recv_llm_call(rx: &mut mpsc::Receiver<LlmCall>) -> LlmCall {
}

async fn recv_report(rx: &mut mpsc::Receiver<Item>) -> Item {
    tokio::time::timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("mock report timeout")
        .expect("mock report channel closed")
}

// ---------- 真 server + 真 ws client ----------

fn e2e_config(llm: &str, asr: &url::Url, tts: &url::Url) -> Config {
    let overrides: HashMap<String, String> = [
        ("MOMO_LLM_ENDPOINT", llm.to_string()),
        ("MOMO_LLM_MODEL", "e2e-mock-model".to_string()),
        ("MOMO_ASR_ENDPOINT", asr.to_string()),
        ("MOMO_TTS_ENDPOINT", tts.to_string()),
        ("MOMO_BARGEIN_MIN_MS", "80".to_string()),
        ("MOMO_SESSION_RESUME_TTL_MS", "60000".to_string()),
        // 空闲自发行为在 e2e 中关闭，避免慢 CI 上的干扰帧。
        ("MOMO_IDLE_TIMEOUT_S", "0".to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    Config::load(|k| overrides.get(k).cloned()).expect("e2e config")
}

async fn serve_app(config: Config) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// 真 momo-ws UI client。
struct Client {
    sock: WsStream,
}

impl Client {
    async fn send_json(&mut self, v: Value) {
        self.sock
            .send(WsMsg::Text(v.to_string().into()))
            .await
            .expect("client send");
    }

    /// 发一帧 uplink `audio.chunk`（32B 头 + PCM 载荷）。
    async fn send_audio(
        &mut self,
        stream: Ulid,
        chunk_seq: u32,
        sample_offset: u32,
        payload: &[u8],
        eos: bool,
    ) {
        let header = AudioHeader {
            direction: Direction::Uplink,
            flags: if eos { FLAG_EOS } else { 0 },
            stream_id: stream,
            chunk_seq,
            sentence_seq: 0,
            sample_offset,
        };
        self.sock
            .send(WsMsg::Binary(encode_audio_frame(header, payload)))
            .await
            .expect("client audio send");
    }

    /// 收一个服务端出站单元（JSON 业务帧 / 下行二进制音频帧）。
    async fn recv_out(&mut self) -> Out {
        loop {
            let msg = tokio::time::timeout(RECV_TIMEOUT, self.sock.next())
                .await
                .expect("server frame timeout")
                .expect("server closed connection")
                .expect("ws read error");
            match msg {
                WsMsg::Text(t) => {
                    return Out::Json(
                        codec::decode_msg(t.as_str()).expect("server frame is valid Msg"),
                    );
                }
                WsMsg::Binary(b) => {
                    assert!(
                        b.len() >= AUDIO_HEADER_LEN,
                        "audio.chunk must carry 32B header"
                    );
                    let (h, payload) = AudioHeader::decode(&b).expect("valid downlink frame");
                    return Out::Audio(h, Bytes::copy_from_slice(payload));
                }
                WsMsg::Close(_) => panic!("unexpected close frame"),
                _ => continue,
            }
        }
    }

    async fn recv_msg(&mut self) -> Msg {
        loop {
            if let Out::Json(m) = self.recv_out().await {
                return m;
            }
        }
    }

    /// 模拟断线： graceful close 后丢弃 socket。
    async fn disconnect(mut self) {
        let _ = self.sock.close(None).await;
    }
}

/// 真 ws 连接 + hello 握手（可选 resume），返回 client 与 session_id。
async fn connect_client(addr: SocketAddr, resume_session_id: Option<&str>) -> (Client, String) {
    let mut req = format!("ws://{addr}/ws/ui")
        .into_client_request()
        .expect("valid ws url");
    req.headers_mut()
        .insert("origin", HeaderValue::from_static(WEB_ORIGIN));
    let (sock, _resp) = connect_async(req).await.expect("ws connect");
    let mut client = Client { sock };

    let mut hello = json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v2",
        "client": "ui",
        "auth_token": "ephemeral-token",
    });
    if let Some(id) = resume_session_id {
        hello["resume_session_id"] = json!(id);
    }
    client.send_json(hello).await;
    let Msg::HelloOk(ok) = client.recv_msg().await else {
        panic!("expected hello.ok");
    };
    (client, ok.session_id)
}

// ---------- 出站消息收集 ----------

/// 服务端出站单元。
enum Out {
    Json(Msg),
    Audio(AudioHeader, Bytes),
}

async fn recv_until(client: &mut Client, mut pred: impl FnMut(&Out) -> bool) -> Vec<Out> {
    let mut seen = Vec::new();
    loop {
        let item = client.recv_out().await;
        let hit = pred(&item);
        seen.push(item);
        if hit {
            return seen;
        }
    }
}

async fn recv_until_msg(client: &mut Client, pred: impl Fn(&Msg) -> bool) -> Vec<Out> {
    recv_until(client, |o| matches!(o, Out::Json(m) if pred(m))).await
}

/// 窗口期内排空出站（"不应出现某帧"/"恰一次"断言用）。
async fn drain_for(client: &mut Client, window: Duration) -> Vec<Out> {
    let deadline = Instant::now() + window;
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return seen;
        }
        match tokio::time::timeout(left, client.sock.next()).await {
            Ok(Some(Ok(WsMsg::Text(t)))) => seen.push(Out::Json(
                codec::decode_msg(t.as_str()).expect("server frame is valid Msg"),
            )),
            Ok(Some(Ok(WsMsg::Binary(b)))) => {
                let (h, payload) = AudioHeader::decode(&b).expect("valid downlink frame");
                seen.push(Out::Audio(h, Bytes::copy_from_slice(payload)));
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(_))) | Ok(None) | Err(_) => return seen,
        }
    }
}

/// `Msg` → wire JSON（断言 envelope 字段的通用入口）。
fn wire(m: &Msg) -> Value {
    serde_json::to_value(m).expect("Msg serializes")
}

/// audio sink worker 产出的消息（其余业务消息归 session actor）。
fn is_sink_produced(m: &Msg) -> bool {
    matches!(
        m,
        Msg::AudioOpen(_) | Msg::AudioEnd(_) | Msg::ControlCancel(_)
    )
}

/// 断言：窗口内同生产者的 `event_seq` 严格递增；跨生产者不断言全局序。
fn assert_per_producer_seq_monotonic(items: &[Out]) {
    let mut actor_last = 0u64;
    let mut sink_last = 0u64;
    for item in items {
        let Out::Json(m) = item else { continue };
        let Some(seq) = wire(m).get("event_seq").and_then(Value::as_u64) else {
            continue;
        };
        let last = if is_sink_produced(m) {
            &mut sink_last
        } else {
            &mut actor_last
        };
        assert!(
            seq > *last || *last == 0,
            "event_seq must increase per producer, got {seq} after {last}"
        );
        *last = (*last).max(seq);
    }
}

/// 断言：窗口内所有带 envelope 的服务端消息 session_id 一致。
fn assert_session_id(items: &[Out], session_id: &str) {
    for item in items {
        let Out::Json(m) = item else { continue };
        if let Some(sid) = wire(m).get("session_id") {
            assert_eq!(sid.as_str().unwrap(), session_id, "envelope session_id");
        }
    }
}

/// 回合窗口内收集到的下行流要素。
#[derive(Default)]
struct Seen {
    sentences: Vec<(u32, String, String)>, // (seq, text, turn_id)
    opens: Vec<(String, u32, String)>,     // (stream_id, sentence_seq, turn_id)
    /// (stream_id raw, chunk_seq, sentence_seq, sample_offset, eos, payload_len)
    frames: Vec<(Ulid, u32, u32, u32, bool, usize)>,
    ends: Vec<(String, u32)>, // (stream_id, last_chunk_seq)
}

fn collect(items: &[Out]) -> Seen {
    let mut seen = Seen::default();
    for item in items {
        match item {
            Out::Json(Msg::OutputSentence(s)) => {
                seen.sentences
                    .push((s.seq, s.text.clone(), s.turn_id.clone()));
            }
            Out::Json(Msg::AudioOpen(o)) if o.direction == Direction::Downlink => {
                seen.opens.push((
                    o.stream_id.clone(),
                    o.sentence_seq.expect("downlink sentence_seq"),
                    o.turn_id.clone().expect("downlink turn_id"),
                ));
            }
            Out::Json(Msg::AudioEnd(e)) => {
                seen.ends.push((e.stream_id.clone(), e.last_chunk_seq));
            }
            Out::Audio(h, payload) => seen.frames.push((
                h.stream_id,
                h.chunk_seq,
                h.sentence_seq,
                h.sample_offset,
                h.eos(),
                payload.len(),
            )),
            _ => {}
        }
    }
    seen
}

// ---------- 客户端消息构造 ----------

fn input_text(id: &str, text: &str) -> Value {
    json!({
        "type": "input.text",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "id": id,
        "text": text,
        "lang": "zh",
    })
}

fn audio_start(capture_id: &str) -> Value {
    json!({
        "type": "input.audio.start",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "capture_id": capture_id,
        "mode": "continuous",
    })
}

fn audio_open_uplink(stream: Ulid, capture_id: &str) -> Value {
    json!({
        "type": "audio.open",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": format!("st_{stream}"),
        "direction": "uplink",
        "capture_id": capture_id,
        "codec": "pcm_s16le",
        "sample_rate": 16000,
        "channels": 1,
    })
}

fn audio_end_uplink(stream: Ulid, last_chunk_seq: u32) -> Value {
    json!({
        "type": "audio.end",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": format!("st_{stream}"),
        "last_chunk_seq": last_chunk_seq,
    })
}

fn playback_started(stream_id: &str, sentence_seq: u32) -> Value {
    json!({
        "type": "playback.started",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id,
        "sentence_seq": sentence_seq,
    })
}

fn playback_progress(stream_id: &str, sentence_seq: u32, played: u64, completed: bool) -> Value {
    json!({
        "type": "playback.progress",
        "session_id": "sess_ui",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id,
        "sentence_seq": sentence_seq,
        "played_samples": played,
        "completed": completed,
    })
}

fn barge_in(turn_id: &str) -> Value {
    let m = BargeInMsg {
        env: Envelope {
            session_id: "sess_ui".into(),
            event_id: new_id("e"),
            at_ms: 1,
            reply_to: None,
        },
        turn_id: turn_id.into(),
        reason: BargeInReason::User,
    };
    serde_json::to_value(Msg::ControlBargeIn(m)).unwrap()
}

/// 开文字回合并取 turn_id（input.accepted → state.turn thinking）。
async fn open_text_turn(client: &mut Client, input_id: &str, text: &str) -> String {
    client.send_json(input_text(input_id, text)).await;
    let msgs = recv_until_msg(client, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Out::Json(Msg::InputAccepted(acc))) = msgs.last() else {
        panic!("expected input.accepted")
    };
    assert_eq!(acc.ref_id, input_id);
    let turn_id = acc.turn_id.clone();
    let msgs = recv_until_msg(client, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Out::Json(Msg::StateTurn(st))) = msgs.last() else {
        panic!("expected state.turn")
    };
    assert_eq!(st.state, TurnState::Thinking);
    assert_eq!(st.turn_id, turn_id);
    turn_id
}

/// 断言窗口内每条下行 stream 的帧序：chunk_seq 0..N 递增、
/// sample_offset 按 1200 累计、仅末帧 EOS、载荷原样透传。
fn assert_stream_frames(seen: &Seen, stream_id: &str, sentence_seq: u32) {
    let raw = Ulid::from_string(
        stream_id
            .strip_prefix("st_")
            .expect("downlink stream_id carries st_ prefix"),
    )
    .expect("stream ulid");
    let fs: Vec<_> = seen
        .frames
        .iter()
        .filter(|f| f.0 == raw && f.2 == sentence_seq)
        .collect();
    assert_eq!(fs.len(), 3, "stream {stream_id} frame count");
    for (i, f) in fs.iter().enumerate() {
        assert_eq!(f.1, i as u32, "chunk_seq strictly increasing");
        assert_eq!(f.3, (i as u32) * 1200, "sample_offset cumulative");
        assert_eq!(f.4, i == 2, "EOS on last frame only");
        assert_eq!(
            f.5,
            if i == 2 { 0 } else { 2400 },
            "pcm payload passthrough"
        );
    }
}

// ---------- 骨架冒烟（router 装配） ----------

#[tokio::test]
async fn healthz_returns_200() {
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn ws_ui_requires_upgrade() {
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ws/ui")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_client_error());
}

// ---------- #16 验收场景 ----------

/// 文本全链：input.text → input.accepted → output.sentence → audio.open
/// → audio.chunk（st_<ULID> 流 + 32B 头）→ playback.* 回执 → output.complete
/// → turn.finished{completed}；LLM 请求体与 svc-tts 入站同步断言。
#[tokio::test]
async fn text_turn_full_pipeline() {
    let (llm_url, mut llm_rx) = spawn_mock_llm().await;
    let (tts_url, mut tts_items) = spawn_mock_tts().await;
    let (asr_url, _asr_items) = spawn_mock_asr("unused").await;
    let addr = serve_app(e2e_config(&llm_url, &asr_url, &tts_url)).await;
    let (mut client, sid) = connect_client(addr, None).await;
    assert!(
        sid.starts_with("sess_"),
        "server assigns sess_* id, got {sid}"
    );

    let t1 = open_text_turn(&mut client, "in_1", "讲个笑话").await;

    // 真 OpenAiCompatLlm 打到 mock：请求体断言（model/stream/messages）。
    let call = recv_llm_call(&mut llm_rx).await;
    assert_eq!(call.body["model"], "e2e-mock-model");
    assert!(call.body["stream"].as_bool().unwrap());
    let req_msgs = call.body["messages"].as_array().expect("messages array");
    assert_eq!(req_msgs.last().unwrap()["role"], "user");
    assert_eq!(req_msgs.last().unwrap()["content"], "讲个笑话");
    call.feed.send("你好。".into()).await.expect("delta");
    call.feed.send("世界。".into()).await.expect("delta");
    drop(call.feed); // → data: [DONE]

    // 窗口 1：两下行 stream 就位 + output.complete 到齐
    // （complete 由 actor 产出，可先于 sink 的 audio.open 到达，复合条件等待）。
    let mut opens = 0usize;
    let mut frames = 0usize;
    let mut complete = false;
    let items = recv_until(&mut client, |o| {
        match o {
            Out::Json(Msg::AudioOpen(a)) if a.direction == Direction::Downlink => opens += 1,
            Out::Json(Msg::OutputComplete(_)) => complete = true,
            Out::Audio(..) => frames += 1,
            _ => {}
        }
        opens >= 2 && frames >= 6 && complete
    })
    .await;
    assert_session_id(&items, &sid);
    assert_per_producer_seq_monotonic(&items);
    let seen = collect(&items);
    assert_eq!(
        seen.sentences,
        vec![
            (1, "你好。".to_string(), t1.clone()),
            (2, "世界。".to_string(), t1.clone()),
        ],
        "output.sentence ordered by seq"
    );
    assert_eq!(seen.opens.len(), 2, "audio.open per sentence stream");
    assert_eq!(seen.opens[0].1, 1);
    assert_eq!(seen.opens[1].1, 2);
    for (stream_id, sent_seq, open_turn) in &seen.opens {
        assert_eq!(open_turn, &t1, "audio.open binds reply turn");
        assert_stream_frames(&seen, stream_id, *sent_seq);
    }
    assert_ne!(seen.opens[0].0, seen.opens[1].0, "new stream per sentence");
    assert!(
        items
            .iter()
            .all(|o| !matches!(o, Out::Json(Msg::TurnFinished(_))))
    );

    // playback 回执驱动 thinking → speaking → idle → finished。
    let (s1, s2) = (seen.opens[0].0.clone(), seen.opens[1].0.clone());
    client.send_json(playback_started(&s1, 1)).await;
    let msgs = recv_until_msg(
        &mut client,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;
    let Some(Out::Json(Msg::StateTurn(st))) = msgs.last() else {
        panic!("expected state.turn speaking")
    };
    assert_eq!(st.turn_id, t1);

    client
        .send_json(playback_progress(&s1, 1, 2400, true))
        .await;
    client.send_json(playback_started(&s2, 2)).await;
    client
        .send_json(playback_progress(&s2, 2, 2400, true))
        .await;
    let msgs = recv_until_msg(&mut client, |m| matches!(m, Msg::TurnFinished(_))).await;
    let tail = collect(&msgs);
    // 两 stream 均以 audio.end 收尾（stream1 的 end 可能已在窗口 1 到达）。
    let total_ends = seen.ends.len() + tail.ends.len();
    assert_eq!(total_ends, 2, "audio.end per stream");
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::StateTurn(s)) if s.state == TurnState::Idle
    )));
    let finished: Vec<_> = msgs
        .iter()
        .filter(|o| matches!(o, Out::Json(Msg::TurnFinished(_))))
        .collect();
    assert_eq!(finished.len(), 1, "turn.finished exactly once");
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == t1 && f.outcome == TurnOutcome::Completed
    )));

    // svc-tts 侧：两句 output.sentence，wire 序 = 句序。
    for (seq, text) in [(1u64, "你好。"), (2, "世界。")] {
        let Item::Json(sent) = recv_report(&mut tts_items).await else {
            panic!("expected output.sentence at svc-tts")
        };
        assert_eq!(sent["type"], "output.sentence");
        assert_eq!(sent["turn_id"], t1);
        assert_eq!(sent["seq"], seq);
        assert_eq!(sent["text"], text);
    }
}

/// 语音全链：audio.open{uplink} + PCM 帧 + EOS → mock svc-asr 回
/// `input.audio.segment{final:true}` → 复用文本链后续事件直至
/// turn.finished{completed}。
#[tokio::test]
async fn voice_turn_full_pipeline() {
    let (llm_url, mut llm_rx) = spawn_mock_llm().await;
    let (tts_url, mut tts_items) = spawn_mock_tts().await;
    let (asr_url, mut asr_items) = spawn_mock_asr("今天天气怎么样").await;
    let addr = serve_app(e2e_config(&llm_url, &asr_url, &tts_url)).await;
    let (mut client, sid) = connect_client(addr, None).await;

    // capture 注册：input.audio.start → accepted{ref_id} + listening。
    client.send_json(audio_start("cap_v1")).await;
    let msgs = recv_until_msg(&mut client, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Out::Json(Msg::InputAccepted(acc))) = msgs.last() else {
        panic!("expected input.accepted")
    };
    assert_eq!(acc.ref_id, "cap_v1");
    let turn_id = acc.turn_id.clone();
    let msgs = recv_until_msg(&mut client, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Out::Json(Msg::StateTurn(st))) = msgs.last() else {
        panic!("expected state.turn")
    };
    assert_eq!(st.state, TurnState::Listening);

    // uplink 绑定 + 2×1600B 数据帧 + EOS + audio.end。
    let stream = Ulid::new();
    client.send_json(audio_open_uplink(stream, "cap_v1")).await;
    client.send_audio(stream, 0, 0, &[0x11; 1600], false).await;
    client
        .send_audio(stream, 1, 800, &[0x22; 1600], false)
        .await;
    client.send_audio(stream, 2, 1600, &[], true).await;
    client.send_json(audio_end_uplink(stream, 2)).await;

    // mock svc-asr 侧：restamp 的 audio.open + 原样帧 + audio.end。
    let Item::Json(open) = recv_report(&mut asr_items).await else {
        panic!("expected audio.open at svc-asr")
    };
    assert_eq!(open["type"], "audio.open");
    assert_eq!(open["direction"], "uplink");
    assert_eq!(open["capture_id"], "cap_v1");
    assert_eq!(open["stream_id"], format!("st_{stream}"));
    assert_eq!(open["sample_rate"], 16000);
    assert_eq!(open["codec"], "pcm_s16le");
    assert_eq!(open["session_id"], MOCK_SESSION);
    for (seq, offset) in [(0u32, 0u32), (1, 800), (2, 1600)] {
        let Item::Audio(header, payload) = recv_report(&mut asr_items).await else {
            panic!("expected uplink frame {seq}")
        };
        assert_eq!(header.direction, Direction::Uplink);
        assert_eq!(header.stream_id, stream);
        assert_eq!(header.chunk_seq, seq);
        assert_eq!(header.sample_offset, offset);
        assert_eq!(header.eos(), seq == 2);
        assert_eq!(payload.len(), if seq == 2 { 0 } else { 1600 });
    }
    let Item::Json(end) = recv_report(&mut asr_items).await else {
        panic!("expected audio.end at svc-asr")
    };
    assert_eq!(end["type"], "audio.end");
    assert_eq!(end["last_chunk_seq"], 2);

    // mock 回 segment：partial 只镜像，final 收敛 capture 为 reply turn。
    let msgs = recv_until_msg(
        &mut client,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::InputAudioSegment(s))
            if !s.is_final && s.capture_id == "cap_v1"
    )));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::InputAudioSegment(s))
            if s.is_final && s.text == "今天天气怎么样"
    )));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::InputAccepted(a)) if a.ref_id == "cap_v1" && a.turn_id == turn_id
    )));

    // 转写文本经真 OpenAiCompatLlm 进 prompt。
    let call = recv_llm_call(&mut llm_rx).await;
    let req_msgs = call.body["messages"].as_array().expect("messages array");
    assert_eq!(req_msgs.last().unwrap()["role"], "user");
    assert_eq!(req_msgs.last().unwrap()["content"], "今天天气怎么样");
    call.feed.send("今天晴朗。".into()).await.expect("delta");
    drop(call.feed);

    // 同文本链：output.sentence → audio.open → audio.chunk → playback → finished。
    let mut opens = 0usize;
    let mut frames = 0usize;
    let mut complete = false;
    let items = recv_until(&mut client, |o| {
        match o {
            Out::Json(Msg::AudioOpen(a)) if a.direction == Direction::Downlink => opens += 1,
            Out::Json(Msg::OutputComplete(_)) => complete = true,
            Out::Audio(..) => frames += 1,
            _ => {}
        }
        opens >= 1 && frames >= 3 && complete
    })
    .await;
    assert_session_id(&items, &sid);
    assert_per_producer_seq_monotonic(&items);
    let seen = collect(&items);
    assert_eq!(seen.sentences.len(), 1);
    assert_eq!(seen.sentences[0].0, 1);
    assert_eq!(seen.sentences[0].1, "今天晴朗。");
    assert_eq!(seen.sentences[0].2, turn_id);
    assert_eq!(seen.opens.len(), 1, "audio.open for reply stream");
    assert_eq!(seen.opens[0].1, 1);
    assert_eq!(seen.opens[0].2, turn_id);
    assert_stream_frames(&seen, &seen.opens[0].0, 1);

    let s1 = seen.opens[0].0.clone();
    client.send_json(playback_started(&s1, 1)).await;
    client
        .send_json(playback_progress(&s1, 1, 2400, true))
        .await;
    let msgs = recv_until_msg(&mut client, |m| matches!(m, Msg::TurnFinished(_))).await;
    let finished: Vec<_> = msgs
        .iter()
        .filter(|o| matches!(o, Out::Json(Msg::TurnFinished(_))))
        .collect();
    assert_eq!(finished.len(), 1, "turn.finished exactly once");
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == turn_id && f.outcome == TurnOutcome::Completed
    )));

    // svc-tts 侧收到该句 output.sentence。
    let Item::Json(sent) = recv_report(&mut tts_items).await else {
        panic!("expected output.sentence at svc-tts")
    };
    assert_eq!(sent["type"], "output.sentence");
    assert_eq!(sent["turn_id"], turn_id);
    assert_eq!(sent["text"], "今天晴朗。");
}

/// barge-in：speaking 中 `control.barge_in{turn_id}` → stop notice
/// （`control.cancel{target:turn}` 下发 UI）→ `state.turn{interrupted}` →
/// `turn.finished{interrupted}` 恰一次；mock svc-tts 收到同 id 的
/// `control.cancel{target:turn}`。
#[tokio::test]
async fn barge_in_interrupts_turn_and_cancels_tts() {
    let (llm_url, mut llm_rx) = spawn_mock_llm().await;
    let (tts_url, mut tts_items) = spawn_mock_tts().await;
    let (asr_url, _asr_items) = spawn_mock_asr("unused").await;
    let addr = serve_app(e2e_config(&llm_url, &asr_url, &tts_url)).await;
    let (mut client, _sid) = connect_client(addr, None).await;

    let t1 = open_text_turn(&mut client, "in_1", "讲个长故事").await;

    // generation 在途：feed 保持不 drop，回合停在 speaking。
    let call = recv_llm_call(&mut llm_rx).await;
    call.feed.send("第一句。".into()).await.expect("delta");

    // 等首个下行 stream 就位后发 playback.started → speaking。
    let items = recv_until(
        &mut client,
        |o| matches!(o, Out::Json(Msg::AudioOpen(a)) if a.direction == Direction::Downlink),
    )
    .await;
    let seen = collect(&items);
    assert_eq!(seen.opens.len(), 1);
    let s1 = seen.opens[0].0.clone();
    client.send_json(playback_started(&s1, 1)).await;
    let _ = recv_until_msg(
        &mut client,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;

    // 打断：stop notice（control.cancel）→ interrupted → finished 恰一次。
    client.send_json(barge_in(&t1)).await;
    let msgs = recv_until_msg(&mut client, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::ControlCancel(c)) if c.target == CancelTarget::Turn && c.id == t1
    )));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::StateTurn(s))
            if s.turn_id == t1 && s.state == TurnState::Interrupted
    )));
    let finished: Vec<_> = msgs
        .iter()
        .filter(|o| matches!(o, Out::Json(Msg::TurnFinished(_))))
        .collect();
    assert_eq!(finished.len(), 1, "turn.finished exactly once");
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == t1 && f.outcome == TurnOutcome::Interrupted
    )));

    // 取消扇出：mock svc-tts 收到 control.cancel{target:turn,id}。
    loop {
        let Item::Json(v) = recv_report(&mut tts_items).await else {
            panic!("expected control.cancel at svc-tts")
        };
        if v["type"] == "control.cancel" {
            assert_eq!(v["target"], "turn");
            assert_eq!(v["id"], t1);
            break;
        }
    }

    // 无第二次 turn.finished（settlement 只回 idle）。
    let late = drain_for(&mut client, Duration::from_millis(600)).await;
    assert!(
        late.iter()
            .all(|o| !matches!(o, Out::Json(Msg::TurnFinished(_)))),
        "no duplicate turn.finished after settlement"
    );
    drop(call.feed);
}

/// 断线 resume：conn A 完成一回合后断开 → conn B 带 `resume_session_id`
/// 重连 → 同 session_id `hello.ok` + `session.snapshot` 收敛
/// （turn=null、receipts 记账一致、last_event_seq 水位）→ 续回合可用。
#[tokio::test]
async fn resume_recovers_session_snapshot() {
    let (llm_url, mut llm_rx) = spawn_mock_llm().await;
    let (tts_url, _tts_items) = spawn_mock_tts().await;
    let (asr_url, _asr_items) = spawn_mock_asr("unused").await;
    let addr = serve_app(e2e_config(&llm_url, &asr_url, &tts_url)).await;

    // conn A：完成一个文字回合后断线。
    let (mut a, sid_a) = connect_client(addr, None).await;
    let t1 = open_text_turn(&mut a, "in_1", "你好").await;
    let call = recv_llm_call(&mut llm_rx).await;
    call.feed.send("你好呀。".into()).await.expect("delta");
    drop(call.feed);
    let mut opens = 0usize;
    let mut frames = 0usize;
    let mut complete = false;
    let items = recv_until(&mut a, |o| {
        match o {
            Out::Json(Msg::AudioOpen(x)) if x.direction == Direction::Downlink => opens += 1,
            Out::Json(Msg::OutputComplete(_)) => complete = true,
            Out::Audio(..) => frames += 1,
            _ => {}
        }
        opens >= 1 && frames >= 3 && complete
    })
    .await;
    let seen = collect(&items);
    assert_eq!(seen.opens.len(), 1);
    let s1 = seen.opens[0].0.clone();
    a.send_json(playback_started(&s1, 1)).await;
    a.send_json(playback_progress(&s1, 1, 2400, true)).await;
    let _ = recv_until_msg(&mut a, |m| matches!(m, Msg::TurnFinished(_))).await;
    a.disconnect().await;
    // 等服务端消费 close（ConnClosed 入队 → session 进保活窗口）。
    tokio::time::sleep(Duration::from_millis(150)).await;

    // conn B：resume 命中 → 同 session_id + snapshot。
    let (mut b, sid_b) = connect_client(addr, Some(&sid_a)).await;
    assert_eq!(sid_b, sid_a, "resume keeps session id");
    let Msg::SessionSnapshot(snap) = b.recv_msg().await else {
        panic!("expected session.snapshot")
    };
    assert_eq!(snap.session_id, sid_a);
    assert_eq!(snap.turn, None, "finished turn converges to null");
    assert!(
        snap.receipts
            .iter()
            .any(|r| r.id == "in_1" && r.turn_id == t1 && r.status == ReceiptStatus::Completed),
        "receipt cache keeps completed status: {:?}",
        snap.receipts
    );
    assert!(snap.capabilities.iter().any(|c| c == "barge_in"));
    assert!(snap.last_event_seq > 0, "event_seq watermark reported");

    // resume 后管线可用：history 含断线前回合，续回合照常收敛。
    let t2 = open_text_turn(&mut b, "in_2", "再说一遍").await;
    let call2 = recv_llm_call(&mut llm_rx).await;
    let req_msgs = call2.body["messages"].as_array().expect("messages array");
    assert_eq!(req_msgs[0]["role"], "system");
    assert_eq!(req_msgs[1]["role"], "user");
    assert_eq!(req_msgs[1]["content"], "你好");
    assert_eq!(req_msgs[2]["role"], "assistant");
    assert_eq!(req_msgs[2]["content"], "你好呀。");
    assert_eq!(req_msgs[3]["content"], "再说一遍");
    call2.feed.send("你好呀。".into()).await.expect("delta");
    drop(call2.feed);
    let mut opens = 0usize;
    let mut complete = false;
    let items = recv_until(&mut b, |o| {
        match o {
            Out::Json(Msg::AudioOpen(x)) if x.direction == Direction::Downlink => opens += 1,
            Out::Json(Msg::OutputComplete(_)) => complete = true,
            _ => {}
        }
        opens >= 1 && complete
    })
    .await;
    let seen = collect(&items);
    let s2 = seen.opens[0].0.clone();
    b.send_json(playback_started(&s2, 1)).await;
    b.send_json(playback_progress(&s2, 1, 2400, true)).await;
    let msgs = recv_until_msg(&mut b, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == t2 && f.outcome == TurnOutcome::Completed
    )));
}
