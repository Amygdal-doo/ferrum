//! Request → code snippet. The request must already be concrete (variables
//! substituted, inherited auth resolved); the URL is used as-is.

mod curl;
mod javascript;
mod python;
mod streaming;

use base64::Engine as _;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, HttpMethod, HttpRequest, RequestBody, RequestKind, RequestSettings,
};
use url::form_urlencoded;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeTarget {
    Curl,
    PythonRequests,
    JavaScriptFetch,
}

impl CodeTarget {
    pub const ALL: [Self; 3] = [Self::Curl, Self::PythonRequests, Self::JavaScriptFetch];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Curl => "cURL",
            Self::PythonRequests => "Python (requests)",
            Self::JavaScriptFetch => "JavaScript (fetch)",
        }
    }
}

/// Snippet for `request` exactly as given (the caller substitutes
/// variables and resolves inherited auth first). GraphQL, SOAP, OData and
/// tRPC requests are rendered as the HTTP request they send; gRPC and
/// WebSocket requests become a `grpcurl` / `websocat` command whatever the
/// target.
#[must_use]
pub fn generate(target: CodeTarget, request: &HttpRequest) -> String {
    match request.kind {
        RequestKind::Grpc => return streaming::grpcurl(&Prepared::new(request), request),
        RequestKind::WebSocket => return streaming::websocat(&Prepared::new(request), request),
        _ => {}
    }
    let lowered = match ferrumapi_domain::protocol::lower(request) {
        Ok(r) => r,
        Err(e) => return format!("# {e}"),
    };
    let p = Prepared::new(&lowered);
    match target {
        CodeTarget::Curl => curl::generate(&p),
        CodeTarget::PythonRequests => python::generate(&p),
        CodeTarget::JavaScriptFetch => javascript::generate(&p),
    }
}

/// `Authorization` value standing in for a per-request Digest / AWS signature.
const SIGNED_PLACEHOLDER: &str = "<computed per request: Digest / AWS Signature v4>";

/// Target-independent view: enabled rows only, auth folded into headers/URL
/// except Basic, which each target renders idiomatically.
struct Prepared<'a> {
    method: HttpMethod,
    url: String,
    headers: Vec<(String, String)>,
    basic: Option<(&'a str, &'a str)>,
    /// Digest / AWS: cURL signs these itself (`--digest`, `--aws-sigv4`);
    /// other targets get a placeholder `Authorization` header.
    signing: Option<&'a Auth>,
    body: &'a RequestBody,
    settings: RequestSettings,
}

impl<'a> Prepared<'a> {
    fn new(r: &'a HttpRequest) -> Self {
        let mut url = r.url.clone();
        let mut headers: Vec<(String, String)> = r
            .headers
            .iter()
            .filter(|h| h.is_active())
            .map(|h| (h.key.clone(), h.value.clone()))
            .collect();
        let mut basic = None;
        let mut signing = None;
        match &r.auth {
            Auth::Inherit | Auth::None => {}
            Auth::Basic { username, password } => {
                basic = Some((username.as_str(), password.as_str()))
            }
            Auth::Bearer { token } => {
                headers.push(("Authorization".into(), format!("Bearer {token}")))
            }
            Auth::ApiKey {
                key,
                value,
                location,
            } => match location {
                ApiKeyLocation::Header => headers.push((key.clone(), value.clone())),
                ApiKeyLocation::Query => {
                    let pair = form_urlencoded::Serializer::new(String::new())
                        .append_pair(key, value)
                        .finish();
                    url = crate::curl::body::append_query(&url, &pair);
                }
            },
            Auth::OAuth2(a) => headers.push((
                "Authorization".into(),
                format!("{} <access token>", a.header_prefix.trim())
                    .trim_start()
                    .to_owned(),
            )),
            Auth::OAuth1(_) => headers.push((
                "Authorization".into(),
                "OAuth <HMAC-SHA1 signature, computed per request>".into(),
            )),
            Auth::Digest(_) | Auth::AwsSigV4(_) => {
                signing = Some(&r.auth);
                headers.push(("Authorization".into(), SIGNED_PLACEHOLDER.into()));
            }
        }
        Self {
            method: r.method,
            url,
            headers,
            basic,
            signing,
            body: &r.body,
            settings: r.settings,
        }
    }

    fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
    }

    /// `Content-Type` the app would add for a raw body, unless one is set.
    fn implied_content_type(&self) -> Option<&'static str> {
        match self.body {
            RequestBody::Raw { language, text }
                if !text.is_empty() && !self.has_header("content-type") =>
            {
                Some(language.content_type())
            }
            _ => None,
        }
    }

    fn basic_header(&self) -> Option<String> {
        self.basic.map(|(u, p)| {
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"));
            format!("Basic {token}")
        })
    }
}

/// Seconds for a timeout, e.g. `30` or `2.5`; `None` when there is no timeout.
fn timeout_secs(s: &RequestSettings) -> Option<String> {
    (s.timeout_ms > 0).then(|| {
        if s.timeout_ms.is_multiple_of(1000) {
            (s.timeout_ms / 1000).to_string()
        } else {
            let text = format!("{}.{:03}", s.timeout_ms / 1000, s.timeout_ms % 1000);
            text.trim_end_matches('0').to_owned()
        }
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
