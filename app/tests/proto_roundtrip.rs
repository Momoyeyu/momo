//!

use momo_app::proto::{
    codec::{self, AudioHeader, DIR_DOWNLINK, DIR_UPLINK, Direction, FLAG_EOS},
    message::{
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
