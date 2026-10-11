//!

use momo_app::proto::{
    message::{
    },
};
use serde_json::json;

#[test]
fn audio_header_constants_match_protocol() {
    assert_eq!(FLAG_EOS, 0x0001);
}

#[test]
fn audio_header_roundtrip() {
    let header = AudioHeader {
    };
    let (decoded, payload) = AudioHeader::decode(&frame).unwrap();
    assert_eq!(decoded, header);
    bad[0] = 9;
}
