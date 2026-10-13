//! momo-app：编排器 `momod` 的库入口。
//!
//! 以库形式导出全部模块，供 `tests/` 下的协议 e2e 测试复用
//! （仓库结构见 momo-docs `02-repos/momo-app.md`）。

pub mod action;
pub mod audio;
pub mod config;
pub mod error;
pub mod logging;
pub mod persona;
pub mod pipeline;
pub mod proto;
pub mod session;
pub mod ws;
