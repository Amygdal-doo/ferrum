use super::*;
use chrono::TimeZone;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0)
        .single()
        .expect("valid date")
}

fn cookie(name: &str, value: &str) -> ResponseCookie {
    ResponseCookie {
        name: name.into(),
        value: value.into(),
        ..ResponseCookie::default()
    }
}

fn names(jar: &CookieJar, host: &str, path: &str, https: bool) -> Vec<String> {
    jar.matching(host, path, https, now())
        .into_iter()
        .map(|c| c.name.clone())
        .collect()
}

#[test]
fn host_only_cookie_is_not_sent_to_subdomains() {
    let mut jar = CookieJar::default();
    assert!(jar.store(&cookie("a", "1"), "Example.com", "/login", now()));
    let stored = &jar.cookies()[0];
    assert!(stored.host_only);
    assert_eq!(
        (stored.domain.as_str(), stored.path.as_str()),
        ("example.com", "/")
    );
    assert_eq!(names(&jar, "example.com", "/x", false), ["a"]);
    assert!(names(&jar, "api.example.com", "/x", false).is_empty());
    assert!(names(&jar, "other.com", "/x", false).is_empty());
}

#[test]
fn domain_cookie_covers_subdomains_and_rejects_foreign_domains() {
    let mut jar = CookieJar::default();
    let mut c = cookie("d", "1");
    c.domain = Some(".Example.com".into());
    assert!(jar.store(&c, "api.example.com", "/", now()));
    assert_eq!(names(&jar, "example.com", "/", false), ["d"]);
    assert_eq!(names(&jar, "deep.api.example.com", "/", false), ["d"]);
    assert!(names(&jar, "badexample.com", "/", false).is_empty());

    c.domain = Some("other.org".into());
    assert!(!jar.store(&c, "api.example.com", "/", now()));
    // A subdomain may not set a cookie for a sibling.
    c.domain = Some("www.example.com".into());
    assert!(!jar.store(&c, "api.example.com", "/", now()));
    assert_eq!(jar.cookies().len(), 1);
}

#[test]
fn domain_matching_rules() {
    assert!(domain_matches("a.b.c", "b.c"));
    assert!(!domain_matches("ab.c", "b.c"));
    assert!(domain_matches("127.0.0.1", "127.0.0.1"));
    assert!(!domain_matches("1.0.0.1", "0.0.1"));
    assert!(!domain_matches("b.c", "a.b.c"));
}

#[test]
fn paths_default_and_match() {
    assert_eq!(default_path("/api/v1/users"), "/api/v1");
    assert_eq!(default_path("/login"), "/");
    assert_eq!(default_path(""), "/");
    assert!(path_matches("/api/v1", "/api"));
    assert!(path_matches("/api/", "/api/"));
    assert!(path_matches("/api", "/api"));
    assert!(!path_matches("/apix", "/api"));
    assert!(path_matches("/anything", "/"));

    let mut jar = CookieJar::default();
    jar.store(&cookie("implicit", "1"), "h.test", "/api/v1/users", now());
    let mut c = cookie("root", "1");
    c.path = Some("/".into());
    jar.store(&c, "h.test", "/api/v1/users", now());
    assert_eq!(
        names(&jar, "h.test", "/api/v1/items", false),
        ["implicit", "root"]
    );
    assert_eq!(names(&jar, "h.test", "/other", false), ["root"]);
}

#[test]
fn secure_cookies_only_over_https() {
    let mut jar = CookieJar::default();
    let mut c = cookie("s", "1");
    c.secure = true;
    jar.store(&c, "h.test", "/", now());
    assert!(names(&jar, "h.test", "/", false).is_empty());
    assert_eq!(names(&jar, "h.test", "/", true), ["s"]);
}

#[test]
fn expiry_from_max_age_and_expires() {
    let mut jar = CookieJar::default();
    let mut c = cookie("short", "1");
    c.expires = Some("Max-Age=60".into());
    jar.store(&c, "h.test", "/", now());
    let mut c = cookie("dated", "1");
    c.expires = Some("Wed, 21 Oct 2026 07:28:00 GMT".into());
    jar.store(&c, "h.test", "/", now());
    let mut c = cookie("past", "1");
    c.expires = Some("Thu, 01 Jan 1970 00:00:00 GMT".into());
    jar.store(&c, "h.test", "/", now());
    assert_eq!(
        jar.cookies().len(),
        2,
        "already-expired cookie is not stored"
    );

    assert_eq!(names(&jar, "h.test", "/", false), ["short", "dated"]);
    let later = now() + Duration::seconds(61);
    let still: Vec<_> = jar.matching("h.test", "/", false, later);
    assert_eq!(still.len(), 1);
    assert_eq!(still[0].name, "dated");
    assert_eq!(jar.remove_expired(later), 1);
    assert_eq!(jar.cookies().len(), 1);
}

#[test]
fn expired_or_zero_max_age_deletes_existing_cookie() {
    let mut jar = CookieJar::default();
    jar.store(&cookie("sid", "1"), "h.test", "/", now());
    let mut del = cookie("sid", "");
    del.expires = Some("Max-Age=0".into());
    jar.store(&del, "h.test", "/", now());
    assert!(jar.is_empty());
}

#[test]
fn replacing_keeps_creation_time_and_header_order() {
    let mut jar = CookieJar::default();
    jar.store(&cookie("a", "1"), "h.test", "/", now());
    jar.store(
        &cookie("b", "2"),
        "h.test",
        "/",
        now() + Duration::seconds(1),
    );
    jar.store(
        &cookie("a", "3"),
        "h.test",
        "/",
        now() + Duration::seconds(2),
    );
    assert_eq!(jar.cookies().len(), 2);
    assert_eq!(
        jar.header_for("h.test", "/", false, now() + Duration::seconds(3))
            .as_deref(),
        Some("a=3; b=2")
    );
    assert_eq!(jar.header_for("other.test", "/", false, now()), None);
}

#[test]
fn removal_and_domains() {
    let mut jar = CookieJar::default();
    jar.store(&cookie("a", "1"), "one.test", "/", now());
    jar.store(&cookie("b", "1"), "one.test", "/", now());
    jar.store(&cookie("c", "1"), "two.test", "/", now());
    assert_eq!(jar.domains(), ["one.test", "two.test"]);
    assert!(jar.remove("one.test", "/", "a"));
    assert!(!jar.remove("one.test", "/", "a"));
    jar.clear_domain("one.test");
    assert_eq!(jar.domains(), ["two.test"]);
}

#[test]
fn cookie_dates_in_common_formats() {
    let want = Utc.with_ymd_and_hms(2026, 10, 21, 7, 28, 0).single();
    assert_eq!(parse_cookie_date("Wed, 21 Oct 2026 07:28:00 GMT"), want);
    assert_eq!(parse_cookie_date("Wed, 21-Oct-2026 07:28:00 GMT"), want);
    assert_eq!(parse_cookie_date("Wednesday, 21-Oct-26 07:28:00 GMT"), want);
    assert_eq!(parse_cookie_date("not a date"), None);
}

#[test]
fn huge_max_age_does_not_overflow() {
    let mut jar = CookieJar::default();
    let mut c = cookie("forever", "1");
    c.expires = Some("Max-Age=999999999999999999".into());
    jar.store(&c, "h.test", "/", now());
    assert_eq!(names(&jar, "h.test", "/", false), ["forever"]);
}

#[test]
fn round_trips_through_json() {
    let mut jar = CookieJar::default();
    let mut c = cookie("a", "1");
    c.expires = Some("Max-Age=60".into());
    jar.store(&c, "h.test", "/", now());
    let json = serde_json::to_string(&jar).expect("serialize");
    let back: CookieJar = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, jar);
}
