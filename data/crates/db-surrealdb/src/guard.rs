//! Client-side read-only guard. SurrealDB has no read-only session mode, so a
//! read-only profile refuses statements that start with a writing keyword.
//! A UX guard, not a security boundary: use a viewer-role user for that.

const WRITING: &[&str] = &[
    "CREATE", "INSERT", "UPDATE", "UPSERT", "DELETE", "RELATE", "DEFINE", "REMOVE", "ALTER",
    "REBUILD", "KILL", "LIVE", "ACCESS", "IMPORT",
];

/// Leading keyword of each `;`-separated part, skipping `--`, `//`, `#` and
/// `/* */` comments. Quotes are not parsed, which errs on the side of refusing.
#[must_use]
pub fn writes(query: &str) -> bool {
    query
        .split(';')
        .any(|part| leading_keyword(part).is_some_and(|k| WRITING.contains(&k.as_str())))
}

fn leading_keyword(sql: &str) -> Option<String> {
    let mut s = sql;
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '(');
        if let Some(rest) = s
            .strip_prefix("--")
            .or_else(|| s.strip_prefix("//"))
            .or_else(|| s.strip_prefix('#'))
        {
            s = rest.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map_or("", |(_, r)| r);
        } else {
            break;
        }
    }
    let word: String = s.chars().take_while(char::is_ascii_alphabetic).collect();
    (!word.is_empty()).then(|| word.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_writes_through_comments_and_multiple_statements() {
        assert!(!writes("SELECT * FROM person"));
        assert!(!writes("INFO FOR DB; RETURN 1"));
        assert!(writes("  // c\n create person:x"));
        assert!(writes("SELECT 1; DELETE person"));
        assert!(writes("# x\nRELATE a:1->likes->b:2"));
        assert!(!writes(""));
    }
}
