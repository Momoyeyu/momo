//! `/api/memories` + `/api/memory/human-block` 路由级测试
//! （docs/03-features/memory.md §API）。
//!
//! `env_only` state 的进程内 sqlite：不碰用户 `memory.db`，端点可复现。
//! 语义层（supersede/软删/去重）单测在 `src/memory/store.rs`，这里验证
//! 路由装配：鉴权、kind 过滤、编辑=supersede、软删、human-block 往返。

use std::sync::Arc;

use axum::{Router, body::Body, http::Request};
use momo_app::{
    config::Config,
    memory::store::MemoryKind,
    ws::server::{self, AppState},
};
use serde_json::{Value, json};
use tower::ServiceExt;

fn env_router() -> (Router, Arc<AppState>) {
    let state = Arc::new(AppState::env_only(Arc::new(
        Config::load(|_| None).expect("default config"),
    )));
    (server::router_from_state(state.clone()), state)
}

async fn call(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut req = Request::builder().method(method).uri(uri);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    app.oneshot(
        req.body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn json_body(resp: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn memories_crud_and_audit_view() {
    let (app, state) = env_router();
    let store = state.memory.as_ref().expect("env_only memory store");
    let id = match store
        .add(MemoryKind::Semantic, "喜欢抹茶", Some("t_1"), 1.0)
        .unwrap()
    {
        momo_app::memory::AddOutcome::Added(id) => id,
        _ => panic!("expected Added"),
    };
    store
        .add(MemoryKind::Episodic, "一起看了海", None, 1.0)
        .unwrap();

    // kind 过滤：semantic 只见一条。
    let resp = call(app.clone(), "GET", "/api/memories?kind=semantic", None).await;
    assert_eq!(resp.status(), 200);
    let body = json_body(resp).await;
    let memories = body["memories"].as_array().expect("memories array");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0]["content"], "喜欢抹茶");
    assert_eq!(memories[0]["provenance_turn_id"], "t_1");

    // 非法 kind → 400 invalid。
    let resp = call(app.clone(), "GET", "/api/memories?kind=bogus", None).await;
    assert_eq!(resp.status(), 400);
    assert_eq!(json_body(resp).await["error"]["code"], "invalid");

    // 编辑 = supersede：旧条失效、新条 manual。
    let resp = call(
        app.clone(),
        "PUT",
        &format!("/api/memories/{id}"),
        Some(json!({"content": "喜欢焙茶"})),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body = json_body(resp).await;
    let new_id = body["memory"]["id"].as_str().expect("new id").to_string();
    assert_ne!(new_id, id);
    assert_eq!(body["memory"]["provenance_turn_id"], "manual");
    let old = store.get(&id).unwrap().unwrap();
    assert_eq!(old.superseded_by.as_deref(), Some(new_id.as_str()));

    // 审计视图：all=1 含失效旧条。
    let resp = call(app.clone(), "GET", "/api/memories?all=1", None).await;
    let body = json_body(resp).await;
    assert_eq!(body["memories"].as_array().unwrap().len(), 3);

    // 软删：默认列表排除，二次删除 404。
    let resp = call(
        app.clone(),
        "DELETE",
        &format!("/api/memories/{new_id}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), 200);
    let resp = call(
        app.clone(),
        "DELETE",
        &format!("/api/memories/{new_id}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = call(app.clone(), "GET", "/api/memories?kind=semantic", None).await;
    let body = json_body(resp).await;
    assert!(body["memories"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn human_block_roundtrip_and_limit() {
    let (app, _state) = env_router();
    let resp = call(app.clone(), "GET", "/api/memory/human-block", None).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(json_body(resp).await["content"], "");

    let resp = call(
        app.clone(),
        "PUT",
        "/api/memory/human-block",
        Some(json!({"content": "用户叫阿树"})),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(json_body(resp).await["content"], "用户叫阿树");

    let long = "x".repeat(2001);
    let resp = call(
        app.clone(),
        "PUT",
        "/api/memory/human-block",
        Some(json!({"content": long})),
    )
    .await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn memories_require_bearer_when_ui_token_set() {
    let state = Arc::new(AppState::env_only(Arc::new(
        Config::load(|k| (k == "MOMO_UI_TOKEN").then(|| "secret".to_string())).expect("config"),
    )));
    let app = server::router_from_state(state);
    let resp = call(app.clone(), "GET", "/api/memories", None).await;
    assert_eq!(resp.status(), 401);
    let req = Request::builder()
        .method("GET")
        .uri("/api/memory/human-block")
        .header("authorization", "Bearer secret");
    let resp = app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), 200);
}
