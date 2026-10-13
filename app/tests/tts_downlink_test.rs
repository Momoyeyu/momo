//! TTS 下行 e2e：真 `conn::serve` → Session → `MomoWsTts` → axum mock svc-tts。
//! 覆盖 #141：`output.sentence` 上线 → PCM 帧经 `AudioSink` 下行
//! （`audio.open`/二进制帧/`audio.end`）→ `playback.*` 回执驱动
//! thinking→speaking→idle；合成失败降级纯文字；barge-in 取消扇出。

use std::future::Future;
use std::sync::{Arc, Mutex};

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
    pipeline::{
        llm::{ChatRequest, ChatStream, LlmClient},
        tts::MomoWsTts,
    },
    proto::{
        codec::{self, AudioHeader, Direction, FLAG_EOS, encode_audio_frame},
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

// ---------- mock svc-tts ----------

/// 起 mock svc-tts 并返回 ws:// 地址 + 服务端收到 JSON 的回流通道。
/// `handler` 驱动剧本（回 PCM/回 error）；断言全放测试体。
async fn spawn_mock<F, Fut>(handler: F) -> (url::Url, mpsc::Receiver<Value>)
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
    (url::Url::parse(&format!("ws://{addr}/ws")).unwrap(), rx)
}

/// 服务端视角收一条文本 JSON；二进制帧（本链路不应出现）跳过。
async fn recv_json(sock: &mut WebSocket) -> Option<Value> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), sock.recv()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                return Some(serde_json::from_str(t.as_str()).unwrap());
            }
            Ok(Some(Ok(_))) => continue,
            _ => return None,
        }
    }
}

async fn send_json(sock: &mut WebSocket, v: Value) {
    sock.send(Message::Text(v.to_string().into()))
        .await
        .unwrap();
}

fn hello_ok() -> Value {
    json!({
        "type": "hello.ok",
        "version": "v2",
        "session_id": "sess_mock",
        "role": "tts",
        "capabilities": [],
    })
}

/// 服务端视角 hello 握手（断言 client=orchestrator 后回 role=tts）。
async fn mock_handshake(sock: &mut WebSocket) {
    let Some(hello) = recv_json(sock).await else {
        panic!("expected hello");
    };
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["client"], "orchestrator");
    send_json(sock, hello_ok()).await;
}

/// 收一条 `output.sentence`（跳过期间其它帧）并回流给测试体。
async fn recv_sentence(sock: &mut WebSocket, report: &mpsc::Sender<Value>) -> Value {
    loop {
        let Some(msg) = recv_json(sock).await else {
            panic!("svc-tts link closed before output.sentence");
        };
        report.send(msg.clone()).await.unwrap();
        if msg["type"] == "output.sentence" {
            return msg;
        }
    }
}

/// 对一句 `output.sentence` 回一个完整下行 stream：
/// `audio.open` → `frames` 个数据帧（末帧 EOS）→ `audio.end`。
/// `frames` 个 2400B（1200 samples）载荷；sample_offset 按 sample 累计。
async fn respond_audio(sock: &mut WebSocket, turn_id: &str, sentence_seq: u32, frames: u32) {
    let stream = Ulid::new();
    send_json(
        sock,
        json!({
            "type": "audio.open",
            "session_id": "sess_mock",
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
    for seq in 0..frames {
        let last = seq == frames - 1;
        let header = AudioHeader {
            direction: Direction::Downlink,
            flags: if last { FLAG_EOS } else { 0 },
            stream_id: stream,
            chunk_seq: seq,
            sentence_seq,
            sample_offset: seq * 1200,
        };
        let payload = if last { Vec::new() } else { vec![0x55; 2400] };
        sock.send(Message::Binary(encode_audio_frame(header, &payload)))
            .await
            .unwrap();
    }
    send_json(
        sock,
        json!({
            "type": "audio.end",
            "session_id": "sess_mock",
            "event_id": new_id("e"),
            "at_ms": 1,
            "event_seq": 2,
            "stream_id": format!("st_{stream}"),
            "last_chunk_seq": frames - 1,
        }),
    )
    .await;
}

    json!({
        "type": "error",
        "session_id": "sess_mock",
        "event_id": new_id("e"),
        "at_ms": 1,
        "event_seq": 3,
        "code": "TTS_FAILED",
        "message": "provider produced no audio",
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

fn test_state(llm: Arc<MockLlm>, tts_url: url::Url) -> Arc<AppState> {
    let deps = SessionDeps {
        history_turns: 20,
        sent_soft_chars: 25,
        sent_max_chars: 60,
        llm: Some(llm),
        asr: None,
        tts: Some(Arc::new(MomoWsTts::new(tts_url))),
        barge_in_enabled: true,
        barge_in_min: Duration::from_millis(120),
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

/// 服务端出站单元：JSON 业务帧或下行二进制音频帧。
enum Out {
    Json(Msg),
    Audio(AudioHeader, Bytes),
}

async fn recv_out(conn: &mut TestConn) -> Out {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), conn.outbound.next())
            .await
            .expect("server frame timeout")
            .expect("connection closed");
        match msg {
            Message::Text(t) => {
                return Out::Json(
                    codec::decode_msg(t.as_str()).expect("server frame is valid Msg"),
                );
            }
            Message::Binary(b) => {
                let (header, payload) = AudioHeader::decode(&b).expect("valid downlink frame");
                return Out::Audio(header, Bytes::copy_from_slice(payload));
            }
            _ => continue,
        }
    }
}

async fn recv_msg(conn: &mut TestConn) -> Msg {
    loop {
        if let Out::Json(m) = recv_out(conn).await {
            return m;
        }
    }
}

async fn recv_until(conn: &mut TestConn, mut pred: impl FnMut(&Out) -> bool) -> Vec<Out> {
    let mut seen = Vec::new();
    loop {
        let item = recv_out(conn).await;
        let hit = pred(&item);
        seen.push(item);
        if hit {
            return seen;
        }
    }
}

async fn recv_until_msg(conn: &mut TestConn, pred: impl Fn(&Msg) -> bool) -> Vec<Out> {
    recv_until(conn, |o| matches!(o, Out::Json(m) if pred(m))).await
}

async fn handshake(conn: &mut TestConn) {
    conn.inbound
        .send(Message::Text(Utf8Bytes::from(hello_json())))
        .await
        .unwrap();
    let msg = recv_msg(conn).await;
    assert!(matches!(msg, Msg::HelloOk(_)), "expected hello.ok");
}

fn client_env() -> Envelope {
    Envelope {
        session_id: "sess_test".into(),
        event_id: new_id("e"),
        at_ms: 1,
        reply_to: None,
    }
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

fn playback_started(stream_id: &str, sentence_seq: u32) -> Value {
    json!({
        "type": "playback.started",
        "session_id": "sess_test",
        "event_id": new_id("e"),
        "at_ms": 1,
        "stream_id": stream_id,
        "sentence_seq": sentence_seq,
    })
}

fn playback_progress(stream_id: &str, sentence_seq: u32, played: u64, completed: bool) -> Value {
    json!({
        "type": "playback.progress",
        "session_id": "sess_test",
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
        env: client_env(),
        turn_id: turn_id.into(),
        reason: BargeInReason::User,
    };
    serde_json::to_value(Msg::ControlBargeIn(m)).unwrap()
}

/// 开文字回合并取 turn_id（等待 input.accepted + state.turn thinking）。
async fn open_text_turn(conn: &mut TestConn, input_id: &str, text: &str) -> String {
    send_text(conn, input_text(input_id, text)).await;
    let msgs = recv_until_msg(conn, |m| matches!(m, Msg::InputAccepted(_))).await;
    let Some(Out::Json(Msg::InputAccepted(acc))) = msgs.last() else {
        panic!("expected input.accepted")
    };
    let turn_id = acc.turn_id.clone();
    let msgs = recv_until_msg(conn, |m| matches!(m, Msg::StateTurn(_))).await;
    let Some(Out::Json(Msg::StateTurn(st))) = msgs.last() else {
        panic!("expected state.turn")
    };
    assert_eq!(st.state, TurnState::Thinking);
    turn_id
}

/// 收集窗口内到达的 output.sentence / downlink stream 对。
struct SeenTurn {
    sentences: Vec<(u32, String)>,
    opens: Vec<(String, u32)>, // (stream_id, sentence_seq)
    /// (stream_id, chunk_seq, sentence_seq, sample_offset, eos, payload_len)
    frames: Vec<(Ulid, u32, u32, u32, bool, usize)>,
    ends: Vec<(String, u32)>, // (stream_id, last_chunk_seq)
}

fn collect(msgs: &[Out]) -> SeenTurn {
    let mut seen = SeenTurn {
        sentences: Vec::new(),
        opens: Vec::new(),
        frames: Vec::new(),
        ends: Vec::new(),
    };
    for item in msgs {
        match item {
            Out::Json(Msg::OutputSentence(s)) => seen.sentences.push((s.seq, s.text.clone())),
            Out::Json(Msg::AudioOpen(o)) if o.direction == Direction::Downlink => {
                seen.opens
                    .push((o.stream_id.clone(), o.sentence_seq.expect("seq")));
            }
            Out::Json(Msg::AudioEnd(e)) => seen.ends.push((e.stream_id.clone(), e.last_chunk_seq)),
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

// ---------- 测试 ----------

/// #141 主链路：mock svc-tts 收 `output.sentence` → 回 PCM → UI 侧收到
/// `audio.open`/二进制帧/`audio.end`；`playback.*` 回执驱动
/// thinking → speaking → idle，`turn.finished{completed}` 恰好一次。
#[tokio::test]
async fn reply_streams_tts_audio_downlink() {
    let (url, mut reports) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock).await;
        // 两句各回一个完整下行 stream（3 帧：2 data + 1 EOS）。
        for _ in 0..2 {
            let sent = recv_sentence(&mut sock, &report).await;
            respond_audio(
                &mut sock,
                sent["turn_id"].as_str().unwrap(),
                sent["seq"].as_u64().unwrap() as u32,
                3,
            )
            .await;
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(llm.clone(), url));
    handshake(&mut conn).await;
    let t1 = open_text_turn(&mut conn, "in_1", "问").await;

    let feed = take_feed(&llm).await;
    feed.send(Ok("第一句。第二句。".into()))
        .await
        .expect("delta");
    drop(feed);

    // 窗口 1：两 stream 的 open + 全部 6 帧 + output.complete 到齐。
    let mut opens = 0usize;
    let mut frames = 0usize;
    let mut complete = false;
    let items = recv_until(&mut conn, |o| {
        match o {
            Out::Json(Msg::AudioOpen(_)) => opens += 1,
            Out::Json(Msg::OutputComplete(_)) => complete = true,
            Out::Audio(..) => frames += 1,
            _ => {}
        }
        opens >= 2 && frames >= 6 && complete
    })
    .await;
    let seen = collect(&items);
    assert_eq!(seen.sentences.len(), 2, "both output.sentence delivered");
    assert_eq!(seen.sentences[0], (1, "第一句。".to_string()));
    assert_eq!(seen.sentences[1], (2, "第二句。".to_string()));
    assert_eq!(seen.opens.len(), 2, "audio.open per sentence stream");
    assert_eq!(seen.opens[0].1, 1);
    assert_eq!(seen.opens[1].1, 2);
    assert_ne!(seen.opens[0].0, seen.opens[1].0, "new stream per sentence");
    // 句 1 的 stream 在句 2 首帧到达时已 audio.end 收尾。
    assert_eq!(seen.ends.len(), 1);
    assert_eq!(seen.ends[0], (seen.opens[0].0.clone(), 2));
    // 每 stream 3 帧：chunk_seq 0..2，sample_offset 按 1200 累计，末帧 EOS。
    for (stream_id, sent_seq) in &seen.opens {
        // 下行 stream_id 按协议约定 st_<ULID>（与 golden audio.open.downlink 同形态）。
        let raw = Ulid::from_string(
            stream_id
                .strip_prefix("st_")
                .expect("downlink stream_id carries st_ prefix"),
        )
        .expect("stream ulid");
        let fs: Vec<_> = seen
            .frames
            .iter()
            .filter(|f| f.0 == raw && f.2 == *sent_seq)
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
    // 未打断前不得有 turn.finished。
    assert!(
        items
            .iter()
            .all(|o| !matches!(o, Out::Json(Msg::TurnFinished(_))))
    );

    // playback 回执：started → speaking，completed ACK ×2 → finished。
    let (stream1, stream2) = (seen.opens[0].0.clone(), seen.opens[1].0.clone());
    send_text(&mut conn, playback_started(&stream1, 1)).await;
    let msgs = recv_until_msg(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;
    let Some(Out::Json(Msg::StateTurn(st))) = msgs.last() else {
        panic!("expected state.turn speaking")
    };
    assert_eq!(st.turn_id, t1);

    send_text(&mut conn, playback_progress(&stream1, 1, 2400, true)).await;
    send_text(&mut conn, playback_started(&stream2, 2)).await;
    send_text(&mut conn, playback_progress(&stream2, 2, 2400, true)).await;
    let msgs = recv_until_msg(&mut conn, |m| matches!(m, Msg::TurnFinished(_))).await;
    // 收尾前最后一个 stream 按协议 audio.end。
    let tail = collect(&msgs);
    assert_eq!(tail.ends.len(), 1);
    assert_eq!(tail.ends[0], (stream2, 2));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::StateTurn(s)) if s.state == TurnState::Idle
    )));
    let finished = msgs
        .iter()
        .filter(|o| matches!(o, Out::Json(Msg::TurnFinished(_))))
        .count();
    assert_eq!(finished, 1, "turn.finished exactly once");
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == t1 && f.outcome == TurnOutcome::Completed
    )));

    // svc-tts 侧：两句 output.sentence，wire 序 = 句序，字段齐。
    let s1 = reports.recv().await.expect("sentence 1");
    let s2 = reports.recv().await.expect("sentence 2");
    for (sent, seq, text) in [(s1, 1, "第一句。"), (s2, 2, "第二句。")] {
        assert_eq!(sent["type"], "output.sentence");
        assert_eq!(sent["turn_id"], t1);
        assert_eq!(sent["seq"], seq);
        assert_eq!(sent["text"], text);
        assert_eq!(sent["emotion"], "neutral");
        assert_eq!(sent["lang"], "zh");
    }
}

/// #141 降级：svc-tts 回 `error{TTS_FAILED}` → 该句纯文字，error 帧通知 UI，
/// 不阻塞后续句与回合收尾（无 playback ACK 也能 completed）。
#[tokio::test]
async fn tts_failure_degrades_to_text_only() {
    let (url, _reports) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock).await;
        for _ in 0..2 {
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(llm.clone(), url));
    handshake(&mut conn).await;

    let feed = take_feed(&llm).await;
    feed.send(Ok("第一句。第二句。".into()))
        .await
        .expect("delta");
    drop(feed);

    let msgs = recv_until_msg(&mut conn, |m| matches!(m, Msg::TurnFinished(_))).await;
    // 文字照常下行；error{TTS_FAILED} 通知 UI；无任何音频帧/stream。
    let seen = collect(&msgs);
    assert_eq!(
        seen.sentences.len(),
        2,
        "sentences stay text-only, not blocked"
    );
    assert!(seen.opens.is_empty(), "no downlink stream on failure");
    assert!(seen.frames.is_empty(), "no audio frames on failure");
        .iter()
    assert!(
        msgs.iter()
            .any(|o| matches!(o, Out::Json(Msg::OutputComplete(_))))
    );
    // pending audio 已摘除：无需 playback ACK 即 completed + idle。
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f)) if f.outcome == TurnOutcome::Completed
    )));
    assert!(
        msgs.iter()
            .all(|o| !matches!(o, Out::Json(Msg::StateTurn(s)) if s.state == TurnState::Speaking))
    );
}

/// #141 打断：Speaking 中 barge_in → 停播指令（control.cancel{turn}）+
/// turn cancel 扇出到 svc-tts（`control.cancel{target:turn}` 上线），
/// settlement 后 finished(interrupted)；迟到 PCM 不再下发。
#[tokio::test]
async fn barge_in_cancels_tts_and_stops_sink() {
    let (url, mut reports) = spawn_mock(|mut sock, report| async move {
        mock_handshake(&mut sock).await;
        let sent = recv_sentence(&mut sock, &report).await;
        // 只回一帧数据：stream 保持 open，等待取消扇出。
        let stream = Ulid::new();
        send_json(
            &mut sock,
            json!({
                "type": "audio.open",
                "session_id": "sess_mock",
                "event_id": new_id("e"),
                "at_ms": 1,
                "event_seq": 1,
                "stream_id": format!("st_{stream}"),
                "direction": "downlink",
                "turn_id": sent["turn_id"],
                "sentence_seq": sent["seq"],
                "codec": "pcm_s16le",
                "sample_rate": 24000,
                "channels": 1,
            }),
        )
        .await;
        let header = AudioHeader {
            direction: Direction::Downlink,
            flags: 0,
            stream_id: stream,
            chunk_seq: 0,
            sentence_seq: sent["seq"].as_u64().unwrap() as u32,
            sample_offset: 0,
        };
        sock.send(Message::Binary(encode_audio_frame(header, &[0x55; 2400])))
            .await
            .unwrap();
        // 回流随后到达的控制帧（control.cancel{turn}，可能多条）。
        while let Some(msg) = recv_json(&mut sock).await {
            report.send(msg).await.unwrap();
        }
    })
    .await;

    let llm = Arc::new(MockLlm::default());
    let mut conn = connect(test_state(llm.clone(), url));
    handshake(&mut conn).await;
    let t1 = open_text_turn(&mut conn, "in_1", "问").await;

    let feed = take_feed(&llm).await;
    feed.send(Ok("第一句。".into())).await.expect("delta");
    drop(feed);

    // 等 downlink stream 开播（audio.open + 首帧）后进 speaking。
    let mut stream_id = String::new();
    let mut frame_seen = false;
    let items = recv_until(&mut conn, |o| {
        match o {
            Out::Json(Msg::AudioOpen(open)) => stream_id = open.stream_id.clone(),
            Out::Audio(..) => frame_seen = true,
            _ => {}
        }
        frame_seen && !stream_id.is_empty()
    })
    .await;
    assert!(items.iter().any(|o| matches!(o, Out::Audio(..))));
    send_text(&mut conn, playback_started(&stream_id, 1)).await;
    let _ = recv_until_msg(
        &mut conn,
        |m| matches!(m, Msg::StateTurn(s) if s.state == TurnState::Speaking),
    )
    .await;

    send_text(&mut conn, barge_in(&t1)).await;
    let msgs = recv_until_msg(&mut conn, |m| matches!(m, Msg::TurnFinished(_))).await;
    // 停播指令直达 UI（数据队列之外的控制通道）。
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::ControlCancel(c)) if c.target == CancelTarget::Turn && c.id == t1
    )));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::StateTurn(s)) if s.state == TurnState::Interrupted
    )));
    assert!(msgs.iter().any(|o| matches!(
        o,
        Out::Json(Msg::TurnFinished(f))
            if f.turn_id == t1 && f.outcome == TurnOutcome::Interrupted
    )));

    // svc-tts 侧：output.sentence 之后收到 control.cancel{target:turn,id=t1}。
    let mut saw_sentence = false;
    let mut cancels = 0usize;
    while let Ok(Some(v)) = tokio::time::timeout(Duration::from_secs(2), reports.recv()).await {
        match v["type"].as_str() {
            Some("output.sentence") => saw_sentence = true,
            Some("control.cancel") if v["target"] == "turn" && v["id"] == t1 => {
                cancels += 1;
            }
            _ => {}
        }
    }
    assert!(saw_sentence, "output.sentence reached svc-tts");
    assert!(cancels >= 1, "control.cancel{{turn}} fanned out to svc-tts");
}
