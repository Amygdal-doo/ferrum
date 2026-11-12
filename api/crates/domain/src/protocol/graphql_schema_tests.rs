use super::*;

const INTROSPECTION: &str = r#"{"data":{"__schema":{
  "queryType":{"name":"Query"},"mutationType":{"name":"Mutation"},"subscriptionType":null,
  "types":[
    {"kind":"OBJECT","name":"Query","fields":[
      {"name":"user","args":[{"name":"id","type":{"kind":"NON_NULL","name":null,"ofType":{"kind":"SCALAR","name":"ID"}}}],
       "type":{"kind":"OBJECT","name":"User"}},
      {"name":"users","args":[],"type":{"kind":"NON_NULL","ofType":{"kind":"LIST","ofType":{"kind":"NON_NULL","ofType":{"kind":"OBJECT","name":"User"}}}}}
    ]},
    {"kind":"OBJECT","name":"Mutation","fields":[{"name":"rename","args":[],"type":{"kind":"OBJECT","name":"User"}}]},
    {"kind":"OBJECT","name":"User","fields":[
      {"name":"id","args":[],"type":{"kind":"SCALAR","name":"ID"}},
      {"name":"name","args":[],"type":{"kind":"SCALAR","name":"String"}},
      {"name":"friends","args":[],"type":{"kind":"LIST","ofType":{"kind":"OBJECT","name":"User"}}}
    ]},
    {"kind":"ENUM","name":"Role","enumValues":[{"name":"ADMIN"},{"name":"USER"}]},
    {"kind":"OBJECT","name":"__Type","fields":[]}
  ]}}}"#;

fn schema() -> GqlSchema {
    parse_introspection(INTROSPECTION.as_bytes()).expect("parse")
}

#[test]
fn introspection_is_read_into_types_with_rendered_type_refs() {
    let s = schema();
    assert_eq!(s.query_type.as_deref(), Some("Query"));
    assert_eq!(s.mutation_type.as_deref(), Some("Mutation"));
    let users = &s.find("Query").expect("Query").fields[1];
    assert_eq!(users.type_ref, "[User!]!");
    assert_eq!(users.base_type, "User");
    assert_eq!(
        s.find("Query").expect("Query").fields[0].args[0].type_ref,
        "ID!"
    );
    assert_eq!(s.find("Role").expect("Role").enum_values, ["ADMIN", "USER"]);
    let names: Vec<_> = s.user_types().iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Query", "Mutation", "Role", "User"]);
}

#[test]
fn errors_explain_a_missing_schema() {
    let err = parse_introspection(br#"{"errors":[{"message":"introspection disabled"}]}"#)
        .expect_err("no schema");
    assert_eq!(err, "introspection disabled");
    assert!(parse_introspection(b"<html>").is_err());
}

#[test]
fn completion_follows_the_selection_set() {
    let s = schema();
    let names = |text: &str| -> Vec<String> {
        complete_fields(&s, text, text.len(), 10)
            .iter()
            .map(|f| f.name.clone())
            .collect()
    };
    assert_eq!(names("{ us"), ["user", "users"]);
    assert_eq!(
        names("query Q { user(id: \"1\") { "),
        ["id", "name", "friends"]
    );
    assert_eq!(names("{ users { friends { na"), ["name"]);
    assert_eq!(names("mutation { re"), ["rename"]);
    assert_eq!(names("{ users { id } us"), ["user", "users"]);
    assert!(names("{ nope { ").is_empty());
    assert_eq!(names("{ # comment {\n users { i"), ["id"]);
    assert_eq!(partial_word("{ user { na", 11), "na");
    assert_eq!(partial_word("{ ", 2), "");
}
