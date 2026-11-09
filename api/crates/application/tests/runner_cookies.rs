//! Collection runner and cookie jar end-to-end against a local mock server.

use ferrumapi_application::domain::{
    Collection, Environment, HttpMethod, HttpRequest, Item, RequestOrigin, SavedRequest, Variable,
};
use ferrumapi_application::{
    AppServices, CancellationToken, RunEvent, RunOutcome, plan_run, prepare,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn services() -> (AppServices, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-run-{}", uuid::Uuid::new_v4()));
    (AppServices::start(&dir).expect("start"), dir)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt")
}

async fn mock_login_flow(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("set-cookie", "sid=abc; Path=/; HttpOnly")
                .append_header("set-cookie", "theme=dark; Path=/ui"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(header("cookie", "sid=abc"))
        .respond_with(ResponseTemplate::new(200).set_body_string("me"))
        .mount(server)
        .await;
    // Without the cookie, /me falls through to wiremock's 404.
}

fn collection(base: &str) -> Collection {
    let mut c = Collection::new("Flow");
    c.variables = vec![Variable::new("base", base)];
    for (name, m, url) in [
        ("Login", HttpMethod::Post, "{{base}}/login"),
        ("Me", HttpMethod::Get, "{{base}}/me"),
        ("Missing", HttpMethod::Get, "{{base}}/nope"),
        ("Broken", HttpMethod::Get, "http://"),
    ] {
        c.items.push(Item::Request(SavedRequest::new(
            name,
            HttpRequest::new(m, url),
        )));
    }
    c
}

#[test]
fn runner_sends_in_order_with_cookies_and_reports_pass_fail() {
    let (svc, dir) = services();
    rt().block_on(async {
        let server = MockServer::start().await;
        mock_login_flow(&server).await;
        let c = collection(&server.uri());
        let items = plan_run(
            c.id,
            None,
            std::slice::from_ref(&c),
            &Environment::globals(),
            None,
        );
        assert_eq!(items.len(), 4);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let s = svc.clone();
        svc.spawn(async move { s.run_collection(items, CancellationToken::new(), tx).await })
            .await
            .expect("join");
        let mut events = Vec::new();
        while let Some(e) = rx.recv().await {
            events.push(e);
        }
        let outcomes: Vec<(usize, RunOutcome)> = events
            .iter()
            .filter_map(|e| match e {
                RunEvent::Finished(i, o) => Some((*i, o.clone())),
                _ => None,
            })
            .collect();
        let indexes: Vec<usize> = outcomes.iter().map(|(i, _)| *i).collect();
        assert_eq!(indexes, [0, 1, 2, 3]);
        let statuses: Vec<Option<u16>> = outcomes
            .iter()
            .map(|(_, o)| match o {
                RunOutcome::Response { status, .. } => Some(*status),
                RunOutcome::Error(_) => None,
            })
            .collect();
        // /me only answers 200 when the jar sent the login cookie.
        assert_eq!(statuses, [Some(200), Some(200), Some(404), None]);
        let passed: Vec<bool> = outcomes.iter().map(|(_, o)| o.passed()).collect();
        assert_eq!(passed, [true, true, false, false]);
        assert_eq!(events.last(), Some(&RunEvent::Done { stopped: false }));

        // Runs do not touch history; the jar persists both cookies.
        let store = svc.store.clone();
        let history = svc
            .spawn(async move { store.list_history(10).await })
            .await
            .expect("join")
            .expect("history");
        assert!(history.is_empty());
        let s = svc.clone();
        let jar = svc
            .spawn(async move { s.cookie_jar().await })
            .await
            .expect("join")
            .expect("jar");
        let mut names: Vec<_> = jar.cookies().iter().map(|c| c.name.clone()).collect();
        names.sort();
        assert_eq!(names, ["sid", "theme"]);
    });
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn single_send_uses_jar_and_clearing_it_stops_cookies() {
    let (svc, dir) = services();
    rt().block_on(async {
        let server = MockServer::start().await;
        mock_login_flow(&server).await;
        let c = collection(&server.uri());
        let send = |name: &'static str| {
            let s = svc.clone();
            let saved = c.requests().into_iter().find(|r| r.name == name).cloned();
            let origin = RequestOrigin {
                collection: c.id,
                item: saved.as_ref().expect("request").id,
            };
            let prepared = prepare(
                &saved.expect("request").request,
                Some(origin),
                std::slice::from_ref(&c),
                &Environment::globals(),
                None,
            );
            async move {
                let authored = prepared.request.clone();
                let t = s.clone();
                t.spawn(async move {
                    s.send(authored, prepared, Some(origin), CancellationToken::new())
                        .await
                })
                .await
                .expect("join")
                .expect("response")
                .status
            }
        };
        assert_eq!(send("Me").await, 404, "no cookie yet");
        assert_eq!(send("Login").await, 200);
        assert_eq!(send("Me").await, 200, "cookie from the jar");

        let s = svc.clone();
        let jar = svc
            .spawn(async move { s.clear_cookies(None).await })
            .await
            .expect("join")
            .expect("clear");
        assert!(jar.is_empty());
        assert_eq!(send("Me").await, 404, "cleared jar sends nothing");

        assert_eq!(send("Login").await, 200);
        let s = svc.clone();
        let jar = svc
            .spawn(async move {
                s.delete_cookie(jar_domain(&server), "/".into(), "sid".into())
                    .await
            })
            .await
            .expect("join")
            .expect("delete");
        assert_eq!(jar.cookies().len(), 1, "only the /ui cookie is left");
        assert_eq!(send("Me").await, 404);
    });
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}

fn jar_domain(server: &MockServer) -> String {
    server.address().ip().to_string()
}
