//! `momoctl switch --to <normal|degraded> [--restart]`。
//! 执行体未实现：.env 符号链接重指 .env.<to>，写 active_profile，输出关键 env diff；
//! --restart 时自动 serve down && serve up。

use crate::cli::SwitchArgs;
use crate::output::{not_implemented, AppError};

pub fn run(_args: &SwitchArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    Ok(not_implemented("switch", ctx.json))
}
