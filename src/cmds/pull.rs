//! `momoctl pull [--manifest PATH] [--only NAME] [--check]`：模型下载 + sha256 校验。
//! 执行体未实现：下载走 HF_ENDPOINT 的 resolve URL，.part 断点续传，
//! 完成后写 `$MOMO_MODELS_DIR/models.lock`（05-ops/model-fetch.md）。

use crate::cli::PullArgs;
use crate::output::{not_implemented, AppError};

pub fn run(_args: &PullArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    Ok(not_implemented("pull", ctx.json))
}
