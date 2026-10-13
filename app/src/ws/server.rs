//! axum router：`GET /ws/ui` upgrade、`GET /healthz`、静态文件。
//!
//! `/ws/action` 为 P4 预留（protocol §2 action 消息组）。

use std::{net::SocketAddr, sync::Arc, time::Instant};

use axum::{
    Router,
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use tower_http::{cors::CorsLayer, services::ServeDir};

use crate::{
    session::{SessionDeps, manager::SessionManager},
    settings::{self, ProviderSettings},
    ws::conn,
};

/// 连接级共享状态。
pub struct AppState {
    pub config: Arc<Config>,
    pub sessions: SessionManager,
    /// 生效 provider 配置（settings.json 覆盖 env；设置面 PUT 更新）。
    pub settings: tokio::sync::RwLock<ProviderSettings>,
    pub llm: Arc<OpenAiCompatLlm>,
    /// settings.json 落盘路径。
    pub settings_path: std::path::PathBuf,
    /// 进程启动时刻，供 `/healthz` 报 uptime。
    pub started_at: Instant,
}

impl AppState {
    pub fn new(config: Arc<Config>) -> Result<Self, AppError> {
        let settings = ProviderSettings::load(&config)?;
        let llm = Arc::new(OpenAiCompatLlm::new(
            settings.llm_endpoint.clone(),
            settings.llm_model.clone(),
            settings.llm_api_key.clone(),
            config.llm_timeout,
        ));
            sessions: SessionManager::new(deps),
            settings: tokio::sync::RwLock::new(settings),
            llm,
            asr,
            tts,
            config,
            started_at: Instant::now(),
    }

    /// 测试用：注入自定义 deps（mock client）；settings 句柄独立构造，
    /// 不读 settings.json，不参与 deps 内的 client 生命周期。
    #[allow(clippy::missing_errors_doc)]
    pub fn from_deps(config: Arc<Config>, deps: SessionDeps) -> Result<Self, AppError> {
        let settings = ProviderSettings::from_config(&config);
        let llm = Arc::new(OpenAiCompatLlm::new(
            settings.llm_endpoint.clone(),
            settings.llm_model.clone(),
            settings.llm_api_key.clone(),
            config.llm_timeout,
        ));
        Ok(Self {
            sessions: SessionManager::new(deps),
            settings: tokio::sync::RwLock::new(settings),
            llm,
            asr,
            tts,
            settings_path: std::path::PathBuf::from("momo-test-settings.json"),
            config,
            started_at: Instant::now(),
        })
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.config.orch_listen
    }
}

/// 构建 momod 的 HTTP/WS 路由。
pub fn router(config: Arc<Config>) -> Result<Router, AppError> {

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        // 组件探活口径统一走 /readyz（profiles/*.toml）；momod 暂无
        // readiness 门控（下游 svc 懒连接），先与 /healthz 同义。
        .route("/readyz", get(healthz))
        .route("/ws/ui", get(ws_ui))
        .route("/api/settings", get(api::get_settings))
        .route("/api/settings", put(api::put_settings))
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

/// Origin ∈ {desktop origins} ∪ {`MOMO_WEB_ORIGIN`} ∪ {`MOMO_UI_DIR` 自托管源}；
/// missing 一律拒绝。
///
/// `pub` 供测试直接对拍白名单逻辑（真 upgrade 路径由 axum extractor 链保证）。
pub fn origin_allowed(headers: &HeaderMap, config: &Config) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    if origin == config.web_origin || DESKTOP_ORIGINS.contains(&origin) {
        return true;
    }
    // UI 由 momod 自身托管（MOMO_UI_DIR）时，浏览器源即 http://{orch_listen}。
    // loopback 监听下 localhost / 127.0.0.1 / [::1] 字面量视为同源——
    // 用户打开 localhost:7800 时 Origin 不会和 127.0.0.1 字符串相等。
    let port = config.orch_listen.port();
    let mut self_served = vec![format!("http://{}", config.orch_listen)];
    if config.orch_listen.ip().is_loopback() {
        self_served.extend([
            format!("http://localhost:{port}"),
            format!("http://127.0.0.1:{port}"),
            format!("http://[::1]:{port}"),
        ]);
    }
    self_served.iter().any(|o| o == origin)
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
