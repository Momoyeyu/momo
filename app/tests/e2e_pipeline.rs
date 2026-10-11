//!

use std::sync::Arc;

use tower::ServiceExt;

}

#[tokio::test]
async fn healthz_returns_200() {
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

