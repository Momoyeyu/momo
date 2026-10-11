//! tracing 初始化（fmt/json），级别与格式来自 [`Config`]。

use tracing_subscriber::EnvFilter;

use crate::config::{Config, LogFormat};

/// 初始化全局 subscriber；重复调用（测试中）静默忽略。
pub fn init(config: &Config) {
    let filter =
        || EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    match config.log_format {
        LogFormat::Pretty => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter())
                .try_init();
        }
        LogFormat::Json => {
            let _ = tracing_subscriber::fmt()
                .json()
                .with_env_filter(filter())
                .try_init();
        }
    }
}
