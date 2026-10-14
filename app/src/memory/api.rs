//! `/api/memories` + `/api/memory/human-block`：记忆审计面 REST 后端
//! （docs/03-features/memory.md §API）。
//!
//! 编辑=旧条 superseded_by 指向新条（provenance='manual'），删除=软删——
//! 失效语义与抽取侧一致，审计视图（`all=1`）可查完整历史。
//! 鉴权沿用 `/api/settings` 的 Bearer 口径（`crate::api::check_auth`）。

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    api::{check_auth, err_body, invalid},
    ws::server::AppState,
};

use super::store::{HUMAN_BLOCK_MAX_CHARS, MEMORY_CONTENT_MAX_CHARS, MemoryKind};

/// 列表 query：`kind` 过滤、`limit`/`offset` 分页、`all=1` 含失效/已删。
#[derive(Debug, Deserialize)]
pub struct ListParams {
    kind: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    /// `1`/`true` → 审计视图（含 superseded/deleted）。
    all: Option<String>,
}

/// `GET /api/memories`：分页列表（默认只 valid，新→旧）。
pub async fn get_memories(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let Some(store) = state.memory.as_ref() else {
        return unavailable();
    };
    let kind = match params.kind.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(k) => match MemoryKind::parse(k) {
            Some(k) => Some(k),
            None => return invalid(format!("unknown kind: {k}"), "kind"),
        },
    };
    let all = matches!(params.all.as_deref(), Some("1") | Some("true"));
    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);
    match store.list(kind, all, limit, offset) {
        Ok(memories) => (
            StatusCode::OK,
            Json(json!({ "schema": 1, "memories": memories })),
        )
            .into_response(),
        Err(e) => internal(&e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ContentBody {
    content: Option<String>,
}

/// `PUT /api/memories/{id}` `{content}`：编辑 = supersede 旧条 + 新条 manual。
pub async fn put_memory(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ContentBody>,
) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let Some(store) = state.memory.as_ref() else {
        return unavailable();
    };
    let Some(content) = valid_content(body.content.as_deref(), MEMORY_CONTENT_MAX_CHARS) else {
        return content_err(body.content.as_deref());
    };
    match store.edit(&id, content) {
        Ok(Some(memory)) => (
            StatusCode::OK,
            Json(json!({ "schema": 1, "memory": memory })),
        )
            .into_response(),
        Ok(None) => not_found("memory", &id),
        Err(e) => internal(&e),
    }
}

/// `DELETE /api/memories/{id}`：软删（`deleted_at`，审计面仍可见）。
pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let Some(store) = state.memory.as_ref() else {
        return unavailable();
    };
    match store.soft_delete(&id) {
        Ok(true) => (StatusCode::OK, Json(json!({ "schema": 1, "deleted": id }))).into_response(),
        Ok(false) => not_found("memory", &id),
        Err(e) => internal(&e),
    }
}

/// `GET /api/memory/human-block`：`{content, updated_at}`（未写过 content=''）。
pub async fn get_human_block(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let Some(store) = state.memory.as_ref() else {
        return unavailable();
    };
    match store.human_block() {
        Ok((content, updated_at)) => (
            StatusCode::OK,
            Json(json!({ "schema": 1, "content": content, "updated_at": updated_at })),
        )
            .into_response(),
        Err(e) => internal(&e),
    }
}

/// `PUT /api/memory/human-block` `{content}`：全量改写，≤2000 字符（core block 上限）。
pub async fn put_human_block(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ContentBody>,
) -> Response {
    if let Some(resp) = check_auth(&state, &headers) {
        return resp;
    }
    let Some(store) = state.memory.as_ref() else {
        return unavailable();
    };
    // human block 允许清空（用户画像重置）。
    let content = body.content.unwrap_or_default();
    if content.chars().count() > HUMAN_BLOCK_MAX_CHARS {
        return invalid(
            format!("content exceeds {HUMAN_BLOCK_MAX_CHARS} chars"),
            "content",
        );
    }
    match store.set_human_block(&content) {
        Ok(()) => get_human_block(State(state), headers).await,
        Err(e) => internal(&e),
    }
}

/// content 校验：去空白后非空且不超长。
fn valid_content(content: Option<&str>, max_chars: usize) -> Option<&str> {
    let c = content?.trim();
    if c.is_empty() || c.chars().count() > max_chars {
        return None;
    }
    Some(c)
}

fn content_err(content: Option<&str>) -> Response {
    let message = match content.map(str::trim) {
        None | Some("") => "content must not be empty".to_string(),
        Some(_) => format!("content exceeds {MEMORY_CONTENT_MAX_CHARS} chars"),
    };
    invalid(message, "content")
}

fn not_found(kind: &str, id: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(err_body(
            "not_found",
            &format!("{kind} {id} not found or invalid"),
        )),
    )
        .into_response()
}

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(err_body("unavailable", "memory store unavailable")),
    )
        .into_response()
}

fn internal(e: &crate::error::AppError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(err_body("internal", &e.to_string())),
    )
        .into_response()
}
