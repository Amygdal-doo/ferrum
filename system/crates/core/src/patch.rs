//! Writes an edit back into the text of `system.json` without disturbing
//! the rest of the file: only the values that changed are replaced, and
//! everything else (indentation, key order, fields FerrumSystem does not
//! know, numbers written as `1` rather than `1.0`) stays as it was.
//!
//! It is a three-way merge. `before` is what the model made of the file
//! and `after` the model once edited; wherever the two agree the file's
//! own text is kept. Array elements are matched by identity (`id`, then
//! `from`/`to`, then `name`), so deleting one component leaves the others'
//! text, unknown fields included, untouched.

use std::collections::HashMap;
use std::ops::Range;

use serde::Serialize;
use serde_json::{Map, Value};

/// A value's place in the text.
#[derive(Debug)]
enum Span {
    Object(Range<usize>, Vec<Entry>),
    Array(Range<usize>, Vec<Span>),
    Scalar(Range<usize>),
}

#[derive(Debug)]
struct Entry {
    key: String,
    /// From the key's opening quote to just after its closing one.
    key_range: Range<usize>,
    value: Span,
}

impl Span {
    fn range(&self) -> Range<usize> {
        match self {
            Span::Object(r, _) | Span::Array(r, _) | Span::Scalar(r) => r.clone(),
        }
    }
}

/// `text` with `before` → `after` applied; `None` when `text` is not a
/// JSON object (the caller then writes the file afresh).
pub fn update(text: &str, before: &Value, after: &Value) -> Option<String> {
    let root = Parser { text, pos: 0 }.document()?;
    if !matches!(root, Span::Object(..)) {
        return None;
    }
    let style = Style::detect(text);
    let mut edits = Vec::new();
    diff(text, &style, &root, before, after, &mut edits);
    Some(apply(text, 0..text.len(), edits))
}

/// One replacement: `range` of the original text becomes `text`.
type Edit = (Range<usize>, String);

/// `text[range]` with `edits` (all inside `range`) applied.
fn apply(text: &str, range: Range<usize>, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(range.len());
    let mut at = range.start;
    for (r, new) in edits {
        out.push_str(&text[at..r.start]);
        out.push_str(&new);
        at = r.end;
    }
    out.push_str(&text[at..range.end]);
    out
}

fn diff(
    text: &str,
    style: &Style,
    span: &Span,
    before: &Value,
    after: &Value,
    edits: &mut Vec<Edit>,
) {
    if before == after {
        return;
    }
    match (span, before, after) {
        (Span::Object(range, entries), Value::Object(b), Value::Object(a)) => {
            diff_object(text, style, range, entries, b, a, edits);
        }
        (Span::Array(range, items), Value::Array(b), Value::Array(a)) if items.len() == b.len() => {
            diff_array(text, style, range, items, b, a, edits);
        }
        _ => {
            let range = span.range();
            edits.push((
                range.clone(),
                style.render(after, indent_at(text, range.start)),
            ));
        }
    }
}

fn diff_object(
    text: &str,
    style: &Style,
    range: &Range<usize>,
    entries: &[Entry],
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    edits: &mut Vec<Edit>,
) {
    let mut added = Vec::new();
    for (key, new) in after {
        let old = before.get(key).unwrap_or(&Value::Null);
        match entries.iter().find(|e| e.key == *key) {
            Some(entry) => diff(text, style, &entry.value, old, new, edits),
            // Not in the file: it read as its default, which is `old`.
            None if old == new => {}
            None => added.push((key, new)),
        }
    }
    if added.is_empty() {
        return;
    }
    let Some(last) = entries.last() else {
        // An empty object in the file: write the whole new one.
        edits.push((
            range.clone(),
            style.render_object(after, indent_at(text, range.start)),
        ));
        return;
    };
    let first = &entries[0];
    let lead = &text[range.start + 1..first.key_range.start];
    let colon = &text[first.key_range.end..first.value.range().start];
    let indent = indent_at(text, first.key_range.start);
    let mut insert = String::new();
    for (key, value) in added {
        insert.push(',');
        insert.push_str(lead);
        insert.push_str(&Value::String(key.clone()).to_string());
        insert.push_str(colon);
        insert.push_str(&style.render(value, indent));
    }
    let end = last.value.range().end;
    edits.push((end..end, insert));
}

fn diff_array(
    text: &str,
    style: &Style,
    range: &Range<usize>,
    items: &[Span],
    before: &[Value],
    after: &[Value],
    edits: &mut Vec<Edit>,
) {
    let (old_keys, new_keys) = (identities(before), identities(after));
    if old_keys == new_keys {
        for ((span, b), a) in items.iter().zip(before).zip(after) {
            diff(text, style, span, b, a, edits);
        }
        return;
    }
    if items.is_empty() || after.is_empty() {
        edits.push((
            range.clone(),
            style.render(&Value::Array(after.to_vec()), indent_at(text, range.start)),
        ));
        return;
    }
    // Rebuild the elements: the ones kept are their own (edited) text, new
    // ones are written in the file's style, removed ones go.
    let indent = indent_at(text, items[0].range().start);
    let sep = if items.len() > 1 {
        text[items[0].range().end..items[1].range().start].to_owned()
    } else {
        format!(",{}", &text[range.start + 1..items[0].range().start])
    };
    let index: HashMap<&str, usize> = old_keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    let pieces: Vec<String> = after
        .iter()
        .zip(&new_keys)
        .map(|(value, key)| match index.get(key.as_str()) {
            Some(&i) => {
                let mut inner = Vec::new();
                diff(text, style, &items[i], &before[i], value, &mut inner);
                apply(text, items[i].range(), inner)
            }
            None => style.render(value, indent),
        })
        .collect();
    let whole = items[0].range().start..items[items.len() - 1].range().end;
    edits.push((whole, pieces.join(&sep)));
}

/// What makes an array element the same one before and after: its `id`,
/// else `from`/`to`, else `name`, else the whole value; repeats are
/// numbered.
fn identities(values: &[Value]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    values
        .iter()
        .map(|v| {
            let field = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            let base = match (field("id"), field("from"), field("to"), field("name")) {
                (Some(id), ..) => format!("id:{id}"),
                (None, Some(from), Some(to), _) => format!("edge:{from}>{to}"),
                (None, _, _, Some(name)) => format!("name:{name}"),
                _ => v.to_string(),
            };
            let n = seen.entry(base.clone()).or_default();
            *n += 1;
            format!("{base}#{n}")
        })
        .collect()
}

/// The leading whitespace of the line `pos` is on.
fn indent_at(text: &str, pos: usize) -> &str {
    let line = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let rest = &text[line..];
    let width = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    &rest[..width]
}

/// How the file is written: pretty with this indent, or on one line.
struct Style {
    /// `None`: compact.
    unit: Option<String>,
}

impl Style {
    fn detect(text: &str) -> Self {
        let unit = text
            .lines()
            .skip(1)
            .map(|l| &l[..l.len() - l.trim_start_matches([' ', '\t']).len()])
            .find(|w| !w.is_empty())
            .map(str::to_owned);
        Self { unit }
    }

    /// `value` as text whose later lines start at `indent`.
    fn render(&self, value: &Value, indent: &str) -> String {
        let Some(unit) = &self.unit else {
            return value.to_string();
        };
        let mut buf = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        if value.serialize(&mut ser).is_err() {
            return value.to_string();
        }
        let pretty = String::from_utf8(buf).unwrap_or_else(|_| value.to_string());
        pretty.replace('\n', &format!("\n{indent}"))
    }

    fn render_object(&self, map: &Map<String, Value>, indent: &str) -> String {
        self.render(&Value::Object(map.clone()), indent)
    }
}

/// Finds where every value is (a validating parse is done separately).
struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn document(mut self) -> Option<Span> {
        let span = self.value()?;
        self.ws();
        (self.pos == self.text.len()).then_some(span)
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Option<()> {
        self.ws();
        (self.peek() == Some(byte)).then(|| self.pos += 1)
    }

    fn value(&mut self) -> Option<Span> {
        self.ws();
        let start = self.pos;
        match self.peek()? {
            b'{' => {
                self.pos += 1;
                let mut entries = Vec::new();
                self.ws();
                if self.eat(b'}').is_none() {
                    loop {
                        self.ws();
                        let key_start = self.pos;
                        self.string()?;
                        let key_range = key_start..self.pos;
                        let key = serde_json::from_str(&self.text[key_range.clone()]).ok()?;
                        self.eat(b':')?;
                        let value = self.value()?;
                        entries.push(Entry {
                            key,
                            key_range,
                            value,
                        });
                        if self.eat(b',').is_none() {
                            self.eat(b'}')?;
                            break;
                        }
                    }
                }
                Some(Span::Object(start..self.pos, entries))
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat(b']').is_none() {
                    loop {
                        items.push(self.value()?);
                        if self.eat(b',').is_none() {
                            self.eat(b']')?;
                            break;
                        }
                    }
                }
                Some(Span::Array(start..self.pos, items))
            }
            b'"' => {
                self.string()?;
                Some(Span::Scalar(start..self.pos))
            }
            _ => {
                while matches!(self.peek(), Some(b) if !b",]} \t\r\n".contains(&b)) {
                    self.pos += 1;
                }
                (self.pos > start).then_some(Span::Scalar(start..self.pos))
            }
        }
    }

    fn string(&mut self) -> Option<()> {
        if self.peek()? != b'"' {
            return None;
        }
        self.pos += 1;
        loop {
            match self.peek()? {
                b'\\' => self.pos += 2,
                b'"' => {
                    self.pos += 1;
                    return Some(());
                }
                _ => self.pos += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(text: &str, after: Value) -> String {
        let before: Value = serde_json::from_str(text).expect("json");
        let out = update(text, &before, &after).expect("update");
        let read: Value = serde_json::from_str(&out).expect("still json");
        assert_eq!(read, after, "{out}");
        out
    }

    #[test]
    fn changes_only_the_value_that_changed() {
        let text = "{\n    \"name\": \"Shop\",\n    \"n\": 1,\n    \"x-team\": \"core\"\n}\n";
        let out = run(text, json!({"name": "Store", "n": 1, "x-team": "core"}));
        assert_eq!(
            out,
            "{\n    \"name\": \"Store\",\n    \"n\": 1,\n    \"x-team\": \"core\"\n}\n"
        );
        assert_eq!(run(text, serde_json::from_str(text).expect("test")), text);
    }

    #[test]
    fn keeps_unknown_fields_of_elements_that_stay() {
        let text = r#"{
  "nodes": [
    { "id": "a", "name": "A", "owner": "team-a" },
    { "id": "b", "name": "B", "owner": "team-b" },
    { "id": "c", "name": "C" }
  ]
}"#;
        let before: Value = serde_json::from_str(text).expect("test");
        // The model knows only id and name: it removes `a`, renames `c`
        // and adds `d`.
        let after = json!({"nodes": [
            {"id": "b", "name": "B"},
            {"id": "c", "name": "Cee"},
            {"id": "d", "name": "D"}
        ]});
        let model = |v: &Value| {
            let mut v = v.clone();
            for n in v["nodes"].as_array_mut().expect("test") {
                n.as_object_mut().expect("test").remove("owner");
            }
            v
        };
        let out = update(text, &model(&before), &after).expect("update");
        assert_eq!(
            out,
            r#"{
  "nodes": [
    { "id": "b", "name": "B", "owner": "team-b" },
    { "id": "c", "name": "Cee" },
    {
      "id": "d",
      "name": "D"
    }
  ]
}"#
        );
    }

    #[test]
    fn adds_a_field_the_file_left_out_only_when_it_changes() {
        // `slot` is missing from the file (it read as null).
        let text = "{\n\t\"id\": \"a\"\n}";
        let before = json!({"id": "a", "slot": null});
        let same = update(text, &before, &before).expect("test");
        assert_eq!(same, text);
        let out = update(text, &before, &json!({"id": "a", "slot": 2.0})).expect("test");
        assert_eq!(out, "{\n\t\"id\": \"a\",\n\t\"slot\": 2.0\n}");
    }

    #[test]
    fn compact_files_stay_compact_and_bad_ones_are_refused() {
        let out = run(
            r#"{"a":[1,2],"b":{}}"#,
            json!({"a": [1, 2, 3], "b": {"c": true}}),
        );
        assert_eq!(out, r#"{"a":[1,2,3],"b":{"c":true}}"#);
        assert!(update("[1]", &json!([1]), &json!([2])).is_none());
        assert!(update("{\"a\": ", &json!({}), &json!({})).is_none());
    }

    #[test]
    fn strings_with_escapes_and_emptied_arrays() {
        let text = "{\n  \"k\\\"ey\": \"a,]}\",\n  \"list\": [\"x\"]\n}";
        let out = run(text, json!({"k\"ey": "a,]}", "list": []}));
        assert_eq!(out, "{\n  \"k\\\"ey\": \"a,]}\",\n  \"list\": []\n}");
        let out = run(&out, json!({"k\"ey": "a,]}", "list": ["y", "z"]}));
        assert_eq!(
            out,
            "{\n  \"k\\\"ey\": \"a,]}\",\n  \"list\": [\n    \"y\",\n    \"z\"\n  ]\n}"
        );
    }
}
