//! Project rules loaded into the system prompt automatically:
//!
//! - `AGENTS.md` at the root (always), and nested `AGENTS.md` files when a
//!   file in their folder is in context;
//! - `CLAUDE.md` / `.claude/CLAUDE.md` (always, for compatibility);
//! - `.ferrum/rules/*.md`: always, unless their frontmatter sets
//!   `alwaysApply: false` (then by `globs`, or listed by `description`);
//! - `.cursor/rules/*.mdc`: Cursor's format — `alwaysApply: true`, or
//!   attached when a context file matches `globs`; rules with only a
//!   `description` are listed so the model can read them when relevant.

use std::path::{Path, PathBuf};

const RULE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource {
    Agents,
    Claude,
    Ferrum,
    Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Relative to the project root, with `/`.
    pub path: String,
    pub source: RuleSource,
    pub description: Option<String>,
    pub always: bool,
    pub globs: Vec<String>,
    pub body: String,
    /// For nested `AGENTS.md`: the folder it governs.
    pub scope: Option<PathBuf>,
}

impl Rule {
    /// Whether the rule applies given the files in context (relative).
    pub fn applies(&self, files: &[PathBuf]) -> bool {
        if let Some(scope) = &self.scope {
            return files.iter().any(|f| f.starts_with(scope));
        }
        if self.always {
            return true;
        }
        let Ok(set) = build_globs(&self.globs) else {
            return false;
        };
        !self.globs.is_empty() && files.iter().any(|f| set.is_match(f))
    }
}

fn build_globs(globs: &[String]) -> Result<globset::GlobSet, globset::Error> {
    let mut builder = globset::GlobSetBuilder::new();
    for glob in globs {
        let glob = glob.trim();
        if glob.is_empty() {
            continue;
        }
        builder.add(globset::Glob::new(glob)?);
        // `*.ts` should match in any folder, as Cursor does.
        if !glob.contains('/') {
            builder.add(globset::Glob::new(&format!("**/{glob}"))?);
        }
    }
    builder.build()
}

/// Splits `---` frontmatter (simple `key: value` lines, YAML lists) from
/// the body.
pub fn frontmatter(text: &str) -> (Vec<(String, String)>, &str) {
    let trimmed = text.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
    else {
        return (Vec::new(), text);
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), text);
    };
    let head = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in head.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if let Some((_, value)) = fields.last_mut() {
                if !value.is_empty() {
                    value.push(',');
                }
                value.push_str(item.trim());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    (fields, body)
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches('"').trim_matches('\'')
}

fn parse_globs(value: Option<&str>) -> Vec<String> {
    value
        .map(|v| v.trim().trim_start_matches('[').trim_end_matches(']'))
        .into_iter()
        .flat_map(|v| v.split(','))
        .map(|g| unquote(g).to_string())
        .filter(|g| !g.is_empty())
        .collect()
}

fn read_capped(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.len() <= RULE_LIMIT {
        return Some(text);
    }
    let mut end = RULE_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}\n…(truncated)", &text[..end]))
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn rule_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == extension))
        .collect();
    files.sort();
    files
}

/// Every rule in the project.
pub fn load(root: &Path) -> Vec<Rule> {
    let mut rules = Vec::new();
    let plain = |path: &Path, source, scope| {
        read_capped(path).map(|body| Rule {
            path: rel(root, path),
            source,
            description: None,
            always: true,
            globs: Vec::new(),
            body,
            scope,
        })
    };
    rules.extend(plain(&root.join("AGENTS.md"), RuleSource::Agents, None));
    for claude in [
        root.join("CLAUDE.md"),
        root.join(".claude").join("CLAUDE.md"),
    ] {
        rules.extend(plain(&claude, RuleSource::Claude, None));
    }
    // Nested AGENTS.md files (gitignore respected, shallow).
    for entry in ignore::WalkBuilder::new(root)
        .max_depth(Some(6))
        .build()
        .flatten()
    {
        let path = entry.path();
        if entry.depth() > 1 && path.file_name().is_some_and(|n| n == "AGENTS.md") {
            let scope = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(Path::to_path_buf);
            rules.extend(plain(path, RuleSource::Agents, scope));
        }
    }
    for path in rule_files(&root.join(".ferrum").join("rules"), "md") {
        let Some(text) = read_capped(&path) else {
            continue;
        };
        let (fields, body) = frontmatter(&text);
        rules.push(Rule {
            path: rel(root, &path),
            source: RuleSource::Ferrum,
            description: field(&fields, "description").map(|d| unquote(d).to_string()),
            always: field(&fields, "alwaysApply").is_none_or(|v| unquote(v) != "false"),
            globs: parse_globs(field(&fields, "globs")),
            body: body.to_string(),
            scope: None,
        });
    }
    for path in rule_files(&root.join(".cursor").join("rules"), "mdc") {
        let Some(text) = read_capped(&path) else {
            continue;
        };
        let (fields, body) = frontmatter(&text);
        rules.push(Rule {
            path: rel(root, &path),
            source: RuleSource::Cursor,
            description: field(&fields, "description")
                .map(|d| unquote(d).to_string())
                .filter(|d| !d.is_empty()),
            always: field(&fields, "alwaysApply").is_some_and(|v| unquote(v) == "true"),
            globs: parse_globs(field(&fields, "globs")),
            body: body.to_string(),
            scope: None,
        });
    }
    rules
}

/// The rules section of the system prompt for the files in context.
pub fn prompt_section(rules: &[Rule], files: &[PathBuf]) -> String {
    let mut out = String::new();
    let mut listed = Vec::new();
    for rule in rules {
        if rule.applies(files) {
            out.push_str(&format!(
                "<rule path=\"{}\">\n{}\n</rule>\n",
                rule.path,
                rule.body.trim()
            ));
        } else if let Some(description) = &rule.description {
            listed.push(format!("- {}: {description}", rule.path));
        }
    }
    if !listed.is_empty() {
        out.push_str("Other project rules (read them with read_file when relevant):\n");
        out.push_str(&listed.join("\n"));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    #[test]
    fn loads_every_rule_format() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write(root, "AGENTS.md", "Use tabs.");
        write(root, "CLAUDE.md", "Be careful.");
        write(root, "web/AGENTS.md", "Web rules.");
        write(root, ".ferrum/rules/style.md", "Prefer small functions.");
        write(
            root,
            ".ferrum/rules/sql.md",
            "---\nalwaysApply: false\nglobs: \"*.sql\"\n---\nUse CTEs.",
        );
        write(
            root,
            ".cursor/rules/ts.mdc",
            "---\ndescription: TypeScript conventions\nglobs: src/**/*.ts, *.tsx\nalwaysApply: false\n---\nNo any.",
        );
        write(
            root,
            ".cursor/rules/always.mdc",
            "---\nalwaysApply: true\n---\nAlways rule.",
        );
        write(
            root,
            ".cursor/rules/smart.mdc",
            "---\ndescription: Database migrations\nglobs:\nalwaysApply: false\n---\nMigrations.",
        );
        write(root, ".cursor/rules/ignored.md", "plain md is ignored");

        let rules = load(root);
        let paths: Vec<&str> = rules.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "AGENTS.md",
                "CLAUDE.md",
                "web/AGENTS.md",
                ".ferrum/rules/sql.md",
                ".ferrum/rules/style.md",
                ".cursor/rules/always.mdc",
                ".cursor/rules/smart.mdc",
                ".cursor/rules/ts.mdc",
            ]
        );

        let none = prompt_section(&rules, &[]);
        assert!(none.contains("Use tabs.") && none.contains("Be careful."));
        assert!(none.contains("Prefer small functions.") && none.contains("Always rule."));
        assert!(
            !none.contains("Use CTEs.")
                && !none.contains("No any.")
                && !none.contains("Web rules.")
        );
        assert!(none.contains("- .cursor/rules/smart.mdc: Database migrations"));
        assert!(none.contains("- .cursor/rules/ts.mdc: TypeScript conventions"));

        let files = [
            PathBuf::from("src/app/x.ts"),
            PathBuf::from("db/q.sql"),
            PathBuf::from("web/index.html"),
        ];
        let some = prompt_section(&rules, &files);
        assert!(
            some.contains("No any.") && some.contains("Use CTEs.") && some.contains("Web rules.")
        );
        assert!(
            rules[7].applies(&[PathBuf::from("deep/c.tsx")]),
            "bare globs match in any folder"
        );
    }

    #[test]
    fn frontmatter_lists_and_missing_end() {
        let (fields, body) = frontmatter("---\nglobs:\n  - \"*.rs\"\n  - \"*.toml\"\n---\nbody");
        assert_eq!(parse_globs(field(&fields, "globs")), ["*.rs", "*.toml"]);
        assert_eq!(body, "body");
        let (fields, body) = frontmatter("---\nno end");
        assert!(fields.is_empty());
        assert_eq!(body, "---\nno end");
    }
}
