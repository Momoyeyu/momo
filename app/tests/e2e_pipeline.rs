//! 协议 e2e：mock 模型服务 + 真 ws client 全链路（momo-app.md §P0 验收指针）。
//!
//! 骨架阶段：只验证 `/healthz` 与路由装配；
//! mock LLM SSE / TTS-WS / ASR 与 barge-in 三动作断言随管线实装补齐。

use std::sync::Arc;

use axum::{body::Body, http::Request};
use momo_app::config::Config;
use tower::ServiceExt;

fn test_config() -> Config {
    Config::load(|_| None).expect("default config")
}

#[tokio::test]
async fn healthz_returns_200() {
    let app = momo_app::ws::server::router(Arc::new(test_config()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn ws_ui_requires_upgrade() {
    // 非 upgrade 请求打到 /ws/ui → axum 回 4xx（不是 404）。
    let app = momo_app::ws::server::router(Arc::new(test_config()));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ws/ui")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_client_error());
}

// TODO(P0): e2e 回合——mock LLM SSE + mock TTS/ASR WS + 真 ws client，
// 断言 input.text → output.sentence → audio.chunk 顺序与历史记账。
//
// TODO(P0): barge-in e2e——Speaking 中发 control.barge_in，断言停播通知、
// 上游 cancel、partial 记账三件事，以及取消后无残留帧。
