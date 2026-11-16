//! Anonymous usage counts: at most once per UTC day the launcher posts a
//! small JSON ping so the number of active installs can be counted.
//!
//! The payload is exactly [`Ping`]: a random install id made once, the
//! launcher version, OS, CPU architecture and the installed apps with their
//! versions. No paths, user names, host names or e-mail addresses. It is on
//! by default with a one-time notice, and off when
//! [`Settings::share_usage_counts`] is false or the environment sets
//! `FERRUM_NO_ANALYTICS=1` or `DO_NOT_TRACK=1`. What the site does with it:
//! <https://tryferrum.dev/privacy#usage-counts>.

use crate::{AppId, LauncherError, Result, Settings, State};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Where pings go.
pub const PING_URL: &str = "https://tryferrum.dev/api/ping";
/// Overrides [`PING_URL`] (tests, a local copy of the site). Also lets a
/// development build, which has no release version, send pings.
pub const PING_URL_ENV: &str = "FERRUM_PING_URL";
/// What is sent and how to turn it off.
pub const PRIVACY_URL: &str = "https://tryferrum.dev/privacy#usage-counts";
/// The one-time notice shown while sharing is on.
pub const NOTICE: &str = "Ferrum sends an anonymous daily count (a random install ID, versions \
     and OS) so we know how many people use it. Turn it off in Settings › Launcher.";
/// Short, so a slow or unreachable server never holds anything up.
const TIMEOUT: Duration = Duration::from_secs(10);
/// More apps than this is not a real install.
const MAX_APPS: usize = 64;

/// What the launcher remembers about pings, in `state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageState {
    /// Random v4 UUID, made before the first ping and kept from then on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_id: Option<Uuid>,
    /// UTC day of the last ping the server accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ping: Option<NaiveDate>,
    /// Whether [`NOTICE`] was shown.
    pub notice_shown: bool,
}

impl UsageState {
    /// No ping yet today (UTC).
    #[must_use]
    pub fn due(&self, today: NaiveDate) -> bool {
        self.last_ping.is_none_or(|last| last < today)
    }

    /// The install id, made now if there is none yet.
    pub fn install_id(&mut self) -> Uuid {
        *self.install_id.get_or_insert_with(Uuid::new_v4)
    }
}

/// The whole payload. Nothing else is ever sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ping {
    pub install_id: Uuid,
    pub launcher_version: String,
    /// `macos`, `linux` or `windows`.
    pub os: &'static str,
    /// `aarch64` or `x86_64`.
    pub arch: &'static str,
    pub apps: Vec<PingApp>,
}

/// One installed app: its catalog id and active version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PingApp {
    pub id: String,
    pub version: String,
}

/// Whether the environment turns pings off: `FERRUM_NO_ANALYTICS` or
/// `DO_NOT_TRACK` set to `1` (or `true` / `yes`).
pub fn env_opted_out(get: impl Fn(&str) -> Option<String>) -> bool {
    ["FERRUM_NO_ANALYTICS", "DO_NOT_TRACK"].iter().any(|key| {
        get(key)
            .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
    })
}

/// Whether the user lets the launcher send pings at all.
#[must_use]
pub fn enabled(settings: &Settings, env: impl Fn(&str) -> Option<String>) -> bool {
    settings.share_usage_counts && !env_opted_out(env)
}

/// `os` / `arch` names the site accepts; `None` elsewhere.
#[must_use]
pub fn os_name(os: &str) -> Option<&'static str> {
    match os {
        "macos" => Some("macos"),
        "linux" => Some("linux"),
        "windows" => Some("windows"),
        _ => None,
    }
}

#[must_use]
pub fn arch_name(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some("aarch64"),
        "x86_64" => Some("x86_64"),
        _ => None,
    }
}

/// The ping for `state` (installed apps' active versions; the launcher is
/// reported as `launcher_version`, not in `apps`).
#[must_use]
pub fn payload(
    install_id: Uuid,
    launcher_version: &str,
    os: &'static str,
    arch: &'static str,
    state: &State,
) -> Ping {
    let apps = state
        .apps
        .iter()
        .filter(|(id, _)| **id != AppId::LAUNCHER && **id != AppId::LEGACY_VIBE)
        .filter_map(|(id, app)| {
            Some(PingApp {
                id: id.as_str().to_owned(),
                version: app.active.as_ref()?.to_string(),
            })
        })
        .take(MAX_APPS)
        .collect();
    Ping {
        install_id,
        launcher_version: launcher_version.to_owned(),
        os,
        arch,
        apps,
    }
}

/// The ping to send now and where to, or `None` (sharing off, opted out by
/// the environment, already sent today, an unsupported platform, or a
/// development build without [`PING_URL_ENV`]). Makes the install id on
/// first use, so the caller saves `state` when this returns `Some`.
pub fn plan(
    state: &mut State,
    env: impl Fn(&str) -> Option<String>,
    today: NaiveDate,
) -> Option<(String, Ping)> {
    if !enabled(&state.settings, &env) || !state.usage.due(today) {
        return None;
    }
    let override_url = env(PING_URL_ENV)
        .map(|url| url.trim().to_owned())
        .filter(|url| !url.is_empty());
    // Dev builds have no release version; they only ping a test server.
    let version = match (&state.running_launcher, &override_url) {
        (Some(version), _) => version.to_string(),
        (None, Some(_)) => env!("CARGO_PKG_VERSION").to_owned(),
        (None, None) => return None,
    };
    let os = os_name(std::env::consts::OS)?;
    let arch = arch_name(std::env::consts::ARCH)?;
    let id = state.usage.install_id();
    let ping = payload(id, &version, os, arch, state);
    Some((override_url.unwrap_or_else(|| PING_URL.to_owned()), ping))
}

/// Today in UTC, the day the site counts the ping for.
#[must_use]
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// Posts `ping` to `url`. Callers ignore failures (logged at debug).
pub async fn send(url: &str, ping: &Ping) -> Result<()> {
    let http = reqwest::Client::builder()
        .user_agent(crate::github::USER_AGENT)
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| LauncherError::Network(e.to_string()))?;
    let response = http
        .post(url)
        .json(ping)
        .send()
        .await
        .map_err(|e| LauncherError::Network(e.to_string()))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(LauncherError::Http {
            status: status.as_u16(),
            message: "usage ping rejected".into(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::store::{AppState, InstalledVersion};
    use semver::Version;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn state_with_apps() -> State {
        let mut state = State {
            running_launcher: Some(Version::new(0, 9, 1)),
            ..State::default()
        };
        let installed = |v: &str| InstalledVersion {
            version: Version::parse(v).unwrap(),
            tag: format!("x-v{v}"),
            asset: "/Users/someone/secret/path.tar.gz".into(),
            size: 1,
            installed_at: Utc::now(),
            notes: "notes".into(),
        };
        state.apps.insert(
            AppId::GIT,
            AppState {
                installed: vec![installed("0.4.0")],
                active: Some(Version::new(0, 4, 0)),
                ..AppState::default()
            },
        );
        // Downloaded but not active: not reported.
        state.apps.insert(
            AppId::API,
            AppState {
                installed: vec![installed("0.2.0")],
                active: None,
                ..AppState::default()
            },
        );
        state.apps.insert(
            AppId::LAUNCHER,
            AppState {
                active: Some(Version::new(0, 9, 1)),
                ..AppState::default()
            },
        );
        state
    }

    #[test]
    fn payload_has_only_the_documented_fields() {
        let mut state = state_with_apps();
        let (url, ping) = plan(&mut state, env(&[]), day("2026-11-16")).unwrap();
        assert_eq!(url, PING_URL);
        let json = serde_json::to_value(&ping).unwrap();
        let object = json.as_object().unwrap();
        let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["apps", "arch", "install_id", "launcher_version", "os"]
        );
        assert_eq!(json["launcher_version"], "0.9.1");
        assert_eq!(
            json["apps"],
            serde_json::json!([{ "id": "git", "version": "0.4.0" }])
        );
        assert!(["macos", "linux", "windows"].contains(&json["os"].as_str().unwrap()));
        // Nothing personal leaks in, whatever the state holds.
        let text = json.to_string();
        for personal in ["/Users", "someone", "secret", "notes", "x-v"] {
            assert!(!text.contains(personal), "{personal}");
        }
        if let Ok(user) = std::env::var("USER")
            && user.len() > 2
        {
            assert!(!text.contains(&user));
        }
        let id = Uuid::parse_str(json["install_id"].as_str().unwrap()).unwrap();
        assert_eq!(id.get_version_num(), 4);
    }

    #[test]
    fn install_id_is_made_once_and_kept() {
        let mut state = state_with_apps();
        let (_, first) = plan(&mut state, env(&[]), day("2026-11-16")).unwrap();
        assert_eq!(state.usage.install_id, Some(first.install_id));
        state.usage.last_ping = Some(day("2026-11-16"));
        let (_, second) = plan(&mut state, env(&[]), day("2026-11-17")).unwrap();
        assert_eq!(first.install_id, second.install_id);
    }

    #[test]
    fn once_per_utc_day() {
        let mut usage = UsageState::default();
        assert!(usage.due(day("2026-11-16")));
        usage.last_ping = Some(day("2026-11-16"));
        assert!(!usage.due(day("2026-11-16")));
        assert!(usage.due(day("2026-11-17")));
        // A clock set back doesn't send twice.
        assert!(!usage.due(day("2026-11-15")));

        let mut state = state_with_apps();
        state.usage.last_ping = Some(day("2026-11-16"));
        assert!(plan(&mut state, env(&[]), day("2026-11-16")).is_none());
        assert!(plan(&mut state, env(&[]), day("2026-11-17")).is_some());
    }

    #[test]
    fn setting_and_environment_turn_it_off() {
        let today = day("2026-11-16");
        let mut state = state_with_apps();
        state.settings.share_usage_counts = false;
        assert!(plan(&mut state, env(&[]), today).is_none());
        assert_eq!(state.usage.install_id, None, "no id made while off");

        let mut state = state_with_apps();
        for vars in [
            [("FERRUM_NO_ANALYTICS", "1")],
            [("DO_NOT_TRACK", "1")],
            [("DO_NOT_TRACK", "true")],
            [("FERRUM_NO_ANALYTICS", " YES ")],
        ] {
            assert!(env_opted_out(env(&vars)), "{vars:?}");
            assert!(plan(&mut state, env(&vars), today).is_none(), "{vars:?}");
        }
        for vars in [[("DO_NOT_TRACK", "0")], [("FERRUM_NO_ANALYTICS", "")]] {
            assert!(!env_opted_out(env(&vars)), "{vars:?}");
        }
        assert!(plan(&mut state, env(&[("DO_NOT_TRACK", "0")]), today).is_some());
    }

    #[test]
    fn dev_builds_only_ping_an_overridden_url() {
        let today = day("2026-11-16");
        let mut state = state_with_apps();
        state.running_launcher = None;
        assert!(plan(&mut state, env(&[]), today).is_none());
        let (url, ping) = plan(
            &mut state,
            env(&[(PING_URL_ENV, "http://127.0.0.1:9/api/ping")]),
            today,
        )
        .unwrap();
        assert_eq!(url, "http://127.0.0.1:9/api/ping");
        assert_eq!(ping.launcher_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn sharing_defaults_on_for_old_state_files() {
        let settings: Settings = serde_json::from_str(r#"{"repo_owner":"a"}"#).unwrap();
        assert!(settings.share_usage_counts);
        let state: State = serde_json::from_str("{}").unwrap();
        assert_eq!(state.usage, UsageState::default());
    }

    #[test]
    fn platform_names_are_the_ones_the_site_accepts() {
        assert_eq!(os_name("macos"), Some("macos"));
        assert_eq!(os_name("freebsd"), None);
        assert_eq!(arch_name("x86_64"), Some("x86_64"));
        assert_eq!(arch_name("riscv64"), None);
    }

    #[tokio::test]
    async fn sends_json_and_reports_failures() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/api/ping", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let mut bodies = Vec::new();
            for status in ["204 No Content", "400 Bad Request"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0; 8192];
                let mut read = 0;
                // Headers, then the Content-Length body.
                let body = loop {
                    read += socket.read(&mut buf[read..]).await.unwrap();
                    let text = String::from_utf8_lossy(&buf[..read]).into_owned();
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let len: usize = head
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        if body.len() >= len {
                            break body.to_owned();
                        }
                    }
                };
                bodies.push(body);
                socket
                    .write_all(format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
            }
            bodies
        });
        let mut state = state_with_apps();
        let (_, ping) = plan(&mut state, env(&[]), day("2026-11-16")).unwrap();
        send(&url, &ping).await.unwrap();
        assert!(send(&url, &ping).await.is_err());
        let bodies = server.await.unwrap();
        let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
        assert_eq!(sent, serde_json::to_value(&ping).unwrap());
    }
}
