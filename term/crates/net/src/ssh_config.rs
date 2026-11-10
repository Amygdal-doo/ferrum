//! Read-only parser for `~/.ssh/config`: the concrete `Host` aliases (no
//! wildcards or negations) with the few options FerrumTerm shows. Connecting
//! to one runs `ssh <alias>`, so ssh itself applies the full config.
//!
//! `Include` is followed (relative to `~/.ssh`, `*` / `?` in the last path
//! component); `Match` blocks are skipped. The file is never written.

use std::path::{Path, PathBuf};

/// One `Host` alias from the config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigHost {
    pub alias: String,
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

/// Maximum `Include` nesting (ssh uses 16).
const MAX_DEPTH: usize = 16;

fn is_pattern(alias: &str) -> bool {
    alias.contains(['*', '?', '!']) || alias.is_empty()
}

/// Splits `Key value`, `Key=value`, `Key = "quoted value"`.
fn split_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = line[..end].to_ascii_lowercase();
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in rest.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    args.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            '#' if !quoted && !any => break,
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        args.push(cur);
    }
    Some((key, args))
}

/// Simple glob for `Include`: `*` and `?`.
fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

/// Files an `Include` argument names, sorted (as ssh does).
fn include_paths(arg: &str, home: &Path) -> Vec<PathBuf> {
    let path = expand_home(arg, home);
    let path = if path.is_absolute() {
        path
    } else {
        home.join(".ssh").join(path)
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains(['*', '?']) {
        return vec![path];
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| glob_match(&name, &e.file_name().to_string_lossy()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

struct Parser<'a> {
    home: &'a Path,
    hosts: Vec<ConfigHost>,
}

impl Parser<'_> {
    /// Parses `text`; `current` holds the indexes of hosts the open `Host`
    /// block applies to (empty inside `Match` or wildcard-only blocks).
    fn parse(&mut self, text: &str, depth: usize) {
        let mut current: Vec<usize> = Vec::new();
        for line in text.lines() {
            let Some((key, args)) = split_line(line) else {
                continue;
            };
            let first = args.first().cloned().unwrap_or_default();
            match key.as_str() {
                "host" => {
                    current.clear();
                    for alias in args.iter().filter(|a| !is_pattern(a)) {
                        let idx = match self.hosts.iter().position(|h| &h.alias == alias) {
                            Some(i) => i,
                            None => {
                                self.hosts.push(ConfigHost {
                                    alias: alias.clone(),
                                    ..ConfigHost::default()
                                });
                                self.hosts.len() - 1
                            }
                        };
                        current.push(idx);
                    }
                }
                "match" => current.clear(),
                "include" if depth < MAX_DEPTH => {
                    for arg in &args {
                        for path in include_paths(arg, self.home) {
                            if let Ok(t) = std::fs::read_to_string(&path) {
                                self.parse(&t, depth + 1);
                            }
                        }
                    }
                }
                _ => {
                    // First value wins in ssh_config.
                    for &i in &current {
                        let h = &mut self.hosts[i];
                        match key.as_str() {
                            "hostname" if h.hostname.is_none() => h.hostname = Some(first.clone()),
                            "user" if h.user.is_none() => h.user = Some(first.clone()),
                            "port" if h.port.is_none() => h.port = first.parse().ok(),
                            "identityfile" if h.identity_file.is_none() => {
                                h.identity_file = Some(first.clone());
                            }
                            "proxyjump" if h.proxy_jump.is_none() => {
                                h.proxy_jump = Some(first.clone());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

/// Parses config text; `home` resolves `~` and relative `Include`s.
#[must_use]
pub fn parse(text: &str, home: &Path) -> Vec<ConfigHost> {
    let mut p = Parser {
        home,
        hosts: Vec::new(),
    };
    p.parse(text, 0);
    p.hosts
}

/// The user's `~/.ssh/config` hosts (empty if there is none).
#[must_use]
pub fn load_user() -> Vec<ConfigHost> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    std::fs::read_to_string(home.join(".ssh").join("config"))
        .map(|t| parse(&t, &home))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
# global defaults
Host *
    ServerAliveInterval 30
    User everyone

Host prod prod-alias *.internal !bastion
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_prod
    ProxyJump bastion

Host=bastion
  HostName="bastion.example.com"   # trailing comment
  User ops
  User ignored-second-value

Match host foo exec "true"
  User matched

Host "quoted name"
  Port 22
"#;

    #[test]
    fn parses_hosts_skipping_patterns_and_match_blocks() {
        let hosts = parse(CONFIG, Path::new("/home/me"));
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, ["prod", "prod-alias", "bastion", "quoted name"]);
        let prod = &hosts[0];
        assert_eq!(prod.hostname.as_deref(), Some("10.0.0.5"));
        assert_eq!(prod.user.as_deref(), Some("deploy"));
        assert_eq!(prod.port, Some(2222));
        assert_eq!(prod.identity_file.as_deref(), Some("~/.ssh/id_prod"));
        assert_eq!(prod.proxy_jump.as_deref(), Some("bastion"));
        assert_eq!(hosts[1].hostname.as_deref(), Some("10.0.0.5"));
        let bastion = &hosts[2];
        assert_eq!(bastion.hostname.as_deref(), Some("bastion.example.com"));
        assert_eq!(bastion.user.as_deref(), Some("ops"), "first value wins");
        assert_eq!(hosts[3].port, Some(22));
    }

    #[test]
    fn follows_includes() {
        let dir = tempfile::tempdir().expect("tmp");
        let ssh = dir.path().join(".ssh");
        std::fs::create_dir_all(ssh.join("conf.d")).expect("mkdir");
        std::fs::write(
            ssh.join("conf.d/a.conf"),
            "Host alpha\n HostName a.example\n",
        )
        .expect("write");
        std::fs::write(ssh.join("conf.d/b.conf"), "Host beta\n").expect("write");
        std::fs::write(ssh.join("conf.d/skip.txt"), "Host nope\n").expect("write");
        std::fs::write(ssh.join("extra"), "Host gamma\n Include extra\n").expect("write");
        let hosts = parse(
            "Include conf.d/*.conf\nInclude ~/.ssh/extra\nHost delta\n",
            dir.path(),
        );
        let names: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(names, ["alpha", "beta", "gamma", "delta"]);
        assert_eq!(hosts[0].hostname.as_deref(), Some("a.example"));
    }

    #[test]
    fn tolerates_junk() {
        assert!(parse("", Path::new("/")).is_empty());
        let hosts = parse(
            "Port abc\nHost x\nPort notanumber\n\tUser\n",
            Path::new("/"),
        );
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].port, None);
        assert!(glob_match("*.conf", "a.conf") && !glob_match("*.conf", "a.txt"));
        assert!(glob_match("a?c", "abc"));
    }
}
