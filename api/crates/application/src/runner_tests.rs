use super::*;
use ferrumapi_domain::{Auth, Folder, HttpRequest, Item, SavedRequest, Variable};
use std::cell::RefCell;

fn ok(status: u16) -> RunOutcome {
    RunOutcome::Response {
        status,
        time_ms: 1,
        passed: (200..300).contains(&status),
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt")
        .block_on(future)
}

#[test]
fn runs_every_step_in_order_then_done() {
    let calls = RefCell::new(Vec::new());
    let mut events = Vec::new();
    let cancel = CancellationToken::new();
    block_on(run_sequence(
        3,
        &cancel,
        |i| {
            calls.borrow_mut().push(i);
            async move { Some(ok(if i == 1 { 500 } else { 200 })) }
        },
        |e| events.push(e),
    ));
    assert_eq!(*calls.borrow(), [0, 1, 2]);
    assert_eq!(
        events,
        [
            RunEvent::Started(0),
            RunEvent::Finished(0, ok(200)),
            RunEvent::Started(1),
            RunEvent::Finished(1, ok(500)),
            RunEvent::Started(2),
            RunEvent::Finished(2, ok(200)),
            RunEvent::Done { stopped: false },
        ]
    );
    assert!(ok(204).passed() && !ok(500).passed() && !RunOutcome::Error("x".into()).passed());
}

#[test]
fn stop_between_steps_skips_the_rest() {
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    block_on(run_sequence(
        5,
        &cancel,
        |i| {
            if i == 1 {
                cancel.cancel();
            }
            async move { Some(ok(200)) }
        },
        |e| events.push(e),
    ));
    // Step 1 was already running when Stop was pressed; step 2 never starts.
    assert_eq!(events.len(), 5);
    assert_eq!(events[3], RunEvent::Finished(1, ok(200)));
    assert_eq!(events[4], RunEvent::Done { stopped: true });
}

#[test]
fn cancelled_step_ends_the_run() {
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    block_on(run_sequence(
        3,
        &cancel,
        |i| async move { (i == 0).then(|| ok(200)) },
        |e| events.push(e),
    ));
    assert_eq!(
        events,
        [
            RunEvent::Started(0),
            RunEvent::Finished(0, ok(200)),
            RunEvent::Started(1),
            RunEvent::Done { stopped: true },
        ]
    );
}

#[test]
fn empty_run_is_done_immediately() {
    let mut events = Vec::new();
    block_on(run_sequence(
        0,
        &CancellationToken::new(),
        |_| async { Some(ok(200)) },
        |e| events.push(e),
    ));
    assert_eq!(events, [RunEvent::Done { stopped: false }]);
}

#[test]
fn plan_follows_tree_order_and_resolves_auth_and_variables() {
    let mut c = Collection::new("C");
    c.auth = Auth::Bearer {
        token: "{{token}}".into(),
    };
    c.variables = vec![Variable::new("token", "t")];
    let mut folder = Folder::new("F");
    let fid = folder.id;
    folder.items.push(Item::Request(SavedRequest::new(
        "Inner",
        HttpRequest::new(HttpMethod::Post, "https://x/inner"),
    )));
    c.items.push(Item::Folder(folder));
    c.items.push(Item::Request(SavedRequest::new(
        "Outer",
        HttpRequest::new(HttpMethod::Get, "https://x/outer"),
    )));
    let collections = [c.clone()];
    let globals = Environment::globals();

    let all = plan_run(c.id, None, &collections, &globals, None);
    let names: Vec<_> = all.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Inner", "Outer"]);
    assert_eq!(all[0].method, HttpMethod::Post);
    assert_eq!(
        all[0].prepared.request.auth, c.auth,
        "inherited auth resolved"
    );
    assert_eq!(all[0].prepared.variables.get("token"), Some("t"));

    let folder_only = plan_run(c.id, Some(fid), &collections, &globals, None);
    assert_eq!(folder_only.len(), 1);
    assert!(plan_run(CollectionId::new(), None, &collections, &globals, None).is_empty());
}

#[test]
fn plans_skip_websockets_but_keep_every_other_kind() {
    use ferrumapi_domain::{Environment, RequestKind};
    let mut c = Collection::new("Mixed");
    for kind in RequestKind::ALL {
        c.items.push(Item::Request(SavedRequest::new(
            kind.label(),
            HttpRequest::of_kind(kind),
        )));
    }
    let plan = plan_run(c.id, None, &[c.clone()], &Environment::globals(), None);
    let names: Vec<_> = plan.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        ["HTTP / REST", "GraphQL", "SOAP", "OData", "tRPC", "gRPC"]
    );
}
