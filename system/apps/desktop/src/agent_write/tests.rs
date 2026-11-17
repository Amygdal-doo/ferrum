use futures::StreamExt as _;
use serde_json::json;

use super::*;
use crate::tests::{Fixture, draw, seen};

fn tool(tools: &[Arc<dyn HostTool>], name: &str) -> Arc<dyn HostTool> {
    tools
        .iter()
        .find(|t| t.def().spec.name == name)
        .cloned()
        .expect("tool")
}

#[test]
fn every_edit_asks_first_and_reaches_the_window() {
    let (bridge, mut rx) = Bridge::new();
    let tools = tools(&bridge);
    assert_eq!(tools.len(), 4);
    assert!(tools.iter().all(|t| t.def().access == Access::Write));
    let add = tool(&tools, "system_add_component");
    assert_eq!(
        add.title(&json!({"name": "Cache", "kind": "cache"})),
        "Add Cache (cache) to the diagram"
    );
    let window = std::thread::spawn(move || {
        futures::executor::block_on(async {
            let mut seen = Vec::new();
            while let Some((call, reply)) = rx.next().await {
                seen.push(call);
                let _ = reply.send(Ok("done".into()));
            }
            seen
        })
    });
    let run =
        |name: &str, input: Value| futures::executor::block_on(tool(&tools, name).call(input));
    assert_eq!(
        run(
            "system_add_component",
            json!({"name": "Cache", "kind": "cache"})
        )
        .text,
        "done"
    );
    let out = run(
        "system_connect",
        json!({"from": "api", "to": "cache", "style": "async"}),
    );
    assert!(!out.is_error);
    assert!(
        run(
            "system_connect",
            json!({"from": "a", "to": "b", "style": "smoke"})
        )
        .is_error
    );
    assert!(
        run("system_rename", json!({"component": "api"})).is_error,
        "no name"
    );
    assert!(run("system_propose_diagram", json!({"diagram": {"nodes": []}})).is_error);
    drop((add, tools));
    drop(bridge);
    let calls = window.join().expect("window");
    assert_eq!(calls.len(), 2, "only valid calls reach the window");
    assert_eq!(
        calls[1],
        SystemCall::Connect {
            from: "api".into(),
            to: "cache".into(),
            style: EdgeStyle::Async,
            label: None,
        }
    );
}

#[test]
fn a_closed_window_is_an_error() {
    let (bridge, rx) = Bridge::new();
    drop(rx);
    let out = futures::executor::block_on(
        tools(&bridge)[2].call(json!({"component": "api", "name": "API"})),
    );
    assert!(out.is_error && out.text.contains("closed"));
}

#[gpui::test]
fn the_agents_edits_are_saved_and_undone_like_the_users(cx: &mut gpui::TestAppContext) {
    let f = Fixture::new();
    let project = f.project("shop", true);
    let (app, cx) = f.open(cx, Some(project.clone()));
    app.update(cx, |app, cx| {
        let added = app.agent_call(
            SystemCall::Add {
                name: "Search".into(),
                kind: "search".into(),
                layer: None,
                tech: Some("not-a-tech".into()),
                description: "Full-text search".into(),
            },
            cx,
        );
        assert_eq!(added.as_deref(), Ok("Added Search [search] to layer data."));
        let node = app.system.node("search").expect("added");
        assert_eq!(node.tech, None, "an unknown technology is dropped");
        app.agent_call(
            SystemCall::Connect {
                from: "API service".into(),
                to: "search".into(),
                style: EdgeStyle::Request,
                label: Some("query".into()),
            },
            cx,
        )
        .expect("connect");
        app.agent_call(
            SystemCall::Rename {
                component: "search".into(),
                name: "Product search".into(),
            },
            cx,
        )
        .expect("rename");
        assert!(
            app.agent_call(
                SystemCall::Rename {
                    component: "nothing".into(),
                    name: "x".into()
                },
                cx
            )
            .is_err()
        );
        assert!(
            app.agent_call(
                SystemCall::Add {
                    name: "X".into(),
                    kind: "teleporter".into(),
                    layer: None,
                    tech: None,
                    description: String::new(),
                },
                cx
            )
            .is_err()
        );
    });
    let saved = ferrumsystem_core::store::load(&project)
        .expect("load")
        .expect("system");
    assert_eq!(
        saved.node("search").map(|n| n.name.as_str()),
        Some("Product search")
    );
    assert!(
        saved
            .edges
            .iter()
            .any(|e| e.from == "api" && e.to == "search" && e.label.as_deref() == Some("query"))
    );
    // Each was one edit: three undos take them all back.
    app.update(cx, |app, cx| {
        app.undo(false, cx);
        app.undo(false, cx);
        app.undo(false, cx);
        assert!(app.system.node("search").is_none());
    });
}

#[gpui::test]
fn an_approved_draft_becomes_the_projects_first_diagram(cx: &mut gpui::TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let project = f.project("bare", false);
    let (app, cx) = f.open(cx, Some(project.clone()));
    let draft: System = serde_json::from_value(json!({
        "name": "Bare",
        "nodes": [
            {"id": "web", "name": "Web", "kind": "web-client", "layer": "clients"},
            {"id": "api", "name": "API", "kind": "service", "layer": "services"}
        ],
        "edges": [{"from": "web", "to": "api", "style": "request"}]
    }))
    .expect("draft");
    app.update(cx, |app, cx| {
        let out = app.agent_call(SystemCall::Propose(Box::new(draft.clone())), cx);
        assert_eq!(
            out.as_deref(),
            Ok("Created the project's diagram: 2 components, 1 connections.")
        );
        // Proposed again: nothing is missing any more.
        let out = app.agent_call(SystemCall::Propose(Box::new(draft)), cx);
        assert!(out.expect("merge").starts_with("Added 0 components"));
    });
    draw(cx);
    let saved = ferrumsystem_core::store::load(&project)
        .expect("load")
        .expect("created");
    assert_eq!(saved.nodes.len(), 2);
}
