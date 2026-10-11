//! 消息序列化与 golden JSON 对拍（momo-app.md §测试策略）。
//!
//! 字段以 protocol.md §6 样例为 golden；codec 头常量同步断言。

use momo_app::proto::{
    codec::{self, AudioHeader, FLAG_EOS, STREAM_DOWNLINK, STREAM_UPLINK},
    message::{
        ClientKind, ClientMsg, ControlCancelMsg, ErrorCode, HelloMsg, InputTextMsg, ServerMsg,
        ServerRole, StateTurnMsg, TurnState,
    },
};
use serde_json::json;

#[test]
fn audio_header_constants_match_protocol() {
    assert_eq!(codec::AUDIO_HEADER_LEN, 12);
    assert_eq!(codec::AUDIO_VER, 1);
    assert_eq!(STREAM_UPLINK, 0x01);
    assert_eq!(STREAM_DOWNLINK, 0x02);
    assert_eq!(FLAG_EOS, 0x0001);
}

#[test]
fn audio_header_roundtrip() {
    let header = AudioHeader {
        stream: STREAM_DOWNLINK,
        flags: FLAG_EOS,
        seq: 42,
        tag: 3,
    };
    let frame = codec::encode_audio_frame(header, b"\x01\x02pcm");
    assert_eq!(frame.len(), 12 + 5);
    assert_eq!(frame[0], 1); // ver
    let (decoded, payload) = AudioHeader::decode(&frame).unwrap();
    assert_eq!(decoded, header);
    assert_eq!(payload, b"\x01\x02pcm");
    // 短帧与错误版本解码失败。
    assert!(AudioHeader::decode(&frame[..8]).is_none());
    let mut bad = frame.to_vec();
    bad[0] = 9;
    assert!(AudioHeader::decode(&bad).is_none());
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
