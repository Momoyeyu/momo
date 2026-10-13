//! `/api/*` 路由树基座：本文件是唯一注册点。
//! 已实装：`healthz`/`readyz`/`events`/`csrf`/`doctor`/`serve status`（只读）
//! 与 `jobs`（REST + SSE 日志）；`serve`/`models`/`config` 其余为 501 占位——
//! 真实 handler 由后续 issue 挂接。handler 只做 HTTP↔core 适配，业务复用
//! `cmds`/`serve`/`manifest`/`env_cfg` 现有 core 函数；出入参一律 `api::dto`
//! 结构化类型（不手写 `json!`），与 CLI `--json` 同 schema；错误统一
//! `{schema:1, error:{code,message}}`。

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{Method, StatusCode, Uri};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use super::WebState;
use crate::api::dto;
use crate::cli::Profile;

/// `/api` 子树。挂到外层 `nest("/api", ..)`；子树内未命中走 JSON 404，
/// 与 SPA fallback（index.html）分流，互不误判。
pub fn router(state: Arc<WebState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/events", get(events))
        .route("/csrf", get(csrf))
        // 实装的只读端点用 any + 方法门控：非 GET/HEAD 一律 JSON 405
        // （保证 /api 错误形态唯一，不落框架默认空 405）。
        .route("/doctor", any(doctor))
        .route("/serve/status", any(serve_status))
        // 占位路由：any 接全方法（后续写操作经 CSRF 中间件后挂真实 handler）。
        .route("/doctor/{*rest}", any(placeholder))
        .route("/serve", any(placeholder))
        .route("/serve/{*rest}", any(placeholder))
        .route("/models", any(placeholder))
        .route("/models/{*rest}", any(placeholder))
        // jobs 已挂真实 handler（web/jobs.rs）；其余命名空间仍是 501 占位。
        .nest("/jobs", super::jobs::router())
        .route("/config", any(placeholder))
        .route("/config/{*rest}", any(placeholder))
        .fallback(not_found)
        .with_state(state)
}

/// `GET /api[/]`：路由索引，便于探测已注册面。
/// 外层对 `/api/` 显式挂本 handler（nest 只覆盖 `/api` 与 `/api/{*rest}`，
/// 恰好 `/api/` 会落到 SPA fallback，须显式分流）。
pub(crate) async fn index() -> Json<dto::IndexReport> {
    Json(dto::IndexReport {
        schema: dto::SCHEMA_VERSION,
        status: "ok",
        routes: vec![
            "GET /api/healthz",
            "GET /api/readyz",
            "GET /api/events",
            "GET /api/csrf",
            "GET /api/doctor",
            "GET /api/serve/status",
            "/api/serve/* (placeholder)",
            "/api/models/* (placeholder)",
            "GET /api/jobs",
            "POST /api/jobs",
            "GET /api/jobs/{id}",
            "POST /api/jobs/{id}/cancel",
            "GET /api/jobs/{id}/logs?cursor= (SSE)",
            "/api/config/* (placeholder)",
        ],
    })
}

/// `GET /api/healthz`：进程级存活探针（server 自身，不聚合下游组件）。
async fn healthz() -> Json<dto::HealthReport> {
    Json(dto::HealthReport {
        schema: dto::SCHEMA_VERSION,
        status: "ok",
    })
}

/// `GET /api/readyz`：就绪探针。基座无下游依赖，恒 ok；
/// 后续可在 handler 内聚合组件 readiness（复用 serve 探活）。
async fn readyz() -> Json<dto::HealthReport> {
    Json(dto::HealthReport {
        schema: dto::SCHEMA_VERSION,
        status: "ok",
    })
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

/// `GET /api/doctor[?profile=]`：环境检查矩阵。
/// core 与 `momoctl doctor` 同为 `cmds::doctor::collect_report`，
/// 响应即 `DoctorReport`——与 `doctor --json` 逐字节同型（契约测试锁定）。
/// 检查有 fail 仍 200：fail 是检查数据而非 HTTP 错误（对应 CLI exit 3 语义）。
async fn doctor(method: Method, Query(q): Query<dto::ProfileQuery>) -> Response {
    let Some(profile) = parse_profile(&method, q.profile.as_deref()) else {
        return bad_method_or_profile(&method, q.profile.as_deref());
    };
    // collect_report 做进程/网络/文件探测（秒级阻塞），offload 到 blocking 池。
    match tokio::task::spawn_blocking(move || crate::cmds::doctor::collect_report(profile)).await {
        Ok(report) => Json(report).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("doctor task failed: {e}"),
        ),
    }
}

/// `GET /api/serve/status[?profile=]`：组件存活/就绪快照。
/// core 与 `momoctl serve status` 同为 `cmds::serve::status_report`（ServeReport）。
/// 恒 200 下发报告；组件 fail 体现在 checks（对应 CLI exit 3 语义）。
async fn serve_status(method: Method, Query(q): Query<dto::ProfileQuery>) -> Response {
    let Some(profile) = parse_profile(&method, q.profile.as_deref()) else {
        return bad_method_or_profile(&method, q.profile.as_deref());
    };
    match tokio::task::spawn_blocking(move || crate::cmds::serve::status_report(profile)).await {
        Ok(Ok((report, _code))) => Json(report).into_response(),
        Ok(Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("serve status failed: {e}"),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("serve status task failed: {e}"),
        ),
    }
}

/// GET/HEAD → Some(profile 解析结果)；其余方法或非法 profile → None
/// （由 `bad_method_or_profile` 产出对应错误响应）。
fn parse_profile(method: &Method, profile: Option<&str>) -> Option<Option<Profile>> {
    if !matches!(*method, Method::GET | Method::HEAD) {
        return None;
    }
    match profile.map(|s| s.to_ascii_lowercase()).as_deref() {
        None => Some(None),
        Some("normal") => Some(Some(Profile::Normal)),
        Some("degraded") => Some(Some(Profile::Degraded)),
        Some(_) => None,
    }
}

/// `parse_profile` 失败的错误分流：非只读方法 → 405；非法 profile 值 → 400。
fn bad_method_or_profile(method: &Method, profile: Option<&str>) -> Response {
    if !matches!(*method, Method::GET | Method::HEAD) {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            format!("{method} is not allowed on this read-only endpoint"),
        );
    }
    error(
        StatusCode::BAD_REQUEST,
        "bad_request",
        format!(
            "invalid profile '{}'; expected normal|degraded",
            profile.unwrap_or("")
        ),
    )
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
