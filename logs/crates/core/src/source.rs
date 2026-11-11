//! Saved log sources: a file (or glob), a shell command, a Docker
//! container or a compose project's services.

use crate::line::ParseOptions;
use serde::{Deserialize, Serialize};

/// Lines `docker logs` / `docker compose logs` replay before following.
pub const DOCKER_TAIL: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    /// A path or glob, relative to the project unless absolute or `~/…`.
    File { path: String },
    /// Run in the project folder with `$SHELL -l -c`.
    Command { command: String },
    /// `docker logs -f` of a container (name or id).
    Docker { container: String },
    /// `docker compose logs -f` in the project folder; empty = every service.
    Compose {
        #[serde(default)]
        service: String,
    },
}

impl SourceKind {
    /// Short kind name for the UI.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            SourceKind::File { .. } => "File",
            SourceKind::Command { .. } => "Command",
            SourceKind::Docker { .. } => "Container",
            SourceKind::Compose { .. } => "Compose",
        }
    }

    /// The path, command, container or service.
    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            SourceKind::File { path } => path,
            SourceKind::Command { command } => command,
            SourceKind::Docker { container } => container,
            SourceKind::Compose { service } => service,
        }
    }

    /// The shell command a process-backed source runs (None for files).
    #[must_use]
    pub fn command_line(&self) -> Option<String> {
        match self {
            SourceKind::File { .. } => None,
            SourceKind::Command { command } => Some(command.clone()),
            SourceKind::Docker { container } => Some(format!(
                "docker logs -f --timestamps --tail {DOCKER_TAIL} {}",
                shell_quote(container)
            )),
            SourceKind::Compose { service } => {
                let mut line = format!("docker compose logs -f --timestamps --tail {DOCKER_TAIL}");
                for service in service.split_whitespace() {
                    line.push(' ');
                    line.push_str(&shell_quote(service));
                }
                Some(line)
            }
        }
    }

    /// How its output is shaped.
    #[must_use]
    pub fn parse_options(&self) -> ParseOptions {
        match self {
            SourceKind::Docker { .. } => ParseOptions {
                timestamp_prefix: true,
                compose_prefix: false,
            },
            SourceKind::Compose { .. } => ParseOptions {
                timestamp_prefix: true,
                compose_prefix: true,
            },
            _ => ParseOptions::default(),
        }
    }

    /// Whether it ends on its own (a command) or follows until stopped.
    #[must_use]
    pub fn is_process(&self) -> bool {
        !matches!(self, SourceKind::File { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: SourceKind,
    /// Started when the project is opened.
    #[serde(default)]
    pub autostart: bool,
}

impl SourceConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, kind: SourceKind) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            kind,
            autostart: false,
        }
    }

    /// Same kind and target (used to hide suggestions already added).
    #[must_use]
    pub fn same_target(&self, other: &SourceConfig) -> bool {
        self.kind == other.kind
    }
}

/// Single-quotes `s` for a POSIX shell when it isn't a plain word.
#[must_use]
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        return s.to_owned();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_commands_are_quoted() {
        let kind = SourceKind::Docker {
            container: "my app".into(),
        };
        assert_eq!(
            kind.command_line().as_deref(),
            Some("docker logs -f --timestamps --tail 500 'my app'")
        );
        let all = SourceKind::Compose {
            service: String::new(),
        };
        assert_eq!(
            all.command_line().as_deref(),
            Some("docker compose logs -f --timestamps --tail 500")
        );
        let two = SourceKind::Compose {
            service: "web db".into(),
        };
        assert!(
            two.command_line()
                .is_some_and(|c| c.ends_with("500 web db"))
        );
        assert!(two.parse_options().compose_prefix);
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert!(
            SourceKind::File {
                path: "x.log".into()
            }
            .command_line()
            .is_none()
        );
    }

    #[test]
    fn serializes_flat() {
        let source = SourceConfig::new(
            "dev",
            SourceKind::Command {
                command: "npm run dev".into(),
            },
        );
        let json = serde_json::to_value(&source).expect("json");
        assert_eq!(json["type"], "command");
        assert_eq!(json["command"], "npm run dev");
        let back: SourceConfig = serde_json::from_value(json).expect("back");
        assert_eq!(back, source);
        assert!(back.same_target(&source));
    }
}
