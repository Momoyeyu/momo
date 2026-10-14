//! 长期记忆（docs/03-features/memory.md）：L0 human block + L2 记忆条目。
//!
//! - [`store`]：sqlite 存储（memories 表 + human_block 单行），Zep 式失效语义；
//! - [`extract`]：turn.finished 后的异步抽取管线（LLM JSON ops → add/invalidate）；
//! - [`api`]：`/api/memories` + `/api/memory/human-block` REST 面。

pub mod api;
pub mod extract;
pub mod store;

pub use store::{
    AddOutcome, HUMAN_BLOCK_MAX_CHARS, MEMORY_CONTENT_MAX_CHARS, Memory, MemoryKind, MemoryStore,
    memory_db_path,
};
