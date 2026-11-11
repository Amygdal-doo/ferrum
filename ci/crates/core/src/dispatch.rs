//! `workflow_dispatch` inputs read from a workflow file, and checking the
//! values the user typed before sending them.

use serde_yaml_ng::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    String,
    Boolean,
    Choice,
    Number,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchInput {
    pub name: String,
    pub description: String,
    pub required: bool,
    /// As text (`true`, `3`, `staging`).
    pub default: String,
    pub kind: InputKind,
    /// Choices for [`InputKind::Choice`].
    pub options: Vec<String>,
}

/// A workflow that can be started by hand, and its inputs in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DispatchSpec {
    pub inputs: Vec<DispatchInput>,
}

impl DispatchSpec {
    /// Initial values: each input's default.
    #[must_use]
    pub fn defaults(&self) -> BTreeMap<String, String> {
        self.inputs
            .iter()
            .map(|i| (i.name.clone(), i.default.clone()))
            .collect()
    }

    /// Checks `values` and returns what to send (empty optional inputs are
    /// left out so the workflow's defaults apply).
    pub fn validate(
        &self,
        values: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut out = BTreeMap::new();
        for input in &self.inputs {
            let value = values.get(&input.name).map_or("", |v| v.trim());
            if value.is_empty() {
                if input.required {
                    return Err(format!("“{}” is required.", input.name));
                }
                continue;
            }
            match input.kind {
                InputKind::Boolean if value != "true" && value != "false" => {
                    return Err(format!("“{}” must be true or false.", input.name));
                }
                InputKind::Number if value.parse::<f64>().is_err() => {
                    return Err(format!("“{}” must be a number.", input.name));
                }
                InputKind::Choice if !input.options.iter().any(|o| o == value) => {
                    return Err(format!(
                        "“{}” must be one of: {}.",
                        input.name,
                        input.options.join(", ")
                    ));
                }
                _ => {}
            }
            out.insert(input.name.clone(), value.to_owned());
        }
        Ok(out)
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// The `on:` section. YAML 1.1 readers turn a bare `on` into `true`, so
/// both keys are accepted.
fn triggers(doc: &Value) -> Option<&Value> {
    let map = doc.as_mapping()?;
    map.get(Value::String("on".into()))
        .or_else(|| map.get(Value::Bool(true)))
}

/// `Ok(None)` when the workflow has no `workflow_dispatch` trigger.
pub fn parse_dispatch(yaml: &str) -> Result<Option<DispatchSpec>, String> {
    let doc: Value = serde_yaml_ng::from_str(yaml)
        .map_err(|e| format!("Could not read the workflow file: {e}"))?;
    let Some(on) = triggers(&doc) else {
        return Ok(None);
    };
    let dispatch = match on {
        Value::String(s) if s == "workflow_dispatch" => return Ok(Some(DispatchSpec::default())),
        Value::Sequence(list) => {
            let found = list.iter().any(|v| v.as_str() == Some("workflow_dispatch"));
            return Ok(found.then(DispatchSpec::default));
        }
        Value::Mapping(map) => match map.get(Value::String("workflow_dispatch".into())) {
            Some(d) => d,
            None => return Ok(None),
        },
        _ => return Ok(None),
    };
    let mut spec = DispatchSpec::default();
    let Some(inputs) = dispatch.get("inputs").and_then(Value::as_mapping) else {
        return Ok(Some(spec));
    };
    for (name, def) in inputs {
        let name = text(name);
        if name.is_empty() {
            continue;
        }
        let field = |k: &str| def.get(k);
        let kind = match field("type").and_then(Value::as_str).unwrap_or("string") {
            "boolean" => InputKind::Boolean,
            "choice" => InputKind::Choice,
            "number" => InputKind::Number,
            "environment" => InputKind::Environment,
            _ => InputKind::String,
        };
        let options = field("options")
            .and_then(Value::as_sequence)
            .map(|o| o.iter().map(text).collect())
            .unwrap_or_default();
        let mut default = field("default").map(text).unwrap_or_default();
        if kind == InputKind::Boolean && default.is_empty() {
            default = "false".into();
        }
        spec.inputs.push(DispatchInput {
            name,
            description: field("description").map(text).unwrap_or_default(),
            required: field("required").and_then(Value::as_bool).unwrap_or(false),
            default,
            kind,
            options,
        });
    }
    Ok(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WF: &str = r"
name: Deploy
on:
  push:
    branches: [main]
  workflow_dispatch:
    inputs:
      environment:
        description: Where to deploy
        required: true
        type: choice
        options: [staging, production]
        default: staging
      dry_run:
        type: boolean
      replicas:
        type: number
        default: 2
      note:
        description: Free text
jobs: {}
";

    #[test]
    fn reads_inputs_in_order() {
        let spec = parse_dispatch(WF).expect("parse").expect("dispatchable");
        let names: Vec<&str> = spec.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["environment", "dry_run", "replicas", "note"]);
        let env = &spec.inputs[0];
        assert!(env.required);
        assert_eq!(env.kind, InputKind::Choice);
        assert_eq!(env.options, ["staging", "production"]);
        assert_eq!(env.default, "staging");
        assert_eq!(spec.inputs[1].default, "false");
        assert_eq!(spec.inputs[2].default, "2");
        assert_eq!(spec.inputs[3].kind, InputKind::String);
    }

    #[test]
    fn short_trigger_forms() {
        assert_eq!(
            parse_dispatch("on: workflow_dispatch\njobs: {}").expect("parse"),
            Some(DispatchSpec::default())
        );
        assert_eq!(
            parse_dispatch("on: [push, workflow_dispatch]").expect("parse"),
            Some(DispatchSpec::default())
        );
        assert_eq!(parse_dispatch("on: [push]").expect("parse"), None);
        assert_eq!(parse_dispatch("on:\n  push: {}").expect("parse"), None);
        assert!(parse_dispatch("on: [unclosed").is_err());
    }

    #[test]
    fn validates_values() {
        let spec = parse_dispatch(WF).expect("parse").expect("spec");
        let mut v = spec.defaults();
        let sent = spec.validate(&v).expect("defaults are valid");
        assert!(!sent.contains_key("note"));
        assert_eq!(sent["environment"], "staging");
        v.insert("environment".into(), "moon".into());
        assert!(
            spec.validate(&v)
                .expect_err("bad choice")
                .contains("staging")
        );
        v.insert("environment".into(), String::new());
        assert!(
            spec.validate(&v)
                .expect_err("required")
                .contains("required")
        );
        v.insert("environment".into(), "production".into());
        v.insert("replicas".into(), "many".into());
        assert!(spec.validate(&v).is_err());
        v.insert("replicas".into(), "3".into());
        v.insert("dry_run".into(), "yes".into());
        assert!(spec.validate(&v).is_err());
    }
}
