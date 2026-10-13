//! 句切边界用例（runtime.md §增量句切）：
//! 标点/tag/超长/code block/ASCII lookahead/逗号软切。

use momo_app::{pipeline::splitter::SentenceSplitter, proto::message::EmotionTag};

const SOFT: usize = 25;
const MAX: usize = 60;

#[test]
fn splits_on_terminal_punctuation() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    let out = s.push("今晚吃火锅。明天再说！");
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].text, "今晚吃火锅。");
    assert_eq!(out[0].index, 1);
    assert_eq!(out[1].text, "明天再说！");
    assert_eq!(out[1].index, 2);
    assert!(s.flush().is_none());
}

#[test]
fn incremental_push_completes_sentence() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    assert!(s.push("今").is_empty());
    assert!(s.push("晚吃").is_empty());
    let out = s.push("火锅？");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "今晚吃火锅？");
}

#[test]
fn extracts_leading_emotion_tag() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    let out = s.push("[happy] 去吃火锅吧！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, Some(EmotionTag::Happy));
    assert_eq!(out[0].text, "去吃火锅吧！");
}

#[test]
fn unknown_tag_stays_in_text() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    let out = s.push("[dancing] 跳起来！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, None);
    assert_eq!(out[0].text, "[dancing] 跳起来！");
}

#[test]
fn hard_cut_at_max_chars() {
    let mut s = SentenceSplitter::new(5, 10);
    // 10 个非句末字符 → 硬切。
    let out = s.push("一二三四五六七八九十还没完");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三四五六七八九十");
}

#[test]
fn prefers_secondary_punctuation_within_max() {
    let mut s = SentenceSplitter::new(5, 10);
    // 逗号在第 4 字符，未到 soft=5 → 只记为安全边界；
    // 到 10 硬上限时回退到该逗号处切。
    let out = s.push("一二三，四五六七八九");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三，");
}

#[test]
fn does_not_split_inside_code_block() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // fence 内的 `。`、`;` 与换行都不切。
    assert!(s.push("看代码：```let a = 1。\nlet b = 2;```").is_empty());
    let out = s.push("结束。");
    assert_eq!(out.len(), 1);
    assert!(out[0].text.contains("```let a = 1。\nlet b = 2;```"));
    assert!(out[0].text.ends_with("结束。"));
}

#[test]
fn flush_emits_remainder() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    assert!(s.push("没说完的话").is_empty());
    let rest = s.flush().expect("residual sentence");
    assert_eq!(rest.text, "没说完的话");
    assert_eq!(rest.index, 1);
}

#[test]
fn ascii_terminal_cuts_before_whitespace() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // `.!?` 后接空白才切；`Done` 无终止符留在 buffer。
    let out = s.push("Hello world! How are you? Fine. Done");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].text, "Hello world!");
    assert_eq!(out[1].text, "How are you?");
    assert_eq!(out[2].text, "Fine.");
    assert_eq!(s.flush().expect("residual").text, "Done");
}

#[test]
fn ascii_terminal_at_buffer_end_waits_then_flushes() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // `!` 在 buffer 末尾：无法确认 stream end，等 lookahead。
    assert!(s.push("See you!").is_empty());
    // 下一 token 到来后 `!` 后接空白 → 补切。
    let out = s.push(" Bye.");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "See you!");
    // 流末尾的 `.` 由 flush 收句（stream end 语义）。
    assert_eq!(s.flush().expect("residual").text, "Bye.");
}

#[test]
fn ascii_terminal_waits_across_incremental_push() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    assert!(s.push("Hi").is_empty());
    assert!(s.push("!").is_empty());
    let out = s.push(" there");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "Hi!");
}

#[test]
fn decimal_and_url_dots_do_not_cut() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // `3.14`、`example.com/a.b` 的 `.` 后接非空白 → 不切。
    assert!(
        s.push("See 3.14 and example.com/a.b for details")
            .is_empty()
    );
    let out = s.push(". Next");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "See 3.14 and example.com/a.b for details.");
    assert_eq!(s.flush().expect("residual").text, "Next");
}

#[test]
fn abbreviation_dot_does_not_cut() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // `e.g.` / `Mr.` 后接空白也不是句末。
    assert!(s.push("Use e.g. apples and Mr. Smith left").is_empty());
    let out = s.push(". Bye");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "Use e.g. apples and Mr. Smith left.");
    assert_eq!(s.flush().expect("residual").text, "Bye");
}

#[test]
fn initial_dot_does_not_cut() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // 词首字母 `J. R.` 不切。
    assert!(s.push("by J. R. Tolkien").is_empty());
    let out = s.push(". Done");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "by J. R. Tolkien.");
}

#[test]
fn interrobang_stays_together() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // `?!` 连写：`?` 后接 `!` 非空白不切，`!` 后接空白才切。
    let out = s.push("What?! Really.");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "What?!");
    assert_eq!(s.flush().expect("residual").text, "Really.");
}

#[test]
fn comma_soft_cut_after_soft_chars() {
    let mut s = SentenceSplitter::new(5, MAX);
    // buffer ≥ 5 后第一个逗号即软切。
    let out = s.push("abcde,fghij");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "abcde,");
    assert_eq!(s.flush().expect("residual").text, "fghij");
}

#[test]
fn comma_before_soft_chars_waits() {
    let mut s = SentenceSplitter::new(5, MAX);
    // 逗号时 buffer 仅 3 < 5 → 不切，仅记为安全边界。
    assert!(s.push("ab,c").is_empty());
    // 第二个逗号时 buffer = 7 ≥ 5 → 软切。
    let out = s.push("def,");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "ab,cdef,");
    assert!(s.flush().is_none());
}

#[test]
fn newline_cuts_immediately() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // 换行与 CJK 强标点同路径：`。` 先切，单独成段的 `\n` 被 trim 丢弃。
    let out = s.push("第一句。\n第二句");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "第一句。");
    assert_eq!(s.flush().expect("residual").text, "第二句");
}

#[test]
fn bare_newline_cuts() {
    let mut s = SentenceSplitter::new(SOFT, MAX);
    // 无句末标点时 `\n` 本身即切点。
    let out = s.push("line one\nline two");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "line one");
    assert_eq!(s.flush().expect("residual").text, "line two");
}

#[test]
fn fullwidth_comma_soft_cut() {
    let mut s = SentenceSplitter::new(6, MAX);
    // 全角逗号同样触发软切。
    let out = s.push("一二三四五六，七八");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三四五六，");
    assert_eq!(s.flush().expect("residual").text, "七八");
}
