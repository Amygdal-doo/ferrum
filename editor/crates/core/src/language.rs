//! Built-in languages: detection from file name / shebang, comment tokens,
//! and the stable ids used in `config.toml` and `snippets.toml`.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    PlainText,
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Toml,
    Yaml,
    Markdown,
    Python,
    Go,
    Html,
    Css,
    Bash,
}

impl Language {
    pub const ALL: [Language; 14] = [
        Language::PlainText,
        Language::Rust,
        Language::JavaScript,
        Language::TypeScript,
        Language::Tsx,
        Language::Json,
        Language::Toml,
        Language::Yaml,
        Language::Markdown,
        Language::Python,
        Language::Go,
        Language::Html,
        Language::Css,
        Language::Bash,
    ];

    /// Display name for the status bar and palette.
    pub fn name(self) -> &'static str {
        match self {
            Language::PlainText => "Plain Text",
            Language::Rust => "Rust",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TSX",
            Language::Json => "JSON",
            Language::Toml => "TOML",
            Language::Yaml => "YAML",
            Language::Markdown => "Markdown",
            Language::Python => "Python",
            Language::Go => "Go",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Bash => "Shell",
        }
    }

    /// Lower-case id used in config files (`[languages.rust]`, `[rust.*]`).
    pub fn id(self) -> &'static str {
        match self {
            Language::PlainText => "text",
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::Json => "json",
            Language::Toml => "toml",
            Language::Yaml => "yaml",
            Language::Markdown => "markdown",
            Language::Python => "python",
            Language::Go => "go",
            Language::Html => "html",
            Language::Css => "css",
            Language::Bash => "shell",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|l| l.id() == id)
            .or(match id.as_str() {
                "js" | "jsx" => Some(Language::JavaScript),
                "ts" => Some(Language::TypeScript),
                "py" => Some(Language::Python),
                "sh" | "bash" | "zsh" => Some(Language::Bash),
                "md" => Some(Language::Markdown),
                "yml" => Some(Language::Yaml),
                "plain" | "plaintext" | "txt" => Some(Language::PlainText),
                _ => None,
            })
    }

    /// Detects from the file name, falling back to a `#!` line.
    pub fn detect(path: &Path, first_line: &str) -> Self {
        Self::from_path(path).unwrap_or_else(|| Self::from_shebang(first_line))
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        let by_name = match name.as_str() {
            "cargo.lock" | "pipfile" | "poetry.lock" => Some(Language::Toml),
            ".bashrc" | ".zshrc" | ".bash_profile" | ".profile" | ".zprofile" | ".envrc" => {
                Some(Language::Bash)
            }
            "dockerfile" | "makefile" => Some(Language::PlainText),
            _ => None,
        };
        if by_name.is_some() {
            return by_name;
        }
        let ext = name.rsplit_once('.')?.1;
        Some(match ext {
            "rs" => Language::Rust,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "json" | "jsonc" | "json5" | "webmanifest" => Language::Json,
            "toml" => Language::Toml,
            "yaml" | "yml" => Language::Yaml,
            "md" | "markdown" | "mdx" => Language::Markdown,
            "py" | "pyi" | "pyw" => Language::Python,
            "go" => Language::Go,
            "html" | "htm" | "xhtml" => Language::Html,
            "css" => Language::Css,
            "sh" | "bash" | "zsh" | "ksh" => Language::Bash,
            "txt" | "text" | "log" => Language::PlainText,
            _ => return None,
        })
    }

    pub fn from_shebang(first_line: &str) -> Self {
        let Some(rest) = first_line.strip_prefix("#!") else {
            return Language::PlainText;
        };
        let words: Vec<&str> = rest.split_whitespace().collect();
        let interpreter = match words.first() {
            Some(first) if first.ends_with("/env") => words.get(1).copied().unwrap_or(""),
            Some(first) => first.rsplit('/').next().unwrap_or(first),
            None => "",
        };
        match interpreter {
            "sh" | "bash" | "zsh" | "dash" | "ksh" => Language::Bash,
            i if i.starts_with("python") => Language::Python,
            "node" | "deno" | "bun" => Language::JavaScript,
            _ => Language::PlainText,
        }
    }

    pub fn line_comment(self) -> Option<&'static str> {
        match self {
            Language::Rust
            | Language::JavaScript
            | Language::TypeScript
            | Language::Tsx
            | Language::Go
            | Language::Json => Some("//"),
            Language::Toml | Language::Yaml | Language::Python | Language::Bash => Some("#"),
            Language::PlainText | Language::Markdown | Language::Html | Language::Css => None,
        }
    }

    pub fn block_comment(self) -> Option<(&'static str, &'static str)> {
        match self {
            Language::Rust
            | Language::JavaScript
            | Language::TypeScript
            | Language::Tsx
            | Language::Go
            | Language::Css => Some(("/*", "*/")),
            Language::Html | Language::Markdown => Some(("<!--", "-->")),
            _ => None,
        }
    }

    /// Whether a line ending in `:` opens an indented block.
    pub fn indents_after_colon(self) -> bool {
        matches!(self, Language::Python | Language::Yaml)
    }

    /// Bracket / quote pairs auto-closed while typing.
    pub fn auto_pairs(self) -> &'static [(char, char)] {
        match self {
            Language::PlainText => &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')],
            Language::Markdown => &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('`', '`')],
            Language::Rust => &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')],
            Language::Html => &[
                ('(', ')'),
                ('[', ']'),
                ('{', '}'),
                ('"', '"'),
                ('\'', '\''),
                ('<', '>'),
            ],
            Language::JavaScript | Language::TypeScript | Language::Tsx => &[
                ('(', ')'),
                ('[', ']'),
                ('{', '}'),
                ('"', '"'),
                ('\'', '\''),
                ('`', '`'),
            ],
            _ => &[('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension_and_name() {
        assert_eq!(
            Language::detect(Path::new("src/main.rs"), ""),
            Language::Rust
        );
        assert_eq!(Language::detect(Path::new("a/App.TSX"), ""), Language::Tsx);
        assert_eq!(
            Language::detect(Path::new("x.mjs"), ""),
            Language::JavaScript
        );
        assert_eq!(
            Language::detect(Path::new("Cargo.lock"), ""),
            Language::Toml
        );
        assert_eq!(Language::detect(Path::new("ci.yml"), ""), Language::Yaml);
        assert_eq!(Language::detect(Path::new(".zshrc"), ""), Language::Bash);
        assert_eq!(
            Language::detect(Path::new("README"), ""),
            Language::PlainText
        );
    }

    #[test]
    fn detects_by_shebang() {
        assert_eq!(
            Language::detect(Path::new("run"), "#!/usr/bin/env python3"),
            Language::Python
        );
        assert_eq!(
            Language::detect(Path::new("run"), "#!/bin/bash -e"),
            Language::Bash
        );
        assert_eq!(
            Language::detect(Path::new("run"), "#!/usr/bin/env node"),
            Language::JavaScript
        );
        assert_eq!(
            Language::detect(Path::new("run"), "hello"),
            Language::PlainText
        );
    }

    #[test]
    fn ids_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_id(language.id()), Some(language));
        }
        assert_eq!(Language::from_id("PY"), Some(Language::Python));
        assert_eq!(Language::from_id("nope"), None);
    }

    #[test]
    fn comment_tokens() {
        assert_eq!(Language::Rust.line_comment(), Some("//"));
        assert_eq!(Language::Python.line_comment(), Some("#"));
        assert_eq!(Language::Html.block_comment(), Some(("<!--", "-->")));
        assert_eq!(Language::Css.line_comment(), None);
    }
}
