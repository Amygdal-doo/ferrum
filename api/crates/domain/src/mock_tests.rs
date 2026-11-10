use super::*;

fn route(name: &str, method: HttpMethod, path: &str) -> MockRoute {
    MockRoute {
        name: name.into(),
        method,
        path: path.into(),
        response: MockResponse {
            name: name.into(),
            ..MockResponse::default()
        },
    }
}

fn req<'a>(method: &'a str, path: &'a str, query: &'a str) -> MockRequest<'a> {
    MockRequest {
        method,
        path,
        query,
        headers: &[],
    }
}

fn vars(pairs: &[(&str, &str)]) -> VariableSet {
    let mut v = VariableSet::new();
    for (k, val) in pairs {
        v.insert(*k, *val);
    }
    v
}

#[test]
fn route_path_strips_base_scheme_host_and_query() {
    let none = VariableSet::new();
    assert_eq!(route_path("{{baseUrl}}/users/:id?x=1", &none), "/users/:id");
    assert_eq!(route_path("https://api.test/v1/users/", &none), "/v1/users");
    assert_eq!(route_path("http://localhost:3000", &none), "/");
    assert_eq!(route_path("api.test/items#frag", &none), "/items");
    assert_eq!(route_path("/health", &none), "/health");
    assert_eq!(route_path("", &none), "/");
    // Variables after the base resolve; unresolved ones become parameters.
    let v = vars(&[("ver", "v2"), ("host", "https://h.test/api")]);
    assert_eq!(route_path("{{base}}/{{ver}}/u/{{id}}", &v), "/v2/u/{{id}}");
    assert_eq!(route_path("{{host}}/x", &v), "/x");
    assert_eq!(route_path("https://{{h}}/a//b", &v), "/a/b");
}

#[test]
fn path_params_in_all_three_styles() {
    for pattern in [
        "/users/:id/posts",
        "/users/{id}/posts",
        "/users/{{id}}/posts",
    ] {
        let (params, literals) = match_path(pattern, "/users/42/posts").expect(pattern);
        assert_eq!(params, [("id".to_owned(), "42".to_owned())]);
        assert_eq!(literals, 2);
    }
    assert!(match_path("/users/:id", "/users").is_none());
    assert!(match_path("/users/:id", "/users/1/extra").is_none());
    assert!(match_path("/users/:id", "/accounts/1").is_none());
    // Trailing slashes are ignored; params are percent-decoded.
    let (params, _) = match_path("/f/:name/", "/f/a%20b/").expect("match");
    assert_eq!(params[0].1, "a b");
    // A bare ':' or '{}' is literal.
    assert!(match_path("/a/:", "/a/:").is_some());
    assert!(match_path("/a/:", "/a/x").is_none());
}

#[test]
fn method_must_match_and_literals_beat_params() {
    let routes = vec![
        route("by id", HttpMethod::Get, "/users/:id"),
        route("me", HttpMethod::Get, "/users/me"),
        route("create", HttpMethod::Post, "/users"),
    ];
    let m = find_route(&routes, &req("GET", "/users/me", "")).expect("match");
    assert_eq!(m.route.name, "me");
    let m = find_route(&routes, &req("get", "/users/7", "")).expect("match");
    assert_eq!(m.route.name, "by id");
    assert_eq!(m.params, [("id".to_owned(), "7".to_owned())]);
    assert!(find_route(&routes, &req("DELETE", "/users/7", "")).is_none());
    assert_eq!(
        find_route(&routes, &req("POST", "/users/", "")).map(|m| m.route.name.clone()),
        Some("create".into())
    );
}

#[test]
fn query_and_header_conditions_select_a_variant() {
    let mut plain = route("list", HttpMethod::Get, "/items");
    plain.response.name = "default".into();
    let mut page2 = route("list", HttpMethod::Get, "/items");
    page2.response.name = "page 2".into();
    page2.response.match_query = vec![KeyValue::new("page", "2")];
    let mut authed = route("list", HttpMethod::Get, "/items");
    authed.response.name = "admin".into();
    authed.response.match_headers = vec![KeyValue::new("x-role", "admin")];
    let mut disabled_cond = route("list", HttpMethod::Get, "/items");
    disabled_cond.response.name = "never preferred".into();
    let mut off = KeyValue::new("nope", "1");
    off.enabled = false;
    disabled_cond.response.match_query = vec![off];
    let routes = vec![plain, page2, authed, disabled_cond];

    let name = |r: MockRequest<'_>| find_route(&routes, &r).map(|m| m.route.response.name.clone());
    assert_eq!(name(req("GET", "/items", "")), Some("default".into()));
    assert_eq!(
        name(req("GET", "/items", "page=2&x=1")),
        Some("page 2".into())
    );
    assert_eq!(name(req("GET", "/items", "page=3")), Some("default".into()));
    let headers = [("X-Role".to_owned(), "admin".to_owned())];
    let with_header = MockRequest {
        headers: &headers,
        ..req("GET", "/items", "")
    };
    assert_eq!(name(with_header), Some("admin".into()));
}

#[test]
fn empty_condition_value_means_present() {
    let mut r = route("q", HttpMethod::Get, "/s");
    r.response.match_query = vec![KeyValue::new("q", "")];
    let routes = vec![r];
    assert!(find_route(&routes, &req("GET", "/s", "q=anything")).is_some());
    assert!(find_route(&routes, &req("GET", "/s", "q")).is_some());
    assert!(find_route(&routes, &req("GET", "/s", "other=1")).is_none());
}

#[test]
fn not_found_lists_routes() {
    let routes = vec![route("Get user", HttpMethod::Get, "/users/:id")];
    let body = not_found_body(&routes, "PUT", "/nope");
    let v: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(v["error"], "No mock matches PUT /nope");
    assert_eq!(v["routes"][0]["method"], "GET");
    assert_eq!(v["routes"][0]["path"], "/users/:id");
    assert_eq!(v["routes"][0]["request"], "Get user");
}

#[test]
fn query_decoding() {
    assert_eq!(
        query_pairs("a=1&b=x%2By&c&&d=a+b&e=%zz"),
        [
            ("a".into(), "1".into()),
            ("b".into(), "x+y".into()),
            ("c".into(), String::new()),
            ("d".into(), "a b".into()),
            ("e".into(), "%zz".into()),
        ]
    );
}

#[test]
fn mocks_round_trip_and_default() {
    let item = ItemId::new();
    let mocks = CollectionMocks {
        port: 4011,
        responses: vec![MockResponse::for_item(item)],
    };
    let json = serde_json::to_string(&mocks).expect("ser");
    let back: CollectionMocks = serde_json::from_str(&json).expect("de");
    assert_eq!(back, mocks);
    assert_eq!(back.for_item(item).count(), 1);
    let empty: CollectionMocks = serde_json::from_str("{}").expect("de");
    assert_eq!(empty.port, DEFAULT_MOCK_PORT);
}
