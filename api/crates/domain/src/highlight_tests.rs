use super::*;

fn texts<'a>(hay: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
    ranges.iter().map(|r| &hay[r.clone()]).collect()
}

#[test]
fn finds_case_insensitive_by_default_and_respects_case_flag() {
    let hay = "Token token TOKEN tok";
    let all = find_matches(hay, "token", false, 100);
    assert_eq!(texts(hay, &all), ["Token", "token", "TOKEN"]);
    let exact = find_matches(hay, "token", true, 100);
    assert_eq!(texts(hay, &exact), ["token"]);
    assert_eq!(exact[0].start, 6);
}

#[test]
fn matches_do_not_overlap_and_respect_limit() {
    assert_eq!(find_matches("aaaa", "aa", true, 100), [0..2, 2..4]);
    assert_eq!(find_matches("aaaa", "aa", false, 100), [0..2, 2..4]);
    assert_eq!(find_matches("ababab", "ab", false, 2).len(), 2);
    assert!(find_matches("abc", "", false, 100).is_empty());
    assert!(find_matches("abc", "abcd", false, 100).is_empty());
    assert!(find_matches("abc", "a", false, 0).is_empty());
}

#[test]
fn unicode_offsets_are_char_boundaries() {
    let hay = "naïve NAÏVE café";
    let found = find_matches(hay, "ï", false, 100);
    assert_eq!(texts(hay, &found), ["ï", "Ï"]);
    let found = find_matches(hay, "CAFÉ", false, 100);
    assert_eq!(texts(hay, &found), ["café"]);
    // Multi-byte chars before a match keep offsets valid.
    let hay = "😀😀 x";
    assert_eq!(texts(hay, &find_matches(hay, "X", false, 100)), ["x"]);
}

#[test]
fn json_tokens_classify_keys_values_and_literals() {
    let text = "{\n  \"id\": 42,\n  \"name\" : \"a \\\"q\\\" b\",\n  \"ok\": true,\n  \"no\": false,\n  \"x\": null,\n  \"n\": [-1.5e3, 0]\n}";
    let tokens = json_tokens(text);
    let got: Vec<(&str, JsonToken)> = tokens.iter().map(|(r, k)| (&text[r.clone()], *k)).collect();
    use JsonToken::*;
    assert_eq!(
        got,
        [
            ("\"id\"", Key),
            ("42", Number),
            ("\"name\"", Key),
            ("\"a \\\"q\\\" b\"", String),
            ("\"ok\"", Key),
            ("true", Bool),
            ("\"no\"", Key),
            ("false", Bool),
            ("\"x\"", Key),
            ("null", Null),
            ("\"n\"", Key),
            ("-1.5e3", Number),
            ("0", Number),
        ]
    );
}

#[test]
fn json_tokens_are_lenient() {
    // Unterminated string runs to the end; stray words are skipped.
    let text = "[nope, \"open";
    let tokens = json_tokens(text);
    assert_eq!(tokens, [(7..text.len(), JsonToken::String)]);
    // Trailing backslash must not run past the end.
    assert_eq!(json_tokens("\"a\\").len(), 1);
    // Non-ASCII inside strings keeps boundaries valid.
    let text = r#"{"ключ": "значение"}"#;
    for (r, _) in json_tokens(text) {
        assert!(text.is_char_boundary(r.start) && text.is_char_boundary(r.end));
    }
}

#[test]
fn layer_spans_splits_and_combines() {
    let base = [(0..4, 'a'), (6..10, 'b')];
    let overlay = [(2..8, 'X')];
    let out = layer_spans(&base, &overlay, |b, o| if *b == 'a' { 'A' } else { *o });
    assert_eq!(
        out,
        [
            (0..2, 'a'),
            (2..4, 'A'),
            (4..6, 'X'),
            (6..8, 'X'),
            (8..10, 'b')
        ]
    );
}

#[test]
fn layer_spans_handles_gaps_and_empty_inputs() {
    let base = [(5..6, 'b')];
    assert_eq!(
        layer_spans(&base, &[(0..2, 'o'), (8..9, 'o')], |_, o| *o),
        [(0..2, 'o'), (5..6, 'b'), (8..9, 'o')]
    );
    assert_eq!(layer_spans::<char>(&[], &[], |_, o| *o), []);
    assert_eq!(layer_spans(&base, &[], |_, o| *o), base);
    // One base span covered by two overlay spans.
    let out = layer_spans(&[(0..10, 'b')], &[(1..2, 'o'), (4..5, 'o')], |_, _| 'm');
    assert_eq!(
        out,
        [
            (0..1, 'b'),
            (1..2, 'm'),
            (2..4, 'b'),
            (4..5, 'm'),
            (5..10, 'b')
        ]
    );
}

#[test]
fn line_numbers() {
    let text = "a\nb\nc";
    assert_eq!(line_of(text, 0), 0);
    assert_eq!(line_of(text, 2), 1);
    assert_eq!(line_of(text, 4), 2);
    assert_eq!(line_of(text, 99), 2);
}
