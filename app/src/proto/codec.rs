//! 帧编解码：文本帧 JSON / 二进制音频帧分流，版本握手检查。

use bytes::{Bytes, BytesMut};

use crate::{
    error::AppError,
};

pub const FLAG_EOS: u16 = 0x0001;

pub const CLOSE_NORMAL: u16 = 1000;
/// 版本不匹配。
pub const CLOSE_VERSION_MISMATCH: u16 = 4001;
pub const CLOSE_PROTOCOL_VIOLATION: u16 = 4400;
/// 服务端内部错误。
pub const CLOSE_INTERNAL: u16 = 1011;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub flags: u16,
}

impl AudioHeader {
    pub fn encode(self) -> [u8; AUDIO_HEADER_LEN] {
        let mut buf = [0u8; AUDIO_HEADER_LEN];
        buf[0] = AUDIO_VER;
        buf
    }

        }
        let header = AudioHeader {
        };
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
