//! JSON with comments and trailing commas (VS Code's `jsonc`), reduced to
//! plain JSON for `serde_json`.

/// Parses JSONC into a value.
pub fn parse(source: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(&strip(source)).map_err(|e| e.to_string())
}

/// Removes comments, then trailing commas (a comma whose next
/// non-whitespace character closes an object or array).
fn strip(source: &str) -> String {
    let no_comments = strip_comments(source);
    let chars: Vec<char> = no_comments.chars().collect();
    let mut out = String::with_capacity(no_comments.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.get(i + 1) {
                    out.push(*next);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}' | ']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

fn strip_comments(source: &str) -> String {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.get(i + 1) {
                    out.push(*next);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            out.push(' ');
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        // a line comment
        "name": "Test // not a comment", /* block
        comment */ "url": "http://x/*y*/",
        "list": [1, 2, 3,],
        "quote": "say \"hi\", ok",
    }"#;

    #[test]
    fn strips_comments_and_trailing_commas() {
        let value = parse(SAMPLE).expect("valid");
        assert_eq!(value["name"], "Test // not a comment");
        assert_eq!(value["url"], "http://x/*y*/");
        assert_eq!(value["list"].as_array().map(Vec::len), Some(3));
        assert_eq!(value["quote"], "say \"hi\", ok");
    }

    #[test]
    fn reports_real_errors() {
        assert!(parse("{ \"a\": }").is_err());
        assert!(parse("\u{feff}{}").is_ok());
    }
}
