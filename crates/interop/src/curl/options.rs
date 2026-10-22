//! curl argument scanning: words → [`Parsed`] options, no interpretation yet.

use crate::InteropError;

/// One `-d`-family argument, in command-line order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Data {
    /// `-d`, `--data-raw`, `--data-binary`, `--json`: sent as-is.
    Raw(String),
    /// `--data-urlencode`: `content`, `name=content`, ...
    Encode(String),
    /// `@path` with `-d` / `--data-binary`: the file is the body.
    File(String),
}

#[derive(Debug, Default)]
pub(super) struct Parsed {
    pub method: Option<String>,
    pub urls: Vec<String>,
    pub headers: Vec<String>,
    pub data: Vec<Data>,
    /// (`name=value` argument, literal: `--form-string`).
    pub form: Vec<(String, bool)>,
    pub user: Option<String>,
    pub bearer: Option<String>,
    pub upload: Option<String>,
    pub cookies: Vec<String>,
    pub max_time: Option<String>,
    pub get: bool,
    pub head: bool,
    pub location: bool,
    pub insecure: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Request,
    Header,
    Data,
    DataRaw,
    DataBinary,
    DataUrlencode,
    Json,
    Form,
    FormString,
    User,
    Get,
    Url,
    Head,
    Location,
    Insecure,
    MaxTime,
    UserAgent,
    Referer,
    Cookie,
    OAuth2Bearer,
    UploadFile,
    /// Unknown/irrelevant option that consumes a value.
    SkipValue,
    /// Unknown/irrelevant boolean option.
    Flag,
}

/// Long options we ignore but that take a value (so it is not taken as the URL).
const SKIP_LONG: &str = "output output-dir write-out connect-timeout proxy proxy-user proxy-header preproxy retry \
    retry-delay retry-max-time cookie-jar cacert capath cert cert-type key key-type pass \
    resolve connect-to interface range time-cond limit-rate max-redirs config trace \
    trace-ascii stderr speed-limit speed-time continue-at dump-header noproxy aws-sigv4 \
    request-target ciphers tls-max dns-servers unix-socket abstract-unix-socket local-port \
    max-filesize proto proto-redir socks5 socks5-hostname socks4 socks4a netrc-file \
    expect100-timeout keepalive-time variable url-query hsts alt-svc etag-save etag-compare \
    pinnedpubkey crlfile engine service-name login-options mail-from mail-rcpt quote \
    ftp-port happy-eyeballs-timeout-ms create-file-mode tlsuser tlspassword";

fn long(name: &str) -> Opt {
    match name {
        "request" => Opt::Request,
        "header" => Opt::Header,
        "data" | "data-ascii" => Opt::Data,
        "data-raw" => Opt::DataRaw,
        "data-binary" => Opt::DataBinary,
        "data-urlencode" => Opt::DataUrlencode,
        "json" => Opt::Json,
        "form" => Opt::Form,
        "form-string" => Opt::FormString,
        "user" => Opt::User,
        "get" => Opt::Get,
        "url" => Opt::Url,
        "head" => Opt::Head,
        "location" | "location-trusted" => Opt::Location,
        "insecure" => Opt::Insecure,
        "max-time" => Opt::MaxTime,
        "user-agent" => Opt::UserAgent,
        "referer" => Opt::Referer,
        "cookie" => Opt::Cookie,
        "oauth2-bearer" => Opt::OAuth2Bearer,
        "upload-file" => Opt::UploadFile,
        n if SKIP_LONG.split_whitespace().any(|s| s == n) => Opt::SkipValue,
        _ => Opt::Flag,
    }
}

fn short(c: char) -> Opt {
    match c {
        'X' => Opt::Request,
        'H' => Opt::Header,
        'd' => Opt::Data,
        'F' => Opt::Form,
        'u' => Opt::User,
        'G' => Opt::Get,
        'I' => Opt::Head,
        'L' => Opt::Location,
        'k' => Opt::Insecure,
        'm' => Opt::MaxTime,
        'A' => Opt::UserAgent,
        'e' => Opt::Referer,
        'b' => Opt::Cookie,
        'T' => Opt::UploadFile,
        'o' | 'w' | 'x' | 'U' | 'E' | 'r' | 'z' | 'c' | 'K' | 'D' | 'Y' | 'y' | 'C' | 't' | 'Q'
        | 'P' => Opt::SkipValue,
        _ => Opt::Flag,
    }
}

const fn takes_value(o: Opt) -> bool {
    !matches!(
        o,
        Opt::Get | Opt::Head | Opt::Location | Opt::Insecure | Opt::Flag
    )
}

/// `args[0]` must already be known to be `curl`.
pub(super) fn scan(args: &[String]) -> Result<Parsed, InteropError> {
    let mut p = Parsed::default();
    let mut rest = args.iter().skip(1);
    let mut only_urls = false;
    while let Some(arg) = rest.next() {
        let mut value_for = |flag: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| InteropError::Parse(format!("option {flag} requires a value")))
        };
        if only_urls || arg == "-" || !arg.starts_with('-') {
            p.urls.push(arg.clone());
        } else if arg == "--" {
            only_urls = true;
        } else if let Some(name) = arg.strip_prefix("--") {
            let opt = long(name);
            let value = if takes_value(opt) {
                Some(value_for(arg)?)
            } else {
                None
            };
            apply(&mut p, opt, value);
        } else {
            let chars: Vec<char> = arg.chars().skip(1).collect();
            for (i, &c) in chars.iter().enumerate() {
                let opt = short(c);
                if !takes_value(opt) {
                    apply(&mut p, opt, None);
                    continue;
                }
                let attached: String = chars[i + 1..].iter().collect();
                let value = if attached.is_empty() {
                    value_for(&format!("-{c}"))?
                } else {
                    attached
                };
                apply(&mut p, opt, Some(value));
                break;
            }
        }
    }
    Ok(p)
}

fn apply(p: &mut Parsed, opt: Opt, value: Option<String>) {
    let v = value.unwrap_or_default();
    match opt {
        Opt::Request => p.method = Some(v),
        Opt::Header => p.headers.push(v),
        Opt::Data | Opt::DataBinary => match v.strip_prefix('@') {
            Some(path) => p.data.push(Data::File(path.to_owned())),
            None => p.data.push(Data::Raw(v)),
        },
        Opt::DataRaw => p.data.push(Data::Raw(v)),
        Opt::DataUrlencode => p.data.push(Data::Encode(v)),
        Opt::Json => {
            p.json = true;
            p.data.push(Data::Raw(v));
        }
        Opt::Form => p.form.push((v, false)),
        Opt::FormString => p.form.push((v, true)),
        Opt::User => p.user = Some(v),
        Opt::Get => p.get = true,
        Opt::Url => p.urls.push(v),
        Opt::Head => p.head = true,
        Opt::Location => p.location = true,
        Opt::Insecure => p.insecure = true,
        Opt::MaxTime => p.max_time = Some(v),
        Opt::UserAgent => p.headers.push(format!("User-Agent: {v}")),
        Opt::Referer => p.headers.push(format!("Referer: {v}")),
        Opt::Cookie => p.cookies.push(v),
        Opt::OAuth2Bearer => p.bearer = Some(v),
        Opt::UploadFile => p.upload = Some(v),
        Opt::SkipValue | Opt::Flag => {}
    }
}
