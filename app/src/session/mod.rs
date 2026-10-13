//! Session/回合状态机：单活跃 session、命令循环、barge-in。
//!
//! 迁移图见 runtime.md §turn 状态机；wire 状态枚举复用
//! [`crate::proto::message::TurnState`]。

pub mod history;
pub mod manager;
// 文档目录树固定为 session/session.rs，同名模块是有意的。
#[allow(clippy::module_inception)]
pub mod session;
pub mod turn;

pub use history::{ChatMessage, History, Role};
pub use manager::SessionManager;
pub use session::{PipelineEvent, Session, SessionCmd, SessionDeps, SessionHandle};
pub use turn::{IllegalTransition, Turn, TurnEvent, TurnId, transition};
