//! 写路径：turn.finished 后异步抽取（docs/03-features/memory.md §写路径）。
//!
//! mem0 式 ADD-only + invalidate：LLM 只产 JSON ops 数组，不直接改库；
//! 本模块负责解析容错与语义应用——`add` 去重入库、`invalidate` 置
//! `superseded_by`（带 content 时先 add 新条再指向新 id，纠正闭环）。
//! 由调用方 `tokio::spawn`，失败只记 warn，不阻塞回合。

use std::sync::Arc;

use futures_util::StreamExt;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{
    error::AppError,
    pipeline::llm::{ChatRequest, LlmClient},
    session::history::{ChatMessage, Role},
};

use super::store::{AddOutcome, MEMORY_CONTENT_MAX_CHARS, MemoryKind, MemoryStore, VOID_EXTRACT};

/// 单轮抽取结果统计（日志用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExtractStats {
    pub added: usize,
    pub invalidated: usize,
    pub skipped: usize,
}

/// LLM 产出的一条记忆操作；未知字段容忍，`reason` 等仅作上下文不消费。
#[derive(Debug, Deserialize)]
struct ExtractOp {
    op: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    target_id: Option<String>,
}

/// 抽取入口：transcript → LLM ops → 应用到 store。
///
/// `assistant_text` 取已记账文本（completed=全量 emitted，interrupted=已播
/// committed）；user_text 为空直接跳过。LLM/解析失败向上返回，由 spawn
/// 调用点记 warn——记忆缺失不升级为回合错误。
pub async fn run_extraction(
    store: &MemoryStore,
    llm: &Arc<dyn LlmClient>,
    turn_id: &str,
    user_text: &str,
    assistant_text: &str,
) -> Result<ExtractStats, AppError> {
    let user_text = user_text.trim();
    if user_text.is_empty() {
        return Ok(ExtractStats::default());
    }
    let context = store.valid_context_lines().unwrap_or_default();
    let correction = has_correction_signal(user_text);
    let req = ChatRequest {
        model: String::new(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: extract_prompt(&context, correction),
                partial: false,
            },
            ChatMessage {
                role: Role::User,
                content: transcript(user_text, assistant_text),
                partial: false,
            },
        ],
        stream: true,
    };
    // 新 cancel token：抽取独立于 turn 生命周期（turn 已 finished）。
    let mut stream = llm.chat_stream(req, CancellationToken::new()).await?;
    let mut raw = String::new();
    while let Some(item) = stream.next().await {
        raw.push_str(&item?);
    }
    Ok(apply_ops(store, turn_id, &parse_ops(&raw)))
}

/// 抽取 prompt：JSON ops 契约 + 现有 valid 记忆摘要（供 invalidate 引 id）。
/// 纠正语义回合强调产出 invalidate（「纠正即失效」而非覆盖旧条）。
fn extract_prompt(context: &[String], correction: bool) -> String {
    let mut p = String::from(
        "你是长期记忆整理器。从对话转写中抽取值得长期记住的条目，只输出 JSON 数组，\
         不输出任何其他文字。\n\
         每条元素形如：\n\
         - {\"op\":\"add\",\"kind\":\"semantic|episodic|procedural\",\"content\":\"...\"}\
         ——semantic=用户事实/偏好，episodic=共同经历/事件，procedural=互动习惯/规则；\n\
         - {\"op\":\"invalidate\",\"target_id\":\"mem_...\",\"reason\":\"...\"}\
         ——某条现有记忆已过时/被否认；\n\
         - {\"op\":\"invalidate\",\"target_id\":\"mem_...\",\"kind\":\"...\",\"content\":\"...\"}\
         ——纠正：旧条失效，同时登记新结论。\n\
         无可记内容输出 []。content 保持精简、第三人称陈述。",
    );
    if correction {
        p.push_str("\n本轮用户含纠正语义：对相关现有记忆优先产出 invalidate（可带新 content）。");
    }
    if context.is_empty() {
        p.push_str("\n\n[现有记忆]\n（空）");
    } else {
        p.push_str("\n\n[现有记忆]\n");
        p.push_str(&context.join("\n"));
    }
    p
}

/// 本轮 transcript 片段（user 原文 + 已记账 assistant 文本）。
fn transcript(user_text: &str, assistant_text: &str) -> String {
    let assistant = assistant_text.trim();
    if assistant.is_empty() {
        format!("用户：{user_text}")
    } else {
        format!("用户：{user_text}\nmomo：{assistant}")
    }
}

/// 用户消息里的纠正信号（中英文标记）；命中时 prompt 强调 invalidate。
fn has_correction_signal(user_text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "不对",
        "记错",
        "忘掉",
        "忘了",
        "纠正",
        "说错",
        "不是",
        "别再记",
        "wrong",
        "forget",
        "incorrect",
        "actually",
    ];
    let lower = user_text.to_lowercase();
    MARKERS.iter().any(|m| lower.contains(m))
}

/// ops 解析容错：截取首个 `[` 到末个 `]`，逐条 serde，坏项跳过。
fn parse_ops(raw: &str) -> Vec<ExtractOp> {
    let Some(start) = raw.find('[') else {
        return Vec::new();
    };
    let Some(end) = raw.rfind(']') else {
        return Vec::new();
    };
    if end <= start {
        return Vec::new();
    }
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&raw[start..=end]) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<ExtractOp>(v) {
            Ok(op) => Some(op),
            Err(e) => {
                warn!(%e, "memory extract: skipping malformed op");
                None
            }
        })
        .collect()
}

/// ops → store：`add` 去重入库；`invalidate` 置 `superseded_by`。
/// invalidate 带 content 时先 add 新条（命中去重直接用既有 id），再让
/// target 指向新条 id；不带 content → `void:extract`。
fn apply_ops(store: &MemoryStore, turn_id: &str, ops: &[ExtractOp]) -> ExtractStats {
    let mut stats = ExtractStats::default();
    for op in ops {
        match op.op.as_str() {
            "add" => {
                let Some(kind) = op.kind.as_deref().and_then(MemoryKind::parse) else {
                    stats.skipped += 1;
                    continue;
                };
                match add_content(store, kind, op.content.as_deref(), turn_id) {
                    Some(AddOutcome::Added(_)) => stats.added += 1,
                    // 去重命中：语义上已记住，不算 skipped。
                    Some(AddOutcome::Duplicate(_)) => stats.added += 1,
                    None => stats.skipped += 1,
                }
            }
            "invalidate" => {
                let Some(target) = op.target_id.as_deref().map(str::trim) else {
                    stats.skipped += 1;
                    continue;
                };
                if target.is_empty() {
                    stats.skipped += 1;
                    continue;
                }
                let replacement = op
                    .content
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .and_then(|content| {
                        // 新条 kind：op 显式给了用 op 的，否则沿用 target 的。
                        let kind = op
                            .kind
                            .as_deref()
                            .and_then(MemoryKind::parse)
                            .or_else(|| store.get(target).ok().flatten().map(|m| m.kind))
                            .unwrap_or(MemoryKind::Semantic);
                        add_content(store, kind, Some(content), turn_id).map(
                            |outcome| match outcome {
                                AddOutcome::Added(id) | AddOutcome::Duplicate(id) => id,
                            },
                        )
                    });
                match replacement {
                    Some(id) if id == target => stats.skipped += 1,
                    Some(id) => match store.supersede(target, &id) {
                        Ok(true) => {
                            stats.added += 1;
                            stats.invalidated += 1;
                        }
                        _ => stats.skipped += 1,
                    },
                    None => match store.supersede(target, VOID_EXTRACT) {
                        Ok(true) => stats.invalidated += 1,
                        _ => stats.skipped += 1,
                    },
                }
            }
            _ => stats.skipped += 1,
        }
    }
    stats
}

/// content 校验 + add（空/超长不入库）。
fn add_content(
    store: &MemoryStore,
    kind: MemoryKind,
    content: Option<&str>,
    turn_id: &str,
) -> Option<AddOutcome> {
    let content = content?.trim();
    if content.is_empty() || content.chars().count() > MEMORY_CONTENT_MAX_CHARS {
        return None;
    }
    store.add(kind, content, Some(turn_id), 1.0).ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use futures_util::stream;

    use super::*;
    use crate::pipeline::llm::ChatStream;

    ///  canned-response mock：chat_stream 返回固定 JSON 文本流。
    struct MockLlm {
        response: String,
        /// 记录最近一次请求的 system prompt（验证上下文/纠错提示注入）。
        last_system: Mutex<Option<String>>,
    }

    impl MockLlm {
        fn new(response: &str) -> Arc<Self> {
            Arc::new(Self {
                response: response.to_string(),
                last_system: Mutex::new(None),
            })
        }
    }

    #[async_trait]
    impl LlmClient for MockLlm {
        async fn chat_stream(
            &self,
            req: ChatRequest,
            _cancel: CancellationToken,
        ) -> Result<ChatStream, AppError> {
            let system = req
                .messages
                .iter()
                .find(|m| m.role == Role::System)
                .map(|m| m.content.clone());
            *self.last_system.lock().expect("mock lock") = system;
            let text = self.response.clone();
            Ok(Box::pin(stream::once(async move { Ok(text) })))
        }
    }

    #[test]
    fn parse_ops_tolerates_prose_and_bad_items() {
        let raw = "思考过程……\n[{\"op\":\"add\",\"kind\":\"semantic\",\"content\":\"喜欢抹茶\"},\
                   {\"op\":\"bogus\"}, 42, {\"op\":\"invalidate\",\"target_id\":\"mem_1\"}]\n完。";
        let ops = parse_ops(raw);
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[0].op, "add");
        assert_eq!(ops[2].target_id.as_deref(), Some("mem_1"));
        // 无 JSON 数组 → 空。
        assert!(parse_ops("没有输出").is_empty());
        assert!(parse_ops("[]").is_empty());
    }

    #[tokio::test]
    async fn extraction_adds_and_invalidates() {
        let store = MemoryStore::in_memory().unwrap();
        let AddOutcome::Added(old_id) = store
            .add(MemoryKind::Semantic, "住在朝阳区", None, 1.0)
            .unwrap()
        else {
            panic!("expected Added");
        };
        let llm = MockLlm::new(&format!(
            "[{{\"op\":\"add\",\"kind\":\"episodic\",\"content\":\"一起看了海\"}},\
              {{\"op\":\"invalidate\",\"target_id\":\"{old_id}\",\"kind\":\"semantic\",\"content\":\"住在海淀区\"}}]"
        ));
        let stats = run_extraction(
            &store,
            &(llm as Arc<dyn LlmClient>),
            "t_1",
            "我搬去海淀了",
            "记住啦",
        )
        .await
        .unwrap();
        assert_eq!(stats.added, 2);
        assert_eq!(stats.invalidated, 1);
        // 旧条 superseded_by 指向新条（纠正闭环），召回不再含旧正文。
        let old = store.get(&old_id).unwrap().unwrap();
        let new_id = old.superseded_by.clone().expect("superseded");
        assert_ne!(new_id, old_id);
        let valid: Vec<_> = store
            .list(None, false, 50, 0)
            .unwrap()
            .into_iter()
            .map(|m| m.content)
            .collect();
        assert!(valid.contains(&"一起看了海".to_string()));
        assert!(valid.contains(&"住在海淀区".to_string()));
        assert!(!valid.contains(&"住在朝阳区".to_string()));
    }

    #[tokio::test]
    async fn invalidate_without_content_marks_void() {
        let store = MemoryStore::in_memory().unwrap();
        let AddOutcome::Added(id) = store
            .add(MemoryKind::Semantic, "讨厌香菜", None, 1.0)
            .unwrap()
        else {
            panic!("expected Added");
        };
        let llm = MockLlm::new(&format!(
            "[{{\"op\":\"invalidate\",\"target_id\":\"{id}\"}}]"
        ));
        let stats = run_extraction(
            &store,
            &(llm as Arc<dyn LlmClient>),
            "t_2",
            "忘掉这个",
            "好",
        )
        .await
        .unwrap();
        assert_eq!(stats.invalidated, 1);
        assert_eq!(
            store.get(&id).unwrap().unwrap().superseded_by.as_deref(),
            Some(VOID_EXTRACT)
        );
    }

    #[tokio::test]
    async fn extraction_prompt_carries_context_and_correction_hint() {
        let store = MemoryStore::in_memory().unwrap();
        store
            .add(MemoryKind::Semantic, "喜欢抹茶", None, 1.0)
            .unwrap();
        let llm = MockLlm::new("[]");
        run_extraction(
            &store,
            &(llm.clone() as Arc<dyn LlmClient>),
            "t_3",
            "不对，我不喝抹茶",
            "哦",
        )
        .await
        .unwrap();
        let system = llm.last_system.lock().unwrap().clone().unwrap();
        assert!(
            system.contains("喜欢抹茶"),
            "prompt 应含现有记忆供 invalidate 引 id"
        );
        assert!(system.contains("纠正"), "纠正信号应触发 invalidate 强调");
    }

    #[tokio::test]
    async fn empty_user_text_skips_llm() {
        let store = MemoryStore::in_memory().unwrap();
        let llm = MockLlm::new("[{\"op\":\"add\",\"kind\":\"semantic\",\"content\":\"x\"}]");
        let stats = run_extraction(&store, &(llm as Arc<dyn LlmClient>), "t_4", "  ", "")
            .await
            .unwrap();
        assert_eq!(stats, ExtractStats::default());
        assert!(store.list(None, true, 50, 0).unwrap().is_empty());
    }
}
