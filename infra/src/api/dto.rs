//! 统一 DTO：CLI `--json` 输出与 REST `/api` 响应共用同一套 serde 类型。
//! 顶层恒 `"schema": 1`；错误统一 `{schema, error:{code,message}}` 形态
//! （tests/api_contract_test.rs 锁定字段名/类型/必选性）。

use serde::Serialize;

/// 全部报告/错误顶层的 schema 版本号。
pub const SCHEMA_VERSION: u8 = 1;

/// `GET /api/csrf` 响应：进程级 CSRF token。
/// state-changing 请求须以 `X-CSRF-Token` 头原样回带（web/guard.rs）。
#[derive(Debug, Serialize)]
pub struct CsrfToken {
    pub schema: u8,
    pub csrf_token: String,
}

/// 统一错误体。
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// 机器可读短标识：unauthorized / forbidden / csrf / not_found /
    /// not_implemented / bad_request / internal。
    pub code: String,
    /// 面向用户的一句话说明（英文）。
    pub message: String,
}

/// 统一错误响应：`{schema:1, error:{code,message}}`。
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub schema: u8,
    pub error: ErrorBody,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            error: ErrorBody {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}
