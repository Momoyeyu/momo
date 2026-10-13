//! `/api/*` 路由树基座：本文件是唯一注册点。
//! 已实装：`healthz`/`readyz`/`events`/`csrf`/`doctor`/`serve status`/
//! `serve components`/`models`（只读）、`serve {up|down|restart}`/
//! `models/pull`（POST）、`serve logs/{name}/stream`（SSE）与 `jobs`
//! （REST + SSE 日志）；`config` 与其余子路由为 501 占位——真实 handler
//! 由后续 issue 挂接。handler 只做 HTTP↔core 适配，业务复用 `cmds`/`serve`/
//! `models`/`manifest`/`env_cfg` 现有 core 函数；出入参一律 `api::dto`
//! 结构化类型（不手写 `json!`），与 CLI `--json` 同 schema；错误统一
//! `{schema:1, error:{code,message}}`。

use std::convert::Infallible;
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use tokio_stream::wrappers::{BroadcastStream, ReceiverStream};
use tokio_stream::{Stream, StreamExt};

use super::WebState;
use crate::api::dto;
use crate::cli::Profile;
use crate::output::AppError;

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
        .route("/serve/components", any(serve_components))
        // 实装的只读端点同样 any + 方法门控：非 GET → JSON 405 而非框架空 405。
        .route("/serve/logs/{name}/stream", any(serve_log_stream))
        // 写端点：POST /api/serve/{up|down|restart}（CSRF 经守卫中间件）；
        // 未知 action 由 handler 给 JSON 404（{action} 与 {*rest} 路由冲突，
        // /serve 下不再挂通配占位——未命中路径走 fallback JSON 404）。
        .route("/serve/{action}", any(serve_action))
        // 占位路由：any 接全方法（后续写操作经 CSRF 中间件后挂真实 handler）。
        .route("/doctor/{*rest}", any(placeholder))
        .route("/serve", any(placeholder))
        .route("/models", any(models))
        // 写端点：POST /api/models/pull {name}（CSRF 经守卫中间件）；
        // 与 /models/{*rest} 不冲突——静态段优先于通配。
        .route("/models/pull", any(models_pull))
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
            "GET /api/serve/components",
            "POST /api/serve/{up|down|restart}",
            "GET /api/serve/logs/{name}/stream (SSE)",
            "GET /api/models",
            "POST /api/models/pull",
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

/// `GET /api/serve/components[?profile=]`：内置 profile 组件表。
/// 渲染组件页表格（order/external/local_endpoint_only 标记）；
/// 只解析内嵌 toml，无 IO，可直接同步返回。
async fn serve_components(method: Method, Query(q): Query<dto::ProfileQuery>) -> Response {
    let Some(profile) = parse_profile(&method, q.profile.as_deref()) else {
        return bad_method_or_profile(&method, q.profile.as_deref());
    };
    match crate::cmds::serve::components_report(profile) {
        Ok(report) => Json(report).into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("serve components failed: {e}"),
        ),
    }
}

/// `POST /api/serve/{up|down|restart}[?profile=]`：组件控制（CSRF 已过守卫）。
/// body `{only:[..]?}`（空 body = 全组件）；与 `momoctl serve <action>` 同 core
/// （`cmds::serve::action_report`），响应 `ServeReport`——逐项 Check 回显，
/// fail 是数据非 HTTP 错误，恒 200；`only` 未命中组件等用法错误 → 400。
async fn serve_action(
    method: Method,
    Path(action): Path<String>,
    Query(q): Query<dto::ProfileQuery>,
    body: Bytes,
) -> Response {
    if method != Method::POST {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            format!("{method} is not allowed; use POST"),
        );
    }
    if !matches!(action.as_str(), "up" | "down" | "restart") {
        return error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no route /api/serve/{action}"),
        );
    }
    let Some(profile) = parse_profile_param(q.profile.as_deref()) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!(
                "invalid profile '{}'; expected normal|degraded",
                q.profile.as_deref().unwrap_or("")
            ),
        );
    };
    let req: dto::ServeActionRequest = if body.is_empty() {
        dto::ServeActionRequest { only: Vec::new() }
    } else {
        match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                return error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    format!("invalid body: {e}"),
                )
            }
        }
    };
    let action_name = action.clone();
    match tokio::task::spawn_blocking(move || {
        crate::cmds::serve::action_report(&action, profile, req.only)
    })
    .await
    {
        Ok(Ok((report, _code))) => Json(report).into_response(),
        Ok(Err(AppError::Config(m))) => error(StatusCode::BAD_REQUEST, "bad_request", m),
        Ok(Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("serve {action_name} failed: {e}"),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("serve {action_name} task failed: {e}"),
        ),
    }
}

/// `GET /api/serve/logs/{name}/stream[?profile=&cursor=]`：组件日志 SSE tail。
/// 无 cursor 时先发有界 backlog（尾部 `LOG_BACKLOG_LINES` 行），之后按字节
/// offset 轮询推送；每帧 `id:` = 已消费字节 offset，`data:` = `LogBatch` JSON。
/// 断线后客户端带最后 cursor（或 EventSource 的 Last-Event-ID 头）重连，
/// 从该 offset 续推，不重放无限 backlog。`?profile=` 仅决定组件名校验口径。
async fn serve_log_stream(
    method: Method,
    State(_state): State<Arc<WebState>>,
    Path(name): Path<String>,
    Query(q): Query<dto::LogStreamQuery>,
    headers: HeaderMap,
) -> Response {
    if !matches!(method, Method::GET | Method::HEAD) {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            format!("{method} is not allowed on this read-only endpoint"),
        );
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!("invalid component name '{name}'"),
        );
    }
    let Some(profile) = parse_profile_param(q.profile.as_deref()) else {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            format!(
                "invalid profile '{}'; expected normal|degraded",
                q.profile.as_deref().unwrap_or("")
            ),
        );
    };
    match crate::cmds::serve::components_report(profile) {
        Ok(report) if report.components.iter().any(|c| c.name == name) => {}
        Ok(_) => {
            return error(
                StatusCode::NOT_FOUND,
                "not_found",
                format!("no component '{name}' in this profile"),
            )
        }
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                format!("serve components failed: {e}"),
            )
        }
    }

    // 初始 offset：?cursor= > Last-Event-ID > 文件末尾（只发 backlog+新增）。
    let header_cursor = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let cursor = q.cursor.or(header_cursor);

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(LOG_STREAM_CAP);
    let path = crate::serve::proc::log_dir().join(format!("{name}.log"));
    // 文件 tail 是阻塞 IO + 定间隔轮询：每连接一条 OS thread 而非占 blocking 池。
    std::thread::spawn(move || tail_log_file(name, path, cursor, tx));
    Sse::new(ReceiverStream::new(rx).map(Ok::<Event, Infallible>))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `?cursor=` 缺省时的 backlog 行数（有界，防初始重放全量日志）。
const LOG_BACKLOG_LINES: usize = 50;
/// SSE 推送 channel 容量（行批，不是字节）。
const LOG_STREAM_CAP: usize = 32;
/// 轮询间隔（与 CLI `serve logs -f` 同口径 300ms）。
const LOG_POLL: std::time::Duration = std::time::Duration::from_millis(300);

/// SSE tail 线程：先发 backlog（无 cursor 时），随后按 offset 轮询追加内容。
/// 每帧 `id` = 消费后 offset；客户端断开（channel 关闭）即退出。
fn tail_log_file(
    name: String,
    path: std::path::PathBuf,
    cursor: Option<u64>,
    tx: tokio::sync::mpsc::Sender<Event>,
) {
    let mut offset: u64 = match cursor {
        Some(c) => c,
        None => {
            let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            // 无 cursor：有界 backlog（最后 N 行），之后从文件末尾续推。
            if len > 0 {
                if let Ok(lines) = crate::cmds::serve::read_last_lines(&path, LOG_BACKLOG_LINES) {
                    if !lines.is_empty() && !send_log_batch(&tx, &name, lines, len) {
                        return;
                    }
                }
            }
            len
        }
    };
    loop {
        if tx.is_closed() {
            return;
        }
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if len < offset {
            offset = 0; // 日志截断/轮转：从头读
        }
        if len > offset {
            if let Ok(mut f) = std::fs::File::open(&path) {
                if f.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buf = Vec::new();
                    if f.read_to_end(&mut buf).is_ok() {
                        offset = len;
                        let text = String::from_utf8_lossy(&buf);
                        let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
                        if !lines.is_empty() && !send_log_batch(&tx, &name, lines, offset) {
                            return;
                        }
                    }
                }
            }
        }
        std::thread::sleep(LOG_POLL);
    }
}

/// 组一帧 `event:log` + `id:<offset>` + `data:{component,lines}` 发送；
/// 客户端断开返回 false（调用方退出线程）。
fn send_log_batch(
    tx: &tokio::sync::mpsc::Sender<Event>,
    name: &str,
    lines: Vec<String>,
    offset: u64,
) -> bool {
    let batch = dto::LogBatch {
        component: name,
        lines: lines.iter().map(String::as_str).collect(),
    };
    let data = serde_json::to_string(&batch).unwrap_or_else(|_| "{}".into());
    tx.blocking_send(
        Event::default()
            .event("log")
            .id(offset.to_string())
            .data(data),
    )
    .is_ok()
}

/// `GET /api/models`：MANIFEST + models.lock merge 清单 + 激活选择（只读）。
/// core 为 `models::collect_report`；MANIFEST 缺失返回 200 + `manifest:null`
/// + warn（区分「未部署清单」与故障），其余 core 错误 → 500。
async fn models(method: Method) -> Response {
    if !matches!(method, Method::GET | Method::HEAD) {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            format!("{method} is not allowed on this read-only endpoint"),
        );
    }
    match tokio::task::spawn_blocking(crate::models::collect_report).await {
        Ok(Ok(report)) => Json(report).into_response(),
        Ok(Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("models report failed: {e}"),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("models report task failed: {e}"),
        ),
    }
}

/// `POST /api/models/pull`：拉取单个 MANIFEST 条目（`{name}`）。
/// 与 `momoctl pull --only <name>` 同 core（`cmds::pull::pull_report`），
/// spawn_blocking 直跑等结果——逐文件进度流待 jobs runner（#54）。
/// 响应 `PullReport`（fail 是数据非 HTTP 错误，恒 200）；
/// body/name 非法、MANIFEST/条目缺失 → 400。
async fn models_pull(method: Method, body: Bytes) -> Response {
    if method != Method::POST {
        return error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            format!("{method} is not allowed; use POST"),
        );
    }
    let req: dto::ModelsPullRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                format!("invalid body: {e}"),
            )
        }
    };
    if req.name.trim().is_empty() {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "field 'name' is required",
        );
    }
    match tokio::task::spawn_blocking(move || crate::cmds::pull::pull_report(None, vec![req.name]))
        .await
    {
        Ok(Ok((report, _code))) => Json(report).into_response(),
        Ok(Err(AppError::Config(m))) => error(StatusCode::BAD_REQUEST, "bad_request", m),
        Ok(Err(e)) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("pull failed: {e}"),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            format!("pull task failed: {e}"),
        ),
    }
}

/// GET/HEAD → Some(profile 解析结果)；其余方法或非法 profile → None
/// （由 `bad_method_or_profile` 产出对应错误响应）。
fn parse_profile(method: &Method, profile: Option<&str>) -> Option<Option<Profile>> {
    if !matches!(*method, Method::GET | Method::HEAD) {
        return None;
    }
    parse_profile_param(profile)
}

/// `?profile=` 值解析：缺省/合法 → Some；非法 → None（调用方给 400）。
fn parse_profile_param(profile: Option<&str>) -> Option<Option<Profile>> {
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
