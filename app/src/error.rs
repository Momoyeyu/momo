//! `AppError`：编排器统一错误类型，ws 层映射为协议 `error` 帧。

use thiserror::Error;

use crate::{config::ConfigError, proto::message::ErrorCode};

/// 编排器错误；`code()` 给出协议 §2 `error.code` 的映射。
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON 非法/字段非法/未知 type → `BAD_MESSAGE`。
    #[error("bad message: {0}")]
    BadMessage(String),

    /// 首消息非 hello / hello.protocol 不符 → close 4400。
    #[error("protocol violation: {0}")]
    Protocol(String),

    /// hello.version 协商失败 → `VERSION_MISMATCH` + close 4001。
    #[error("version mismatch: {0}")]
    VersionMismatch(String),

    /// hello.auth_token 校验失败 → `UNAUTHORIZED` + close 4401。
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// 上游模型服务断连 → `MODEL_UNREACHABLE`。
    #[error("model service unreachable: {0}")]
    ModelUnreachable(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// 映射为 protocol.md §2 的 error code。
    pub fn code(&self) -> ErrorCode {
        match self {
            AppError::Config(_) | AppError::Io(_) | AppError::Internal(_) => ErrorCode::Internal,
            AppError::BadMessage(_) => ErrorCode::BadMessage,
            AppError::Protocol(_) => ErrorCode::Internal,
            AppError::VersionMismatch(_) => ErrorCode::VersionMismatch,
            AppError::Unauthorized(_) => ErrorCode::Unauthorized,
            AppError::ModelUnreachable(_) => ErrorCode::ModelUnreachable,
        }
    }
}
