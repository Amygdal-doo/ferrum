//! Command line: `ferrumeditor [path[:line[:col]]...]`.

use std::path::{Path, PathBuf};

pub const HELP: &str = "\
FerrumIDE — a small, hackable text editor.

Usage:
  ferrumeditor [options] [path[:line[:column]]...]

Paths may be files or folders. A folder opens in the tree view; files open
in tabs, optionally at a 1-based line and column (e.g. src/main.rs:42:7).
With no paths, the last session is restored.

Options:
  -h, --help       Show this help
  -v, --version    Show the version
  --               Treat every following argument as a path
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    /// Absolute (cwd-joined) path; not required to exist.
    pub path: PathBuf,
    /// 1-based line.
    pub line: Option<usize>,
    /// 1-based column.
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run(Vec<OpenTarget>),
    Help,
    Version,
}

/// Parses arguments (without argv[0]). Unknown `-` options are ignored
/// (e.g. macOS's `-psn_…`).
pub fn parse_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> CliAction {
    let mut targets = Vec::new();
    let mut only_paths = false;
    for arg in args {
        if !only_paths && arg.starts_with('-') && arg.len() > 1 {
            match arg.as_str() {
                "-h" | "--help" => return CliAction::Help,
                "-v" | "-V" | "--version" => return CliAction::Version,
                "--" => only_paths = true,
                _ => {}
            }
            continue;
        }
        if arg.is_empty() {
            continue;
        }
        targets.push(parse_target(&arg, cwd));
    }
    CliAction::Run(targets)
}

/// `file`, `file:10` or `file:10:5`. A path that exists verbatim (even with
/// colons in its name) is taken as-is.
pub fn parse_target(arg: &str, cwd: &Path) -> OpenTarget {
    let whole = absolute(arg, cwd);
    if whole.exists() {
        return OpenTarget {
            path: whole,
            line: None,
            column: None,
        };
    }
    let mut rest = arg;
    let mut numbers = Vec::new();
    while numbers.len() < 2 {
        let Some((head, tail)) = rest.rsplit_once(':') else {
            break;
        };
        match tail.parse::<usize>() {
            Ok(n) if !head.is_empty() && !tail.is_empty() => {
                numbers.push(n);
                rest = head;
            }
            _ => break,
        }
    }
    numbers.reverse();
    if numbers.is_empty() {
        return OpenTarget {
            path: whole,
            line: None,
            column: None,
        };
    }
    OpenTarget {
        path: absolute(rest, cwd),
        line: numbers.first().copied().map(|n| n.max(1)),
        column: numbers.get(1).copied().map(|n| n.max(1)),
    }
}

fn absolute(path: &str, cwd: &Path) -> PathBuf {
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => match home_dir() {
            Some(home) => home.join(rest),
            None => PathBuf::from(path),
        },
        None => PathBuf::from(path),
    };
    if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> PathBuf {
        PathBuf::from("/work")
    }

    fn target(path: &str, line: Option<usize>, column: Option<usize>) -> OpenTarget {
        OpenTarget {
            path: PathBuf::from(path),
            line,
            column,
        }
    }

    #[test]
    fn plain_paths() {
        assert_eq!(
            parse_target("a.rs", &cwd()),
            target("/work/a.rs", None, None)
        );
        assert_eq!(
            parse_target("/abs/b.rs", &cwd()),
            target("/abs/b.rs", None, None)
        );
    }

    #[test]
    fn line_and_column() {
        assert_eq!(
            parse_target("a.rs:10", &cwd()),
            target("/work/a.rs", Some(10), None)
        );
        assert_eq!(
            parse_target("src/a.rs:10:5", &cwd()),
            target("/work/src/a.rs", Some(10), Some(5))
        );
        assert_eq!(
            parse_target("a.rs:0", &cwd()),
            target("/work/a.rs", Some(1), None)
        );
    }

    #[test]
    fn non_numeric_suffix_is_part_of_path() {
        assert_eq!(parse_target("a:b", &cwd()), target("/work/a:b", None, None));
        assert_eq!(
            parse_target("a.rs:", &cwd()),
            target("/work/a.rs:", None, None)
        );
        assert_eq!(
            parse_target("x:1:2:3", &cwd()),
            target("/work/x:1", Some(2), Some(3))
        );
    }

    #[test]
    fn existing_path_with_colon_is_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let name = "odd:12";
        if std::fs::write(dir.path().join(name), "").is_err() {
            return; // File systems without ':' support.
        }
        let parsed = parse_target(name, dir.path());
        assert_eq!(parsed.path, dir.path().join(name));
        assert_eq!(parsed.line, None);
    }

    #[test]
    fn args_and_flags() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(parse_args(args(&["--help", "a"]), &cwd()), CliAction::Help);
        assert_eq!(parse_args(args(&["-v"]), &cwd()), CliAction::Version);
        assert_eq!(
            parse_args(args(&["-psn_0_123", "a.rs:3", "dir"]), &cwd()),
            CliAction::Run(vec![
                target("/work/a.rs", Some(3), None),
                target("/work/dir", None, None)
            ])
        );
        assert_eq!(
            parse_args(args(&["--", "--help"]), &cwd()),
            CliAction::Run(vec![target("/work/--help", None, None)])
        );
        assert_eq!(parse_args(Vec::new(), &cwd()), CliAction::Run(Vec::new()));
    }
}
