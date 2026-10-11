//! 子命令实现。`Ctx` 为 dispatch 时注入的全局上下文。

pub mod bench;
pub mod doctor;
pub mod gc;
pub mod pull;
pub mod serve;
pub mod switch;

#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub json: bool,
    pub verbose: u8,
}
