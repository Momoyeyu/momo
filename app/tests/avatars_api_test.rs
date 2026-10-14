//! `GET /api/avatars` 与 `/avatars/` 静态托管的路由级冒烟
//! （docs/01-architecture/settings.md §形象 manifest）。
//!
//! 扫描逻辑的单测在 `src/api.rs`（临时目录注入）；这里验证路由装配：
//! manifest 结构、内置项首位、check_auth 复用与静态目录挂载。

use std::sync::Arc;

use axum::{Router, body::Body, http::Request};
use momo_app::{
    config::Config,
    ws::server::{self, AppState},
};
use serde_json::Value;
use tower::ServiceExt;

fn env_router() -> Router {
    server::router_from_state(Arc::new(AppState::env_only(Arc::new(
        Config::load(|_| None).expect("default config"),
    ))))
}

async fn get(app: Router, uri: &str, token: Option<&str>) -> axum::response::Response {
    let mut req = Request::builder().uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    app.oneshot(req.body(Body::empty()).unwrap()).await.unwrap()
}

#[tokio::test]
async fn avatars_manifest_has_builtin_first() {
    let resp = get(env_router(), "/api/avatars", None).await;
    assert_eq!(resp.status(), 200);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let avatars = json["avatars"].as_array().expect("avatars array");
    assert!(!avatars.is_empty());
    // 内置 Hiyori 恒在首位；其余项（若有）来自用户 avatars 目录，字段齐全即可。
    assert_eq!(avatars[0]["id"], "builtin");
    assert_eq!(avatars[0]["url"], "/live2d/hiyori/Hiyori.model3.json");
    for a in avatars {
        assert!(a["id"].is_string());
        assert!(a["name"].is_string());
        assert!(a["url"].is_string());
    }
}

#[tokio::test]
async fn avatars_requires_bearer_when_ui_token_set() {
    let app = server::router_from_state(Arc::new(AppState::env_only(Arc::new(
        Config::load(|k| (k == "MOMO_UI_TOKEN").then(|| "secret".to_string())).expect("config"),
    ))));
    let resp = get(app.clone(), "/api/avatars", None).await;
    assert_eq!(resp.status(), 401);
    let resp = get(app, "/api/avatars", Some("secret")).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn avatars_static_dir_missing_path_is_404() {
    let resp = get(
        env_router(),
        "/avatars/definitely-missing-pack/model.model3.json",
        None,
    )
    .await;
    assert_eq!(resp.status(), 404);
}
