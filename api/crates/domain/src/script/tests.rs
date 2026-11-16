use super::*;

fn response() -> HttpResponse {
    let body = r#"{"data":{"token":"abc","items":[{"id":7,"name":"A"},{"id":8}],"ok":true,"none":null},"count":"2"}"#;
    HttpResponse {
        status: 201,
        time_ms: 120,
        headers: vec![
            (
                "Content-Type".into(),
                "application/json; charset=utf-8".into(),
            ),
            ("X-Rate".into(), "10".into()),
        ],
        body: body.as_bytes().to_vec(),
        size_bytes: body.len() as u64,
        ..HttpResponse::default()
    }
}

fn tests(src: &str) -> ScriptReport {
    let mut vars = VariableSet::new();
    vars.insert("base", "http://x");
    run_tests(src, &mut vars, &response())
}

fn passed(src: &str) -> bool {
    let r = tests(src);
    assert!(r.errors.is_empty(), "{src}: {:?}", r.errors);
    assert_eq!(r.tests.len(), 1, "{src}");
    r.tests[0].passed
}

#[test]
fn checks_status_headers_and_json_paths() {
    for ok in [
        "assert status == 201",
        "assert status == \"201\"",
        "assert status >= 200 and status < 300",
        "assert time < 500",
        "assert size > 10",
        "assert header(\"content-type\") contains \"json\"",
        "assert headers[\"Content-Type\"] startswith \"application/json\"",
        "assert headers[\"x-rate\"] == 10",
        "assert json.data.token == \"abc\"",
        "assert json.data.items[0].id == 7",
        "assert json.data.items[-1].id == 8",
        "assert json[\"data\"][\"items\"][1][\"id\"] == 8",
        "assert json.data.items.length == 2",
        "assert len(json.data.items) == 2",
        "assert json.data.items is array",
        "assert json.data.none is null",
        "assert json.data.none exists",
        "assert json.data.missing not exists",
        "assert json.data.items[1].name not exists",
        "assert json.data.ok",
        "assert !(status == 500)",
        "assert not json.data.missing",
        "assert json.count == 2",
        "assert json.data.token matches \"^a.c$\"",
        "assert body contains \"items\"",
        "assert lower(json.data.items[0].name) == \"a\"",
        "assert json.data.items[0] contains \"name\"",
        "assert status == 404 || status == 201",
        "assert \"{{base}}/a\" == \"http://x/a\"",
        "assert json.data.missing is empty",
        "assert json.data.token is not number",
        "assert 1 + 2 == 3 and \"a\" + 1 == \"a1\" and -1 < 0",
    ] {
        assert!(passed(ok), "{ok}");
    }
    for bad in [
        "assert status == 200",
        "assert json.data.missing exists",
        "assert json.data.items is object",
        "assert header(\"content-type\") not contains \"json\"",
    ] {
        assert!(!passed(bad), "{bad}");
    }
}

#[test]
fn failure_messages_show_the_values() {
    let r = tests(
        "test \"created\": status == 200\nassert json.data.x exists\nassert json.data.token is number",
    );
    let msgs: Vec<(&str, bool, &str)> = r
        .tests
        .iter()
        .map(|t| (t.name.as_str(), t.passed, t.message.as_str()))
        .collect();
    assert_eq!(
        msgs,
        [
            ("created", false, "201 == 200 is false"),
            ("json.data.x exists", false, "missing"),
            (
                "json.data.token is number",
                false,
                "expected number, got \"abc\""
            ),
        ]
    );
    assert!(!r.passed());
}

#[test]
fn set_and_unset_update_variables_and_are_reported() {
    let mut vars = VariableSet::new();
    vars.insert("old", "1");
    let r = run_tests(
        "set env.token = json.data.token\nset var.id = json.data.items[0].id\n\
         set global.\"odd-name\" = \"v-{{token}}\"\nunset env.old\nlog \"id=\" + var.id\n\
         assert env.token == \"abc\" and vars.id == \"7\" and env[\"odd-name\"] == \"v-abc\"",
        &mut vars,
        &response(),
    );
    assert!(r.passed(), "{r:?}");
    assert_eq!(vars.get("token"), Some("abc"));
    assert_eq!(vars.get("old"), None);
    assert_eq!(r.logs, ["id=7"]);
    let changes: Vec<(VarScope, &str, Option<&str>)> = r
        .changes
        .iter()
        .map(|c| (c.scope, c.key.as_str(), c.value.as_deref()))
        .collect();
    assert_eq!(
        changes,
        [
            (VarScope::Environment, "token", Some("abc")),
            (VarScope::Run, "id", Some("7")),
            (VarScope::Global, "odd-name", Some("v-abc")),
            (VarScope::Environment, "old", None),
        ]
    );
}

#[test]
fn pre_request_has_variables_but_no_response() {
    let mut vars = VariableSet::new();
    vars.insert("user", "ada");
    let r = run_pre_request(
        "# comment\n// another\nset var.stamp = timestamp()\nset var.who = upper(env.user)\n\
         set var.n = random_int(1, 3)\nset var.id = uuid()\nassert status == 200",
        &mut vars,
    );
    assert_eq!(vars.get("who"), Some("ADA"));
    let n: i64 = vars.get("n").and_then(|n| n.parse().ok()).expect("n");
    assert!((1..=3).contains(&n));
    assert_eq!(vars.get("id").map(str::len), Some(36));
    assert!(vars.get("stamp").is_some_and(|s| s.parse::<i64>().is_ok()));
    assert_eq!(r.tests.len(), 1);
    assert!(!r.tests[0].passed);
    assert!(
        r.tests[0]
            .message
            .contains("only available in test scripts")
    );
}

#[test]
fn errors_name_the_line_and_do_not_stop_the_script() {
    let r = tests(
        "assert status ==\nfrobnicate x\nset nope.x = 1\nassert \"a\" matches \"(\"\nassert 1 < \"b\"\nassert status == 201",
    );
    assert_eq!(r.tests.len(), 3);
    assert!(r.tests[2].passed);
    assert!(r.tests[0].message.contains("bad pattern"), "{:?}", r.tests);
    assert!(
        r.tests[1].message.contains("cannot compare"),
        "{:?}",
        r.tests
    );
    assert_eq!(r.errors.len(), 3, "{:?}", r.errors);
    assert!(r.errors[0].starts_with("tests line 1:"));
    assert!(r.errors[1].contains("unknown statement `frobnicate`"));
    assert!(r.errors[2].contains("unknown scope `nope`"));
    assert!(!r.passed());
    assert_eq!(
        check("assert status == 1\nwhat\n"),
        [(
            2,
            "unknown statement `what` (use set, unset, test, assert or log)".to_owned()
        )]
    );
}

#[test]
fn hostile_input_is_bounded() {
    let deep = format!("assert {}1{}", "(".repeat(200), ")".repeat(200));
    let r = tests(&deep);
    assert!(r.errors[0].contains("nested too deeply"), "{:?}", r.errors);
    let huge = "assert true\n".repeat(MAX_SCRIPT_BYTES / 10);
    assert!(tests(&huge).errors[0].contains("longer than"));
    // A pattern too large to compile is an error, not a hang.
    let r = tests("assert body matches \"(a{1000}){1000}\"");
    assert!(r.tests[0].message.contains("bad pattern"), "{:?}", r.tests);
    assert!(Scripts::default().is_empty());
}
