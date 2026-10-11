//! momod 入口：load config → init logging → bind ws server。

use std::sync::Arc;

use momo_app::{config::Config, error::AppError, logging, ws};

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let config = Arc::new(Config::from_env()?);
    logging::init(&config);

    // 生效配置打进日志；敏感项由 SecretString 的 Debug 打码（config.md §4）。
    tracing::info!(config = ?config, "momod effective config");

    let listener = tokio::net::TcpListener::bind(config.orch_listen).await?;
    tracing::info!(addr = %config.orch_listen, "momod listening");

    axum::serve(listener, app).await?;
    Ok(())
}
