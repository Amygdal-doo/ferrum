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
    pub const ALL: [Tool; 16] = [
        Tool::Json,
        Tool::JsonSchema,
        Tool::Sql,
        Tool::Base64,
        Tool::Jwt,
        Tool::Url,
        Tool::Timestamp,
        Tool::Cron,
        Tool::Ids,
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

#[cfg(test)]
mod tests {
    use super::*;

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
