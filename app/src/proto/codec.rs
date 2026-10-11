//! 帧编解码：文本帧 JSON / 二进制音频帧分流，版本握手检查。
//!
//! 二进制 `audio.chunk` 头布局见 protocol.md §4：固定 12B
//! `[ver:u8][stream:u8][flags:u16][seq:u32][tag:u32]`，多字节字段大端序。

use bytes::{Bytes, BytesMut};

use crate::{
    error::AppError,
    proto::message::{ClientMsg, HelloMsg, PROTOCOL_NAME, PROTOCOL_VERSION, ServerMsg},
};

/// `audio.chunk` 二进制头长度（protocol §4）。
pub const AUDIO_HEADER_LEN: usize = 12;
/// 头 `ver` 字段恒为 1。
pub const AUDIO_VER: u8 = 1;
/// `stream`：上行（麦→ASR）。
pub const STREAM_UPLINK: u8 = 0x01;
/// `stream`：下行（TTS→播放）。
pub const STREAM_DOWNLINK: u8 = 0x02;
/// `flags` bit0：EOS，本段/本句最后一帧。
pub const FLAG_EOS: u16 = 0x0001;

/// WS close code（protocol §8）。
pub const CLOSE_NORMAL: u16 = 1000;
/// 版本不匹配。
pub const CLOSE_VERSION_MISMATCH: u16 = 4001;
/// 协议违反（如首消息非 hello）。
pub const CLOSE_PROTOCOL_VIOLATION: u16 = 4400;
/// 服务端内部错误。
pub const CLOSE_INTERNAL: u16 = 1011;

/// `audio.chunk` 12B 头（payload 不含在内）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub stream: u8,
    pub flags: u16,
    pub seq: u32,
    pub tag: u32,
}

impl AudioHeader {
    /// 编码为 12B 头（`ver` 自动置 [`AUDIO_VER`]）。
    pub fn encode(self) -> [u8; AUDIO_HEADER_LEN] {
        let mut buf = [0u8; AUDIO_HEADER_LEN];
        buf[0] = AUDIO_VER;
        buf[1] = self.stream;
        buf[2..4].copy_from_slice(&self.flags.to_be_bytes());
        buf[4..8].copy_from_slice(&self.seq.to_be_bytes());
        buf[8..12].copy_from_slice(&self.tag.to_be_bytes());
        buf
    }

    /// 解码头并返回载荷切片；长度不足或 `ver` 不符返回 `None`。
    pub fn decode(frame: &[u8]) -> Option<(Self, &[u8])> {
        if frame.len() < AUDIO_HEADER_LEN || frame[0] != AUDIO_VER {
            return None;
        }
        let header = AudioHeader {
            stream: frame[1],
            flags: u16::from_be_bytes(frame[2..4].try_into().ok()?),
            seq: u32::from_be_bytes(frame[4..8].try_into().ok()?),
            tag: u32::from_be_bytes(frame[8..12].try_into().ok()?),
        };
        Some((header, &frame[AUDIO_HEADER_LEN..]))
    }
}

/// `header + payload` 拼成完整二进制帧。
pub fn encode_audio_frame(header: AudioHeader, payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(AUDIO_HEADER_LEN + payload.len());
    buf.extend_from_slice(&header.encode());
    buf.extend_from_slice(payload);
    buf.freeze()
}

/// 文本帧 → `ClientMsg`；JSON 非法/字段非法 → `BAD_MESSAGE`。
pub fn decode_client_msg(text: &str) -> Result<ClientMsg, AppError> {
    serde_json::from_str(text).map_err(|e| AppError::BadMessage(e.to_string()))
}

/// `ServerMsg` → 文本帧。
pub fn encode_server_msg(msg: &ServerMsg) -> Result<String, AppError> {
    serde_json::to_string(msg).map_err(|e| AppError::Internal(e.to_string()))
}

/// 版本协商（protocol §3/§8）：`protocol` 必须 `momo-ws`，`version` 只支持 `v1`。
pub fn check_hello(hello: &HelloMsg) -> Result<(), AppError> {
    if hello.protocol != PROTOCOL_NAME {
        return Err(AppError::Protocol(format!(
            "expected protocol {PROTOCOL_NAME:?}, got {:?}",
            hello.protocol
        )));
    }
    if hello.version != PROTOCOL_VERSION {
        return Err(AppError::VersionMismatch(format!(
            "server supports {PROTOCOL_VERSION}, client requested {:?}",
            hello.version
        )));
    }
    Ok(())
}
