//! 句切边界用例（momo-app.md §splitter）：
//! 标点/tag/超长/code block。骨架阶段覆盖核心规则。

use momo_app::{pipeline::splitter::SentenceSplitter, proto::message::EmotionTag};

const MAX: usize = 60;

#[test]
fn splits_on_terminal_punctuation() {
    let mut s = SentenceSplitter::new(MAX);
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
    let mut s = SentenceSplitter::new(MAX);
    assert!(s.push("今").is_empty());
    assert!(s.push("晚吃").is_empty());
    let out = s.push("火锅？");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "今晚吃火锅？");
}

#[test]
fn extracts_leading_emotion_tag() {
    let mut s = SentenceSplitter::new(MAX);
    let out = s.push("[happy] 去吃火锅吧！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, Some(EmotionTag::Happy));
    assert_eq!(out[0].text, "去吃火锅吧！");
}

#[test]
fn unknown_tag_stays_in_text() {
    let mut s = SentenceSplitter::new(MAX);
    let out = s.push("[dancing] 跳起来！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, None);
    assert_eq!(out[0].text, "[dancing] 跳起来！");
}

#[test]
fn hard_cut_at_max_chars() {
    let mut s = SentenceSplitter::new(10);
    // 10 个非句末字符 → 硬切。
    let out = s.push("一二三四五六七八九十还没完");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三四五六七八九十");
}

#[test]
fn prefers_secondary_punctuation_within_max() {
    let mut s = SentenceSplitter::new(10);
    // 在次级标点处（第 4 字符后）优先切；剩余不足上限不再切。
    let out = s.push("一二三，四五六七八九");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三，");
}

#[test]
fn does_not_split_inside_code_block() {
    let mut s = SentenceSplitter::new(MAX);
    assert!(s.push("看代码：```let a = 1。let b = 2;```").is_empty());
    let out = s.push("结束。");
    assert_eq!(out.len(), 1);
    assert!(out[0].text.contains("```let a = 1。let b = 2;```"));
    assert!(out[0].text.ends_with("结束。"));
}

#[test]
fn flush_emits_remainder() {
    let mut s = SentenceSplitter::new(MAX);
    assert!(s.push("没说完的话").is_empty());
    let rest = s.flush().expect("residual sentence");
    assert_eq!(rest.text, "没说完的话");
    assert_eq!(rest.index, 1);
}
