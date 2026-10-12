//! momo-ws 协议类型与编解码。
//!
//! 字段 schema 的唯一权威是 `docs/01-architecture/protocol.md`（v2）；
//! 本模块是它的机械翻译，新增消息类型必须先改协议文档。

pub mod codec;
pub mod message;
