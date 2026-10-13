//! `AudioSink`：push 保序下发 + stream 生命周期（`audio.open`/`audio.end`）、
//! `stop` 立即生效、`drain` 排空确认、`BACKPRESSURE_OVERFLOW` 溢出保护。
//!
//! 不经真 socket：sink 写 `out_tx`，测试从 `Receiver<OutboundFrame>` 收帧断言。

use std::time::Duration;

use bytes::Bytes;
use momo_app::{
    audio::sink::{AudioSink, DOWNLINK_BUFFER_FRAMES},
    pipeline::tts::AudioChunk,
    proto::{
        codec::Direction,
        message::{CancelTarget, ErrorCode, Msg, ServerEnv},
    },
    ws::conn::OutboundFrame,
};
use tokio::sync::mpsc;

/// 50ms @24kHz mono pcm_s16le = 1200 samples = 2400 bytes。
fn chunk(seq: u32, tag: u32, eos: bool) -> AudioChunk {
    AudioSink::chunk(seq, tag, Bytes::from(vec![0u8; 2400]), eos)
}

async fn recv(rx: &mut mpsc::Receiver<OutboundFrame>) -> OutboundFrame {
    tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("frame timeout")
        .expect("out_tx closed")
}

async fn recv_json(rx: &mut mpsc::Receiver<OutboundFrame>) -> Msg {
    match recv(rx).await {
        OutboundFrame::Json(m) => m,
        other => panic!("expected json frame, got {other:?}"),
    }
}

async fn recv_audio(rx: &mut mpsc::Receiver<OutboundFrame>) -> (u32, u32, u32, bool, ulid::Ulid) {
    match recv(rx).await {
        OutboundFrame::Audio(h, _) => (
            h.chunk_seq,
            h.sentence_seq,
            h.sample_offset,
            h.eos(),
            h.stream_id,
        ),
        other => panic!("expected audio frame, got {other:?}"),
    }
}

/// 短时排空：断言此后不再有帧到达（未播队列确实被丢弃）。
async fn assert_quiet(rx: &mut mpsc::Receiver<OutboundFrame>) {
    match tokio::time::timeout(Duration::from_millis(300), rx.recv()).await {
        Ok(Some(f)) => panic!("unexpected frame after stop: {f:?}"),
        Ok(None) => panic!("out_tx closed unexpectedly"),
        Err(_) => {}
    }
}

#[tokio::test]
async fn push_opens_stream_and_sends_ordered_frames() {
    let (out_tx, mut rx) = mpsc::channel(32);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));

    sink.push(chunk(0, 1, false)).expect("push");
    sink.push(chunk(1, 1, true)).expect("push");

    // 句 1 先 audio.open 绑定 stream，再按序下发二进制帧。
    let Msg::AudioOpen(open) = recv_json(&mut rx).await else {
        panic!("expected audio.open");
    };
    assert_eq!(open.direction, Direction::Downlink);
    assert_eq!(open.turn_id.as_deref(), Some("t_1"));
    assert_eq!(open.sentence_seq, Some(1));
    assert!(
        open.capture_id.is_none(),
        "downlink must not carry capture_id"
    );
    let stream_id = open.stream_id.clone();

    assert!(stream_id.starts_with("st_"), "downlink stream_id st_ prefixed");
    let (seq, sent, off, eos, sid) = recv_audio(&mut rx).await;
    assert_eq!((seq, sent, off, eos), (0, 1, 0, false));
    assert_eq!(format!("st_{sid}"), stream_id);
    let (seq, sent, off, eos, sid) = recv_audio(&mut rx).await;
    assert_eq!((seq, sent, off, eos), (1, 1, 1200, true));
    assert_eq!(format!("st_{sid}"), stream_id);
}

#[tokio::test]
async fn sentence_switch_ends_previous_stream() {
    let (out_tx, mut rx) = mpsc::channel(32);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));

    sink.push(chunk(0, 1, true)).expect("push");
    sink.push(chunk(1, 2, true)).expect("push");

    let Msg::AudioOpen(open1) = recv_json(&mut rx).await else {
        panic!("expected audio.open");
    };
    let _ = recv_audio(&mut rx).await;
    // 换句：audio.end 关闭 stream1，audio.open 开 stream2。
    let Msg::AudioEnd(end) = recv_json(&mut rx).await else {
        panic!("expected audio.end");
    };
    assert_eq!(end.stream_id, open1.stream_id);
    assert_eq!(end.last_chunk_seq, 0);
    let Msg::AudioOpen(open2) = recv_json(&mut rx).await else {
        panic!("expected audio.open");
    };
    assert_ne!(open2.stream_id, open1.stream_id);
    assert_eq!(open2.sentence_seq, Some(2));
    let (seq, sent, off, eos, sid) = recv_audio(&mut rx).await;
    assert_eq!((seq, sent, off, eos), (0, 2, 0, true));
    assert_eq!(format!("st_{sid}"), open2.stream_id);
}

#[tokio::test]
async fn drain_flushes_queue_and_closes_stream() {
    let (out_tx, mut rx) = mpsc::channel(32);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));

    sink.push(chunk(0, 1, false)).expect("push");
    sink.push(chunk(1, 1, true)).expect("push");
    // drain 等到全部下发完成 + 按协议发 audio.end。
    sink.drain().await.expect("drain");

    let _open = recv_json(&mut rx).await;
    let _ = recv_audio(&mut rx).await;
    let _ = recv_audio(&mut rx).await;
    let Msg::AudioEnd(end) = recv_json(&mut rx).await else {
        panic!("expected audio.end after drain");
    };
    assert_eq!(end.last_chunk_seq, 1);
}

#[tokio::test]
async fn drain_empty_sink_returns_immediately() {
    let (out_tx, _rx) = mpsc::channel(8);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));
    sink.drain().await.expect("drain on empty sink");
}

#[tokio::test]
async fn stop_discards_queue_and_notifies() {
    // conn_tx 容量 1 且不消费 → worker 在第二帧处阻塞，内部队列有积压。
    let (out_tx, mut rx) = mpsc::channel(1);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));

    sink.push(chunk(0, 1, false)).expect("push");
    sink.push(chunk(1, 1, false)).expect("push");
    sink.push(chunk(2, 1, false)).expect("push");
    // 等 worker 把 audio.open + chunk0 顶进 conn_tx 并阻塞在 chunk1。
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    sink.stop();

    // 已下发部分照常到达；stop 后未播队列丢弃 + 收到停播指令。
    let _open = recv_json(&mut rx).await;
    let mut audio_seen = 0usize;
    loop {
        match recv(&mut rx).await {
            OutboundFrame::Audio(..) => audio_seen += 1,
            OutboundFrame::Json(Msg::ControlCancel(c)) => {
                assert_eq!(c.target, CancelTarget::Turn);
                assert_eq!(c.id, "t_1");
                break;
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }
    // chunk0 已在飞行中；chunk1/chunk2 被丢弃（共 3 个 push，到达 < 3）。
    assert!(audio_seen < 3, "pending chunks must be dropped");
    // worker 退出（在飞的 send 完成后经 biased select 立刻 break）。
    for _ in 0..100 {
        if sink.worker_done() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(sink.worker_done(), "worker should exit after stop");
    // 后续 push 显式拒绝。
    assert!(sink.push(chunk(3, 1, false)).is_err());
    assert_quiet(&mut rx).await;
}

#[tokio::test]
async fn stop_is_idempotent() {
    let (out_tx, mut rx) = mpsc::channel(8);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));
    sink.stop();
    sink.stop();
    // 停播指令只发一次。
    let Msg::ControlCancel(_) = recv_json(&mut rx).await else {
        panic!("expected control.cancel");
    };
    assert_quiet(&mut rx).await;
}

#[tokio::test]
async fn overflow_emits_backpressure_error_and_cancel() {
    // conn_tx 容量 1 不消费 → 内部 96 帧队列迅速打满。
    let (out_tx, mut rx) = mpsc::channel(1);
    let sink = AudioSink::spawn(out_tx, "t_1".into(), ServerEnv::new("sess_t"));

    let mut overflow_err = None;
    for i in 0..(DOWNLINK_BUFFER_FRAMES + 8) as u32 {
        match sink.push(chunk(i, 1, false)) {
            Ok(()) => {}
            Err(e) => {
                overflow_err = Some(e);
                break;
            }
        }
    }
    let err = overflow_err.expect("push must fail on overflow");
    assert_eq!(err.code(), ErrorCode::BackpressureOverflow);

    // 排空 conn_tx：audio.open + 在飞帧 + error{BACKPRESSURE_OVERFLOW} + control.cancel。
    let mut saw_error = false;
    let mut saw_cancel = false;
    for _ in 0..(DOWNLINK_BUFFER_FRAMES + 8) {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
            Ok(Some(OutboundFrame::Json(Msg::Error(e)))) => {
                assert_eq!(e.code, ErrorCode::BackpressureOverflow);
                saw_error = true;
            }
            Ok(Some(OutboundFrame::Json(Msg::ControlCancel(c)))) => {
                assert_eq!(c.target, CancelTarget::Turn);
                assert_eq!(c.id, "t_1");
                saw_cancel = true;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("out_tx closed"),
            Err(_) => break,
        }
        if saw_error && saw_cancel {
            break;
        }
    }
    assert!(saw_error, "BACKPRESSURE_OVERFLOW error frame expected");
    assert!(saw_cancel, "control.cancel for the turn expected");
}
