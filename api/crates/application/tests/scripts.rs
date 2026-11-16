//! Pre-request and test scripts end to end against a local mock server:
//! a single send (report on the response, variables used by the request)
//! and a collection run (variables carried to later requests, pass / fail
//! from tests, environment changes reported).

use ferrumapi_application::domain::script::{Scripts, VarScope};
use ferrumapi_application::domain::{
    Collection, Environment, HttpMethod, HttpRequest, Item, SavedRequest, Variable,
};
use ferrumapi_application::{
    AppServices, CancellationToken, RunEvent, RunOutcome, plan_run, prepare,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn services() -> (AppServices, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-scripts-{}", uuid::Uuid::new_v4()));
    (AppServices::start(&dir).expect("start"), dir)
}

/// (index, status, passed, tests passed / total) of a finished request.
type Row = (usize, u16, bool, Option<(usize, usize)>);

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt")
}

fn scripted(m: HttpMethod, url: &str, pre: &str, tests: &str) -> HttpRequest {
    let mut r = HttpRequest::new(m, url);
    r.scripts = Scripts {
        pre_request: pre.into(),
        tests: tests.into(),
    };
    r
}

async fn server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login"))
        .and(header("x-request", "from-script"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"token":"t-123","user":{"id":42}}"#, "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/42"))
        .and(header("authorization", "Bearer t-123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"name":"Ada"}"#, "application/json"),
        )
        .mount(&server)
        .await;
    server
}

#[test]
fn single_send_runs_both_scripts() {
    let (svc, dir) = services();
    rt().block_on(async {
        let server = server().await;
        let mut r = scripted(
            HttpMethod::Post,
            &format!("{}/login", server.uri()),
            "set var.marker = \"from-script\"",
            "test \"ok\": status == 200\nassert json.user.id == 42\n\
             set env.token = json.token\nassert json.missing exists",
        );
        r.headers = vec![ferrumapi_application::domain::KeyValue::new(
            "X-Request",
            "{{marker}}",
        )];
        let mut env = Environment::new("dev");
        env.variables = vec![Variable::new("unused", "1")];
        let prepared = prepare(&r, None, &[], &Environment::globals(), Some(&env));
        let s = svc.clone();
        let resp = svc
            .spawn(async move { s.send(r, prepared, None, CancellationToken::new()).await })
            .await
            .expect("join")
            .expect("response");
        let report = resp.script.expect("report");
        let results: Vec<(&str, bool)> = report
            .tests
            .iter()
            .map(|t| (t.name.as_str(), t.passed))
            .collect();
        assert_eq!(
            results,
            [
                ("ok", true),
                ("json.user.id == 42", true),
                ("json.missing exists", false)
            ]
        );
        let env_changes: Vec<_> = report
            .changes
            .iter()
            .filter(|c| c.scope == VarScope::Environment)
            .map(|c| (c.key.as_str(), c.value.as_deref()))
            .collect();
        assert_eq!(env_changes, [("token", Some("t-123"))]);
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn runs_carry_script_variables_and_pass_on_tests() {
    let (svc, dir) = services();
    rt().block_on(async {
        let server = server().await;
        let mut c = Collection::new("Flow");
        c.variables = vec![Variable::new("base", server.uri())];
        let mut login = scripted(
            HttpMethod::Post,
            "{{base}}/login",
            "",
            "set env.token = json.token\nset var.uid = json.user.id\nassert status == 200",
        );
        login.headers = vec![ferrumapi_application::domain::KeyValue::new(
            "X-Request",
            "from-script",
        )];
        let mut me = scripted(
            HttpMethod::Get,
            "{{base}}/users/{{uid}}",
            "",
            "assert json.name == \"Ada\"\nassert json.name == \"Bob\"",
        );
        me.auth = ferrumapi_application::domain::Auth::Bearer {
            token: "{{token}}".into(),
        };
        // No tests: 2xx passes, as before scripts existed.
        let plain = HttpRequest::new(HttpMethod::Get, "{{base}}/users/{{uid}}");
        for (name, r) in [("Login", login), ("Me", me), ("Plain", plain)] {
            c.items.push(Item::Request(SavedRequest::new(name, r)));
        }
        let items = plan_run(
            c.id,
            None,
            std::slice::from_ref(&c),
            &Environment::globals(),
            None,
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let s = svc.clone();
        svc.spawn(async move { s.run_collection(items, CancellationToken::new(), tx).await })
            .await
            .expect("join");
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        let outcomes: Vec<Row> = events
            .iter()
            .filter_map(|e| match e {
                RunEvent::Finished(
                    i,
                    RunOutcome::Response {
                        status,
                        passed,
                        checks,
                        ..
                    },
                ) => Some((*i, *status, *passed, *checks)),
                _ => None,
            })
            .collect();
        assert_eq!(
            outcomes,
            [
                (0, 200, true, Some((1, 1))),
                (1, 200, false, Some((1, 2))),
                // The Bearer token is not inherited here, so 404: fails.
                (2, 404, false, None),
            ]
        );
        let changes: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                RunEvent::Changes(i, c) => Some((*i, c.len(), c[0].key.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            changes,
            [(0, 1, "token".to_owned())],
            "var. changes stay in the run"
        );
    });
    let _ = std::fs::remove_dir_all(dir);
}
