
use momo_app::{pipeline::splitter::SentenceSplitter, proto::message::EmotionTag};

const MAX: usize = 60;

#[test]
fn splits_on_terminal_punctuation() {
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
    assert!(s.push("今").is_empty());
    assert!(s.push("晚吃").is_empty());
    let out = s.push("火锅？");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "今晚吃火锅？");
}

#[test]
fn extracts_leading_emotion_tag() {
    let out = s.push("[happy] 去吃火锅吧！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, Some(EmotionTag::Happy));
    assert_eq!(out[0].text, "去吃火锅吧！");
}

#[test]
fn unknown_tag_stays_in_text() {
    let out = s.push("[dancing] 跳起来！");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].emotion, None);
    assert_eq!(out[0].text, "[dancing] 跳起来！");
}

#[test]
fn hard_cut_at_max_chars() {
    // 10 个非句末字符 → 硬切。
    let out = s.push("一二三四五六七八九十还没完");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三四五六七八九十");
}

#[test]
fn prefers_secondary_punctuation_within_max() {
    let out = s.push("一二三，四五六七八九");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].text, "一二三，");
}

#[test]
fn does_not_split_inside_code_block() {
    let out = s.push("结束。");
    assert_eq!(out.len(), 1);
    assert!(out[0].text.ends_with("结束。"));
}

#[test]
fn flush_emits_remainder() {
    assert!(s.push("没说完的话").is_empty());
    let rest = s.flush().expect("residual sentence");
    assert_eq!(rest.text, "没说完的话");
    assert_eq!(rest.index, 1);
}
