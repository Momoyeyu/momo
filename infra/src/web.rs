//! `momoctl web` 的 axum server 基座：`/api/*` 路由树 + include_dir 内嵌 SPA。
//! 请求级认证链（token + CSRF 中间件）由后续 issue 以 layer 形式挂到
//! [`router`] 的 api 子树上；本模块只定义组装点与共享状态。

pub mod api;
pub mod guard;
pub mod spa;

use std::sync::Arc;

use axum::Router;
use tokio::sync::broadcast;

use crate::output::AppError;

/// `/api/events` broadcast 通道容量：超出后慢消费者收到 Lagged 并被跳过。
/// 后续 jobs/serve 日志流复用同一通道。
pub const EVENT_CHANNEL_CAP: usize = 256;

/// web server 跨 handler 共享状态。
#[derive(Debug)]
pub struct WebState {
    /// SSE 事件源：jobs/serve 日志等事件统一经此 broadcast 下发。
    pub events_tx: broadcast::Sender<String>,
    /// `MOMO_WEB_TOKEN` 是否已配置（只存 bool，不存值：敏感键不回显，config.md §4）。
    pub token_configured: bool,
}

/// 组装完整 Router：`/api` 子树集中注册，其余路径走 SPA fallback。
/// 认证中间件（启动守卫外的请求级链路）后续在这里 `.layer()` 到 api 子树。
pub fn router(state: Arc<WebState>) -> Router {
    Router::new()
        // `/api/`（带尾斜杠）不匹配 nest 的 `/api/{*rest}`，会落 SPA fallback；
        // 显式挂到 api 索引，保证 api 命名空间不回 HTML。
        .route("/api/", axum::routing::get(api::index))
        .nest("/api", api::router(state))
        .fallback(spa::fallback)
}

/// 在已 bind 好的 listener 上服务至 Ctrl-C。
/// listener 由调用方先 bind：CLI 可在启动日志里打印真实 local_addr（含 :0 随机端口）。
pub async fn serve(listener: tokio::net::TcpListener, app: Router) -> Result<(), AppError> {
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(AppError::Io)
}

async fn shutdown_signal() {
    // Ctrl-C 两平台均有；注册失败（如嵌入式无信号支持）时静默——进程仍可被外部终止。
    let _ = tokio::signal::ctrl_c().await;
}
