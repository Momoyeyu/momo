//! axum router：`GET /ws/ui` upgrade、`GET /healthz`、静态文件。
//!
//! `/ws/action` 为 P4 预留（protocol §2 action 消息组）。

use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    Router,
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::{
    session::{SessionDeps, manager::SessionManager},
    ws::conn,
};

/// 连接级共享状态。
pub struct AppState {
    pub config: Arc<Config>,
    pub sessions: SessionManager,
    /// 进程启动时刻，供 `/healthz` 报 uptime。
    pub started_at: Instant,
}

impl AppState {
            config,
            started_at: Instant::now(),
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.config.orch_listen
    }
}

/// 构建 momod 的 HTTP/WS 路由。

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .route("/ws/ui", get(ws_ui))
        // TODO(P4): /ws/action upgrade（action 链路注册/调用）
        .with_state(state.clone())
        .layer(cors_layer(&state.config));

    if let Some(dir) = state.config.ui_dir.as_ref().filter(|d| d.is_dir()) {
        // MOMO_UI_DIR 存在 → ServeDir 托管 web/dist（生产单源部署）。
        app = app.fallback_service(ServeDir::new(dir));
    }

}

/// momo-infra `momoctl serve status` 探活端点。
async fn healthz(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        format!("ok uptime_s={}", state.started_at.elapsed().as_secs()),
    )
}

/// Tauri webview 各平台的 desktop Origin 形态（protocol §认证）。
const DESKTOP_ORIGINS: &[&str] = &[
    "tauri://localhost",       // macOS/Linux custom scheme
    "http://tauri.localhost",  // Windows WebView2
    "https://tauri.localhost", // 部分 webkit 打包形态
];

/// UI 链路 upgrade；握手与逐连接 actor 在 [`conn`]。
///
/// upgrade 前 Origin 白名单：missing 或未批准直接 403（protocol §认证）。
async fn ws_ui(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !origin_allowed(&headers, &state.config) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| conn::run(socket, state))
        .into_response()
}

///
/// `pub` 供测试直接对拍白名单逻辑（真 upgrade 路径由 axum extractor 链保证）。
pub fn origin_allowed(headers: &HeaderMap, config: &Config) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return false;
    };
}

fn cors_layer(config: &Config) -> CorsLayer {
    let layer = CorsLayer::new()
    match config.web_origin.parse::<HeaderValue>() {
        Err(_) => {
            tracing::warn!(origin = %config.web_origin, "invalid MOMO_WEB_ORIGIN, CORS permissive");
            CorsLayer::permissive()
        }
    }
}
