//! `Turn` 状态机迁移表穷举（momo-app.md §session）。
//!
//! 骨架阶段：先验证回合基础设施（id/取消令牌扇出/已播记账语义），
//! 完整迁移表随回合管线实装补齐。

use momo_app::{
    pipeline::splitter::Sentence,
    proto::message::TurnState,
    session::{History, turn::Turn},
};

#[test]
fn turn_ids_are_unique_and_prefixed() {
    let a = Turn::new();
    let b = Turn::new();
    assert_ne!(a.id, b.id);
    assert!(a.id.starts_with("t_"));
}

#[test]
fn cancel_token_fans_out_to_children() {
    let turn = Turn::new();
    let llm = turn.cancel.child_token();
    let tts = turn.cancel.child_token();
    assert!(!llm.is_cancelled() && !tts.is_cancelled());

    // barge-in 第二动作：一枚 token 扇出终止全部上游任务。
    turn.cancel.cancel();
    assert!(llm.is_cancelled());
    assert!(tts.is_cancelled());
}

#[test]
fn turn_state_wire_values_match_protocol() {
    // `state.turn.state` 枚举 lower_snake（protocol §6）。
    let cases = [
        (TurnState::Idle, "idle"),
        (TurnState::Listening, "listening"),
        (TurnState::Thinking, "thinking"),
        (TurnState::Speaking, "speaking"),
        (TurnState::Interrupted, "interrupted"),
    ];
    for (state, want) in cases {
        let got = serde_json::to_value(state).unwrap();
        assert_eq!(got, serde_json::json!(want));
    }
}

#[test]
fn partial_commit_joins_played_sentences() {
    // barge-in 第三动作：已播句拼一条 partial assistant 记账。
    let mut history = History::new(20);
    let played = vec![
        Sentence {
            index: 1,
            text: "今晚吃".into(),
            emotion: None,
        },
        Sentence {
            index: 2,
            text: "火锅。".into(),
            emotion: None,
        },
    ];
    history.commit_partial(&played);
    let msgs = history.messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "今晚吃火锅。");
    assert!(msgs[0].partial);
}

#[test]
fn empty_commit_is_noop() {
    let mut history = History::new(20);
    history.commit_partial(&[]);
    assert!(history.messages().is_empty());
}
