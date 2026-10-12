//! 消息序列化与 golden JSON 对拍（momo-app.md §测试策略）。
//!
//! 字段以 protocol.md §6 样例为 golden；codec 头常量同步断言。

use momo_app::proto::{
    codec::{self, AudioHeader, DIR_DOWNLINK, DIR_UPLINK, Direction, FLAG_EOS},
    message::{
        ClientKind, ClientMsg, ControlCancelMsg, ErrorCode, HelloMsg, InputTextMsg, ServerMsg,
        ServerRole, StateTurnMsg, TurnState,
    },
};
use serde_json::json;
use ulid::Ulid;

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

#[test]
fn decodes_hello_golden_json() {
    // protocol §3 样例逐字段对拍。
    let text = json!({
        "type": "hello",
        "protocol": "momo-ws",
        "version": "v1",
        "client": "ui",
        "audio": {
            "uplink":   {"codec": "pcm_s16le", "sample_rate": 16000, "channels": 1, "chunk_ms": 50},
            "downlink": {"codec": "pcm_s16le", "sample_rate": 24000, "channels": 1, "chunk_ms": 50}
        }
    })
    .to_string();

    let msg = codec::decode_client_msg(&text).unwrap();
    let ClientMsg::Hello(hello) = msg else {
        panic!("expected hello, got {msg:?}")
    };
    assert_eq!(hello.protocol, "momo-ws");
    assert_eq!(hello.version, "v1");
    assert_eq!(hello.client, ClientKind::Ui);
    let uplink = &hello.audio.as_ref().unwrap().uplink;
    assert_eq!(uplink.as_ref().unwrap().sample_rate, 16000);
    assert!(codec::check_hello(&hello).is_ok());
}

#[test]
fn rejects_wrong_protocol_and_version() {
    let mut hello = HelloMsg {
        protocol: "other".into(),
        version: "v1".into(),
        client: ClientKind::Ui,
        audio: None,
    };
    assert!(codec::check_hello(&hello).is_err());
    hello.protocol = "momo-ws".into();
    hello.version = "v2".into();
    assert!(codec::check_hello(&hello).is_err());
}

#[test]
fn decodes_input_text_golden() {
    let text = r#"{"type":"input.text","id":"r_01JXXX","text":"今晚吃什么","lang":"auto","at_ms":1730000000000}"#;
    let msg = codec::decode_client_msg(text).unwrap();
    let ClientMsg::InputText(InputTextMsg {
        id,
        text,
        lang,
        at_ms,
    }) = msg
    else {
        panic!("expected input.text")
    };
    assert_eq!(id, "r_01JXXX");
    assert_eq!(text, "今晚吃什么");
    assert_eq!(lang.as_deref(), Some("auto"));
    assert_eq!(at_ms, Some(1730000000000));
}

#[test]
fn encodes_hello_ok_golden() {
    let msg = ServerMsg::HelloOk(momo_app::proto::message::HelloOkMsg {
        version: "v1".into(),
        session_id: "sess_01JXXX".into(),
        server: "momod".into(),
        role: ServerRole::Orchestrator,
        capabilities: vec!["barge_in".into(), "actions".into(), "emotion_tags".into()],
    });
    let got: serde_json::Value =
        serde_json::from_str(&codec::encode_server_msg(&msg).unwrap()).unwrap();
    assert_eq!(
        got,
        json!({
            "type": "hello.ok",
            "version": "v1",
            "session_id": "sess_01JXXX",
            "server": "momod",
            "role": "orchestrator",
            "capabilities": ["barge_in", "actions", "emotion_tags"]
        })
    );
}

#[test]
fn encodes_state_turn_with_optional_ref_id() {
    let msg = ServerMsg::StateTurn(StateTurnMsg {
        turn_id: "t_01JXXX".into(),
        state: TurnState::Thinking,
        seq: 3,
        at_ms: 1730000000000,
        ref_id: Some("r_01JXXX".into()),
    });
    let got: serde_json::Value =
        serde_json::from_str(&codec::encode_server_msg(&msg).unwrap()).unwrap();
    assert_eq!(got["type"], "state.turn");
    assert_eq!(got["state"], "thinking");
    assert_eq!(got["ref_id"], "r_01JXXX");
}

#[test]
fn encodes_control_cancel_golden() {
    let msg = ServerMsg::ControlCancel(ControlCancelMsg {
        target: momo_app::proto::message::CancelTarget::Turn,
        id: "t_01JXXX".into(),
        reason: Some(momo_app::proto::message::CancelReason::BargeIn),
    });
    let got: serde_json::Value =
        serde_json::from_str(&codec::encode_server_msg(&msg).unwrap()).unwrap();
    assert_eq!(
        got,
        json!({"type": "control.cancel", "target": "turn", "id": "t_01JXXX", "reason": "barge_in"})
    );
}

#[test]
fn unknown_type_maps_to_unknown_variant() {
    let msg = codec::decode_client_msg(r#"{"type":"future.msg","x":1}"#).unwrap();
    assert_eq!(msg, ClientMsg::Unknown);
    // 未知字段被忽略（protocol §2 兼容规则）。
    let msg =
        codec::decode_client_msg(r#"{"type":"input.text","id":"r_x","text":"hi","extra":true}"#)
            .unwrap();
    assert!(matches!(msg, ClientMsg::InputText(_)));
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
