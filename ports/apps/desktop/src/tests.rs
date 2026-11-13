//! Headless GPUI tests: the root view with injected scan results (no
//! window is shown, no auto-refresh runs).

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use ferrum_ui::NoticeTone;
use ferrumports_core::kill::{KillError, Signal};
use ferrumports_core::{Listener, Protocol, Row, ScanError, Snapshot};
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::app::{FerrumPorts, KillStage, Options};

fn listener(port: u16, process: &str, pid: u32, project: Option<&str>) -> Listener {
    Listener {
        protocol: Protocol::Tcp,
        port,
        addresses: vec!["127.0.0.1".into(), "::1".into()],
        pid: Some(pid),
        process: process.into(),
        user: Some("me".into()),
        command: Some(format!("{process} --port {port}")),
        started: None,
        cwd: project.map(|p| PathBuf::from(p).join("src")),
        project: project.map(PathBuf::from),
    }
}

fn sample() -> Snapshot {
    Snapshot {
        listeners: vec![
            listener(3000, "node", 4_000_001, Some("/code/shop")),
            listener(5432, "postgres", 4_000_002, Some("/code/shop")),
            listener(9229, "node", 4_000_001, Some("/code/shop")),
            listener(11434, "ollama", 4_000_003, None),
        ],
        warnings: Vec::new(),
    }
}

fn window(cx: &mut TestAppContext) -> (Entity<FerrumPorts>, &mut VisualTestContext) {
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::app::bind_keys(cx);
    });
    let (app, cx) = cx.add_window_view(|window, cx| {
        FerrumPorts::new(
            Options {
                project: None,
                autostart: false,
            },
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (app, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn ports(app: &Entity<FerrumPorts>, cx: &mut VisualTestContext) -> Vec<String> {
    app.read_with(cx, |app, cx| {
        app.rows(cx)
            .into_iter()
            .map(|row| match row {
                Row::Group { name, summary, .. } => format!("[{name}: {summary}]"),
                Row::Listener { index, .. } => app.listeners[index].port.to_string(),
            })
            .collect()
    })
}

#[gpui::test]
fn shows_searches_and_groups_listeners(cx: &mut TestAppContext) {
    let (app, cx) = window(cx);
    // Before the first scan: the "Scanning…" state renders.
    draw(cx);
    app.update(cx, |app, cx| app.apply_scan(Ok(sample()), cx));
    draw(cx);
    assert_eq!(ports(&app, cx), ["3000", "5432", "9229", "11434"]);

    app.update(cx, |app, cx| {
        app.search
            .update(cx, |input, cx| input.set_text("node", cx));
    });
    assert_eq!(ports(&app, cx), ["3000", "9229"]);
    app.update(cx, |app, cx| {
        app.search.update(cx, |input, cx| input.set_text("", cx));
        app.grouped = true;
    });
    draw(cx);
    assert_eq!(
        ports(&app, cx),
        [
            "[shop: 3000 web, 5432 postgres, 9229 inspector]",
            "3000",
            "5432",
            "9229",
            "[Other: 11434 ollama]",
            "11434"
        ]
    );
    // "This project" without a project in the strip filters nothing.
    app.update(cx, |app, _| app.only_project = true);
    assert_eq!(ports(&app, cx).len(), 6);
}

#[gpui::test]
fn selection_survives_refresh_and_detail_renders(cx: &mut TestAppContext) {
    let (app, cx) = window(cx);
    app.update(cx, |app, cx| {
        app.apply_scan(Ok(sample()), cx);
        let key = app.listeners[0].key();
        app.select(key, cx);
    });
    draw(cx);
    app.update(cx, |app, cx| {
        assert_eq!(app.selected_listener().map(|l| l.port), Some(3000));
        // A failed scan keeps the rows and shows the error.
        app.apply_scan(Err(ScanError::Missing("lsof")), cx);
        assert!(app.error.as_deref().is_some_and(|e| e.contains("lsof")));
        assert_eq!(app.listeners.len(), 4);
        // The process went away: the selection is dropped.
        let mut snapshot = sample();
        snapshot.listeners.remove(0);
        app.apply_scan(Ok(snapshot), cx);
        assert!(app.error.is_none());
        assert!(app.selected.is_none());
    });
    draw(cx);
}

#[gpui::test]
fn empty_state_renders(cx: &mut TestAppContext) {
    let (app, cx) = window(cx);
    app.update(cx, |app, cx| app.apply_scan(Ok(Snapshot::default()), cx));
    draw(cx);
    assert!(ports(&app, cx).is_empty());
}

#[gpui::test]
fn kill_asks_then_offers_sigkill(cx: &mut TestAppContext) {
    let (app, cx) = window(cx);
    app.update(cx, |app, cx| {
        app.apply_scan(Ok(sample()), cx);
        let l = app.listeners[1].clone();
        app.ask_kill(l, cx);
    });
    draw(cx);
    app.update(cx, |app, cx| {
        assert_eq!(app.dialog.as_ref().map(|d| d.stage), Some(KillStage::Term));
        // SIGTERM went out but the process is still alive.
        app.kill_finished(Signal::Term, Ok(()), true, cx);
        assert_eq!(app.dialog.as_ref().map(|d| d.stage), Some(KillStage::Force));
    });
    draw(cx);
    app.update(cx, |app, cx| {
        app.kill_finished(
            Signal::Kill,
            Err(KillError::PermissionDenied {
                pid: 4_000_002,
                user: Some("root".into()),
                signal: "KILL",
            }),
            false,
            cx,
        );
        assert!(app.dialog.is_none());
        let notice = app.notice.clone().expect("notice");
        assert_eq!(notice.tone, NoticeTone::Error);
        assert!(notice.message.contains("belongs to root"));
    });
    draw(cx);
}

#[gpui::test]
fn kill_stops_a_real_process(cx: &mut TestAppContext) {
    let (app, cx) = window(cx);
    cx.executor().allow_parking();
    let Ok(mut child) = Command::new("sleep").arg("30").spawn() else {
        return;
    };
    let pid = child.id();
    // Reap the child as soon as it exits, like a real parent would.
    let reaper = std::thread::spawn(move || child.wait());
    app.update(cx, |app, cx| {
        app.ask_kill(listener(45_678, "sleep", pid, None), cx);
        app.confirm_kill(cx);
        assert!(app.dialog.as_ref().is_some_and(|d| d.busy));
    });
    cx.run_until_parked();
    reaper.join().expect("reaper").expect("wait");
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    app.read_with(cx, |app, _| {
        assert!(app.dialog.is_none());
        let notice = app.notice.clone().expect("notice");
        assert!(
            notice.message.contains("Stopped sleep"),
            "{}",
            notice.message
        );
    });
}
