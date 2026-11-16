//! AWS Signature Version 4: canonical request, string to sign, derived
//! signing key, `Authorization` header.

use super::{aws_enc, hex, hmac_sha256, sha256_hex};
use ferrumapi_domain::AwsSigV4Auth;
use url::Url;

/// Payload hash used when the body cannot be hashed up front (streamed
/// multipart). Accepted by S3; other services require a hashed payload.
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

/// What [`sign`] adds to the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub authorization: String,
    /// `x-amz-*` headers to send (signed), in order.
    pub headers: Vec<(String, String)>,
}

/// Signs `method url` with `extra` headers (lower-case names, already on
/// the request and to be signed: e.g. `content-type`). `amz_date` is
/// `YYYYMMDD'T'HHMMSS'Z'`.
#[must_use]
pub fn sign(
    method: &str,
    url: &Url,
    extra: &[(String, String)],
    payload_hash: &str,
    cfg: &AwsSigV4Auth,
    amz_date: &str,
) -> Signed {
    let service = cfg.service.trim();
    let region = cfg.region.trim();
    let mut added = vec![("x-amz-date".to_owned(), amz_date.to_owned())];
    if service == "s3" {
        added.push(("x-amz-content-sha256".to_owned(), payload_hash.to_owned()));
    }
    if !cfg.session_token.is_empty() {
        added.push((
            "x-amz-security-token".to_owned(),
            cfg.session_token.trim().to_owned(),
        ));
    }
    let mut signed: Vec<(String, String)> = vec![("host".to_owned(), host(url))];
    signed.extend(extra.iter().cloned());
    signed.extend(added.iter().cloned());
    signed.sort_by(|a, b| a.0.cmp(&b.0));
    signed.dedup_by(|a, b| a.0 == b.0);
    let canonical_headers: String = signed
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", collapse(v)))
        .collect();
    let signed_names = signed
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_names}\n{payload_hash}",
        method.to_ascii_uppercase(),
        canonical_uri(url, service == "s3"),
        canonical_query(url),
    );
    let date = amz_date.get(..8).unwrap_or(amz_date);
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical.as_bytes())
    );
    let mut key = hmac_sha256(
        format!("AWS4{}", cfg.secret_access_key.trim()).as_bytes(),
        date.as_bytes(),
    );
    for part in [region, service, "aws4_request"] {
        key = hmac_sha256(&key, part.as_bytes());
    }
    let signature = hex(&hmac_sha256(&key, to_sign.as_bytes()));
    Signed {
        authorization: format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_names}, Signature={signature}",
            cfg.access_key_id.trim()
        ),
        headers: added,
    }
}

/// `host[:port]` as the `Host` header carries it.
fn host(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_owned(),
    }
}

/// Trims and collapses runs of spaces in a header value.
fn collapse(v: &str) -> String {
    v.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The path is already percent-encoded once by `Url`; services other than
/// S3 encode each segment a second time.
fn canonical_uri(url: &Url, s3: bool) -> String {
    let path = url.path();
    let path = if path.is_empty() { "/" } else { path };
    if s3 {
        return path.to_owned();
    }
    path.split('/').map(aws_enc).collect::<Vec<_>>().join("/")
}

fn canonical_query(url: &Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (aws_enc(&k), aws_enc(&v)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(service: &str) -> AwsSigV4Auth {
        AwsSigV4Auth {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: service.into(),
            ..AwsSigV4Auth::default()
        }
    }

    /// `get-vanilla` and `get-vanilla-query-order-key-case` from the AWS
    /// SigV4 test suite.
    #[test]
    fn matches_the_aws_test_suite() {
        let empty = sha256_hex(b"");
        let url = Url::parse("https://example.amazonaws.com/").expect("url");
        let s = sign(
            "GET",
            &url,
            &[],
            &empty,
            &creds("service"),
            "20150830T123600Z",
        );
        assert_eq!(
            s.authorization,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
             SignedHeaders=host;x-amz-date, \
             Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
        assert_eq!(
            s.headers,
            vec![("x-amz-date".to_owned(), "20150830T123600Z".to_owned())]
        );
        let url =
            Url::parse("https://example.amazonaws.com/?Param2=value2&Param1=value1").expect("url");
        let s = sign(
            "GET",
            &url,
            &[],
            &empty,
            &creds("service"),
            "20150830T123600Z",
        );
        assert!(
            s.authorization.ends_with(
                "Signature=b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
            ),
            "{}",
            s.authorization
        );
    }

    #[test]
    fn s3_adds_payload_hash_and_session_token() {
        let mut c = creds("s3");
        c.session_token = "tok".into();
        let url = Url::parse("http://localhost:9000/bucket/a%20b.txt").expect("url");
        let s = sign("PUT", &url, &[], UNSIGNED_PAYLOAD, &c, "20240101T000000Z");
        assert!(
            s.authorization.contains(
                "SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token"
            )
        );
        assert_eq!(host(&url), "localhost:9000");
        assert_eq!(canonical_uri(&url, true), "/bucket/a%20b.txt");
        assert_eq!(canonical_uri(&url, false), "/bucket/a%2520b.txt");
    }
}
