//! `/api/*` 路由树基座：本文件是唯一注册点。
//! `healthz`/`readyz`/`events`/`csrf` 已实装；`doctor`/`serve`/`models`/`jobs`/`config`
//! 为 501 占位——真实 handler 由后续 issue 挂接（DTO 统一、页面功能），
//! 一律复用 `cmds`/`serve`/`manifest`/`env_cfg` 现有 core 函数，
//! JSON 顶层恒 `schema:1`，与 CLI `--json` 同口径；
//! 错误统一 `{schema:1, error:{code,message}}`（`api::dto::ApiError`）。

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::json;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use super::WebState;
use crate::api::dto;

/// `/api` 子树。挂到外层 `nest("/api", ..)`；子树内未命中走 JSON 404，
/// 与 SPA fallback（index.html）分流，互不误判。
pub fn router(state: Arc<WebState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/events", get(events))
        .route("/csrf", get(csrf))
        // 占位路由：any 接全方法（后续写操作经 CSRF 中间件后挂真实 handler）。
        .route("/doctor", any(placeholder))
        .route("/serve", any(placeholder))
        .route("/serve/{*rest}", any(placeholder))
        .route("/models", any(placeholder))
        .route("/models/{*rest}", any(placeholder))
        .route("/jobs", any(placeholder))
        .route("/jobs/{*rest}", any(placeholder))
        .route("/config", any(placeholder))
        .route("/config/{*rest}", any(placeholder))
        .fallback(not_found)
        .with_state(state)
}

/// `GET /api[/]`：路由索引，便于探测已注册面。
/// 外层对 `/api/` 显式挂本 handler（nest 只覆盖 `/api` 与 `/api/{*rest}`，
/// 恰好 `/api/` 会落到 SPA fallback，须显式分流）。
pub(crate) async fn index() -> Json<serde_json::Value> {
    Json(json!({
        "schema": 1,
        "status": "ok",
        "routes": [
            "GET /api/healthz",
            "GET /api/readyz",
            "GET /api/events",
            "GET /api/csrf",
            "/api/doctor (placeholder)",
            "/api/serve/* (placeholder)",
            "/api/models/* (placeholder)",
            "/api/jobs/* (placeholder)",
            "/api/config/* (placeholder)",
        ],
    }))
}

/// `GET /api/healthz`：进程级存活探针（server 自身，不聚合下游组件）。
async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "schema": 1, "status": "ok" }))
}

/// `GET /api/readyz`：就绪探针。基座无下游依赖，恒 ok；
/// 后续可在 handler 内聚合组件 readiness（复用 serve 探活）。
async fn readyz() -> Json<serde_json::Value> {
    Json(json!({ "schema": 1, "status": "ok" }))
}

/// `GET /api/events`：SSE 通道基座。broadcast 转发 `WebState::events_tx`，
/// 慢消费者 Lagged 时跳过失真事件、保持连接（jobs/serve 日志流复用此通路）。
async fn events(
    State(state): State<Arc<WebState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.events_tx.subscribe()).filter_map(|msg| match msg {
        Ok(data) => Some(Ok(Event::default().event("message").data(data))),
        Err(_) => None,
    });
    Sse::new(stream)
}

/// `GET /api/csrf`：签发进程级 CSRF token，state-changing 请求以
/// `X-CSRF-Token` 头回带（guard::require_auth 校验）。
/// 响应不带 CORS 允许头——跨源页面读不到该值（SOP），token 不会泄漏给第三方站点。
async fn csrf(State(state): State<Arc<WebState>>) -> Json<dto::CsrfToken> {
    Json(dto::CsrfToken {
        schema: dto::SCHEMA_VERSION,
        csrf_token: state.csrf_token.clone(),
    })
}

/// 统一错误出口：`{schema:1, error:{code,message}}`（dto::ApiError 口径）。
/// guard 中间件与 handler 共用，保证 /api 错误形态唯一。
pub(crate) fn error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (status, Json(dto::ApiError::new(code, message))).into_response()
}

/// 占位 handler：501 + 错误 DTO。挂接点前任何方法均无 core 副作用。
async fn placeholder(uri: Uri) -> Response {
    error(
        StatusCode::NOT_IMPLEMENTED,
        "not_implemented",
        format!("route {} is not implemented yet", uri.path()),
    )
}

/// `/api` 子树内未命中 → JSON 404（区别于 SPA fallback 的 index.html）。
async fn not_found(uri: Uri) -> Response {
    error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("no route {}", uri.path()),
    )
}
