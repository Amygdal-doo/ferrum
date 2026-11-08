//! Syntax highlighting for diffs (syntect, pure-Rust regex engine), cached.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
/// Highlighted lines of one hunk.
pub type Highlighted = Arc<Vec<Vec<Token>>>;

static CACHE: LazyLock<Mutex<HashMap<u64, Highlighted>>> = LazyLock::new(Mutex::default);

/// Lines longer than this are not highlighted (minified files).
const MAX_LINE: usize = 2000;

/// A run of text and its CSS colour (`None` = inherit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub color: Option<String>,
}

fn theme(dark: bool) -> Option<&'static Theme> {
    let name = if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    };
    THEMES.themes.get(name)
}

/// Highlights consecutive `lines` of `path` (one hunk). Unknown file types and
/// failures fall back to plain tokens.
pub fn highlight(path: &str, lines: &[&str], dark: bool) -> Highlighted {
    let mut h = DefaultHasher::new();
    (path, lines, dark).hash(&mut h);
    let key = h.finish();
    if let Some(hit) = CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return hit.clone();
    }
    let plain = |l: &str| {
        vec![Token {
            text: l.to_owned(),
            color: None,
        }]
    };
    let ext = path.rsplit('.').next().unwrap_or("");
    let name = path.rsplit('/').next().unwrap_or(path);
    let syntax = SYNTAXES
        .find_syntax_by_extension(ext)
        .or_else(|| SYNTAXES.find_syntax_by_extension(name));
    let out: Vec<Vec<Token>> = match (syntax, theme(dark)) {
        (Some(syntax), Some(theme)) => {
            let mut hl = HighlightLines::new(syntax, theme);
            lines
                .iter()
                .map(|l| {
                    if l.len() > MAX_LINE {
                        return plain(l);
                    }
                    let with_nl = format!("{l}\n");
                    match hl.highlight_line(&with_nl, &SYNTAXES) {
                        Ok(ranges) => ranges
                            .into_iter()
                            .map(|(style, text)| {
                                let c = style.foreground;
                                Token {
                                    text: text.trim_end_matches('\n').to_owned(),
                                    color: Some(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
                                }
                            })
                            .filter(|t| !t.text.is_empty())
                            .collect(),
                        Err(_) => plain(l),
                    }
                })
                .collect()
        }
        _ => lines.iter().map(|l| plain(l)).collect(),
    };
    let out = Arc::new(out);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() > 400 {
        cache.clear();
    }
    cache.insert(key, out.clone());
    out
}

#[cfg(test)]
mod tests {
    use super::highlight;

    #[test]
    fn highlights_known_and_plain_unknown() {
        let rs = highlight("src/main.rs", &["fn main() {", "    let x = 1;", "}"], true);
        assert_eq!(rs.len(), 3);
        assert!(rs[0].len() > 1, "keywords split into tokens");
        assert!(rs[0].iter().all(|t| t.color.is_some()));
        assert_eq!(
            rs[1].iter().map(|t| t.text.as_str()).collect::<String>(),
            "    let x = 1;"
        );
        let plain = highlight("notes.unknownext", &["hello world"], false);
        assert_eq!(plain[0][0].color, None);
    }
}
