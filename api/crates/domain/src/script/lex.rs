//! Tokens of one script line.

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    /// Punctuation and operators: `( ) [ ] . , : = == != < <= > >= + - ! && ||`.
    P(&'static str),
}

const PUNCT: [&str; 19] = [
    "==", "!=", "<=", ">=", "&&", "||", "(", ")", "[", "]", ".", ",", ":", "=", "<", ">", "+", "-",
    "!",
];

/// Splits a line (comment already removed) into tokens.
pub(super) fn tokens(line: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let (s, next) = string(&chars, i)?;
            out.push(Tok::Str(s));
            i = next;
        } else if c.is_ascii_digit() {
            let start = i;
            while chars.get(i).is_some_and(char::is_ascii_digit) {
                i += 1;
            }
            // A fraction only when a digit follows the dot (`a[0].b` stays a path).
            if chars.get(i) == Some(&'.') && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                i += 1;
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            let n = text
                .parse::<f64>()
                .map_err(|_| format!("bad number `{text}`"))?;
            out.push(Tok::Num(n));
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while chars
                .get(i)
                .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let Some(p) = PUNCT.iter().find(|p| rest.starts_with(**p)) else {
                return Err(format!("unexpected `{c}`"));
            };
            out.push(Tok::P(p));
            i += p.len();
        }
    }
    Ok(out)
}

/// A quoted string starting at `start`; returns it and the index after it.
fn string(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let quote = chars[start];
    let mut s = String::new();
    let mut i = start + 1;
    loop {
        match chars.get(i) {
            None => return Err("unterminated string".into()),
            Some(&c) if c == quote => return Ok((s, i + 1)),
            Some('\\') => {
                i += 1;
                match chars.get(i) {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(&c) => s.push(c),
                    None => return Err("unterminated string".into()),
                }
            }
            Some(&c) => s.push(c),
        }
        i += 1;
    }
}

/// The line without a trailing `#` or `//` comment (quotes respected).
pub(super) fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut prev = ' ';
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' => return &line[..i],
            None if c == '/' && prev == '/' => return &line[..i - 1],
            None => {}
        }
        prev = c;
    }
    line
}
