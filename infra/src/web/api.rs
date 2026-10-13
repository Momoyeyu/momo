//! `/api/*` 路由树基座：本文件是唯一注册点。
//! `healthz`/`readyz`/`events` 已实装；`doctor`/`serve`/`models`/`jobs`/`config`
//! 为 501 占位——真实 handler 由后续 issue 挂接（DTO 统一、页面功能），
//! 一律复用 `cmds`/`serve`/`manifest`/`env_cfg` 现有 core 函数，
//! JSON 顶层恒 `schema:1`，与 CLI `--json` 同口径。

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

/// `/api` 子树。挂到外层 `nest("/api", ..)`；子树内未命中走 JSON 404，
/// 与 SPA fallback（index.html）分流，互不误判。
pub fn router(state: Arc<WebState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/events", get(events))
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

/// 占位 handler：501 + `schema:1` JSON。挂接点前任何方法均无 core 副作用。
async fn placeholder(uri: Uri) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "schema": 1,
            "status": "not_implemented",
            "route": uri.path(),
        })),
    )
        .into_response()
}

/// `/api` 子树内未命中 → JSON 404（区别于 SPA fallback 的 index.html）。
async fn not_found(uri: Uri) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "schema": 1,
            "status": "not_found",
            "route": uri.path(),
        })),
    )
        .into_response()
}
