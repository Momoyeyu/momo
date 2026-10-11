//! 增量句切：标点 + 上限 + 情感 tag 提取（momo-app.md §splitter）。
//!
//! 规则：句末标点 `。！？!?；;` 立即切；超 `max_chars` 在次级标点 `，,` 处切
//! （无次级标点则硬切）；行首 `[happy]` 类 tag 提取进 `emotion` 并从 `text`
//! 剔除；code block（``` 围栏）内不切。纯字符串操作、无依赖，是单元测试重点。

use crate::proto::message::EmotionTag;

/// 句末标点：立即切。
const SENT_END: &[char] = &['。', '！', '？', '!', '?', '；', ';'];
/// 次级标点：超上限时优先在此切。
const SECONDARY: &[char] = &['，', ','];
/// code block 围栏。
const CODE_FENCE: &str = "```";

/// 一条成形句：`output.sentence` 的数据来源。
#[derive(Debug, Clone, PartialEq)]
pub struct Sentence {
    /// 回合内句序号（映射 `output.sentence.seq`，从 1 递增由调用方保证）。
    pub index: u32,
    pub text: String,
    /// 行首 `[tag]` 提取的情感；无 tag 为 `None`（由下游归一为 neutral）。
    pub emotion: Option<EmotionTag>,
}

/// 增量句切器：逐 token `push`，吐完整句；流结束 `flush` 吐残句。
#[derive(Debug)]
pub struct SentenceSplitter {
    buf: String,
    next_index: u32,
    /// `MOMO_SENT_MAX_CHARS` 硬上限。
    max_chars: usize,
}

impl SentenceSplitter {
    pub fn new(max_chars: usize) -> Self {
        Self {
            buf: String::new(),
            next_index: 1,
            max_chars,
        }
    }

    /// 增量喂 token，吐完整句。
    pub fn push(&mut self, delta: &str) -> Vec<Sentence> {
        self.buf.push_str(delta);
        let mut out = Vec::new();
        while let Some(end) = self.find_cut() {
            let piece: String = self.buf.drain(..end).collect();
            if let Some(s) = self.emit(&piece) {
                out.push(s);
            }
        }
        out
    }

    /// 流结束时吐残句；空缓冲返回 `None`。
    pub fn flush(&mut self) -> Option<Sentence> {
        let rest = std::mem::take(&mut self.buf);
        self.emit(&rest)
    }

    /// 找一个切割点，返回切割长度（字节数）。
    ///
    /// 只在 code block 外切；因此 `buf` 开头永远处于 code 外，
    /// code 状态无需跨 push 持久化。
    fn find_cut(&self) -> Option<usize> {
        let mut in_code = false;
        let mut last_secondary: Option<usize> = None;
        let mut chars = 0usize;
        let mut i = 0usize;

        while i < self.buf.len() {
            if self.buf[i..].starts_with(CODE_FENCE) {
                in_code = !in_code;
                i += CODE_FENCE.len();
                continue;
            }
            let ch = self.buf[i..].chars().next()?;
            let end = i + ch.len_utf8();
            if !in_code {
                if SENT_END.contains(&ch) {
                    return Some(end);
                }
                if SECONDARY.contains(&ch) {
                    last_secondary = Some(end);
                }
                chars += 1;
                if chars >= self.max_chars {
                    // 超上限：有次级标点切那里，否则硬切当前位置。
                    return Some(last_secondary.unwrap_or(end));
                }
            }
            i = end;
        }
        None
    }

    /// 提取行首 `[tag]`，产出 `Sentence`；纯空白丢弃。
    fn emit(&mut self, piece: &str) -> Option<Sentence> {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            return None;
        }
        let (emotion, text) = extract_emotion(trimmed);
        if text.is_empty() {
            return None;
        }
        let sentence = Sentence {
            index: self.next_index,
            text,
            emotion,
        };
        self.next_index += 1;
        Some(sentence)
    }
}

/// 剥离行首 `[happy]` 类 tag；表外 tag 不视为情感（留在文本里）。
fn extract_emotion(text: &str) -> (Option<EmotionTag>, String) {
    let Some(rest) = text.strip_prefix('[') else {
        return (None, text.to_string());
    };
    let Some(close) = rest.find(']') else {
        return (None, text.to_string());
    };
    match EmotionTag::from_tag(&rest[..close]) {
        EmotionTag::Unknown => (None, text.to_string()),
        tag => (Some(tag), rest[close + 1..].trim_start().to_string()),
    }
}
