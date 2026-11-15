use chrono::{Duration, TimeZone, Utc};
use serde_json::json;

use super::*;
use crate::modes::Mode;
use crate::thread::ThreadEvent;
use crate::tools::Access;

fn t(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap() + Duration::minutes(minutes)
}

fn ended(outcome: TurnOutcome) -> StatusInput {
    StatusInput::TurnEnded {
        outcome,
        summary: "All green.".into(),
        more_queued: false,
    }
}

fn asked(what: &str) -> StatusInput {
    StatusInput::PermissionAsked {
        summary: what.into(),
    }
}

#[test]
fn a_turn_finished_unseen_is_done_and_alerts_once() {
    let mut e = StatusEntry::idle(t(0));
    assert_eq!(e.apply(StatusInput::TurnStarted, false, t(1)), None);
    assert_eq!(e.status, ThreadStatus::Working);
    assert_eq!(e.changed, t(1));
    let alert = e.apply(ended(TurnOutcome::Finished), false, t(5));
    assert_eq!(
        alert,
        Some(Alert::Finished {
            summary: "All green.".into(),
            error: false
        })
    );
    assert_eq!(e.status, ThreadStatus::Done);
    assert_eq!(e.changed, t(5));
    // Opening it clears Done.
    assert_eq!(e.apply(StatusInput::Viewed, true, t(6)), None);
    assert_eq!(e.status, ThreadStatus::Idle);
    assert_eq!(e.changed, t(6));
}

#[test]
fn a_turn_finished_while_viewed_is_idle_without_alert() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, true, t(1));
    assert_eq!(e.apply(ended(TurnOutcome::Finished), true, t(2)), None);
    assert_eq!(e.status, ThreadStatus::Idle);
}

#[test]
fn permission_prompts_need_you_until_all_are_answered() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, false, t(0));
    assert_eq!(
        e.apply(asked("Allow Edit a.txt?"), false, t(1)),
        Some(Alert::NeedsYou {
            summary: "Allow Edit a.txt?".into()
        })
    );
    assert_eq!(e.status, ThreadStatus::NeedsYou);
    // A second prompt: still Needs you, no second alert, first summary kept.
    assert_eq!(e.apply(asked("Allow Run ls?"), false, t(2)), None);
    assert_eq!(e.prompts, 2);
    assert_eq!(e.summary, "Allow Edit a.txt?");
    // Viewing does not clear an open prompt.
    e.apply(StatusInput::Viewed, true, t(3));
    assert_eq!(e.status, ThreadStatus::NeedsYou);
    e.apply(StatusInput::PermissionAnswered, true, t(4));
    assert_eq!(e.status, ThreadStatus::NeedsYou);
    e.apply(StatusInput::PermissionAnswered, true, t(5));
    assert_eq!(e.status, ThreadStatus::Working);
    assert_eq!(e.changed, t(5));
    // A prompt closed after the turn ended changes nothing.
    e.apply(ended(TurnOutcome::Finished), true, t(6));
    e.apply(StatusInput::PermissionAnswered, true, t(7));
    assert_eq!(e.status, ThreadStatus::Idle);
    assert_eq!(e.prompts, 0);
}

#[test]
fn a_prompt_while_viewed_needs_you_without_alert() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, true, t(0));
    assert_eq!(e.apply(asked("Allow it?"), true, t(1)), None);
    assert_eq!(e.status, ThreadStatus::NeedsYou);
}

#[test]
fn errors_are_done_with_the_error_flag() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, false, t(0));
    e.apply(asked("Allow it?"), false, t(1));
    let alert = e.apply(ended(TurnOutcome::Failed), false, t(2));
    assert_eq!(
        alert,
        Some(Alert::Finished {
            summary: "All green.".into(),
            error: true
        })
    );
    assert_eq!(
        (e.status, e.error, e.prompts),
        (ThreadStatus::Done, true, 0)
    );
    // The next turn clears the flag.
    e.apply(StatusInput::TurnStarted, false, t(3));
    assert_eq!((e.status, e.error), (ThreadStatus::Working, false));
}

#[test]
fn cancelled_turns_go_idle_quietly() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, false, t(0));
    e.apply(asked("Allow it?"), false, t(1));
    assert_eq!(e.apply(ended(TurnOutcome::Cancelled), false, t(2)), None);
    assert_eq!(e.status, ThreadStatus::Idle);
}

#[test]
fn queued_messages_keep_the_thread_working() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, false, t(0));
    let next = StatusInput::TurnEnded {
        outcome: TurnOutcome::Finished,
        summary: String::new(),
        more_queued: true,
    };
    assert_eq!(e.apply(next, false, t(1)), None);
    assert_eq!((e.status, e.changed), (ThreadStatus::Working, t(0)));
    // A failure still reports, queue or not.
    let failed = StatusInput::TurnEnded {
        outcome: TurnOutcome::Failed,
        summary: "boom".into(),
        more_queued: true,
    };
    assert!(e.apply(failed, false, t(2)).is_some());
    assert_eq!(e.status, ThreadStatus::Done);
}

#[test]
fn a_written_plan_needs_you_until_viewed() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::TurnStarted, false, t(0));
    e.apply(StatusInput::PlanWritten, false, t(1));
    let alert = e.apply(ended(TurnOutcome::Finished), false, t(2));
    assert!(
        matches!(alert, Some(Alert::NeedsYou { summary }) if summary.starts_with("Plan ready"))
    );
    assert_eq!(e.status, ThreadStatus::NeedsYou);
    e.apply(StatusInput::Viewed, true, t(3));
    assert_eq!((e.status, e.plan), (ThreadStatus::Idle, false));
    // Watched while it was written: nothing to report.
    e.apply(StatusInput::TurnStarted, true, t(4));
    e.apply(StatusInput::PlanWritten, true, t(5));
    assert_eq!(e.apply(ended(TurnOutcome::Finished), true, t(6)), None);
    assert_eq!(e.status, ThreadStatus::Idle);
}

#[test]
fn viewing_idle_or_working_threads_changes_nothing() {
    let mut e = StatusEntry::idle(t(0));
    e.apply(StatusInput::Viewed, true, t(1));
    assert_eq!((e.status, e.changed), (ThreadStatus::Idle, t(0)));
    e.apply(StatusInput::TurnStarted, false, t(2));
    e.apply(StatusInput::Viewed, true, t(3));
    assert_eq!(e.status, ThreadStatus::Working);
}

#[test]
fn the_board_groups_threads_in_section_order_newest_first() {
    let mut board = StatusBoard::default();
    board.apply("w", StatusInput::TurnStarted, false, t(1));
    board.apply("n", StatusInput::TurnStarted, false, t(1));
    board.apply("n", asked("Allow it?"), false, t(2));
    for (id, at) in [("d1", 3), ("d2", 4)] {
        board.apply(id, StatusInput::TurnStarted, false, t(0));
        board.apply(id, ended(TurnOutcome::Finished), false, t(at));
    }
    let threads = [
        ("i1", t(-10)),
        ("d1", t(-5)),
        ("w", t(-5)),
        ("i2", t(-2)),
        ("n", t(-5)),
        ("d2", t(-5)),
    ];
    let groups = board.group(&threads, |t| t.0, |t| t.1);
    let ids: Vec<(ThreadStatus, Vec<&str>)> = groups
        .into_iter()
        .map(|(s, rows)| (s, rows.into_iter().map(|r| r.0).collect()))
        .collect();
    assert_eq!(
        ids,
        [
            (ThreadStatus::NeedsYou, vec!["n"]),
            (ThreadStatus::Done, vec!["d2", "d1"]),
            (ThreadStatus::Working, vec!["w"]),
            (ThreadStatus::Idle, vec!["i2", "i1"]),
        ]
    );
    assert_eq!(board.needs_you(), 1);
    assert_eq!(board.active().count(), 4);
    assert_eq!(board.status("unknown"), ThreadStatus::Idle);
    board.remove("n");
    assert_eq!(board.status("n"), ThreadStatus::Idle);
}

#[test]
fn short_ago_buckets() {
    assert_eq!(short_ago(t(0), t(0)), "<1m");
    assert_eq!(short_ago(t(5), t(0)), "<1m", "clock skew is now");
    assert_eq!(short_ago(t(0), t(12)), "12m");
    assert_eq!(short_ago(t(0), t(59)), "59m");
    assert_eq!(short_ago(t(0), t(180)), "3h");
    assert_eq!(short_ago(t(0), t(1440)), "1d");
    assert_eq!(short_ago(t(0), t(1440 * 9)), "9d");
}

#[test]
fn summaries_are_one_line() {
    assert_eq!(one_line("\n  first line \nsecond", 100), "first line");
    assert_eq!(one_line("abcdef", 4), "abc…");
    let request = PermissionRequest {
        id: 1,
        tool_call_id: "t1".into(),
        tool: "edit_file".into(),
        title: "Edit a.txt".into(),
        input: json!({}),
        access: Access::Write,
    };
    assert_eq!(permission_summary(&request), "Allow Edit a.txt?");

    let mut thread = Thread::new("/p".into(), "anthropic/m", Mode::Agent);
    assert_eq!(
        turn_summary(&thread, &StopReason::EndTurn),
        "Turn finished."
    );
    thread.apply(&ThreadEvent::AssistantStarted { id: 1 });
    thread.apply(&ThreadEvent::TextDelta("Fixed the test.\nDetails…".into()));
    assert_eq!(
        turn_summary(&thread, &StopReason::EndTurn),
        "Fixed the test."
    );
    assert_eq!(
        turn_summary(&thread, &StopReason::Error("401".into())),
        "Failed: 401"
    );
    assert_eq!(turn_summary(&thread, &StopReason::Cancelled), "Stopped.");
}
