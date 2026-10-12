//! 帧编解码：文本帧 JSON / 二进制音频帧分流，版本握手检查。

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::{
    error::AppError,
};

/// 二进制音频帧头长度（protocol.md 二进制帧一节）。
pub const AUDIO_HEADER_LEN: usize = 32;
/// 头 `ver` 字段恒为 2（protocol v2）。
pub const AUDIO_VER: u8 = 2;
/// `direction`：上行（UI→orchestrator→ASR）。
pub const DIR_UPLINK: u8 = 1;
/// `direction`：下行（TTS→orchestrator→UI）。
pub const DIR_DOWNLINK: u8 = 2;
/// `flags` bit0：EOS，本 stream 最后一帧；其余 bit 为 reserved，非零即拒绝。
pub const FLAG_EOS: u16 = 0x0001;

/// WS close code（protocol.md close code 一节）。
pub const CLOSE_NORMAL: u16 = 1000;
/// 版本不匹配。
pub const CLOSE_VERSION_MISMATCH: u16 = 4001;
/// hello timeout / malformed / 协议违反。
pub const CLOSE_PROTOCOL_VIOLATION: u16 = 4400;
/// 未授权（token 校验失败）。
pub const CLOSE_UNAUTHORIZED: u16 = 4401;
/// 服务端内部错误。
pub const CLOSE_INTERNAL: u16 = 1011;

/// 帧头 `direction` / `audio.open.direction` 共用枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// 上行：UI → orchestrator → ASR。
    Uplink,
    /// 下行：TTS → orchestrator → UI。
    Downlink,
}

impl Direction {
    /// 帧头字节值：`1` uplink / `2` downlink。
    pub fn as_u8(self) -> u8 {
        match self {
            Direction::Uplink => DIR_UPLINK,
            Direction::Downlink => DIR_DOWNLINK,
        }
    }

    fn from_u8(v: u8) -> Option<Self> {
        match v {
            DIR_UPLINK => Some(Direction::Uplink),
            DIR_DOWNLINK => Some(Direction::Downlink),
            _ => None,
        }
    }
}

/// 二进制音频帧 32B 头（payload 不含在内）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub direction: Direction,
    /// bit0=EOS；reserved bit 非零的帧在 [`AudioHeader::decode`] 时即被拒绝。
    pub flags: u16,
    /// 所属 stream 的原始 ULID（128-bit，非文字前缀）。
    pub stream_id: Ulid,
    /// 每 stream 从 `0` 开始严格递增的帧序号。
    pub chunk_seq: u32,
    /// 所属 `output.sentence.seq`；uplink 固定 `0`。
    pub sentence_seq: u32,
    /// 本帧首 sample 的偏移；从 `0` 开始按每声道 PCM sample 累计。
    pub sample_offset: u32,
}

impl AudioHeader {
    /// 编码为 32B 头（`ver` 自动置 [`AUDIO_VER`]）。
    pub fn encode(self) -> [u8; AUDIO_HEADER_LEN] {
        let mut buf = [0u8; AUDIO_HEADER_LEN];
        buf[0] = AUDIO_VER;
        buf[1] = self.direction.as_u8();
        buf[2..4].copy_from_slice(&self.flags.to_le_bytes());
        buf[4..20].copy_from_slice(&self.stream_id.to_bytes());
        buf[20..24].copy_from_slice(&self.chunk_seq.to_le_bytes());
        buf[24..28].copy_from_slice(&self.sentence_seq.to_le_bytes());
        buf[28..32].copy_from_slice(&self.sample_offset.to_le_bytes());
        buf
    }

    /// 解码头并返回载荷切片。
    pub fn decode(frame: &[u8]) -> Result<(Self, &[u8]), AppError> {
        fn bad(why: &str) -> AppError {
            AppError::BadMessage(format!("audio frame: {why}"))
        }
        if frame.len() < AUDIO_HEADER_LEN {
            return Err(bad("header shorter than 32 bytes"));
        }
        if frame[0] != AUDIO_VER {
            return Err(bad("unsupported version"));
        }
        let direction = Direction::from_u8(frame[1]).ok_or_else(|| bad("invalid direction"))?;
        let flags = u16::from_le_bytes(frame[2..4].try_into().expect("4-byte field"));
        if flags & !FLAG_EOS != 0 {
            return Err(bad("reserved flags set"));
        }
        let header = AudioHeader {
            direction,
            flags,
            stream_id: Ulid::from_bytes(frame[4..20].try_into().expect("16-byte field")),
            chunk_seq: u32::from_le_bytes(frame[20..24].try_into().expect("4-byte field")),
            sentence_seq: u32::from_le_bytes(frame[24..28].try_into().expect("4-byte field")),
            sample_offset: u32::from_le_bytes(frame[28..32].try_into().expect("4-byte field")),
        };
        let payload = &frame[AUDIO_HEADER_LEN..];
        if payload.len() % 2 != 0 {
            return Err(bad("odd payload length (pcm_s16le)"));
        }
        if payload.is_empty() && !header.eos() {
            return Err(bad("empty payload without EOS"));
        }
        if header.direction == Direction::Uplink && header.sentence_seq != 0 {
            return Err(bad("uplink sentence_seq must be 0"));
        }
        Ok((header, payload))
    }

    /// EOS：本 stream/sentence 数据结束（可携带空载荷表示零长度 capture）。
    pub fn eos(&self) -> bool {
        self.flags & FLAG_EOS != 0
    }
}

/// `header + payload` 拼成完整二进制帧。
pub fn encode_audio_frame(header: AudioHeader, payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(AUDIO_HEADER_LEN + payload.len());
    buf.extend_from_slice(&header.encode());
    buf.extend_from_slice(payload);
    buf.freeze()
}

    serde_json::from_str(text).map_err(|e| AppError::BadMessage(e.to_string()))
}

    serde_json::to_string(msg).map_err(|e| AppError::Internal(e.to_string()))
}

/// 版本协商（protocol 认证一节）：`protocol` 必须 `momo-ws`，`version` 必须匹配。
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
