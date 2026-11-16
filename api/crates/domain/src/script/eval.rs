//! Evaluates expressions against the variables and (in tests) the response.
//! Values are JSON values; `None` is "missing" (an absent JSON path or
//! variable), which only `exists` tells apart from `null`.

use super::parse::{Expr, Op};
use crate::{HttpResponse, VariableSet};
use serde_json::{Number, Value};

/// Largest compiled `matches` pattern (bytes of the automaton).
const REGEX_SIZE_LIMIT: usize = 1 << 20;

pub(super) struct Ctx<'a> {
    pub vars: &'a mut VariableSet,
    pub response: Option<&'a HttpResponse>,
    /// Parsed response body, once asked for.
    json: Option<Value>,
}

impl<'a> Ctx<'a> {
    pub(super) fn new(vars: &'a mut VariableSet, response: Option<&'a HttpResponse>) -> Self {
        Self {
            vars,
            response,
            json: None,
        }
    }

    fn response(&self, what: &str) -> Result<&'a HttpResponse, String> {
        self.response
            .ok_or_else(|| format!("`{what}` is only available in test scripts"))
    }

    fn root(&mut self, name: &str) -> Result<Value, String> {
        let r = self.response(name)?;
        Ok(match name {
            "status" => Value::from(r.status),
            "time" => Value::from(r.time_ms),
            "size" => Value::from(r.size_bytes),
            "body" => Value::String(r.body_text()),
            "headers" => Value::Object(
                r.headers
                    .iter()
                    .rev()
                    .map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone())))
                    .collect(),
            ),
            _ => self
                .json
                .get_or_insert_with(|| serde_json::from_slice(&r.body).unwrap_or(Value::Null))
                .clone(),
        })
    }

    pub(super) fn eval(&mut self, e: &Expr) -> Result<Option<Value>, String> {
        Ok(Some(match e {
            Expr::Lit(v) => v.clone(),
            Expr::Str(s) => Value::String(self.vars.resolve(s)),
            Expr::Root(name) => self.root(name)?,
            Expr::Var(name) => match self.vars.get(name) {
                Some(v) => Value::String(v.to_owned()),
                None => return Ok(None),
            },
            Expr::Field(base, name) => {
                let Some(base) = self.eval(base)? else {
                    return Ok(None);
                };
                return Ok(member(&base, name));
            }
            Expr::Index(base, index) => {
                let (Some(base), Some(index)) = (self.eval(base)?, self.eval(index)?) else {
                    return Ok(None);
                };
                return Ok(match &index {
                    Value::String(key) => member(&base, key),
                    Value::Number(n) => n.as_f64().and_then(|i| element(&base, i)),
                    _ => None,
                });
            }
            Expr::Call(name, args) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(self.eval(a)?.unwrap_or(Value::Null));
                }
                self.call(name, &values)?
            }
            Expr::Not(inner) => Value::Bool(!truthy(self.eval(inner)?.as_ref())),
            Expr::Neg(inner) => {
                let v = self.eval(inner)?.unwrap_or(Value::Null);
                number(-to_f64(&v).ok_or("`-` needs a number")?)
            }
            Expr::Exists(inner, want) => Value::Bool(self.eval(inner)?.is_some() == *want),
            Expr::Is(inner, ty, want) => {
                let v = self.eval(inner)?;
                Value::Bool(is_type(v.as_ref(), ty)? == *want)
            }
            Expr::Bin(Op::And, l, r) => {
                let ok = truthy(self.eval(l)?.as_ref()) && truthy(self.eval(r)?.as_ref());
                Value::Bool(ok)
            }
            Expr::Bin(Op::Or, l, r) => {
                let ok = truthy(self.eval(l)?.as_ref()) || truthy(self.eval(r)?.as_ref());
                Value::Bool(ok)
            }
            Expr::Bin(op, l, r) => {
                let l = self.eval(l)?.unwrap_or(Value::Null);
                let r = self.eval(r)?.unwrap_or(Value::Null);
                binary(*op, &l, &r)?
            }
        }))
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Null);
        let text = |i: usize| display(&arg(i));
        let arity = |n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!("{name}() takes {n} argument(s)"))
            }
        };
        Ok(match name {
            "len" | "length" => {
                arity(1)?;
                Value::from(match arg(0) {
                    Value::String(s) => s.chars().count(),
                    Value::Array(a) => a.len(),
                    Value::Object(o) => o.len(),
                    _ => 0,
                })
            }
            "lower" => {
                arity(1)?;
                Value::String(text(0).to_lowercase())
            }
            "upper" => {
                arity(1)?;
                Value::String(text(0).to_uppercase())
            }
            "trim" => {
                arity(1)?;
                Value::String(text(0).trim().to_owned())
            }
            "string" => {
                arity(1)?;
                Value::String(text(0))
            }
            "number" => {
                arity(1)?;
                to_f64(&arg(0)).map_or(Value::Null, number)
            }
            "header" => {
                arity(1)?;
                let r = self.response("header()")?;
                r.header(&text(0))
                    .map_or(Value::Null, |v| Value::String(v.to_owned()))
            }
            "uuid" => {
                arity(0)?;
                Value::String(uuid::Uuid::new_v4().to_string())
            }
            "timestamp" => {
                arity(0)?;
                Value::from(chrono::Utc::now().timestamp())
            }
            "timestamp_ms" => {
                arity(0)?;
                Value::from(chrono::Utc::now().timestamp_millis())
            }
            "random_int" => {
                arity(2)?;
                let (lo, hi) = (to_f64(&arg(0)), to_f64(&arg(1)));
                let (Some(lo), Some(hi)) = (lo, hi) else {
                    return Err("random_int() needs two numbers".into());
                };
                let (lo, hi) = (lo.floor() as i64, hi.floor() as i64);
                if hi < lo {
                    return Err("random_int(min, max) needs min <= max".into());
                }
                let span = (hi - lo) as u128 + 1;
                let r = uuid::Uuid::new_v4().as_u128() % span;
                Value::from(lo + r as i64)
            }
            other => return Err(format!("unknown function `{other}()`")),
        })
    }
}

/// `base.name`; `length` of arrays and strings as in JavaScript.
fn member(base: &Value, name: &str) -> Option<Value> {
    match base {
        Value::Object(map) => map.get(name).cloned().or_else(|| {
            map.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        }),
        Value::Array(a) if name == "length" => Some(Value::from(a.len())),
        Value::String(s) if name == "length" => Some(Value::from(s.chars().count())),
        Value::Array(_) => name.parse::<f64>().ok().and_then(|i| element(base, i)),
        _ => None,
    }
}

/// `base[i]`; negative indexes count from the end.
fn element(base: &Value, i: f64) -> Option<Value> {
    let Value::Array(a) = base else { return None };
    let i = i as i64;
    let i = if i < 0 { a.len() as i64 + i } else { i };
    usize::try_from(i).ok().and_then(|i| a.get(i)).cloned()
}

pub(super) fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Value::from(n as i64)
    } else {
        Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
}

fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

pub(super) fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// Text of a value as `set` stores it and messages show it.
pub(super) fn display(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Short form for test messages: strings quoted, long values cut.
pub(super) fn shown(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 80 {
        format!("{}…", s.chars().take(80).collect::<String>())
    } else {
        s
    }
}

fn is_type(v: Option<&Value>, ty: &str) -> Result<bool, String> {
    Ok(match ty {
        "number" => matches!(v, Some(Value::Number(_))),
        "string" => matches!(v, Some(Value::String(_))),
        "bool" | "boolean" => matches!(v, Some(Value::Bool(_))),
        "array" | "list" => matches!(v, Some(Value::Array(_))),
        "object" => matches!(v, Some(Value::Object(_))),
        "null" => matches!(v, Some(Value::Null)),
        "empty" => match v {
            None | Some(Value::Null) => true,
            Some(Value::String(s)) => s.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::Object(o)) => o.is_empty(),
            Some(_) => false,
        },
        other => {
            return Err(format!(
                "unknown type `{other}` (number, string, bool, array, object, null, empty)"
            ));
        }
    })
}

/// Equality that lets `status == "200"` and `1 == 1.0` hold.
pub(super) fn loose_eq(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Number(_), Value::Number(_) | Value::String(_))
        | (Value::String(_), Value::Number(_)) => match (to_f64(l), to_f64(r)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        },
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| loose_eq(x, y))
        }
        _ => l == r,
    }
}

fn binary(op: Op, l: &Value, r: &Value) -> Result<Value, String> {
    let both_text = || match (l, r) {
        (Value::String(a), Value::String(b)) => Ok((a.as_str(), b.as_str())),
        _ => Err(format!("`{}` needs two strings", op.text())),
    };
    Ok(Value::Bool(match op {
        Op::Add => {
            return Ok(match (l, r) {
                (Value::Number(_), Value::Number(_)) => {
                    number(to_f64(l).unwrap_or(0.0) + to_f64(r).unwrap_or(0.0))
                }
                _ => Value::String(display(l) + &display(r)),
            });
        }
        Op::Eq => loose_eq(l, r),
        Op::Ne => !loose_eq(l, r),
        Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            let ord = match (to_f64(l), to_f64(r)) {
                (Some(a), Some(b)) => a.partial_cmp(&b),
                _ => match (l, r) {
                    (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
                    _ => None,
                },
            }
            .ok_or_else(|| format!("cannot compare {} and {}", shown(l), shown(r)))?;
            match op {
                Op::Lt => ord.is_lt(),
                Op::Le => ord.is_le(),
                Op::Gt => ord.is_gt(),
                _ => ord.is_ge(),
            }
        }
        Op::Contains | Op::NotContains => {
            let found = match l {
                Value::String(s) => s.contains(&display(r)),
                Value::Array(a) => a.iter().any(|x| loose_eq(x, r)),
                Value::Object(o) => o.contains_key(&display(r)),
                _ => false,
            };
            found == (op == Op::Contains)
        }
        Op::StartsWith => {
            let (a, b) = both_text()?;
            a.starts_with(b)
        }
        Op::EndsWith => {
            let (a, b) = both_text()?;
            a.ends_with(b)
        }
        Op::Matches => {
            let pattern = display(r);
            let re = regex::RegexBuilder::new(&pattern)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|e| format!("bad pattern: {e}"))?;
            re.is_match(&display(l))
        }
        Op::And | Op::Or => return Err("and / or are evaluated lazily".into()),
    }))
}
