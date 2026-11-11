//! `.env` files: parsing (comments, `export `, single/double/backtick
//! quotes, escapes, multi-line quoted values), writing, finding them in a
//! project folder and checking `.gitignore`.

use std::path::{Path, PathBuf};

use crate::model::{Variable, is_valid_key};

/// The result of [`parse`]: variables in file order (a repeated key keeps
/// its first position and its last value) and per-line warnings.
#[derive(Debug, Default)]
pub struct ParsedEnv {
    pub variables: Vec<Variable>,
    pub warnings: Vec<String>,
}

/// Parses `.env` text. Never fails: unreadable lines become warnings.
/// A `# comment` line directly above a variable becomes its comment.
#[must_use]
pub fn parse(text: &str) -> ParsedEnv {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = ParsedEnv::default();
    let mut pending_comment: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let number = i + 1;
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() {
            pending_comment.clear();
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            pending_comment.push(comment.trim().to_string());
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .or_else(|| line.strip_prefix("export\t"))
            .map_or(line, str::trim_start);
        let Some((key, rest)) = line.split_once('=') else {
            out.warnings
                .push(format!("line {number}: expected KEY=value"));
            pending_comment.clear();
            continue;
        };
        let key = key.trim();
        if !is_valid_key(key) {
            out.warnings
                .push(format!("line {number}: \"{key}\" is not a valid name"));
            pending_comment.clear();
            continue;
        }
        let rest = rest.trim_start();
        let (value, inline_comment) = match rest.chars().next() {
            Some(quote @ ('"' | '\'' | '`')) => {
                match read_quoted(&rest[1..], quote, &lines, &mut i) {
                    Some((value, after)) => (value, comment_after(after)),
                    None => {
                        out.warnings
                            .push(format!("line {number}: unterminated {quote} quote"));
                        (rest.to_string(), None)
                    }
                }
            }
            _ => split_unquoted(rest),
        };
        let mut var = Variable::new(key, value);
        var.comment = inline_comment.unwrap_or_else(|| pending_comment.join(" "));
        pending_comment.clear();
        match out.variables.iter_mut().find(|v| v.key == var.key) {
            Some(slot) => *slot = var,
            None => out.variables.push(var),
        }
    }
    out
}

/// Reads a quoted value starting after the opening quote, continuing onto
/// following lines (advancing `next_line`) until the closing quote.
/// Returns the value and the text after the closing quote.
fn read_quoted<'a>(
    first: &'a str,
    quote: char,
    lines: &[&'a str],
    next_line: &mut usize,
) -> Option<(String, &'a str)> {
    let mut value = String::new();
    let mut current = first;
    loop {
        let mut chars = current.char_indices();
        while let Some((index, c)) = chars.next() {
            if c == quote {
                return Some((value, &current[index + 1..]));
            }
            if c == '\\' && quote == '"' {
                match chars.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 'r')) => value.push('\r'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, e @ ('\\' | '"' | '$' | '\''))) => value.push(e),
                    Some((_, other)) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => value.push('\\'),
                }
            } else {
                value.push(c);
            }
        }
        let line = lines.get(*next_line)?;
        *next_line += 1;
        value.push('\n');
        current = line;
    }
}

fn comment_after(after: &str) -> Option<String> {
    after
        .trim()
        .strip_prefix('#')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
}

/// `value # comment`: a `#` after whitespace starts a comment.
fn split_unquoted(rest: &str) -> (String, Option<String>) {
    if let Some(comment) = rest.strip_prefix('#') {
        return (String::new(), Some(comment.trim().to_string()));
    }
    let cut = rest
        .char_indices()
        .find(|&(i, c)| c == '#' && rest[..i].ends_with([' ', '\t']));
    match cut {
        Some((i, _)) => (
            rest[..i].trim_end().to_string(),
            Some(rest[i + 1..].trim().to_string()).filter(|c| !c.is_empty()),
        ),
        None => (rest.trim_end().to_string(), None),
    }
}

/// Writes variables as `.env` text that [`parse`] reads back unchanged.
#[must_use]
pub fn to_dotenv(variables: &[Variable]) -> String {
    let mut out = String::new();
    for var in variables {
        for line in var.comment.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str("# ");
            out.push_str(line.trim());
            out.push('\n');
        }
        out.push_str(&var.key);
        out.push('=');
        out.push_str(&quote(&var.value));
        out.push('\n');
    }
    out
}

fn quote(value: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./:@,+%=~^".contains(c);
    if value.chars().all(plain) {
        return value.to_string();
    }
    if !value.contains(['\'', '\n', '\r']) {
        return format!("'{value}'");
    }
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '$' => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `.env` and `.env.*` files directly in `dir`, `.env` first.
#[must_use]
pub fn find_env_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy())
                .is_some_and(|n| n == ".env" || n.starts_with(".env."))
        })
        .collect();
    files.sort_by_key(|p| {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (name != ".env", name)
    });
    files
}

/// The environment a file imports into: `.env` → `development`,
/// `.env.production` → `production`, `.env.local` → `local`.
#[must_use]
pub fn environment_for_file(file_name: &str) -> String {
    match file_name.strip_prefix(".env.") {
        Some(rest) if !rest.is_empty() => rest.to_string(),
        _ => "development".to_string(),
    }
}

/// The file an environment is usually written to: `development` → `.env`,
/// otherwise `.env.<name>`.
#[must_use]
pub fn file_for_environment(environment: &str) -> String {
    if environment.eq_ignore_ascii_case("development") {
        ".env".to_string()
    } else {
        format!(".env.{}", environment.to_lowercase().replace(' ', "-"))
    }
}

/// Whether `.gitignore` text ignores a file called `file_name` in the same
/// folder. Handles plain names, `/name`, `*`/`?` globs and `!` negation;
/// patterns with a `/` inside are not considered.
#[must_use]
pub fn gitignore_covers(gitignore: &str, file_name: &str) -> bool {
    let mut ignored = false;
    for line in gitignore.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (negate, pattern) = match line.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, line),
        };
        let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
        if pattern.contains('/') {
            continue;
        }
        if glob(pattern.as_bytes(), file_name.as_bytes()) {
            ignored = !negate;
        }
    }
    ignored
}

fn glob(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob(&pattern[1..], name) || (!name.is_empty() && glob(pattern, &name[1..]))
        }
        (Some(b'?'), Some(_)) => glob(&pattern[1..], &name[1..]),
        (Some(p), Some(n)) if p == n => glob(&pattern[1..], &name[1..]),
        _ => false,
    }
}

/// Appends `file_name` to `dir/.gitignore` unless already covered.
/// Returns whether it was added.
pub fn add_to_gitignore(dir: &Path, file_name: &str) -> std::io::Result<bool> {
    let path = dir.join(".gitignore");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if gitignore_covers(&existing, file_name) {
        return Ok(false);
    }
    let mut text = existing;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(file_name);
    text.push('\n');
    std::fs::write(&path, text)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(parsed: &'a ParsedEnv, key: &str) -> &'a str {
        &parsed
            .variables
            .iter()
            .find(|v| v.key == key)
            .expect(key)
            .value
    }

    #[test]
    fn parses_the_common_forms() {
        let text = "\
# Database
DATABASE_URL=postgres://u:p@localhost/db
export API_KEY = abc123   # inline note
EMPTY=
HASH=a#b
SINGLE='keep $HOME \\n literal'
DOUBLE=\"line1\\nline2 \\\"q\\\" \\$X\"
BACKTICK=`it's`
MULTI=\"-----BEGIN KEY-----
abc
-----END KEY-----\"
CRLF=yes\r
not a variable
9BAD=x
DUP=first
DUP=second
";
        let parsed = parse(text);
        assert_eq!(
            value(&parsed, "DATABASE_URL"),
            "postgres://u:p@localhost/db"
        );
        assert_eq!(parsed.variables[0].comment, "Database");
        assert_eq!(value(&parsed, "API_KEY"), "abc123");
        assert_eq!(parsed.variables[1].comment, "inline note");
        assert_eq!(value(&parsed, "EMPTY"), "");
        assert_eq!(value(&parsed, "HASH"), "a#b");
        assert_eq!(value(&parsed, "SINGLE"), "keep $HOME \\n literal");
        assert_eq!(value(&parsed, "DOUBLE"), "line1\nline2 \"q\" $X");
        assert_eq!(value(&parsed, "BACKTICK"), "it's");
        assert_eq!(
            value(&parsed, "MULTI"),
            "-----BEGIN KEY-----\nabc\n-----END KEY-----"
        );
        assert_eq!(value(&parsed, "CRLF"), "yes");
        assert_eq!(value(&parsed, "DUP"), "second");
        assert_eq!(parsed.warnings.len(), 2, "{:?}", parsed.warnings);
        let keys: Vec<_> = parsed.variables.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(keys.last(), Some(&"DUP"));
    }

    #[test]
    fn unterminated_quotes_warn() {
        let parsed = parse("A=\"open\nB=2\n");
        assert_eq!(parsed.warnings.len(), 1);
        assert_eq!(value(&parsed, "A"), "\"open");
    }

    #[test]
    fn writing_round_trips() {
        let mut vars = vec![
            Variable::new("PLAIN", "abc-123_./:@"),
            Variable::new("SPACES", "hello world $HOME"),
            Variable::new("QUOTE", "it's \"x\" \\ $y\tz"),
            Variable::new("MULTI", "a\nb\r\nc"),
            Variable::new("EMPTY", ""),
            Variable::new("HASH", "a #b"),
        ];
        vars[0].comment = "first".into();
        let text = to_dotenv(&vars);
        assert!(text.starts_with("# first\nPLAIN=abc-123_./:@\n"));
        let back = parse(&text);
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        assert_eq!(back.variables, vars);
    }

    #[test]
    fn finds_files_and_names_environments() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in [".env.production", ".env", ".env.local", ".envrc", "env"] {
            std::fs::write(dir.path().join(name), "A=1\n").expect("write");
        }
        let names: Vec<String> = find_env_files(dir.path())
            .iter()
            .map(|p| p.file_name().expect("name").to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [".env", ".env.local", ".env.production"]);
        assert_eq!(environment_for_file(".env"), "development");
        assert_eq!(environment_for_file(".env.production"), "production");
        assert_eq!(file_for_environment("development"), ".env");
        assert_eq!(file_for_environment("Staging"), ".env.staging");
    }

    #[test]
    fn gitignore_matching() {
        assert!(gitignore_covers(".env\n", ".env"));
        assert!(gitignore_covers("/.env.local\n", ".env.local"));
        assert!(gitignore_covers(".env*\n", ".env.production"));
        assert!(gitignore_covers("*.local\n", ".env.local"));
        assert!(!gitignore_covers(".env\n", ".env.local"));
        assert!(!gitignore_covers(".env*\n!.env.example\n", ".env.example"));
        assert!(!gitignore_covers("config/.env\n", ".env"));
        assert!(!gitignore_covers("# .env\n", ".env"));

        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(".gitignore"), "target").expect("write");
        assert!(add_to_gitignore(dir.path(), ".env").expect("add"));
        assert!(!add_to_gitignore(dir.path(), ".env").expect("again"));
        let text = std::fs::read_to_string(dir.path().join(".gitignore")).expect("read");
        assert_eq!(text, "target\n.env\n");
    }
}
