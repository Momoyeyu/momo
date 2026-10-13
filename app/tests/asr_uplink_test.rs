//! 语音上行 e2e：真 `conn::serve` → Session → `MomoWsAsr` → axum mock svc-asr。
//! 覆盖 #14 uplink 转发（audio.open 绑定/帧序/audio.end/seq gap 中止）、
//! #15 VAD→barge-in 防抖与取消扇出、#26 partial 镜像 + final→新 turn 上下文。

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    Router,
    extract::ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    routing::get,
};
use bytes::Bytes;
use futures_channel::mpsc as fmpsc;
use futures_util::{SinkExt, StreamExt};
use momo_app::{
    config::Config,
    error::AppError,
    persona::Persona,
    pipeline::{
        asr::MomoWsAsr,
        llm::{ChatRequest, ChatStream, LlmClient},
        tts::{AudioStream, SynthRequest, TtsClient},
    },
    proto::{
        codec::{self, AudioHeader, Direction, encode_audio_frame},
        message::{
            BargeInMsg, BargeInReason, CancelTarget, Envelope, ErrorCode, Msg, TurnOutcome,
            TurnState, new_id,
        },
    },
    ws::{conn, server::AppState},
};
use serde_json::{Value, json};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use ulid::Ulid;

const STREAM_ULID: &str = "01J5M0N8T1RZ8K2W4V7H6X9P3E";

fn stream_id() -> String {
    format!("st_{STREAM_ULID}")
}

fn stream_ulid() -> Ulid {
    Ulid::from_string(STREAM_ULID).unwrap()
}

// ---------- mock svc-asr ----------

/// mock 服务端 → 测试体的回报单元。
#[derive(Debug)]
enum Item {
    Json(Value),
    Audio(AudioHeader, Bytes),
}

/// 起 mock svc-asr 并返回 ws:// 地址 + 接收项回流通道。
async fn spawn_mock<F, Fut>(handler: F) -> (url::Url, mpsc::Receiver<Item>)
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url::Url::parse(&format!("ws://{addr}/ws")).unwrap(), rx)
}

/// 服务端视角收一帧：文本 → JSON，二进制 → 解码头。
async fn recv_item(sock: &mut WebSocket) -> Item {
    match sock.recv().await {
        Some(Ok(Message::Text(t))) => Item::Json(serde_json::from_str(t.as_str()).unwrap()),
        Some(Ok(Message::Binary(b))) => {
            let (header, payload) = AudioHeader::decode(&b).unwrap();
            Item::Audio(header, Bytes::copy_from_slice(payload))
        }
        other => panic!("mock expected data frame, got {other:?}"),
    }
}

async fn send_json(sock: &mut WebSocket, v: Value) {
    sock.send(Message::Text(v.to_string().into()))
        .await
        .unwrap();
}

fn hello_ok(role: &str) -> Value {
    json!({
        "type": "hello.ok",
        "version": "v2",
        "session_id": "sess_mock",
        "role": role,
        "capabilities": [],
    })
}

/// 服务端视角 hello 握手（断言 client=orchestrator 后回 role）。
async fn mock_handshake(sock: &mut WebSocket, role: &str) {
    let Item::Json(hello) = recv_item(sock).await else {
        panic!("expected hello");
    };
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["client"], "orchestrator");
    send_json(sock, hello_ok(role)).await;
}

fn vad_json(capture_id: &str, event: &str) -> Value {
    json!({
        "type": "input.vad",
        "session_id": "sess_mock",
        "event_id": new_id("e"),
        "at_ms": 1,
        "event_seq": 1,
        "capture_id": capture_id,
        "event": event,
    })
}

fn segment_json(capture_id: &str, text: &str, is_final: bool) -> Value {
    json!({
        "type": "input.audio.segment",
        "session_id": "sess_mock",
        "event_id": new_id("e"),
        "at_ms": 1,
        "event_seq": 1,
        "capture_id": capture_id,
        "text": text,
        "final": is_final,
    })
}

// ---------- 测试侧 session/conn ----------

/// 手动喂 delta 的 mock LLM（同 session_turn_test 模式）。
#[derive(Default)]
struct MockLlm {
    requests: Mutex<Vec<Vec<ChatMessage>>>,
    feeds: Mutex<Vec<mpsc::Sender<Result<String, AppError>>>>,
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn chat_stream(
        &self,
        req: ChatRequest,
        _cancel: CancellationToken,
    ) -> Result<ChatStream, AppError> {
        self.requests.lock().expect("mock").push(req.messages);
        let (tx, rx) = mpsc::channel(8);
        self.feeds.lock().expect("mock").push(tx);
        Ok(Box::pin(futures_util::stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|item| (item, rx)) },
        )))
    }
}

async fn take_feed(llm: &MockLlm) -> mpsc::Sender<Result<String, AppError>> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(tx) = llm.feeds.lock().expect("mock").pop() {
                return tx;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("llm request feed")
}

/// 记录 cancel_turn 的 TTS stub（barge-in 扇出断言）。
#[derive(Default)]
struct RecordingTts {
    cancels: Mutex<Vec<String>>,
}

#[async_trait]
impl TtsClient for RecordingTts {
    async fn synth_stream(
        &self,
        _req: SynthRequest,
        _cancel: CancellationToken,
    ) -> Result<AudioStream, AppError> {
        Err(AppError::Internal("tts unused in asr uplink tests".into()))
    }

    async fn cancel_turn(&self, turn_id: &str) -> Result<(), AppError> {
        self.cancels.lock().expect("mock").push(turn_id.to_string());
        Ok(())
    }
}

fn test_state(
    llm: Option<Arc<MockLlm>>,
    asr_url: url::Url,
    tts: Option<Arc<RecordingTts>>,
    barge_in_min: Duration,
) -> Arc<AppState> {
    let deps = SessionDeps {
        history_turns: 20,
        sent_soft_chars: 25,
        sent_max_chars: 60,
        llm: llm.map(|l| {
            let client: Arc<dyn LlmClient> = l;
            client
        }),
        asr: Some(Arc::new(MomoWsAsr::new(asr_url))),
        tts: tts.map(|t| {
            let client: Arc<dyn TtsClient> = t;
            client
        }),
        barge_in_enabled: true,
        barge_in_min,
        resume_ttl: Duration::from_secs(60),
        persona: Persona::new(std::path::PathBuf::from("persona/momo.yaml")),
    };
}

/// 测试侧 UI 连接（inbound 注入帧，outbound 收服务端帧）。
struct TestConn {
    inbound: fmpsc::Sender<Message>,
    outbound: fmpsc::Receiver<Message>,
    _task: JoinHandle<()>,
}

fn connect(state: Arc<AppState>) -> TestConn {
    let (in_tx, in_rx) = fmpsc::channel::<Message>(64);
    let (out_tx, out_rx) = fmpsc::channel::<Message>(64);
    let stream = in_rx.map(Ok::<Message, axum::Error>);
    let task = tokio::spawn(conn::serve(stream, out_tx, state));
    TestConn {
        inbound: in_tx,
        outbound: out_rx,
        _task: task,
    }
}

fn hello_json() -> String {
    json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v2",
        "client": "ui",
        "auth_token": "ephemeral-token",
    })
    .to_string()
}

async fn send_text(conn: &mut TestConn, v: Value) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(v.to_string())))
        .await
        .expect("inbound send");
}

async fn send_audio(
    conn: &mut TestConn,
    chunk_seq: u32,
    sample_offset: u32,
    payload: &[u8],
    eos: bool,
) {
    let header = AudioHeader {
        direction: Direction::Uplink,
        flags: if eos { codec::FLAG_EOS } else { 0 },
        stream_id: stream_ulid(),
        chunk_seq,
        sentence_seq: 0,
        sample_offset,
    };
    conn.inbound
        .send(Message::Binary(encode_audio_frame(header, payload)))
        .await
        .expect("inbound send");
}

/// 收一帧服务端 JSON 消息（跳过非文本帧）。
async fn recv_msg(conn: &mut TestConn) -> Msg {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), conn.outbound.next())
            .await
            .expect("server frame timeout")
            .expect("connection closed");
        if let Message::Text(text) = msg {
            return codec::decode_msg(text.as_str()).expect("server frame is valid Msg");
        }
    }
}

async fn recv_until(conn: &mut TestConn, pred: impl Fn(&Msg) -> bool) -> Vec<Msg> {
    let mut seen = Vec::new();
    loop {
        let msg = recv_msg(conn).await;
        let hit = pred(&msg);
        seen.push(msg);
        if hit {
            return seen;
        }
    }
}

/// 窗口期内排空 outbound，返回收到的消息（用于"未打断"断言）。
async fn drain_for(conn: &mut TestConn, window: Duration) -> Vec<Msg> {
    let deadline = Instant::now() + window;
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return seen;
        }
        match tokio::time::timeout(left, conn.outbound.next()).await {
            Ok(Some(Message::Text(t))) => {
                seen.push(codec::decode_msg(t.as_str()).expect("valid Msg"));
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => return seen,
        }
    }
}

async fn handshake(conn: &mut TestConn) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(hello_json())))
        .await
        .unwrap();
    let Msg::HelloOk(_) = recv_msg(conn).await else {
        panic!("expected hello.ok");
    };
}

fn client_env() -> Envelope {
    Envelope {
        session_id: "sess_test".into(),
        event_id: new_id("e"),
        at_ms: 1,
        reply_to: None,
    }
}

fn audio_start(capture_id: &str) -> Value {
    json!({
        "type": "input.audio.start",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "capture_id": capture_id,
        "mode": "continuous",
    })
}

fn audio_open_uplink(capture_id: &str) -> Value {
    json!({
        "type": "audio.open",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id(),
        "direction": "uplink",
        "capture_id": capture_id,
        "codec": "pcm_s16le",
        "sample_rate": 16000,
        "channels": 1,
    })
}

fn audio_end(last_chunk_seq: u32) -> Value {
    json!({
        "type": "audio.end",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id(),
        "last_chunk_seq": last_chunk_seq,
    })
}

fn input_text(id: &str, text: &str) -> Value {
    json!({
        "type": "input.text",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "id": id,
        "text": text,
        "lang": "zh",
    })
}

fn playback_started(sentence_seq: u32) -> Value {
    json!({
        "type": "playback.started",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id(),
        "sentence_seq": sentence_seq,
    })
}

fn playback_progress(sentence_seq: u32, played: u64, completed: bool) -> Value {
    json!({
        "type": "playback.progress",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id(),
        "sentence_seq": sentence_seq,
        "played_samples": played,
        "completed": completed,
    })
}

fn barge_in(turn_id: &str) -> Value {
    let m = BargeInMsg {
        env: client_env(),
        turn_id: turn_id.into(),
        reason: BargeInReason::User,
    };
    serde_json::to_value(Msg::ControlBargeIn(m)).unwrap()
}

/// 开文字回合并取 turn_id（等待 input.accepted + state.turn thinking）。
async fn open_text_turn(conn: &mut TestConn, input_id: &str, text: &str) -> String {
    send_text(conn, input_text(input_id, text)).await;
    let msgs = recv_until(conn, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Msg::InputAccepted(acc)) = msgs.last() else {
        panic!("expected input.accepted")
    };
    let turn_id = acc.turn_id.clone();
    let msgs = recv_until(conn, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn")
    };
    assert_eq!(st.state, TurnState::Thinking);
    turn_id
}

// ---------- 测试 ----------

/// #14：audio.open{uplink} 绑定 → 二进制帧原样转发 → audio.end；
/// 回流 partial 只镜像、final 收敛 capture 为 reply turn。
#[tokio::test]
async fn uplink_forwards_frames_and_final_segment_replies() {
    let (url, mut items) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        // audio.open + 3 数据帧（末帧 EOS）+ audio.end，逐项回流断言。
        for _ in 0..5 {
            let item = recv_item(&mut sock).await;
            report.send(item).await.unwrap();
        }
        // 回流 partial + final segment。
        send_json(&mut sock, segment_json("cap_1", "语音问", false)).await;
        send_json(&mut sock, segment_json("cap_1", "语音问", true)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(
        Some(llm.clone()),
        url,
        None,
        Duration::from_millis(120),
    ));
    handshake(&mut conn).await;

    // capture + uplink 绑定。
    send_text(&mut conn, audio_start("cap_1")).await;
    let msgs = recv_until(&mut conn, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Msg::InputAccepted(acc)) = msgs.last() else {
        panic!("expected input.accepted")
    };
    assert_eq!(acc.ref_id, "cap_1");
    let capture_turn = acc.turn_id.clone();
    let _ = recv_until(&mut conn, |m| matches!(m, Msg::StateTurn(_))).await;

    send_text(&mut conn, audio_open_uplink("cap_1")).await;
    send_audio(&mut conn, 0, 0, &[0x11; 1600], false).await;
    send_audio(&mut conn, 1, 800, &[0x22; 1600], false).await;
    send_audio(&mut conn, 2, 1600, &[], true).await;
    send_text(&mut conn, audio_end(2)).await;

    // mock 侧断言：restamp envelope 的 audio.open + 原样帧 + audio.end。
    let Item::Json(open) = items.recv().await.unwrap() else {
        panic!("expected audio.open")
    };
    assert_eq!(open["type"], "audio.open");
    assert_eq!(open["direction"], "uplink");
    assert_eq!(open["capture_id"], "cap_1");
    assert_eq!(open["stream_id"], stream_id());
    assert_eq!(open["sample_rate"], 16000);
    assert_eq!(open["channels"], 1);
    assert_eq!(open["codec"], "pcm_s16le");
    assert_eq!(open["session_id"], "sess_mock");
    assert!(open.get("event_seq").is_none());

    for (seq, offset) in [(0u32, 0u32), (1, 800), (2, 1600)] {
        let Item::Audio(header, payload) = items.recv().await.unwrap() else {
            panic!("expected audio frame {seq}")
        };
        assert_eq!(header.direction, Direction::Uplink);
        assert_eq!(header.stream_id, stream_ulid());
        assert_eq!(header.chunk_seq, seq);
        assert_eq!(header.sample_offset, offset);
        assert_eq!(header.eos(), seq == 2);
        assert_eq!(payload.len(), if seq == 2 { 0 } else { 1600 });
    }

    let Item::Json(end) = items.recv().await.unwrap() else {
        panic!("expected audio.end")
    };
    assert_eq!(end["type"], "audio.end");
    assert_eq!(end["stream_id"], stream_id());
    assert_eq!(end["last_chunk_seq"], 2);

    // partial 只镜像 UI，不开 turn。
    let msgs = recv_until(&mut conn, |m| matches!(m, Msg::InputAudioSegment(_))).await;
    let Some(Msg::InputAudioSegment(seg)) = msgs.last() else {
        panic!("expected mirrored segment")
    };
    assert!(!seg.is_final);
    assert_eq!(seg.capture_id, "cap_1");
    assert!(msgs.iter().all(|m| !matches!(
        m,
        Msg::StateTurn(s) if s.state == TurnState::Thinking
    )));

    // final：镜像 + input.accepted{ref_id=capture_id} + 同体 turn 转 thinking。
    let msgs = recv_until(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::InputAudioSegment(s) if s.is_final && s.text == "语音问"
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::InputAccepted(a) if a.ref_id == "cap_1" && a.turn_id == capture_turn
    )));
    let Some(Msg::StateTurn(st)) = msgs.last() else {
        panic!("expected state.turn thinking")
    };
    assert_eq!(
        st.turn_id, capture_turn,
        "capture turn continues as reply turn"
    );

    // 转写文本进入 LLM prompt。
    let _feed = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[0].clone();
    assert_eq!(req.last().expect("prompt").content, "语音问");
    assert_eq!(req.last().expect("prompt").role, Role::User);
}

/// #15：speech_start 未达 MOMO_BARGEIN_MIN_MS 不打断；持续满防抖后
/// interrupted → listening，turn.finished 恰好一次，control.cancel 扇出上游。
#[tokio::test]
async fn vad_speech_start_debounces_then_barges_in() {
    let (url, mut items) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        // 等 audio.open 确认订阅/链路就位后发 speech_start，再收 control.cancel。
        let open = recv_item(&mut sock).await;
        report.send(open).await.unwrap();
        send_json(&mut sock, vad_json("cap_vad", "speech_start")).await;
        let cancel = recv_item(&mut sock).await;
        report.send(cancel).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let tts = Arc::new(RecordingTts::default());
    let mut conn = connect(test_state(
        Some(llm.clone()),
        url,
        Some(tts.clone()),
        Duration::from_millis(120),
    ));
    handshake(&mut conn).await;

    let t1 = open_text_turn(&mut conn, "in_1", "问").await;
    let _feed = take_feed(&llm).await; // generation 在途，保持 thinking

    // uplink 绑定只建链路（不注册 capture turn）。
    send_text(&mut conn, audio_open_uplink("cap_u")).await;
    let Item::Json(_) = items.recv().await.unwrap() else {
        panic!("expected audio.open at svc-asr")
    };

    // 防抖窗口内：只有镜像的 input.vad，无 interrupted。
    let early = drain_for(&mut conn, Duration::from_millis(60)).await;
    assert!(
        early
            .iter()
            .any(|m| matches!(m, Msg::InputVad(v) if v.capture_id == "cap_vad")),
        "mirrored input.vad expected"
    );
    assert!(early.iter().all(|m| !matches!(
        m,
        Msg::StateTurn(s) if s.state == TurnState::Interrupted
    )));
    assert!(early.iter().all(|m| !matches!(m, Msg::TurnFinished(_))));

    // 防抖到点：interrupted → turn.finished + 新 capture listening。
    let msgs = recv_until(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Listening),
    )
    .await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::StateTurn(s) if s.turn_id == t1
            && s.state == TurnState::Interrupted
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::ControlCancel(c) if c.target == CancelTarget::Turn && c.id == t1
    )));
    let finished: Vec<_> = msgs
        .iter()
        .filter(|m| matches!(m, Msg::TurnFinished(_)))
        .collect();
    assert_eq!(finished.len(), 1, "turn.finished exactly once");
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1
            && f.outcome == TurnOutcome::Interrupted
    )));

    // 取消扇出：svc-asr 收 control.cancel{target:turn}；TTS stub 记录同 id。
    let Item::Json(cancel) = items.recv().await.unwrap() else {
        panic!("expected control.cancel at svc-asr")
    };
    assert_eq!(cancel["type"], "control.cancel");
    assert_eq!(cancel["target"], "turn");
    assert_eq!(cancel["id"], t1);
    assert_eq!(tts.cancels.lock().expect("mock").as_slice(), [t1]);
}

/// #15：防抖窗口内 speech_end → false positive，不打断活动回合。
#[tokio::test]
async fn speech_end_before_debounce_cancels_barge_in() {
    let (url, mut items) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        let open = recv_item(&mut sock).await;
        report.send(open).await.unwrap();
        send_json(&mut sock, vad_json("cap_s", "speech_start")).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        send_json(&mut sock, vad_json("cap_s", "speech_end")).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(
        Some(llm.clone()),
        url,
        None,
        Duration::from_millis(120),
    ));
    handshake(&mut conn).await;
    let t1 = open_text_turn(&mut conn, "in_1", "问").await;
    let _feed = take_feed(&llm).await;

    send_text(&mut conn, audio_open_uplink("cap_u")).await;
    let Item::Json(_) = items.recv().await.unwrap() else {
        panic!("expected audio.open at svc-asr")
    };

    // 防抖全程：仅镜像两条 input.vad，无 interrupted/turn.finished。
    let seen = drain_for(&mut conn, Duration::from_millis(400)).await;
    let vads = seen
        .iter()
        .filter(|m| matches!(m, Msg::InputVad(_)))
        .count();
    assert_eq!(vads, 2, "mirrored speech_start + speech_end");
    assert!(seen.iter().all(|m| !matches!(
        m,
        Msg::StateTurn(s) if s.state == TurnState::Interrupted
    )));
    assert!(
        seen.iter().all(|m| !matches!(m, Msg::TurnFinished(_))),
        "no interruption expected"
    );
    let _ = t1;
}

/// #26：打断旧 turn 后 voice final → 新 reply turn；已播 partial 注入新 prompt。
#[tokio::test]
async fn final_segment_after_barge_in_carries_spoken_context() {
    let (url, mut items) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        let open = recv_item(&mut sock).await;
        report.send(open).await.unwrap();
        // 等 control.barge_in 生效（客户端会先停播），再发 final segment。
        tokio::time::sleep(Duration::from_millis(80)).await;
        send_json(&mut sock, segment_json("cap_2", "追问", true)).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(
        Some(llm.clone()),
        url,
        None,
        Duration::from_millis(120),
    ));
    handshake(&mut conn).await;
    let t1 = open_text_turn(&mut conn, "in_1", "问").await;

    // 一句已生成且已播（committed）。
    let feed = take_feed(&llm).await;
    feed.send(Ok("第一句。".into())).await.expect("delta");
    let _ = recv_until(&mut conn, |m| matches!(m, Msg::OutputSentence(_))).await;
    send_text(&mut conn, playback_started(1)).await;
    let _ = recv_until(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;
    send_text(&mut conn, playback_progress(1, 1200, true)).await;

    // uplink 链路就位（只建链路），随后显式 barge-in。
    send_text(&mut conn, audio_open_uplink("cap_u")).await;
    let Item::Json(_) = items.recv().await.unwrap() else {
        panic!("expected audio.open at svc-asr")
    };
    send_text(&mut conn, barge_in(&t1)).await;
    let _ = recv_until(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Interrupted),
    )
    .await;

    // voice final：旧 turn finished(interrupted) + 新 turn thinking。
    let msgs = recv_until(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Thinking),
    )
    .await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.turn_id == t1
            && f.outcome == TurnOutcome::Interrupted
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::InputAccepted(a) if a.ref_id == "cap_2" && a.turn_id != t1
    )));

    // 新 prompt：已播句以 partial assistant 注入 + 新 user 转写。
    let _feed2 = take_feed(&llm).await;
    let req = llm.requests.lock().expect("mock")[1].clone();
    assert_eq!(req.len(), 4);
    assert_eq!(req[0].role, Role::System);
    assert_eq!(req[1].role, Role::User);
    assert_eq!(req[1].content, "问");
    assert_eq!(req[2].role, Role::Assistant);
    assert_eq!(req[2].content, "第一句。");
    assert!(req[2].partial, "interrupted spoken context is partial");
    assert_eq!(req[3].role, Role::User);
    assert_eq!(req[3].content, "追问");
}

/// #14：chunk_seq gap → 中止 capture：UI 收 error + control.cancel{capture}，
/// svc-asr 侧收 control.cancel{capture}，不静默丢帧。
#[tokio::test]
async fn uplink_seq_gap_aborts_capture() {
    let (url, mut items) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock, "asr").await;
        // audio.open + 1 数据帧（seq0）+ control.cancel{capture}。
        for _ in 0..3 {
            let item = recv_item(&mut sock).await;
            report.send(item).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    })
    .await;

    let mut conn = connect(test_state(None, url, None, Duration::from_millis(120)));
    handshake(&mut conn).await;
    send_text(&mut conn, audio_start("cap_g")).await;
    let _ = recv_until(&mut conn, |m| matches!(m, Msg::InputAccepted(_))).await;
    let _ = recv_until(&mut conn, |m| matches!(m, Msg::StateTurn(_))).await;
    send_text(&mut conn, audio_open_uplink("cap_g")).await;
    send_audio(&mut conn, 0, 0, &[0x33; 1600], false).await;
    // 跳过 seq 1：gap 必须中止 capture（stale 规则）。
    send_audio(&mut conn, 2, 1600, &[0x44; 1600], false).await;

    let msgs = recv_until(&mut conn, |m| matches!(m, Msg::TurnFinished(_))).await;
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::Error(e) if e.code == ErrorCode::BadMessage
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::ControlCancel(c) if c.target == CancelTarget::Capture && c.id == "cap_g"
    )));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Msg::TurnFinished(f) if f.outcome == TurnOutcome::Interrupted
    )));

    // svc-asr 侧：open → seq0 帧 → control.cancel{target:capture}。
    let Item::Json(_) = items.recv().await.unwrap() else {
        panic!("expected audio.open")
    };
    let Item::Audio(header, _) = items.recv().await.unwrap() else {
        panic!("expected data frame")
    };
    assert_eq!(header.chunk_seq, 0);
    let Item::Json(cancel) = items.recv().await.unwrap() else {
        panic!("expected control.cancel at svc-asr")
    };
    assert_eq!(cancel["type"], "control.cancel");
    assert_eq!(cancel["target"], "capture");
    assert_eq!(cancel["id"], "cap_g");
}
