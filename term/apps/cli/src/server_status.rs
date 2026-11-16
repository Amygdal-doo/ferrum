//! `fterm server status` text (which build is running, whether it is older
//! than this `fterm`, whether the LaunchAgent points at another binary) and
//! the `fterm server restart` confirmation.

use ferrumterm_protocol::{AwakeMode, SERVER_REVISION, ServerAge, ServerInfo, build_version};
use std::path::Path;

/// The status report, one line per entry. `local`: the server on this Mac
/// (so `fterm server restart` applies).
pub fn status_lines(info: &ServerInfo, now_ms: u64, local: bool) -> Vec<String> {
    let up = now_ms.saturating_sub(info.started_ms) / 1000;
    let revision = match info.revision {
        0 => "revision unknown".to_owned(),
        r => format!("revision {r}"),
    };
    let mut lines = vec![format!(
        "FerrumTerm server {} ({revision}, pid {}), up {}h{:02}m",
        info.version,
        info.pid,
        up / 3600,
        (up % 3600) / 60
    )];
    if let Some(age) = age_line(info, local) {
        lines.push(format!("  {age}"));
    }
    lines.push(format!(
        "  sessions {}, blocks {}, clients {}",
        info.sessions, info.blocks, info.clients
    ));
    lines.push(format!(
        "  keep-awake {:?}{}",
        info.awake_mode,
        if info.awake { " (holding)" } else { "" }
    ));
    for l in &info.listeners {
        lines.push(format!("  listening {l}"));
    }
    lines
}

/// How the server compares with this `fterm`, when it differs.
pub fn age_line(info: &ServerInfo, local: bool) -> Option<String> {
    let this = format!("{} (revision {SERVER_REVISION})", build_version());
    let restart = if local {
        "run `fterm server restart`"
    } else {
        "restart it on that machine"
    };
    match ferrumterm_protocol::version::compare_with_this_build(&info.version, info.revision) {
        ServerAge::Current => None,
        ServerAge::Older => Some(format!(
            "older than this fterm ({this}); agent status and some features need a restart: {restart}"
        )),
        ServerAge::OlderBuild => Some(format!(
            "an older build than this fterm ({this}); {restart} to pick up its fixes"
        )),
        ServerAge::Newer => Some(format!(
            "newer than this fterm ({this}); update FerrumTerm to match"
        )),
    }
}

/// `--json` status: the server info plus how it compares with this `fterm`.
pub fn status_json(info: &ServerInfo) -> serde_json::Value {
    let mut v = serde_json::to_value(info).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        let age =
            ferrumterm_protocol::version::compare_with_this_build(&info.version, info.revision);
        o.insert("age".into(), serde_json::to_value(age).unwrap_or_default());
        o.insert("fterm_version".into(), build_version().into());
        o.insert("fterm_revision".into(), SERVER_REVISION.into());
    }
    v
}

/// The program a LaunchAgent plist runs (first `ProgramArguments` string).
pub fn plist_program(plist: &str) -> Option<String> {
    let rest = &plist[plist.find("<key>ProgramArguments</key>")?..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    Some(
        rest[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

/// A warning when the LaunchAgent starts a binary other than this `fterm`
/// (e.g. one left behind by an older install). `exe`: this `fterm`;
/// `exists`: whether a path is a file (injected for tests).
pub fn launch_agent_note(
    plist_path: &Path,
    plist: &str,
    exe: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Option<String> {
    let program = plist_program(plist)?;
    let program = Path::new(&program);
    if !exists(program) {
        return Some(format!(
            "LaunchAgent {} runs {}, which no longer exists; run `fterm server install` to point it at this fterm",
            plist_path.display(),
            program.display()
        ));
    }
    (program != exe).then(|| {
        format!(
            "LaunchAgent {} runs {}, not this fterm ({}); at login it starts that build. Run `fterm server install` to update it",
            plist_path.display(),
            program.display(),
            exe.display()
        )
    })
}

/// What `fterm server restart` asks before ending every program.
pub fn restart_warning(counts: Option<(usize, usize)>) -> String {
    match counts {
        Some((sessions, blocks)) => format!(
            "Restarting the FerrumTerm server ends every program running in its terminals ({sessions} {}, {blocks} {}).",
            plural(sessions, "session", "sessions"),
            plural(blocks, "terminal", "terminals")
        ),
        None => "Restarting the FerrumTerm server ends every program running in its terminals."
            .to_owned(),
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// `fterm server start` flags that recreate a running server's listeners
/// and keep-awake mode.
pub fn restart_args(info: &ServerInfo) -> Vec<String> {
    let awake = match info.awake_mode {
        AwakeMode::Auto => "auto",
        AwakeMode::Always => "always",
        AwakeMode::Never => "never",
    };
    let mut args = vec!["--awake".to_owned(), awake.to_owned()];
    for l in &info.listeners {
        if let Some(addr) = l.strip_prefix("tcp://") {
            args.extend(["--listen".to_owned(), addr.to_owned()]);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn info(version: &str, revision: u32) -> ServerInfo {
        ServerInfo {
            version: version.into(),
            revision,
            protocol: ferrumterm_protocol::PROTOCOL_VERSION,
            pid: 42,
            started_ms: 0,
            socket: "/s".into(),
            listeners: vec!["unix:///s".into(), "tcp://100.1.2.3:7397".into()],
            sessions: 2,
            blocks: 5,
            clients: 1,
            awake_mode: AwakeMode::Always,
            awake: false,
        }
    }

    #[test]
    fn status_of_an_old_server_says_restart() {
        let lines = status_lines(&info("0.1.0", 0), 7_380_000, true);
        assert_eq!(
            lines[0],
            "FerrumTerm server 0.1.0 (revision unknown, pid 42), up 2h03m"
        );
        assert!(
            lines[1].starts_with(&format!("  older than this fterm ({}", build_version())),
            "{}",
            lines[1]
        );
        assert!(
            lines[1].ends_with("run `fterm server restart`"),
            "{}",
            lines[1]
        );
        assert_eq!(lines[2], "  sessions 2, blocks 5, clients 1");
    }

    #[test]
    fn status_of_a_current_server_has_no_age_line() {
        let lines = status_lines(&info(build_version(), SERVER_REVISION), 0, true);
        assert_eq!(
            lines[0],
            format!(
                "FerrumTerm server {} (revision {SERVER_REVISION}, pid 42), up 0h00m",
                build_version()
            )
        );
        assert_eq!(lines[1], "  sessions 2, blocks 5, clients 1");
        assert_eq!(
            status_json(&info(build_version(), SERVER_REVISION))["age"],
            "current"
        );
    }

    #[test]
    fn remote_and_newer_servers() {
        let remote = age_line(&info("0.1.0", 0), false).unwrap_or_default();
        assert!(remote.ends_with("restart it on that machine"), "{remote}");
        let newer = age_line(&info("9.0.0", SERVER_REVISION + 1), true).unwrap_or_default();
        assert!(newer.starts_with("newer than this fterm"), "{newer}");
        assert_eq!(status_json(&info("0.1.0", 0))["age"], "older");
    }

    const PLIST: &str = r"<plist><dict>
  <key>Label</key><string>dev.ferrumterm.server</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/Old &amp; Co/fterm</string>
    <string>server</string>
  </array></dict></plist>";

    #[test]
    fn reads_the_launch_agent_program() {
        assert_eq!(
            plist_program(PLIST).as_deref(),
            Some("/Applications/Old & Co/fterm")
        );
        assert_eq!(plist_program("<plist/>"), None);
    }

    #[test]
    fn launch_agent_pointing_elsewhere_is_reported() {
        let plist = Path::new("/L/dev.ferrumterm.server.plist");
        let old = PathBuf::from("/Applications/Old & Co/fterm");
        let here = PathBuf::from("/Applications/FerrumTerm.app/Contents/MacOS/fterm");
        assert_eq!(launch_agent_note(plist, PLIST, &old, |_| true), None);
        let moved = launch_agent_note(plist, PLIST, &here, |_| true).unwrap_or_default();
        assert!(moved.contains("not this fterm"), "{moved}");
        assert!(moved.contains("fterm server install"), "{moved}");
        let gone = launch_agent_note(plist, PLIST, &here, |_| false).unwrap_or_default();
        assert!(gone.contains("no longer exists"), "{gone}");
    }

    #[test]
    fn restart_warning_counts_what_ends() {
        assert_eq!(
            restart_warning(Some((1, 3))),
            "Restarting the FerrumTerm server ends every program running in its terminals (1 session, 3 terminals)."
        );
        assert!(restart_warning(None).ends_with("its terminals."));
    }

    #[test]
    fn restart_keeps_listeners_and_awake_mode() {
        assert_eq!(
            restart_args(&info("0.1.0", 0)),
            ["--awake", "always", "--listen", "100.1.2.3:7397"]
        );
    }
}
