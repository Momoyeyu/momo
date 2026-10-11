//! `momoctl bench [--turns N] [--json] [--strict]`：端到端延迟测量。
//! 执行体未实现：ws 客户端向 momod 发固定 input.text，计
//! 发→首个 output.sentence / 发→首个 audio.chunk / 回合全长，N 回合取 p50/p95，
//! 目标 <1.5s 出声；--strict 超标时退出码 3。

use crate::cli::BenchArgs;
use crate::output::{not_implemented, AppError};

pub fn run(_args: &BenchArgs, ctx: &super::Ctx) -> Result<i32, AppError> {
    Ok(not_implemented("bench", ctx.json))
}
