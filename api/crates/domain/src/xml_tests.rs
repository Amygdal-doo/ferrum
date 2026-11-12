use super::*;

#[test]
fn pretty_prints_nested_elements_and_keeps_text_inline() {
    let raw = r#"<?xml version="1.0"?><s:Envelope xmlns:s="ns"><s:Body><AddResponse><AddResult>5</AddResult><Empty/><Blank></Blank></AddResponse><!-- done --></s:Body></s:Envelope>"#;
    let pretty = pretty_xml(raw).expect("xml");
    assert_eq!(
        pretty,
        "<?xml version=\"1.0\"?>\n<s:Envelope xmlns:s=\"ns\">\n  <s:Body>\n    <AddResponse>\n      \
         <AddResult>5</AddResult>\n      <Empty/>\n      <Blank></Blank>\n    </AddResponse>\n    \
         <!-- done -->\n  </s:Body>\n</s:Envelope>"
    );
    assert_eq!(pretty_xml("{\"a\":1}"), None);
    assert_eq!(pretty_xml("</a></b>"), None);
    // Attribute values containing '>' do not end the tag.
    assert_eq!(
        pretty_xml("<a x=\"1>2\"><b/></a>").expect("xml"),
        "<a x=\"1>2\">\n  <b/>\n</a>"
    );
}

#[test]
fn tokens_cover_tags_attributes_and_comments() {
    let text = r#"<a x="1"><!-- c --><b y='2'/></a>"#;
    let toks: Vec<(&str, XmlToken)> = xml_tokens(text)
        .into_iter()
        .map(|(r, k)| (&text[r], k))
        .collect();
    assert_eq!(
        toks,
        [
            ("<a", XmlToken::Tag),
            ("x", XmlToken::AttrName),
            ("\"1\"", XmlToken::AttrValue),
            (">", XmlToken::Tag),
            ("<!-- c -->", XmlToken::Comment),
            ("<b", XmlToken::Tag),
            ("y", XmlToken::AttrName),
            ("'2'", XmlToken::AttrValue),
            ("/>", XmlToken::Tag),
            ("</a", XmlToken::Tag),
            (">", XmlToken::Tag),
        ]
    );
    assert!(looks_like_xml(Some("text/xml; charset=utf-8"), ""));
    assert!(looks_like_xml(None, "  <?xml version='1.0'?><a/>"));
    assert!(!looks_like_xml(Some("application/json"), "{}"));
}
