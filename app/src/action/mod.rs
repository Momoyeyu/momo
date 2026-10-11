//! P4 预留：action 注册表与调用校验。
//!
//! 协议面（protocol §6）：`action.register` / `action.unregister`（提供方→orch）、
//! `action.invoke` / `action.result`（orch→提供方）；`/ws/action` 链路届时实装。
//! 消息类型已在 [`crate::proto::message`] 中定义，本模块到时承载：
//! - 提供方连接注册表（断连视为全部注销）；
//! - `json_schema` 透传给 LLM tool 列表；
//! - invoke 超时合成 `error{ACTION_TIMEOUT}`。
