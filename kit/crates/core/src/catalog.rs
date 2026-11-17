//! The list of tools shown in the sidebar and the search filter over it.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Json,
    JsonSchema,
    Sql,
    Base64,
    Jwt,
    Url,
    Timestamp,
    Cron,
    Ids,
    Qr,
    Hash,
    Regex,
    Diff,
    NumberBase,
    Color,
    Case,
    Lines,
}

impl Tool {
    /// Sidebar order.
    pub const ALL: [Tool; 17] = [
        Tool::Json,
        Tool::JsonSchema,
        Tool::Sql,
        Tool::Base64,
        Tool::Jwt,
        Tool::Url,
        Tool::Timestamp,
        Tool::Cron,
        Tool::Ids,
        Tool::Qr,
        Tool::Hash,
        Tool::Regex,
        Tool::Diff,
        Tool::NumberBase,
        Tool::Color,
        Tool::Case,
        Tool::Lines,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Json => "JSON / YAML / TOML",
            Tool::JsonSchema => "JSON Schema",
            Tool::Sql => "SQL Formatter",
            Tool::Base64 => "Base64",
            Tool::Jwt => "JWT Decoder",
            Tool::Url => "URL Encode / Parse",
            Tool::Timestamp => "Unix Timestamp",
            Tool::Cron => "Cron Expression",
            Tool::Ids => "UUID / ULID",
            Tool::Qr => "QR Code",
            Tool::Hash => "Hash Generator",
            Tool::Regex => "Regex Tester",
            Tool::Diff => "Text Diff",
            Tool::NumberBase => "Number Base",
            Tool::Color => "Color Converter",
            Tool::Case => "Case Converter",
            Tool::Lines => "Line Tools",
        }
    }

    /// Stable key for stored data (history, saved inputs).
    pub fn id(self) -> &'static str {
        match self {
            Tool::Json => "json",
            Tool::JsonSchema => "json-schema",
            Tool::Sql => "sql",
            Tool::Base64 => "base64",
            Tool::Jwt => "jwt",
            Tool::Url => "url",
            Tool::Timestamp => "timestamp",
            Tool::Cron => "cron",
            Tool::Ids => "ids",
            Tool::Qr => "qr",
            Tool::Hash => "hash",
            Tool::Regex => "regex",
            Tool::Diff => "diff",
            Tool::NumberBase => "number-base",
            Tool::Color => "color",
            Tool::Case => "case",
            Tool::Lines => "lines",
        }
    }

    /// Inputs are usually secrets (tokens): no history unless turned on.
    pub fn sensitive(self) -> bool {
        matches!(self, Tool::Jwt)
    }

    pub fn description(self) -> &'static str {
        match self {
            Tool::Json => "Format, minify and validate JSON; convert between JSON, YAML and TOML.",
            Tool::JsonSchema => {
                "Validate JSON against a JSON Schema, or generate a schema from a sample."
            }
            Tool::Sql => "Lay out SQL one clause per line, with keyword case and indent options.",
            Tool::Base64 => "Encode and decode text as standard or URL-safe Base64.",
            Tool::Jwt => {
                "Decode a JSON Web Token's header and payload. Does not verify signatures."
            }
            Tool::Url => "Percent-encode or decode text and break a URL into its parts.",
            Tool::Timestamp => {
                "Convert Unix timestamps (seconds or milliseconds) to dates and back."
            }
            Tool::Cron => "Explain a cron schedule in plain English and list its next run times.",
            Tool::Ids => "Generate UUID v4, UUID v7 and ULID identifiers.",
            Tool::Qr => "Make a QR code from text or a URL; copy it as an image or save a PNG.",
            Tool::Hash => "MD5, SHA-1, SHA-256 and SHA-512 digests of text.",
            Tool::Regex => "Test a regular expression (Rust regex syntax) against text.",
            Tool::Diff => "Line-by-line difference between two texts.",
            Tool::NumberBase => "Convert integers between decimal, hexadecimal, octal and binary.",
            Tool::Color => "Convert colours between hex, RGB, HSL and OKLCH, with a preview.",
            Tool::Case => {
                "camelCase, snake_case, kebab-case, PascalCase, SCREAMING_SNAKE, Title Case."
            }
            Tool::Lines => "Sort, dedupe, trim and count lines.",
        }
    }

    /// Extra search terms besides the name.
    fn keywords(self) -> &'static str {
        match self {
            Tool::Json => "json yaml yml toml format pretty minify validate beautify convert",
            Tool::JsonSchema => "json schema validate validator draft 2020-12 generate infer types",
            Tool::Sql => "sql query format pretty beautify select postgres mysql",
            Tool::Base64 => "base64 b64 encode decode url-safe",
            Tool::Jwt => "jwt token jws bearer decode claims exp",
            Tool::Url => "url uri percent encode decode query parse params",
            Tool::Timestamp => "unix epoch timestamp time date seconds milliseconds",
            Tool::Cron => "cron crontab schedule job timer next run every minute hour",
            Tool::Ids => "uuid guid ulid v4 v7 generate id",
            Tool::Qr => "qr qrcode barcode code image png generate url link",
            Tool::Hash => "hash md5 sha1 sha256 sha512 digest checksum",
            Tool::Regex => "regex regexp regular expression match pattern",
            Tool::Diff => "diff compare text lines changes",
            Tool::NumberBase => "number base hex hexadecimal binary octal decimal radix",
            Tool::Color => "color colour hex rgb hsl oklch oklab swatch preview",
            Tool::Case => "case camel snake kebab pascal screaming title convert",
            Tool::Lines => "lines sort dedupe unique trim count text",
        }
    }

    fn matches(self, query: &str) -> bool {
        let haystack = format!("{} {}", self.name(), self.keywords()).to_lowercase();
        query
            .split_whitespace()
            .all(|term| haystack.contains(&term.to_lowercase()))
    }
}

/// Tools whose name or keywords contain every whitespace-separated term of
/// `query` (case-insensitive), in sidebar order. An empty query returns all.
pub fn filter_tools(query: &str) -> Vec<Tool> {
    Tool::ALL
        .into_iter()
        .filter(|tool| tool.matches(query))
        .collect()
}

/// A tool found by [`fuzzy_tools`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMatch {
    pub tool: Tool,
    /// Char indices of [`Tool::name`] that matched (for highlighting).
    pub name_chars: Vec<usize>,
    score: i32,
}

/// Char indices where `query` occurs in order in `text` (both lowercase),
/// with a score that favours word starts and runs; `None` when it does not.
fn subsequence(query: &[char], text: &str) -> Option<(i32, Vec<usize>)> {
    let chars: Vec<char> = text.chars().collect();
    let mut indices = Vec::with_capacity(query.len());
    let mut score = 0;
    let mut from = 0;
    for q in query {
        let at = (from..chars.len()).find(|i| chars[*i] == *q)?;
        let word_start = at == 0 || !chars[at - 1].is_alphanumeric();
        let follows = indices.last().is_some_and(|last| at == last + 1);
        score += 10 + if word_start { 8 } else { 0 } + if follows { 5 } else { 0 };
        score -= (at - from).min(10) as i32;
        indices.push(at);
        from = at + 1;
    }
    Some((score, indices))
}

/// Tools for a command palette, best first: an exact name, id or keyword
/// beats a prefix, which beats a substring, which beats letters in order
/// (`b64` finds Base64). Ties keep sidebar order; an empty query lists all.
pub fn fuzzy_tools(query: &str) -> Vec<ToolMatch> {
    let query = query.trim().to_lowercase();
    let mut found: Vec<ToolMatch> = Tool::ALL
        .into_iter()
        .filter_map(|tool| {
            let name = tool.name().to_lowercase();
            if query.is_empty() {
                return Some(ToolMatch {
                    tool,
                    name_chars: Vec::new(),
                    score: 0,
                });
            }
            let words: Vec<&str> = std::iter::once(tool.id())
                .chain(tool.keywords().split_whitespace())
                .chain(name.split(|c: char| !c.is_alphanumeric()))
                .filter(|w| !w.is_empty())
                .collect();
            let named = subsequence(&query.chars().collect::<Vec<_>>(), &name);
            let text_score = if name == query || words.contains(&query.as_str()) {
                1000
            } else if name.starts_with(&query) {
                800
            } else if words.iter().any(|w| w.starts_with(&query)) {
                700
            } else if name.contains(&query) || words.iter().any(|w| w.contains(&query)) {
                600
            } else {
                0
            };
            let fuzzy = named.as_ref().map_or(0, |(score, _)| *score);
            let keyword_fuzzy = || {
                let q: Vec<char> = query.chars().collect();
                words
                    .iter()
                    .filter_map(|w| subsequence(&q, w).map(|(s, _)| s))
                    .max()
            };
            let score = match (text_score, fuzzy) {
                (0, 0) => keyword_fuzzy()? / 2,
                (text, fuzzy) => text + fuzzy,
            };
            Some(ToolMatch {
                tool,
                name_chars: named.map(|(_, indices)| indices).unwrap_or_default(),
                score,
            })
        })
        .collect();
    // Stable: equal scores stay in sidebar order.
    found.sort_by_key(|m| std::cmp::Reverse(m.score));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(query: &str) -> Option<Tool> {
        fuzzy_tools(query).first().map(|m| m.tool)
    }

    #[test]
    fn fuzzy_finds_tools_by_name_id_keyword_and_letters() {
        assert_eq!(first("base64"), Some(Tool::Base64));
        assert_eq!(first("Base64"), Some(Tool::Base64));
        assert_eq!(first("b64"), Some(Tool::Base64));
        assert_eq!(first("jwt"), Some(Tool::Jwt));
        assert_eq!(first("cron"), Some(Tool::Cron));
        assert_eq!(first("qr"), Some(Tool::Qr));
        assert_eq!(first("sql"), Some(Tool::Sql));
        assert_eq!(first("schema"), Some(Tool::JsonSchema));
        assert_eq!(first("oklch"), Some(Tool::Color));
        assert_eq!(first("sha256"), Some(Tool::Hash));
        assert_eq!(first("rgx"), Some(Tool::Regex));
        assert_eq!(first("tstmp"), Some(Tool::Timestamp));
        assert!(fuzzy_tools("zzzz").is_empty());
        assert_eq!(fuzzy_tools("").len(), Tool::ALL.len());
        let m = &fuzzy_tools("b64")[0];
        // "Base64": b, 6, 4.
        assert_eq!(m.name_chars, [0, 4, 5]);
    }

    #[test]
    fn empty_query_lists_everything_in_order() {
        assert_eq!(filter_tools(""), Tool::ALL.to_vec());
        assert_eq!(filter_tools("   "), Tool::ALL.to_vec());
    }

    #[test]
    fn matches_name_and_keywords_case_insensitively() {
        assert_eq!(filter_tools("JWT"), vec![Tool::Jwt]);
        assert_eq!(filter_tools("sha256"), vec![Tool::Hash]);
        assert_eq!(filter_tools("yaml"), vec![Tool::Json]);
        assert!(filter_tools("hex").contains(&Tool::Color));
        assert!(filter_tools("hex").contains(&Tool::NumberBase));
    }

    #[test]
    fn all_terms_must_match() {
        assert_eq!(filter_tools("hex rgb"), vec![Tool::Color]);
        assert!(filter_tools("jwt sha1").is_empty());
    }

    #[test]
    fn every_tool_has_text_and_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for tool in Tool::ALL {
            assert!(!tool.name().is_empty());
            assert!(!tool.description().is_empty());
            assert!(ids.insert(tool.id()));
        }
    }
}
