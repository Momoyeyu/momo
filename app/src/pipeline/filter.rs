//! `ContentFilter` trait：输入/输出双向过滤，默认 `Noop`。
//!
//! 开关：`MOMO_FILTER_ENABLED`（默认 `false`）。

use async_trait::async_trait;

use crate::{error::AppError, pipeline::splitter::Sentence};

/// 过滤结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterVerdict {
    Allow,
    /// 拦截（附原因，写日志/回报）。
    Block {
        reason: String,
    },
}

/// 双向过滤：用户输入（text/转写）与 LLM 输出（句）。
#[async_trait]
pub trait ContentFilter: Send + Sync {
    async fn check_input(&self, text: &str) -> Result<FilterVerdict, AppError>;
    async fn check_output(&self, sentence: &Sentence) -> Result<FilterVerdict, AppError>;
}

/// 默认放行实现（`MOMO_FILTER_ENABLED=false` 时使用）。
#[derive(Debug, Default)]
pub struct NoopFilter;

#[async_trait]
impl ContentFilter for NoopFilter {
    async fn check_input(&self, _text: &str) -> Result<FilterVerdict, AppError> {
        Ok(FilterVerdict::Allow)
    }

    async fn check_output(&self, _sentence: &Sentence) -> Result<FilterVerdict, AppError> {
        Ok(FilterVerdict::Allow)
    }
}
