//! `Turn` 状态机迁移表穷举（runtime.md「turn 状态机」）。

use momo_app::{
    pipeline::splitter::Sentence,
    proto::message::{TurnOutcome, TurnState},
    session::{
        History,
        turn::{Turn, TurnEvent, transition},
    },
};

const STATES: [TurnState; 5] = [
    TurnState::Idle,
    TurnState::Listening,
    TurnState::Thinking,
    TurnState::Speaking,
    TurnState::Interrupted,
];

const EVENTS: [TurnEvent; 12] = [
    TurnEvent::CaptureStart,
    TurnEvent::ReplyAccepted,
    TurnEvent::SegmentFinal,
    TurnEvent::CaptureCancel,
    TurnEvent::NewInput,
    TurnEvent::Interrupt,
    TurnEvent::PlaybackStarted,
    TurnEvent::OutputDegraded,
    TurnEvent::LlmFailed,
    TurnEvent::TextComplete,
    TurnEvent::PlaybackComplete,
    TurnEvent::Settled,
];

/// runtime.md 迁移表全行：`(from, event, to)`。
const LEGAL: &[(TurnState, TurnEvent, TurnState)] = &[
    // idle → listening: input.audio.start / VAD speech_start
    (
        TurnState::Idle,
        TurnEvent::CaptureStart,
        TurnState::Listening,
    ),
    // idle → thinking: input.text accepted / voice final
    (
        TurnState::Idle,
        TurnEvent::ReplyAccepted,
        TurnState::Thinking,
    ),
    // listening → thinking: input.audio.segment final
    (
        TurnState::Listening,
        TurnEvent::SegmentFinal,
        TurnState::Thinking,
    ),
    // listening → idle: cancel / empty / false positive
    (
        TurnState::Listening,
        TurnEvent::CaptureCancel,
        TurnState::Idle,
    ),
    // thinking → speaking: playback.started
    (
        TurnState::Thinking,
        TurnEvent::PlaybackStarted,
        TurnState::Speaking,
    ),
    // thinking → thinking: new text invalidates old turn then accepts new
    (
        TurnState::Thinking,
        TurnEvent::NewInput,
        TurnState::Thinking,
    ),
    // thinking → interrupted: Stop / PTT / VAD speech_start
    (
        TurnState::Thinking,
        TurnEvent::Interrupt,
        TurnState::Interrupted,
    ),
    // thinking → idle: LLM failed + turn.finished(failed)
    (TurnState::Thinking, TurnEvent::LlmFailed, TurnState::Idle),
    // thinking → idle: text-only output.complete
    (
        TurnState::Thinking,
        TurnEvent::TextComplete,
        TurnState::Idle,
    ),
    // speaking → interrupted: Stop / PTT / VAD speech_start
    (
        TurnState::Speaking,
        TurnEvent::Interrupt,
        TurnState::Interrupted,
    ),
    // speaking → interrupted: new input.text
    (
        TurnState::Speaking,
        TurnEvent::NewInput,
        TurnState::Interrupted,
    ),
    // speaking → thinking: device / mute / TTS failure; failed streams removed
    (
        TurnState::Speaking,
        TurnEvent::OutputDegraded,
        TurnState::Thinking,
    ),
    // speaking → idle: output.complete + all required completed ACK
    (
        TurnState::Speaking,
        TurnEvent::PlaybackComplete,
        TurnState::Idle,
    ),
    // speaking → idle: LLM failed + invalidate audio + turn.finished(failed)
    (TurnState::Speaking, TurnEvent::LlmFailed, TurnState::Idle),
    // interrupted → listening: new capture remains active
    (
        TurnState::Interrupted,
        TurnEvent::CaptureStart,
        TurnState::Listening,
    ),
    // interrupted → thinking: new text accepted
    (
        TurnState::Interrupted,
        TurnEvent::ReplyAccepted,
        TurnState::Thinking,
    ),
    (
        TurnState::Interrupted,
        TurnEvent::NewInput,
        TurnState::Thinking,
    ),
    // interrupted → idle: no new input after settlement
    (TurnState::Interrupted, TurnEvent::Settled, TurnState::Idle),
];

#[test]
fn transition_table_matches_runtime_doc() {
    assert_eq!(LEGAL.len(), 18, "runtime.md table has 18 rows");
    for &(state, event, want) in LEGAL {
        assert_eq!(
            transition(state, event),
            Ok(want),
            "row {state:?} + {event:?}"
        );
    }
}

#[test]
fn state_event_matrix_rejects_illegal_transitions() {
    for state in STATES {
        for event in EVENTS {
            match LEGAL.iter().find(|(s, e, _)| *s == state && *e == event) {
                Some(&(_, _, want)) => {
                    assert_eq!(transition(state, event), Ok(want), "{state:?} + {event:?}");
                }
                None => {
                    let err =
                        transition(state, event).expect_err("illegal transition must be rejected");
                    assert_eq!(err.state, state);
                    assert_eq!(err.event, event);
                }
            }
        }
    }
}

#[test]
fn rejected_transition_leaves_turn_unchanged() {
    let mut turn = Turn::for_reply("hello", "zh");
    let token = turn.cancel.clone();
    // speaking 中 playback.started 重复到达是非法迁移（不静默吞）。
    turn.on_event(TurnEvent::PlaybackStarted).unwrap();
    let err = turn.on_event(TurnEvent::PlaybackStarted).unwrap_err();
    assert_eq!(err.state, TurnState::Speaking);
    assert_eq!(err.event, TurnEvent::PlaybackStarted);
    assert_eq!(turn.state(), TurnState::Speaking);
    assert!(!token.is_cancelled());
}

#[test]
fn speaking_only_via_playback_started() {
    // 任何非 playback.started 事件都无法进入 Speaking。
    for state in STATES {
        for event in EVENTS {
            if let Ok(next) = transition(state, event) {
                if event != TurnEvent::PlaybackStarted {
                    assert_ne!(next, TurnState::Speaking, "{state:?} + {event:?}");
                }
            }
        }
    }
    // 且 playback.started 只在 Thinking 合法。
    assert_eq!(
        transition(TurnState::Thinking, TurnEvent::PlaybackStarted),
        Ok(TurnState::Speaking)
    );
    for state in STATES {
        if state != TurnState::Thinking {
            assert!(transition(state, TurnEvent::PlaybackStarted).is_err());
        }
    }
}

#[test]
fn interrupting_events_fan_out_cancel() {
    // 打断类事件扇出 cancel（LLM/splitter/TTS/audio 的 child token）。
    for event in [
        TurnEvent::NewInput,
        TurnEvent::Interrupt,
        TurnEvent::LlmFailed,
    ] {
        let mut turn = Turn::for_reply("q", "zh");
        let child = turn.cancel.child_token();
        turn.on_event(event).unwrap();
        assert!(child.is_cancelled(), "{event:?} must cancel");
    }
    // capture cancel 同理。
    let mut cap = Turn::for_capture("cap_1");
    let child = cap.cancel.child_token();
    cap.on_event(TurnEvent::CaptureCancel).unwrap();
    assert!(child.is_cancelled());
    // 普通迁移不触碰令牌。
    let mut turn = Turn::for_reply("q", "zh");
    let child = turn.cancel.child_token();
    turn.on_event(TurnEvent::PlaybackStarted).unwrap();
    assert!(!child.is_cancelled());
}

#[test]
fn turn_finished_emitted_exactly_once() {
    let mut turn = Turn::for_reply("q", "zh");
    assert_eq!(
        turn.finish(TurnOutcome::Completed),
        Some(TurnOutcome::Completed)
    );
    // 幂等 finalize：第二次收尾不再产出终态。
    assert_eq!(turn.finish(TurnOutcome::Interrupted), None);
    assert!(turn.is_finished());
    assert_eq!(turn.outcome(), Some(TurnOutcome::Completed));
}

#[test]
fn full_barge_in_walk() {
    // 语音回合全程：capture → reply → speaking → barge-in → settlement。
    let mut turn = Turn::for_capture("cap_1");
    assert_eq!(turn.state(), TurnState::Listening);
    turn.on_event(TurnEvent::SegmentFinal).unwrap();
    turn.user_text = "今晚吃什么".into();
    turn.mark_emitted(Sentence {
        index: 1,
        text: "吃火锅。".into(),
        emotion: None,
    });
    turn.expect_audio(1);
    turn.on_event(TurnEvent::PlaybackStarted).unwrap();
    assert_eq!(turn.state(), TurnState::Speaking);
    turn.mark_played(1);
    turn.ack_completed(1);
    assert!(turn.acks_settled());
    assert_eq!(turn.committed.len(), 1);
    turn.on_event(TurnEvent::Interrupt).unwrap();
    assert_eq!(turn.state(), TurnState::Interrupted);
    assert!(turn.cancel.is_cancelled());
    turn.on_event(TurnEvent::Settled).unwrap();
    assert_eq!(turn.state(), TurnState::Idle);
}

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

fn sentence(index: u32, text: &str) -> Sentence {
    Sentence {
        index,
        text: text.into(),
        emotion: None,
    }
}

#[test]
fn commit_records_user_and_full_reply() {
    let mut history = History::new(20);
    history.commit(
        "今晚吃什么",
        &[sentence(1, "吃火锅。"), sentence(2, "我请客。")],
    );
    let msgs = history.messages();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, momo_app::session::Role::User);
    assert_eq!(msgs[0].content, "今晚吃什么");
    assert_eq!(msgs[1].content, "吃火锅。我请客。");
    assert!(!msgs[1].partial);
}

#[test]
fn commit_partial_joins_played_sentences() {
    // barge-in 记账：已播句拼一条 partial assistant；未播句不进入 history。
    let mut history = History::new(20);
    let played = vec![sentence(1, "今晚吃"), sentence(2, "火锅。")];
    history.commit_partial("今晚吃什么", &played);
    let msgs = history.messages();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].content, "今晚吃火锅。");
    assert!(msgs[1].partial);
}

#[test]
fn commit_partial_omits_unplayed_sentences() {
    let mut history = History::new(20);
    // seq=2 未播：只记账 seq 前缀。
    history.commit_partial("q", &[sentence(1, "第一句。")]);
    let msgs = history.messages();
    assert_eq!(msgs[1].content, "第一句。");
    assert!(!msgs[1].content.contains("未播"));
}

#[test]
fn empty_commit_is_noop() {
    let mut history = History::new(20);
    history.commit_partial("", &[]);
    assert!(history.messages().is_empty());
}

#[test]
fn history_trims_to_max_turns_keeping_system() {
    let mut history = History::new(1);
    history.push(momo_app::session::ChatMessage {
        role: momo_app::session::Role::System,
        content: "persona".into(),
        partial: false,
    });
    history.commit("q1", &[sentence(1, "a1。")]);
    history.commit("q2", &[sentence(1, "a2。")]);
    let msgs = history.messages();
    // max_turns=1：system 保留 + 最近一回合。
    assert_eq!(msgs[0].role, momo_app::session::Role::System);
    let non_system: Vec<_> = msgs
        .iter()
        .filter(|m| m.role != momo_app::session::Role::System)
        .collect();
    assert_eq!(non_system.len(), 2);
    assert_eq!(non_system[0].content, "q2");
}
