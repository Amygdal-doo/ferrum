//! OAuth 1/2, Digest and AWS auth: Postman import / export (secrets never
//! exported) and code generation.

use ferrumapi_domain::{
    Auth, AwsSigV4Auth, ClientAuthentication, DigestAuth, HttpMethod, HttpRequest, Item,
    OAuth2Grant,
};
use ferrumapi_interop::codegen::{CodeTarget, generate};
use ferrumapi_interop::postman::{export_collection, import_collection};
use serde_json::json;

fn kv(pairs: &[(&str, &str)]) -> serde_json::Value {
    pairs
        .iter()
        .map(|(k, v)| json!({"key": k, "value": v, "type": "string"}))
        .collect()
}

fn request_with(auth: serde_json::Value) -> serde_json::Value {
    json!({"name": "r", "request": {"method": "GET", "url": "https://x.test/a", "auth": auth}})
}

#[test]
fn postman_auth_types_import_and_export_without_secrets() {
    let doc = json!({
        "info": {"name": "Auth", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
        "item": [
            request_with(json!({"type": "oauth2", "oauth2": kv(&[
                ("grant_type", "client_credentials"), ("accessTokenUrl", "https://id/token"),
                ("clientId", "cid"), ("clientSecret", "csecret"), ("scope", "a b"),
                ("client_authentication", "body"), ("headerPrefix", "Token")])})),
            request_with(json!({"type": "oauth1", "oauth1": kv(&[
                ("consumerKey", "ck"), ("consumerSecret", "cs"), ("token", "t"),
                ("tokenSecret", "ts"), ("signatureMethod", "HMAC-SHA1")])})),
            request_with(json!({"type": "digest", "digest": kv(&[("username", "u"), ("password", "pw")])})),
            request_with(json!({"type": "awsv4", "awsv4": kv(&[
                ("accessKey", "AK"), ("secretKey", "SK"), ("region", "eu-west-1"), ("service", "s3")])})),
            request_with(json!({"type": "oauth1", "oauth1": kv(&[("signatureMethod", "RSA-SHA1")])})),
        ]
    });
    let c = import_collection(&doc.to_string()).expect("import");
    let auths: Vec<(&Auth, &str)> = c
        .items
        .iter()
        .map(|i| match i {
            Item::Request(r) => (&r.request.auth, r.description.as_str()),
            Item::Folder(_) => panic!("no folders"),
        })
        .collect();
    let Auth::OAuth2(o2) = auths[0].0 else {
        panic!("oauth2")
    };
    assert_eq!(o2.grant, OAuth2Grant::ClientCredentials);
    assert_eq!(
        (
            o2.token_url.as_str(),
            o2.client_id.as_str(),
            o2.client_secret.as_str()
        ),
        ("https://id/token", "cid", "csecret")
    );
    assert_eq!(
        (o2.client_auth, o2.header_prefix.as_str()),
        (ClientAuthentication::Body, "Token")
    );
    let Auth::OAuth1(o1) = auths[1].0 else {
        panic!("oauth1")
    };
    assert_eq!(
        (o1.consumer_secret.as_str(), o1.token_secret.as_str()),
        ("cs", "ts")
    );
    assert!(matches!(auths[2].0, Auth::Digest(d) if d.password == "pw"));
    let Auth::AwsSigV4(aws) = auths[3].0 else {
        panic!("aws")
    };
    assert_eq!(
        (
            aws.region.as_str(),
            aws.service.as_str(),
            aws.secret_access_key.as_str()
        ),
        ("eu-west-1", "s3", "SK")
    );
    assert!(auths[4].1.contains("RSA-SHA1"), "unsupported method noted");

    let out = export_collection(&c).expect("export");
    for secret in ["csecret", "\"cs\"", "\"ts\"", "\"pw\"", "\"SK\""] {
        assert!(!out.contains(secret), "{secret} leaked into {out}");
    }
    let back = import_collection(&out).expect("re-import");
    let Item::Request(r) = &back.items[0] else {
        panic!("request")
    };
    assert!(
        matches!(&r.request.auth, Auth::OAuth2(a) if a.client_id == "cid" && a.client_secret.is_empty())
    );
}

#[test]
fn curl_signs_digest_and_aws_itself() {
    let mut r = HttpRequest::new(HttpMethod::Get, "https://x.test/a");
    r.auth = Auth::Digest(Box::new(DigestAuth {
        username: "u".into(),
        password: "p".into(),
        ..DigestAuth::default()
    }));
    let curl = generate(CodeTarget::Curl, &r);
    assert!(
        curl.contains("--digest") && curl.contains("--user 'u:p'"),
        "{curl}"
    );
    assert!(!curl.contains("computed per request"), "{curl}");
    let py = generate(CodeTarget::PythonRequests, &r);
    assert!(py.contains("computed per request"), "{py}");

    r.auth = Auth::AwsSigV4(Box::new(AwsSigV4Auth {
        access_key_id: "AK".into(),
        secret_access_key: "SK".into(),
        service: "execute-api".into(),
        ..AwsSigV4Auth::default()
    }));
    let curl = generate(CodeTarget::Curl, &r);
    assert!(
        curl.contains("--aws-sigv4 'aws:amz:us-east-1:execute-api'"),
        "{curl}"
    );
    assert!(curl.contains("--user 'AK:SK'"), "{curl}");

    r.auth = Auth::OAuth2(Box::default());
    let curl = generate(CodeTarget::Curl, &r);
    assert!(
        curl.contains("Authorization: Bearer <access token>"),
        "{curl}"
    );
}
