//! Problems in build output: rustc / cargo, the TypeScript compiler, Go,
//! and the common `file:line:col: message` form (gcc, clang, eslint
//! unix format, many others). Only paths that exist are kept, so stray
//! `12:30:45` timestamps don't become problems.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::lsp::Severity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProblem {
    pub path: PathBuf,
    /// 1-based, as compilers print them.
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub message: String,
}

/// Removes ANSI escape sequences (colours, cursor moves, OSC titles).
pub fn strip_ansi(text: &str) -> String {
    static ANSI: OnceLock<Regex> = OnceLock::new();
    let ansi = ANSI.get_or_init(|| {
        Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[@-Z\\-_])")
            .expect("valid regex")
    });
    ansi.replace_all(text, "").replace('\r', "")
}

struct Patterns {
    rust_head: Regex,
    rust_at: Regex,
    tsc_paren: Regex,
    tsc_pretty: Regex,
    generic: Regex,
    go_test: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        rust_head: Regex::new(r"^(error|warning)(\[[A-Za-z0-9]+\])?: (.+)$").expect("regex"),
        rust_at: Regex::new(r"^\s*--> (.+?):(\d+):(\d+)\s*$").expect("regex"),
        tsc_paren: Regex::new(r"^(.+?)\((\d+),(\d+)\): (error|warning) (TS\d+): (.+)$")
            .expect("regex"),
        tsc_pretty: Regex::new(r"^(.+?):(\d+):(\d+) - (error|warning) (TS\d+): (.+)$")
            .expect("regex"),
        generic: Regex::new(
            r"^\s*([^\s:][^:]*?):(\d+):(\d+):?\s*(?:(fatal error|error|warning|note|info)\s*:?\s*)?(.+)$",
        )
        .expect("regex"),
        go_test: Regex::new(r"^\s+([\w./-]+\.go):(\d+): (.+)$").expect("regex"),
    })
}

fn severity(word: &str) -> Severity {
    match word {
        "warning" => Severity::Warning,
        "note" | "info" => Severity::Information,
        _ => Severity::Error,
    }
}

/// Problems found in `output` (ANSI codes allowed); relative paths are
/// resolved against `cwd`. Duplicates are dropped.
pub fn parse_problems(output: &str, cwd: &Path) -> Vec<BuildProblem> {
    let text = strip_ansi(output);
    let p = patterns();
    let resolve = |raw: &str| -> Option<PathBuf> {
        let raw = raw.trim();
        let path = Path::new(raw);
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        full.is_file().then(|| full.canonicalize().unwrap_or(full))
    };
    let number = |s: &str| s.parse::<u32>().unwrap_or(1).max(1);
    let mut out: Vec<BuildProblem> = Vec::new();
    let mut push = |problem: BuildProblem| {
        if !out.contains(&problem) {
            out.push(problem);
        }
    };
    // rustc: the message line comes first, its location on a later line.
    let mut pending: Option<(Severity, String)> = None;
    for line in text.lines() {
        if let Some(c) = p.rust_head.captures(line) {
            // "error[E0308]: x" becomes "x [E0308]".
            let message = match c.get(2) {
                Some(code) => format!("{} {}", &c[3], code.as_str()),
                None => c[3].to_string(),
            };
            pending = Some((severity(&c[1]), message));
            continue;
        }
        if let Some(c) = p.rust_at.captures(line) {
            if let (Some((severity, message)), Some(path)) = (pending.take(), resolve(&c[1])) {
                push(BuildProblem {
                    path,
                    line: number(&c[2]),
                    column: number(&c[3]),
                    severity,
                    message,
                });
            }
            continue;
        }
        if let Some(c) = p
            .tsc_paren
            .captures(line)
            .or_else(|| p.tsc_pretty.captures(line))
        {
            if let Some(path) = resolve(&c[1]) {
                push(BuildProblem {
                    path,
                    line: number(&c[2]),
                    column: number(&c[3]),
                    severity: severity(&c[4]),
                    message: format!("{} ({})", &c[6], &c[5]),
                });
            }
            continue;
        }
        if let Some(c) = p.generic.captures(line)
            && let Some(path) = resolve(&c[1])
        {
            push(BuildProblem {
                path,
                line: number(&c[2]),
                column: number(&c[3]),
                severity: c.get(4).map_or(Severity::Error, |m| severity(m.as_str())),
                message: c[5].trim().to_string(),
            });
            continue;
        }
        if let Some(c) = p.go_test.captures(line)
            && let Some(path) = resolve(&c[1])
        {
            push(BuildProblem {
                path,
                line: number(&c[2]),
                column: 1,
                severity: Severity::Error,
                message: c[3].trim().to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for f in [
            "src/main.rs",
            "src/app.ts",
            "main.go",
            "pkg/x_test.go",
            "a.c",
        ] {
            let path = dir.path().join(f);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(path, "").expect("write");
        }
        dir
    }

    #[test]
    fn strips_colours_and_titles() {
        assert_eq!(
            strip_ansi("\x1b[1m\x1b[38;5;9merror\x1b[0m: x\r\n\x1b]0;title\x07ok"),
            "error: x\nok"
        );
    }

    #[test]
    fn cargo_errors_and_warnings() {
        let dir = project();
        let output = "\x1b[1m\x1b[31merror[E0308]\x1b[0m: mismatched types\n  \x1b[34m-->\x1b[0m src/main.rs:4:13\n   |\nwarning: unused variable: `x`\n --> src/main.rs:2:9\nwarning: `demo` (bin \"demo\") generated 1 warning\nerror: could not compile `demo`\n";
        let problems = parse_problems(output, dir.path());
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].message, "mismatched types [E0308]");
        assert_eq!((problems[0].line, problems[0].column), (4, 13));
        assert_eq!(problems[0].severity, Severity::Error);
        assert_eq!(problems[1].severity, Severity::Warning);
        assert!(problems[1].path.ends_with("src/main.rs"));
    }

    #[test]
    fn typescript_both_formats() {
        let dir = project();
        let output = "src/app.ts(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\nsrc/app.ts:9:1 - warning TS6133: 'x' is declared but never used.\n";
        let problems = parse_problems(output, dir.path());
        assert_eq!(problems.len(), 2);
        assert_eq!(
            problems[0].message,
            "Type 'string' is not assignable to type 'number'. (TS2322)"
        );
        assert_eq!(
            (problems[1].line, problems[1].severity),
            (9, Severity::Warning)
        );
    }

    #[test]
    fn go_gcc_and_go_test() {
        let dir = project();
        let output = "# demo\n./main.go:3:2: undefined: fmt.Printn\na.c:10:5: warning: unused variable 'y' [-Wunused-variable]\n--- FAIL: TestX (0.00s)\n    pkg/x_test.go:12: expected 2, got 3\n12:30:45 server started\nmissing.go:1:1: nope\n";
        let problems = parse_problems(output, dir.path());
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert_eq!(problems[0].message, "undefined: fmt.Printn");
        assert_eq!(problems[1].severity, Severity::Warning);
        assert_eq!(
            problems[1].message,
            "unused variable 'y' [-Wunused-variable]"
        );
        assert_eq!((problems[2].line, problems[2].column), (12, 1));
    }
}
