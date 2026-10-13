//! 对话历史与已播文本记账。
//!
//! 打断时把已播句（`played_seq` 之前）拼成一条 `partial` assistant 消息写入历史；
//! `MOMO_HISTORY_TURNS` 控制注入 LLM 的回合数上限。

use serde::{Deserialize, Serialize};

use crate::pipeline::splitter::Sentence;

/// LLM 对话消息（OpenAI chat 结构对齐）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// 被打断回合的 assistant 消息标记为 partial。
    #[serde(default, skip_serializing_if = "is_false")]
    pub partial: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// 按 `index` 保序拼接句文本（记账文本只认序，不认到达顺序）。
fn join_sentences(sentences: &[Sentence]) -> String {
    let mut sorted: Vec<&Sentence> = sentences.iter().collect();
    sorted.sort_by_key(|s| s.index);
    sorted.iter().map(|s| s.text.as_str()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// 会话历史（有界，超出丢弃最旧的非 system 消息）。
#[derive(Debug)]
pub struct History {
    messages: Vec<ChatMessage>,
    /// 保留的回合数（一回合 ≈ user+assistant 两条）；默认 `MOMO_HISTORY_TURNS`。
    max_turns: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(20)
    }
}

impl History {
    pub fn new(max_turns: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_turns,
        }
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.trim();
    }

    /// 正常完成记账：user + 全量生成句拼一条 assistant。
    pub fn commit(&mut self, user: &str, generated: &[Sentence]) {
        self.push_user(user);
        let content = join_sentences(generated);
        if content.is_empty() {
            return;
        }
        self.push(ChatMessage {
            role: Role::Assistant,
            content,
            partial: false,
        });
    }

    /// barge-in 记账：user + 已播句前缀拼一条 `partial` assistant。
    /// 未播 sentence 不进入 history。
    pub fn commit_partial(&mut self, user: &str, played: &[Sentence]) {
        self.push_user(user);
        let content = join_sentences(played);
        if content.is_empty() {
            return;
        }
        self.push(ChatMessage {
            role: Role::Assistant,
            content,
            partial: true,
        });
    }

    fn push_user(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push(ChatMessage {
            role: Role::User,
            content: text.into(),
            partial: false,
        });
    }

    /// 注入 LLM 的消息序列（含 system 头）。
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// 保留最近 `max_turns` 个回合；system 消息永远保留。
    fn trim(&mut self) {
        let keep = self.max_turns * 2;
        let non_system = self
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .count();
        if non_system <= keep {
            return;
        }
        let mut drop_n = non_system - keep;
        self.messages.retain(|m| {
            if drop_n > 0 && m.role != Role::System {
                drop_n -= 1;
                false
            } else {
                true
            }
        });
    }
}
