
use crate::proto::message::EmotionTag;

/// CJK 句末强标点与换行：立即切（`\r\n` 由 `\n` 触发，`\r` 不单独处理）。
const SENT_END: &[char] = &['。', '！', '？', '；', ';', '\n'];
/// ASCII 句末标点：仅在后接空白时切（需 lookahead）。
const ASCII_END: &[char] = &['.', '!', '?'];
/// 逗号：buffer ≥ `soft_chars` 时软切；超上限兜底也优先在最近逗号处切。
const SECONDARY: &[char] = &['，', ','];
/// code block 围栏。
const CODE_FENCE: &str = "```";
/// 常见英文缩写（小写、不含尾点）：`Mr. `/`e.g. ` 这类 `.`+空白不是句末。
/// 非穷举，只覆盖高频情形；表外缩写后接空白仍按句末处理。
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "st", "jr", "sr", "prof", "vs", "etc", "no", "vol", "fig", "approx",
    "dept", "inc", "ltd", "co", "corp", "mt", "ft", "e.g", "i.e", "a.m", "p.m",
];

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
    /// `MOMO_SENT_SOFT_CHARS` 软切阈值：buffer 达到后遇逗号即切。
    soft_chars: usize,
    /// `MOMO_SENT_MAX_CHARS` 硬上限。
    max_chars: usize,
}

impl SentenceSplitter {
    /// `soft_chars` 对应 `Config::sent_soft_chars`，`max_chars` 对应
    /// `Config::sent_max_chars`；预期 `soft_chars <= max_chars`。
    pub fn new(soft_chars: usize, max_chars: usize) -> Self {
        Self {
            buf: String::new(),
            next_index: 1,
            soft_chars,
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
                chars += 1;
                if SENT_END.contains(&ch) {
                    return Some(end);
                }
                if ASCII_END.contains(&ch) && self.is_ascii_sentence_end(i, end, ch) {
                    return Some(end);
                }
                if SECONDARY.contains(&ch) {
                    last_secondary = Some(end);
                    // 软切：buffer 达阈值后在第一个逗号处收句。
                    if chars >= self.soft_chars {
                        return Some(end);
                    }
                }
                if chars >= self.max_chars {
                    // 超上限：有次级标点切那里，否则硬切当前位置。
                    return Some(last_secondary.unwrap_or(end));
                }
            }
            i = end;
        }
        None
    }

    /// ASCII `.!?` 的 lookahead：仅在后接空白时成句；buffer 末尾不算切点，
    /// 等下一 token 或 `flush`——否则 `e.`（`e.g.` 的增量前缀）会被误切。
    /// 后接非空白时直接豁免：`3.14`、`example.com`、`a.b/c`、`?!` 连写。
    fn is_ascii_sentence_end(&self, start: usize, end: usize, ch: char) -> bool {
        let Some(next) = self.buf[end..].chars().next() else {
            return false;
        };
        if !next.is_whitespace() {
            return false;
        }
        // 后接空白时，`.` 再排除缩写（`Mr. `、`e.g. `）与词首字母（`J. R.`）。
        ch != '.' || !self.is_abbreviation(start)
    }

    /// `dot` 处 `.` 的前置词（`e.g` / `Mr` / `J`）是否为缩写或词首字母。
    /// 词取 `.` 前连续的 ASCII 字母数字与内嵌 `.`（`e.g` 整体算一个词）。
    fn is_abbreviation(&self, dot: usize) -> bool {
        let mut begin = dot;
        while begin > 0 {
            let prev = self.buf[..begin].chars().next_back().expect("begin > 0");
            if prev.is_ascii_alphanumeric() || prev == '.' {
                begin -= prev.len_utf8();
            } else {
                break;
            }
        }
        let word = &self.buf[begin..dot];
        if word.len() == 1 && word.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return true; // 单字母词首：`J.` `A.`。
        }
        ABBREVIATIONS.contains(&word.to_ascii_lowercase().as_str())
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
