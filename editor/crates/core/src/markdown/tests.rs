use super::*;

fn plain(text: &str) -> Span {
    Span {
        text: text.to_string(),
        style: SpanStyle::default(),
    }
}

fn styled(text: &str, f: impl FnOnce(&mut SpanStyle)) -> Span {
    let mut style = SpanStyle::default();
    f(&mut style);
    Span {
        text: text.to_string(),
        style,
    }
}

fn text_of(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn atx_and_setext_headings() {
    let blocks = parse("# One\n## Two ##\n####### seven\n\nTitle\n=====\nSub\n---\n#nospace");
    assert_eq!(
        blocks[0],
        Block::Heading {
            level: 1,
            spans: vec![plain("One")]
        }
    );
    assert_eq!(
        blocks[1],
        Block::Heading {
            level: 2,
            spans: vec![plain("Two")]
        }
    );
    assert_eq!(blocks[2], Block::Paragraph(vec![plain("####### seven")]));
    assert_eq!(
        blocks[3],
        Block::Heading {
            level: 1,
            spans: vec![plain("Title")]
        }
    );
    assert_eq!(
        blocks[4],
        Block::Heading {
            level: 2,
            spans: vec![plain("Sub")]
        }
    );
    assert_eq!(blocks[5], Block::Paragraph(vec![plain("#nospace")]));
}

#[test]
fn paragraphs_join_lines_and_hard_breaks() {
    let blocks = parse("one\ntwo\n\nthree  \nfour");
    assert_eq!(blocks[0], Block::Paragraph(vec![plain("one two")]));
    assert_eq!(blocks[1], Block::Paragraph(vec![plain("three\nfour")]));
}

#[test]
fn fenced_and_indented_code() {
    let blocks = parse(
        "```rust\nfn x() {}\n\n  y\n```\n~~~\n# not heading\n~~~\n\n    indented\n    code\n",
    );
    assert_eq!(
        blocks[0],
        Block::Code {
            lang: Some("rust".into()),
            text: "fn x() {}\n\n  y".into()
        }
    );
    assert_eq!(
        blocks[1],
        Block::Code {
            lang: None,
            text: "# not heading".into()
        }
    );
    assert_eq!(
        blocks[2],
        Block::Code {
            lang: None,
            text: "indented\ncode".into()
        }
    );
}

#[test]
fn unclosed_fence_runs_to_end() {
    let blocks = parse("```\na\nb");
    assert_eq!(
        blocks,
        vec![Block::Code {
            lang: None,
            text: "a\nb".into()
        }]
    );
}

#[test]
fn lists_nested_ordered_and_tasks() {
    let blocks = parse("- a\n  - b\n    continued\n- [x] done\n* [ ] todo\n\n3. three\n4) four\n");
    let summary: Vec<_> = blocks
        .iter()
        .map(|b| match b {
            Block::ListItem {
                depth,
                marker,
                checked,
                spans,
            } => (*depth, *marker, *checked, text_of(spans)),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (0, ListMarker::Bullet, None, "a".to_string()),
            (1, ListMarker::Bullet, None, "b continued".to_string()),
            (0, ListMarker::Bullet, Some(true), "done".to_string()),
            (0, ListMarker::Bullet, Some(false), "todo".to_string()),
            (0, ListMarker::Number(3), None, "three".to_string()),
            (0, ListMarker::Number(4), None, "four".to_string()),
        ]
    );
}

#[test]
fn rules_and_quotes() {
    let blocks = parse("---\n* * *\n> quoted **bold**\n> # Head\n\nafter");
    assert_eq!(blocks[0], Block::Rule);
    assert_eq!(blocks[1], Block::Rule);
    let Block::Quote(inner) = &blocks[2] else {
        panic!("quote expected")
    };
    assert_eq!(
        inner[0],
        Block::Paragraph(vec![plain("quoted "), styled("bold", |s| s.bold = true)])
    );
    assert_eq!(
        inner[1],
        Block::Heading {
            level: 1,
            spans: vec![plain("Head")]
        }
    );
    assert_eq!(blocks[3], Block::Paragraph(vec![plain("after")]));
}

#[test]
fn gfm_tables() {
    let blocks = parse("| a | b |\n|---|:-:|\n| 1 | `x` |\n| 2 |\n\nnext");
    let Block::Table { header, rows } = &blocks[0] else {
        panic!("table expected")
    };
    assert_eq!(
        header.iter().map(|c| text_of(c)).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][1], vec![styled("x", |s| s.code = true)]);
    assert!(rows[1][1].is_empty());
    assert_eq!(blocks[1], Block::Paragraph(vec![plain("next")]));
    // A pipe without a delimiter row is just text.
    assert_eq!(parse("a | b"), vec![Block::Paragraph(vec![plain("a | b")])]);
}

#[test]
fn inline_emphasis() {
    assert_eq!(
        parse_inline("**b** __B__ *i* _I_ ~~s~~"),
        vec![
            styled("b", |s| s.bold = true),
            plain(" "),
            styled("B", |s| s.bold = true),
            plain(" "),
            styled("i", |s| s.italic = true),
            plain(" "),
            styled("I", |s| s.italic = true),
            plain(" "),
            styled("s", |s| s.strike = true),
        ]
    );
    assert_eq!(
        parse_inline("*a **b** c*"),
        vec![
            styled("a ", |s| s.italic = true),
            styled("b", |s| {
                s.italic = true;
                s.bold = true
            }),
            styled(" c", |s| s.italic = true),
        ]
    );
}

#[test]
fn intraword_underscores_and_lone_stars_are_literal() {
    assert_eq!(
        parse_inline("snake_case_name"),
        vec![plain("snake_case_name")]
    );
    assert_eq!(parse_inline("2 * 3 * 4"), vec![plain("2 * 3 * 4")]);
    assert_eq!(parse_inline("**unclosed"), vec![plain("**unclosed")]);
}

#[test]
fn inline_code_and_escapes() {
    assert_eq!(
        parse_inline("use `a*b*c` and ``x ` y`` \\*lit\\*"),
        vec![
            plain("use "),
            styled("a*b*c", |s| s.code = true),
            plain(" and "),
            styled("x ` y", |s| s.code = true),
            plain(" *lit*"),
        ]
    );
}

#[test]
fn links_images_and_autolinks() {
    let link = |s: &mut SpanStyle| s.link = Some("https://x.dev".into());
    assert_eq!(
        parse_inline("see [the **site**](https://x.dev \"t\")!"),
        vec![
            plain("see "),
            styled("the ", link),
            styled("site", |s| {
                link(s);
                s.bold = true
            }),
            plain("!"),
        ]
    );
    assert_eq!(
        parse_inline("![logo](https://x.dev)"),
        vec![styled("logo", link)]
    );
    assert_eq!(
        parse_inline("<https://x.dev>"),
        vec![styled("https://x.dev", link)]
    );
    assert_eq!(
        parse_inline("<me@x.dev>"),
        vec![styled("me@x.dev", |s| s.link = Some("mailto:me@x.dev".into()))]
    );
    assert_eq!(
        parse_inline("[no link] a < b"),
        vec![plain("[no link] a < b")]
    );
}
