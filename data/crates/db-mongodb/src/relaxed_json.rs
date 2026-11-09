//! A forgiving JSON reader for the MongoDB query input, so queries can be typed
//! the way the mongo shell accepts them:
//!
//! - unquoted keys (`{ age: { $gt: 30 } }`), single-quoted strings, trailing commas,
//!   `//` and `/* */` comments;
//! - shell constructors, rewritten to MongoDB Extended JSON: `ObjectId("…")`,
//!   `ISODate("…")` / `new Date("…")`, `NumberLong(…)`, `NumberInt(…)`,
//!   `NumberDecimal("…")`, `UUID("…")`.
//!
//! The result is a `serde_json::Value` that `bson` converts (interpreting
//! `$oid`, `$date`, … as typed BSON values).

use serde_json::{Map, Number, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte offset in the input.
    pub at: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at character {})", self.message, self.at + 1)
    }
}

pub struct Reader<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn error<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            message: message.into(),
            at: self.pos,
        })
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    #[must_use]
    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Skips whitespace and comments.
    pub fn skip_ws(&mut self) {
        loop {
            let trimmed = self.rest().trim_start();
            self.pos = self.src.len() - trimmed.len();
            if let Some(after) = trimmed.strip_prefix("//") {
                let line = after.find('\n').map_or(after.len(), |n| n + 1);
                self.pos += 2 + line;
            } else if let Some(after) = trimmed.strip_prefix("/*") {
                let end = after.find("*/").map_or(after.len(), |n| n + 2);
                self.pos += 2 + end;
            } else {
                return;
            }
        }
    }

    /// Consumes `c` (after whitespace) or fails.
    pub fn expect(&mut self, c: char) -> Result<(), ParseError> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            Ok(())
        } else {
            self.error(format!("expected '{c}'"))
        }
    }

    /// Consumes `c` if it is next (after whitespace).
    pub fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    /// An identifier: letters, digits, `_` and `$`, not starting with a digit.
    pub fn ident(&mut self) -> Option<&'a str> {
        self.skip_ws();
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(i, c)| {
                !(c == '_' || c == '$' || c.is_alphanumeric()) || (i == 0 && c.is_ascii_digit())
            })
            .map_or(rest.len(), |(i, _)| i);
        if len == 0 {
            return None;
        }
        self.pos += len;
        Some(&rest[..len])
    }

    pub fn value(&mut self) -> Result<Value, ParseError> {
        self.skip_ws();
        match self.peek() {
            None => self.error("unexpected end of input"),
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"' | '\'') => self.string().map(Value::String),
            Some(c) if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() => self.number(),
            Some(_) => self.word(),
        }
    }

    fn object(&mut self) -> Result<Value, ParseError> {
        self.expect('{')?;
        let mut map = Map::new();
        loop {
            if self.eat('}') {
                return Ok(Value::Object(map));
            }
            self.skip_ws();
            let key = match self.peek() {
                Some('"' | '\'') => self.string()?,
                _ => match self.key_word() {
                    Some(k) => k.to_owned(),
                    None => return self.error("expected a field name"),
                },
            };
            self.expect(':')?;
            let value = self.value()?;
            map.insert(key, value);
            if !self.eat(',') {
                self.expect('}')?;
                return Ok(Value::Object(map));
            }
        }
    }

    /// Unquoted key: identifier characters plus `.` (dotted paths like `a.b`).
    fn key_word(&mut self) -> Option<&'a str> {
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(_, c)| !(c == '_' || c == '$' || c == '.' || c.is_alphanumeric()))
            .map_or(rest.len(), |(i, _)| i);
        if len == 0 {
            return None;
        }
        self.pos += len;
        Some(&rest[..len])
    }

    fn array(&mut self) -> Result<Value, ParseError> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            if self.eat(']') {
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            if !self.eat(',') {
                self.expect(']')?;
                return Ok(Value::Array(items));
            }
        }
    }

    pub fn string(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        let Some(quote) = self.bump().filter(|q| *q == '"' || *q == '\'') else {
            return self.error("expected a string");
        };
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return self.error("unterminated string"),
                Some(c) if c == quote => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('0') => out.push('\0'),
                    Some('u') => out.push(self.unicode_escape()?),
                    Some(c) => out.push(c),
                    None => return self.error("unterminated string"),
                },
                Some(c) => out.push(c),
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let hex = self.rest().get(..4).unwrap_or_default();
        let Ok(code) = u32::from_str_radix(hex, 16) else {
            return self.error("invalid \\u escape");
        };
        self.pos += 4;
        // Surrogate pair: 😀
        if (0xD800..0xDC00).contains(&code)
            && let Some(low) = self.rest().strip_prefix("\\u").and_then(|r| r.get(..4))
            && let Ok(low) = u32::from_str_radix(low, 16)
            && (0xDC00..0xE000).contains(&low)
        {
            self.pos += 6;
            let combined = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(combined).map_or_else(|| self.error("invalid surrogate"), Ok);
        }
        char::from_u32(code).map_or_else(|| self.error("invalid \\u escape"), Ok)
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(_, c)| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .map_or(rest.len(), |(i, _)| i);
        let text = rest[..len].trim_start_matches('+');
        self.pos += len;
        if let Ok(i) = text.parse::<i64>() {
            return Ok(Value::Number(i.into()));
        }
        match text.parse::<f64>().ok().and_then(Number::from_f64) {
            Some(n) => Ok(Value::Number(n)),
            None => Err(ParseError {
                message: format!("invalid number '{text}'"),
                at: start,
            }),
        }
    }

    /// `true`, `false`, `null`, `undefined` and shell constructors.
    fn word(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        let Some(mut word) = self.ident() else {
            return self.error("unexpected character");
        };
        if word == "new" {
            word = match self.ident() {
                Some(w) => w,
                None => return self.error("expected a constructor after 'new'"),
            };
        }
        match word {
            "true" => return Ok(Value::Bool(true)),
            "false" => return Ok(Value::Bool(false)),
            "null" | "undefined" => return Ok(Value::Null),
            _ => {}
        }
        if !self.eat('(') {
            return Err(ParseError {
                message: format!("unknown value '{word}' (quote strings)"),
                at: start,
            });
        }
        let arg = if self.eat(')') {
            None
        } else {
            let v = self.value()?;
            self.expect(')')?;
            Some(v)
        };
        constructor(word, arg).map_err(|message| ParseError { message, at: start })
    }
}

/// Maps a shell constructor call to its Extended JSON form.
fn constructor(name: &str, arg: Option<Value>) -> Result<Value, String> {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let wrap = |key: &str, v: Value| {
        let mut m = Map::new();
        m.insert(key.to_owned(), v);
        Value::Object(m)
    };
    match (name, arg) {
        ("ObjectId", Some(Value::String(hex))) => Ok(wrap("$oid", Value::String(hex))),
        ("ISODate" | "Date", Some(Value::String(s))) => Ok(wrap("$date", Value::String(s))),
        ("ISODate" | "Date", Some(Value::Number(ms))) => Ok(wrap(
            "$date",
            wrap("$numberLong", Value::String(ms.to_string())),
        )),
        ("NumberLong", Some(v)) => Ok(wrap("$numberLong", Value::String(text(&v)))),
        ("NumberDecimal", Some(v)) => Ok(wrap("$numberDecimal", Value::String(text(&v)))),
        ("NumberInt", Some(v)) => text(&v)
            .parse::<i32>()
            .map(|i| Value::Number(i.into()))
            .map_err(|_| format!("invalid NumberInt({})", text(&v))),
        ("UUID", Some(Value::String(s))) => Ok(wrap("$uuid", Value::String(s))),
        (name, _) => Err(format!("unsupported constructor {name}(…)")),
    }
}

/// Parses one complete value; trailing input is an error.
#[cfg(test)]
pub fn parse(src: &str) -> Result<Value, ParseError> {
    let mut r = Reader::new(src);
    let v = r.value()?;
    r.skip_ws();
    if r.peek().is_some() {
        return r.error("unexpected text after the value");
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_shell_style_objects() {
        let v = parse("{ age: { $gt: 30 }, 'name': \"Ann\", a.b: [1, -2.5, true, null,], }")
            .expect("parse");
        assert_eq!(
            v,
            json!({"age": {"$gt": 30}, "name": "Ann", "a.b": [1, -2.5, true, null]})
        );
    }

    #[test]
    fn rewrites_constructors_to_extended_json() {
        let v = parse(
            "{ _id: ObjectId('65a1b2c3d4e5f60718293a4b'), at: ISODate(\"2024-01-02T03:04:05Z\"), \
             n: NumberLong(5), d: NumberDecimal('1.10'), i: NumberInt('7'), t: new Date(0) }",
        )
        .expect("parse");
        assert_eq!(
            v,
            json!({
                "_id": {"$oid": "65a1b2c3d4e5f60718293a4b"},
                "at": {"$date": "2024-01-02T03:04:05Z"},
                "n": {"$numberLong": "5"},
                "d": {"$numberDecimal": "1.10"},
                "i": 7,
                "t": {"$date": {"$numberLong": "0"}}
            })
        );
    }

    #[test]
    fn strings_escapes_and_comments() {
        assert_eq!(
            parse("/* c */ 'it\\'s \\u00e9 \\uD83D\\uDE00' // tail").expect("parse"),
            json!("it's é 😀")
        );
    }

    #[test]
    fn reports_errors_with_position() {
        let e = parse("{ a: }").expect_err("bad");
        assert_eq!(e.at, 5);
        assert!(parse("{ a: 1 } extra").is_err());
        assert!(
            parse("{ a: Foo(1) }")
                .expect_err("ctor")
                .message
                .contains("Foo")
        );
        assert!(
            parse("{ a: bare }")
                .expect_err("bare")
                .message
                .contains("bare")
        );
        assert!(parse("'unterminated").is_err());
    }
}
