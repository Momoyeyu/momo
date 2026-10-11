//! `momoctl gc [--dry-run] [--keep N] [--yes] [--json]`：模型 GC。
//! 执行体未实现：tts/ft-<engine>-<date> 保留 N-1 代；扫描未被
//! models.lock 与 .env 引用的目录；默认 dry-run，--yes 才实删并同步 models.lock。

use crate::cli::GcArgs;
use crate::output::{not_implemented, AppError};

pub fn run(_args: &GcArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    Ok(not_implemented("gc", ctx.json))
}
