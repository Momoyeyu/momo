//! 消息序列化与 golden JSON 对拍，以及二进制音频帧 roundtrip。
//!
//! 字段以 `docs/01-architecture/protocol.md` 为唯一权威；
//! `tests/golden/*.json` 逐类型覆盖协议必需字段表，encode/decode 双向对拍。

use std::path::Path;

use momo_app::proto::{
    codec::{self, AudioHeader, DIR_DOWNLINK, DIR_UPLINK, Direction, FLAG_EOS},
    message::{
        AudioCodec, AudioEndMsg, AudioOpenMsg, BargeInMsg, BargeInReason, CancelTarget,
        CaptureMode, ClientKind, ControlCancelMsg, EmotionTag, Envelope, ErrorCode, ErrorMsg,
        HelloMsg, HelloOkMsg, InputAcceptedMsg, InputAudioMsg, InputAudioSegmentMsg,
        InputAudioStartMsg, InputReceipt, InputRejectedMsg, InputTextMsg, InputVadMsg, Msg,
        OutputCompleteMsg, OutputSentenceMsg, PlaybackProgressMsg, PlaybackStartedMsg,
        ReceiptStatus, ServerEnvelope, ServerRole, SessionSnapshotMsg, SnapshotTurn, StateTurnMsg,
        TurnFinishedMsg, TurnOutcome, TurnState, VadEvent,
    },
};
use serde_json::json;
use ulid::Ulid;

const SESSION: &str = "sess_01J5M0N8T1RZ8K2W4V7H6X9P3A";
const EVENT: &str = "01J5M0N8T1RZ8K2W4V7H6X9P3B";
const TURN: &str = "t_01J5M0N8T1RZ8K2W4V7H6X9P3C";
const CAPTURE: &str = "cap_01J5M0N8T1RZ8K2W4V7H6X9P3D";
const STREAM: &str = "st_01J5M0N8T1RZ8K2W4V7H6X9P3E";
const RECEIPT: &str = "r_01J5M0N8T1RZ8K2W4V7H6X9P3F";
const AT_MS: u64 = 1_730_000_000_000;
const EVENT_SEQ: u64 = 42;

fn env() -> Envelope {
    Envelope {
        session_id: SESSION.into(),
        event_id: EVENT.into(),
        at_ms: AT_MS,
        reply_to: None,
    }
}

fn senv() -> ServerEnvelope {
    ServerEnvelope {
        session_id: SESSION.into(),
        event_id: EVENT.into(),
        at_ms: AT_MS,
        reply_to: None,
        event_seq: EVENT_SEQ,
    }
}

fn golden(name: &str) -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// protocol.md 必需字段表全类型覆盖：encode 对拍 golden，decode golden 还原。
#[test]
fn all_message_types_match_golden_json() {
    let cases: Vec<(&str, Msg)> = vec![
        (
            "hello.json",
            Msg::Hello(HelloMsg {
                protocol: "momo-ws".into(),
                version: "v2".into(),
                client: ClientKind::Ui,
                auth_token: "ephemeral-token".into(),
                resume_session_id: Some(SESSION.into()),
            }),
        ),
        (
            "hello.ok.json",
            Msg::HelloOk(HelloOkMsg {
                version: "v2".into(),
                session_id: SESSION.into(),
                role: ServerRole::Orchestrator,
                capabilities: vec!["barge_in".into(), "emotion_tags".into()],
            }),
        ),
        (
            "session.snapshot.json",
            Msg::SessionSnapshot(SessionSnapshotMsg {
                session_id: SESSION.into(),
                turn: Some(SnapshotTurn {
                    id: TURN.into(),
                    state: TurnState::Speaking,
                }),
                receipts: vec![InputReceipt {
                    id: RECEIPT.into(),
                    turn_id: TURN.into(),
                    status: ReceiptStatus::Completed,
                }],
                capabilities: vec!["barge_in".into(), "emotion_tags".into()],
                last_event_seq: 41,
            }),
        ),
        (
            "input.text.json",
            Msg::InputText(InputTextMsg {
                env: env(),
                id: RECEIPT.into(),
                text: "今晚吃什么".into(),
                lang: "zh".into(),
            }),
        ),
        (
            "input.accepted.json",
            Msg::InputAccepted(InputAcceptedMsg {
                env: senv(),
                ref_id: RECEIPT.into(),
                turn_id: TURN.into(),
            }),
        ),
        (
            "input.rejected.json",
            Msg::InputRejected(InputRejectedMsg {
                env: senv(),
                ref_id: RECEIPT.into(),
                reason: "empty_input".into(),
            }),
        ),
        (
            "input.audio.start.json",
            Msg::InputAudioStart(InputAudioStartMsg {
                env: env(),
                capture_id: CAPTURE.into(),
                mode: CaptureMode::Ptt,
            }),
        ),
        (
            "input.audio.end.json",
            Msg::InputAudioEnd(InputAudioMsg {
                env: env(),
                capture_id: CAPTURE.into(),
            }),
        ),
        (
            "input.audio.cancel.json",
            Msg::InputAudioCancel(InputAudioMsg {
                env: env(),
                capture_id: CAPTURE.into(),
            }),
        ),
        (
            "input.vad.json",
            Msg::InputVad(InputVadMsg {
                env: senv(),
                capture_id: CAPTURE.into(),
                event: VadEvent::SpeechStart,
            }),
        ),
        (
            "input.audio.segment.json",
            Msg::InputAudioSegment(InputAudioSegmentMsg {
                env: senv(),
                capture_id: CAPTURE.into(),
                text: "你好".into(),
                is_final: true,
            }),
        ),
        (
            "audio.open.uplink.json",
            Msg::AudioOpen(AudioOpenMsg {
                env: env(),
                event_seq: None,
                stream_id: STREAM.into(),
                direction: Direction::Uplink,
                capture_id: Some(CAPTURE.into()),
                turn_id: None,
                sentence_seq: None,
                codec: AudioCodec::PcmS16le,
                sample_rate: 16000,
                channels: 1,
            }),
        ),
        (
            "audio.open.downlink.json",
            Msg::AudioOpen(AudioOpenMsg {
                env: env(),
                event_seq: Some(EVENT_SEQ),
                stream_id: STREAM.into(),
                direction: Direction::Downlink,
                capture_id: None,
                turn_id: Some(TURN.into()),
                sentence_seq: Some(2),
                codec: AudioCodec::PcmS16le,
                sample_rate: 24000,
                channels: 1,
            }),
        ),
        (
            "audio.end.json",
            Msg::AudioEnd(AudioEndMsg {
                env: env(),
                event_seq: None,
                stream_id: STREAM.into(),
                last_chunk_seq: 7,
            }),
        ),
        (
            "output.sentence.json",
            Msg::OutputSentence(OutputSentenceMsg {
                env: senv(),
                turn_id: TURN.into(),
                seq: 2,
                text: "今晚吃面吧。".into(),
                emotion: EmotionTag::Happy,
                lang: "zh".into(),
            }),
        ),
        (
            "output.complete.json",
            Msg::OutputComplete(OutputCompleteMsg {
                env: senv(),
                turn_id: TURN.into(),
                last_sentence_seq: 3,
            }),
        ),
        (
            "playback.started.json",
            Msg::PlaybackStarted(PlaybackStartedMsg {
                env: env(),
                stream_id: STREAM.into(),
                sentence_seq: 2,
            }),
        ),
        (
            "playback.progress.json",
            Msg::PlaybackProgress(PlaybackProgressMsg {
                env: env(),
                stream_id: STREAM.into(),
                sentence_seq: 2,
                played_samples: 48000,
                completed: false,
            }),
        ),
        (
            "control.barge_in.json",
            Msg::ControlBargeIn(BargeInMsg {
                env: env(),
                turn_id: TURN.into(),
                reason: BargeInReason::User,
            }),
        ),
        (
            "control.cancel.json",
            Msg::ControlCancel(ControlCancelMsg {
                env: senv(),
                target: CancelTarget::Turn,
                id: TURN.into(),
            }),
        ),
        (
            "turn.finished.json",
            Msg::TurnFinished(TurnFinishedMsg {
                env: senv(),
                turn_id: TURN.into(),
                outcome: TurnOutcome::Interrupted,
            }),
        ),
        (
            "state.turn.json",
            Msg::StateTurn(StateTurnMsg {
                env: senv(),
                turn_id: TURN.into(),
                state: TurnState::Speaking,
            }),
        ),
        (
            "error.json",
            Msg::Error(ErrorMsg {
                session_id: Some(SESSION.into()),
                event_id: Some(EVENT.into()),
                at_ms: Some(AT_MS),
                event_seq: Some(EVENT_SEQ),
            }),
        ),
    ];
    assert_eq!(
        cases.len(),
        23,
        "protocol.md should have 23 goldens covering all types"
    );
    for (file, msg) in cases {
        let want = golden(file);
        let got = serde_json::to_value(&msg).unwrap();
        assert_eq!(got, want, "{file}: encode mismatch");
        let back: Msg = serde_json::from_value(want).unwrap();
        assert_eq!(back, msg, "{file}: decode mismatch");
    }
}

#[test]
fn audio_open_validate_enforces_direction_binding() {
    let mut open = AudioOpenMsg {
        env: env(),
        event_seq: None,
        stream_id: STREAM.into(),
        direction: Direction::Uplink,
        capture_id: Some(CAPTURE.into()),
        turn_id: None,
        sentence_seq: None,
        codec: AudioCodec::PcmS16le,
        sample_rate: 16000,
        channels: 1,
    };
    assert!(open.validate().is_ok());
    // uplink 不得带 sentence_seq。
    open.sentence_seq = Some(1);
    assert!(open.validate().is_err());
    // downlink 必须带 turn_id + sentence_seq，不得带 capture_id。
    open.direction = Direction::Downlink;
    open.sentence_seq = Some(1);
    open.turn_id = Some(TURN.into());
    assert!(open.validate().is_err()); // 仍带 capture_id
    open.capture_id = None;
    assert!(open.validate().is_ok());
}

#[test]
fn rejects_wrong_protocol_and_version() {
    let mut hello = HelloMsg {
        protocol: "other".into(),
        version: "v2".into(),
        client: ClientKind::Ui,
        auth_token: "t".into(),
        resume_session_id: None,
    };
    assert!(codec::check_hello(&hello).is_err());
    hello.protocol = "momo-ws".into();
    assert!(codec::check_hello(&hello).is_ok());
    hello.version = "v1".into();
    assert!(codec::check_hello(&hello).is_err());
}

#[test]
fn error_frame_without_envelope_decodes() {
    // 握手期错误无 session envelope（protocol：仅 code,message 必需）。
    let msg = codec::decode_msg(r#"{"type":"error","code":"UNAUTHORIZED","message":"bad token"}"#)
        .unwrap();
    let Msg::Error(err) = msg else {
        panic!("expected error, got {msg:?}")
    };
    assert_eq!(err.code, ErrorCode::Unauthorized);
    assert!(err.session_id.is_none());
    // 表外 code 解码为 Unknown，不产生解析失败。
    let msg = codec::decode_msg(r#"{"type":"error","code":"FUTURE_CODE","message":"x"}"#).unwrap();
    assert!(matches!(msg, Msg::Error(e) if e.code == ErrorCode::Unknown));
}

#[test]
fn unknown_type_maps_to_unknown_variant() {
    let msg = codec::decode_msg(r#"{"type":"future.msg","x":1}"#).unwrap();
    assert_eq!(msg, Msg::Unknown);
    // 未知字段被忽略。
    let msg = codec::decode_msg(
        r#"{"type":"input.text","session_id":"s","event_id":"e","at_ms":0,"id":"r","text":"hi","lang":"zh","extra":true}"#,
    )
    .unwrap();
    assert!(matches!(msg, Msg::InputText(_)));
}

#[test]
fn emotion_unknown_normalizes_to_neutral() {
    let tag: EmotionTag = serde_json::from_value(json!("excited")).unwrap();
    assert_eq!(tag, EmotionTag::Unknown);
    assert_eq!(tag.normalized(), EmotionTag::Neutral);
    assert_eq!(serde_json::to_value(tag).unwrap(), json!("neutral"));
}

#[test]
fn error_code_serializes_screaming_snake() {
    assert_eq!(
        serde_json::to_value(ErrorCode::VersionMismatch).unwrap(),
        json!("VERSION_MISMATCH")
    );
    assert_eq!(
        serde_json::to_value(ErrorCode::BackpressureOverflow).unwrap(),
        json!("BACKPRESSURE_OVERFLOW")
    );
}

#[test]
fn audio_header_constants_match_protocol() {
    // protocol.md「32-byte 二进制帧」。
    assert_eq!(codec::AUDIO_HEADER_LEN, 32);
    assert_eq!(codec::AUDIO_VER, 2);
    assert_eq!(Direction::Uplink.as_u8(), DIR_UPLINK);
    assert_eq!(Direction::Downlink.as_u8(), DIR_DOWNLINK);
    assert_eq!(FLAG_EOS, 0x0001);
}

#[test]
fn audio_header_roundtrip() {
    let header = AudioHeader {
        direction: Direction::Downlink,
        flags: 0,
        stream_id: Ulid::new(),
        chunk_seq: 42,
        sentence_seq: 3,
        sample_offset: 9600,
    };
    let frame = codec::encode_audio_frame(header, b"\x01\x02\x03\x04");
    assert_eq!(frame.len(), 32 + 4);
    assert_eq!(frame[0], 2); // ver
    assert_eq!(frame[1], DIR_DOWNLINK);
    // 多字节字段小端序。
    assert_eq!(&frame[20..24], &42u32.to_le_bytes());
    assert_eq!(&frame[24..28], &3u32.to_le_bytes());
    assert_eq!(&frame[28..32], &9600u32.to_le_bytes());
    assert_eq!(&frame[4..20], &header.stream_id.to_bytes());

    let (decoded, payload) = AudioHeader::decode(&frame).unwrap();
    assert_eq!(decoded, header);
    assert!(!decoded.eos());
    assert_eq!(payload, b"\x01\x02\x03\x04");
}

#[test]
fn audio_header_rejects_malformed() {
    let header = AudioHeader {
        direction: Direction::Uplink,
        flags: FLAG_EOS,
        stream_id: Ulid::new(),
        chunk_seq: 0,
        sentence_seq: 0,
        sample_offset: 0,
    };
    // EOS 允许空载荷（零长度 capture 结束）。
    let eos = codec::encode_audio_frame(header, b"");
    let (decoded, payload) = AudioHeader::decode(&eos).unwrap();
    assert!(decoded.eos());
    assert!(payload.is_empty());

    // 短帧 / 错误版本 / 非法 direction。
    assert!(AudioHeader::decode(&eos[..31]).is_err());
    let mut bad = eos.to_vec();
    bad[0] = 9;
    assert!(AudioHeader::decode(&bad).is_err());
    let mut bad = eos.to_vec();
    bad[1] = 7;
    assert!(AudioHeader::decode(&bad).is_err());

    // reserved flags 非零即拒绝。
    let mut bad = codec::encode_audio_frame(header, b"\x00\x00").to_vec();
    bad[2] = 0x02;
    assert!(AudioHeader::decode(&bad).is_err());

    // 非 EOS 空载荷与奇数长度载荷均非法。
    let data = AudioHeader { flags: 0, ..header };
    assert!(AudioHeader::decode(&codec::encode_audio_frame(data, b"")).is_err());
    assert!(AudioHeader::decode(&codec::encode_audio_frame(data, b"\x01")).is_err());

    // uplink 帧 sentence_seq 必须固定为 0。
    let bad_seq = AudioHeader {
        sentence_seq: 1,
        ..header
    };
    assert!(AudioHeader::decode(&codec::encode_audio_frame(bad_seq, b"\x00\x00")).is_err());
}
