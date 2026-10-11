//! 流式管线：LLM → 句切 → TTS → 音频下发；模型服务 client 与双向过滤。

pub mod asr;
pub mod filter;
pub mod llm;
pub mod splitter;
pub mod tts;
