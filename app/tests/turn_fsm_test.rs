
use momo_app::{
    pipeline::splitter::Sentence,
};

#[test]
fn turn_ids_are_unique_and_prefixed() {
    let a = Turn::new();
    let b = Turn::new();
    assert_ne!(a.id, b.id);
    assert!(a.id.starts_with("t_"));
}

#[test]
fn turn_state_wire_values_match_protocol() {
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
    let mut history = History::new(20);
    let msgs = history.messages();
}

#[test]
fn empty_commit_is_noop() {
    let mut history = History::new(20);
    assert!(history.messages().is_empty());
}
