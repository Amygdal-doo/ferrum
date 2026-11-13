//! @-mentions: files, folders, an editor selection, the git diff, terminal
//! output (handed over by the host) and URLs, resolved into
//! [`Attachment`]s sent with the user's message. Also the picker's
//! suggestions and parsing `@path` tokens typed in the composer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::thread::Attachment;
use crate::tools::Sandbox;

const FILE_LIMIT: usize = 100 * 1024;
const FOLDER_LIMIT: usize = 300;
const URL_LIMIT: usize = 40 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mention {
    File {
        path: String,
    },
    Folder {
        path: String,
    },
    Selection {
        path: String,
        start_line: usize,
        end_line: usize,
        text: String,
    },
    GitDiff,
    Terminal {
        title: String,
        text: String,
    },
    Url {
        url: String,
    },
}

impl Mention {
    /// The chip text: `src/main.rs`, `main.rs:3-9`, `git diff`.
    pub fn label(&self) -> String {
        match self {
            Mention::File { path } => path.clone(),
            Mention::Folder { path } => format!("{}/", path.trim_end_matches('/')),
            Mention::Selection {
                path,
                start_line,
                end_line,
                ..
            } => format!("{path}:{start_line}-{end_line}"),
            Mention::GitDiff => "git diff".into(),
            Mention::Terminal { title, .. } => format!("terminal: {title}"),
            Mention::Url { url } => url.clone(),
        }
    }

    /// Project files it names (relative), for rule globs.
    pub fn files(&self) -> Vec<PathBuf> {
        match self {
            Mention::File { path } | Mention::Selection { path, .. } => vec![PathBuf::from(path)],
            Mention::Folder { path } => vec![PathBuf::from(path).join("_")],
            _ => Vec::new(),
        }
    }
}

/// The text a mention contributes. Errors become the attachment text so
/// the model (and the user) see what went wrong.
pub async fn resolve(mention: &Mention, sandbox: &Sandbox, http: &reqwest::Client) -> Attachment {
    let label = mention.label();
    let text = match mention {
        Mention::File { path } => read_file(sandbox, path),
        Mention::Folder { path } => list_folder(sandbox, path),
        Mention::Selection {
            path,
            start_line,
            end_line,
            text,
        } => Ok(format!(
            "{path} lines {start_line}-{end_line}:\n```\n{text}\n```"
        )),
        Mention::GitDiff => git_diff(sandbox.root()),
        Mention::Terminal { text, .. } => Ok(crate::tools::runner::tail(text, FILE_LIMIT)),
        Mention::Url { url } => fetch(http, url).await,
    };
    Attachment {
        label,
        text: text.unwrap_or_else(|e| format!("(could not include: {e})")),
    }
}

fn read_file(sandbox: &Sandbox, path: &str) -> Result<String, String> {
    let abs = sandbox.resolve(path).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&abs).map_err(|e| e.to_string())?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err("binary file".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut end = text.len().min(FILE_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let more = if end < text.len() {
        "\n…(truncated)"
    } else {
        ""
    };
    Ok(format!("```{path}\n{}{more}\n```", &text[..end]))
}

fn list_folder(sandbox: &Sandbox, path: &str) -> Result<String, String> {
    let abs = sandbox.resolve(path).map_err(|e| e.to_string())?;
    if !abs.is_dir() {
        return Err("not a folder".into());
    }
    let mut out = String::new();
    for (i, entry) in ignore::WalkBuilder::new(&abs)
        .max_depth(Some(3))
        .build()
        .flatten()
        .filter(|e| e.depth() > 0)
        .enumerate()
    {
        if i >= FOLDER_LIMIT {
            out.push_str("…\n");
            break;
        }
        out.push_str(&sandbox.display(entry.path()));
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            out.push('/');
        }
        out.push('\n');
    }
    Ok(out)
}

fn git_diff(root: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["diff", "--no-color", "HEAD"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(if diff.trim().is_empty() {
        "No uncommitted changes.".into()
    } else {
        crate::tools::runner::tail(&diff, FILE_LIMIT)
    })
}

async fn fetch(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only http(s) URLs".into());
    }
    let response = http
        .get(parsed)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.contains("html"));
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(2 * 1024 * 1024)]).to_string();
    let text = if html { html_to_text(&text) } else { text };
    let mut end = text.len().min(URL_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(text[..end].to_string())
}

/// Crude HTML → text: drops scripts, styles and tags, decodes a few
/// entities, collapses blank space.
pub fn html_to_text(html: &str) -> String {
    let drop = regex::Regex::new(
        r"(?is)<(script|style|noscript|svg)[^>]*>.*?</(script|style|noscript|svg)>",
    )
    .map(|r| r.replace_all(html, " ").to_string())
    .unwrap_or_else(|_| html.to_string());
    let blocks = regex::Regex::new(r"(?i)<(br|/p|/div|/h[1-6]|/li|/tr)[^>]*>")
        .map(|r| r.replace_all(&drop, "\n").to_string())
        .unwrap_or(drop);
    let tags = regex::Regex::new(r"(?s)<[^>]+>")
        .map(|r| r.replace_all(&blocks, " ").to_string())
        .unwrap_or(blocks);
    let decoded = tags
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut out = String::new();
    for line in decoded.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !line.is_empty() {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// A row in the @-mention picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub mention: Mention,
    pub label: String,
    pub detail: String,
}

/// Picker rows for `query`: special entries, then files and folders whose
/// path contains the query's characters in order (best first).
pub fn suggest(root: &Path, query: &str, limit: usize) -> Vec<Suggestion> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    if q.starts_with("http://") || q.starts_with("https://") {
        out.push(Suggestion {
            mention: Mention::Url {
                url: query.trim().into(),
            },
            label: query.trim().into(),
            detail: "Fetch URL".into(),
        });
        return out;
    }
    if "diff".starts_with(&q) || "git diff".starts_with(&q) {
        out.push(Suggestion {
            mention: Mention::GitDiff,
            label: "git diff".into(),
            detail: "Uncommitted changes".into(),
        });
    }
    let mut scored: Vec<(i64, Suggestion)> = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .flatten()
        .filter(|e| e.depth() > 0)
        .take(50_000)
    {
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let Some(score) = fuzzy_score(&rel.to_lowercase(), &q) else {
            continue;
        };
        let dir = entry.file_type().is_some_and(|t| t.is_dir());
        let mention = if dir {
            Mention::Folder { path: rel.clone() }
        } else {
            Mention::File { path: rel.clone() }
        };
        scored.push((
            score,
            Suggestion {
                label: mention.label(),
                detail: if dir { "Folder".into() } else { "File".into() },
                mention,
            },
        ));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    out.extend(scored.into_iter().map(|(_, s)| s));
    out.truncate(limit);
    out
}

/// Subsequence match score: consecutive runs and matches in the file
/// name score higher; shorter paths win ties. `None` when no match.
pub fn fuzzy_score(path: &str, query: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(-(path.len() as i64));
    }
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    let chars: Vec<char> = path.chars().collect();
    let mut score = 0i64;
    let mut at = 0usize;
    let mut last: Option<usize> = None;
    let mut byte = 0usize;
    let mut offsets = Vec::with_capacity(chars.len());
    for c in &chars {
        offsets.push(byte);
        byte += c.len_utf8();
    }
    for q in query.chars() {
        let found = (at..chars.len()).find(|&i| chars[i] == q)?;
        score += 10;
        if last == Some(found.wrapping_sub(1)) {
            score += 15;
        }
        if offsets[found] >= name_start {
            score += 5;
        }
        last = Some(found);
        at = found + 1;
    }
    if path[name_start..].starts_with(query) {
        score += 50;
    }
    Some(score * 10 - path.len() as i64)
}

/// `@tokens` typed in a message: `@diff`, `@https://…`, or an existing
/// project path.
pub fn parse_inline(root: &Path, text: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let Some(token) = word.strip_prefix('@') else {
            continue;
        };
        let token = token.trim_end_matches([',', '.', ';', ':', ')', '?', '!']);
        let mention = if token == "diff" || token == "git-diff" {
            Some(Mention::GitDiff)
        } else if token.starts_with("http://") || token.starts_with("https://") {
            Some(Mention::Url { url: token.into() })
        } else if !token.is_empty() && !token.contains("..") {
            let path = root.join(token);
            if path.is_dir() {
                Some(Mention::Folder { path: token.into() })
            } else if path.is_file() {
                Some(Mention::File { path: token.into() })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(m) = mention
            && !out.contains(&m)
        {
            out.push(m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_inline_mentions_and_resolution() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/ui")).expect("mkdir");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write");
        std::fs::write(root.join("src/ui/mod.rs"), "").expect("write");
        std::fs::write(root.join("README.md"), "hi").expect("write");

        let found = suggest(root, "main", 5);
        assert_eq!(found[0].label, "src/main.rs");
        let diff = suggest(root, "dif", 5);
        assert_eq!(diff[0].mention, Mention::GitDiff);
        assert!(suggest(root, "zzz", 5).is_empty());
        assert_eq!(
            suggest(root, "https://example.com", 5)[0].mention,
            Mention::Url {
                url: "https://example.com".into()
            }
        );

        let inline = parse_inline(root, "look at @src/main.rs, @src/ui and @diff @nope @../x");
        assert_eq!(
            inline,
            [
                Mention::File {
                    path: "src/main.rs".into()
                },
                Mention::Folder {
                    path: "src/ui".into()
                },
                Mention::GitDiff
            ]
        );

        let sandbox = Sandbox::new(root).expect("sandbox");
        let http = reqwest::Client::new();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let file = rt.block_on(resolve(&inline[0], &sandbox, &http));
        assert!(file.text.contains("fn main()"), "{}", file.text);
        let folder = rt.block_on(resolve(&inline[1], &sandbox, &http));
        assert_eq!(folder.text, "src/ui/mod.rs\n");
        let outside = rt.block_on(resolve(
            &Mention::File {
                path: "../../etc/passwd".into(),
            },
            &sandbox,
            &http,
        ));
        assert!(
            outside.text.starts_with("(could not include"),
            "{}",
            outside.text
        );
        let bad_url = rt.block_on(resolve(
            &Mention::Url {
                url: "file:///etc/passwd".into(),
            },
            &sandbox,
            &http,
        ));
        assert!(bad_url.text.contains("only http(s)"));
    }

    #[test]
    fn html_is_reduced_to_text() {
        let text = html_to_text(
            "<html><head><style>a{}</style><script>x()</script></head><body><h1>Title</h1><p>One &amp; two</p></body></html>",
        );
        assert_eq!(text, "Title\nOne & two\n");
    }
}
