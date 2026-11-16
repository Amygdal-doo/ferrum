//! Pre-request and test scripts: a small, sandboxed statement language
//! (there is no JavaScript engine in the workspace). Scripts can only read
//! and set variables and check the response: there is no file system, no
//! network and no loop, so every script ends after one pass over its lines.
//!
//! One statement per line; `#` or `//` starts a comment.
//!
//! ```text
//! set env.token = json.data.token      # active environment (globals when none)
//! set global.seen = timestamp()
//! set var.id = json.items[0].id        # this send / collection run only
//! unset env.old
//! test "status is 200": status == 200
//! assert header("Content-Type") contains "json"
//! assert json.items is array and len(json.items) > 0
//! assert json.user.email exists
//! assert time < 500
//! log "token: " + env.token
//! ```
//!
//! Values: numbers, `"strings"` (with `{{variables}}` resolved), `true`,
//! `false`, `null`; `status`, `time` (ms), `size` (bytes), `body` (text),
//! `json` (parsed body; `.field`, `[index]`, `["key"]`, `.length`),
//! `headers` (`headers["content-type"]`), `env.x` / `global.x` / `var.x` /
//! `vars.x` (the value `{{x}}` resolves to). Operators: `== != < <= > >=`,
//! `contains`, `not contains`, `matches` (regex), `startswith`, `endswith`,
//! `exists`, `not exists`, `is [not] number|string|bool|array|object|null|empty`,
//! `+`, `and` / `&&`, `or` / `||`, `not` / `!`, `-`, parentheses. Functions:
//! `len`, `lower`, `upper`, `trim`, `string`, `number`, `header(name)`,
//! `uuid()`, `timestamp()`, `timestamp_ms()`, `random_int(min, max)`.

mod eval;
mod lex;
mod parse;

use crate::{HttpResponse, VariableSet};
use eval::{Ctx, display, shown, truthy};
use parse::{Expr, Op, Stmt};
use serde::{Deserialize, Serialize};

/// Longest script accepted.
pub const MAX_SCRIPT_BYTES: usize = 64 * 1024;

/// A request's scripts. Empty scripts are not serialized.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scripts {
    pub pre_request: String,
    pub tests: String,
}

impl Scripts {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pre_request.trim().is_empty() && self.tests.trim().is_empty()
    }
}

/// Where a `set` / `unset` goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarScope {
    /// The active environment; the globals when none is active.
    Environment,
    Global,
    /// This send, or the rest of a collection run.
    Run,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarChange {
    pub scope: VarScope,
    pub key: String,
    /// `None` unsets.
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    /// Why it failed (values compared, or the error).
    pub message: String,
}

/// What the scripts of one send did.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ScriptReport {
    pub tests: Vec<TestResult>,
    pub changes: Vec<VarChange>,
    pub logs: Vec<String>,
    /// Lines that could not run, as `pre-request line 3: …`.
    pub errors: Vec<String>,
}

impl ScriptReport {
    /// Every test passed and no line failed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.errors.is_empty() && self.tests.iter().all(|t| t.passed)
    }

    #[must_use]
    pub fn passed_count(&self) -> usize {
        self.tests.iter().filter(|t| t.passed).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tests.is_empty()
            && self.changes.is_empty()
            && self.logs.is_empty()
            && self.errors.is_empty()
    }

    pub fn extend(&mut self, other: Self) {
        self.tests.extend(other.tests);
        self.changes.extend(other.changes);
        self.logs.extend(other.logs);
        self.errors.extend(other.errors);
    }
}

/// Runs a pre-request script: `set` / `unset` update `vars` (used to send
/// the request) and are reported for the environment / globals.
#[must_use]
pub fn run_pre_request(source: &str, vars: &mut VariableSet) -> ScriptReport {
    run(source, "pre-request", vars, None)
}

/// Runs a test script against `response`.
#[must_use]
pub fn run_tests(source: &str, vars: &mut VariableSet, response: &HttpResponse) -> ScriptReport {
    run(source, "tests", vars, Some(response))
}

/// Checks a script without running it: `(line, message)` per bad line.
#[must_use]
pub fn check(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| parse::statement(line).err().map(|e| (i + 1, e)))
        .collect()
}

fn run(
    source: &str,
    phase: &str,
    vars: &mut VariableSet,
    response: Option<&HttpResponse>,
) -> ScriptReport {
    let mut report = ScriptReport::default();
    if source.len() > MAX_SCRIPT_BYTES {
        report.errors.push(format!(
            "{phase}: script is longer than {} KB",
            MAX_SCRIPT_BYTES / 1024
        ));
        return report;
    }
    let mut ctx = Ctx::new(vars, response);
    for (i, line) in source.lines().enumerate() {
        let at = |e: String| format!("{phase} line {}: {e}", i + 1);
        match parse::statement(line) {
            Ok(None) => {}
            Ok(Some(stmt)) => {
                if let Err(e) = execute(&stmt, &mut ctx, &mut report) {
                    report.errors.push(at(e));
                }
            }
            Err(e) => report.errors.push(at(e)),
        }
    }
    report
}

fn execute(stmt: &Stmt, ctx: &mut Ctx<'_>, report: &mut ScriptReport) -> Result<(), String> {
    match stmt {
        Stmt::Set(scope, key, expr) => {
            let value = display(&ctx.eval(expr)?.unwrap_or_default());
            ctx.vars.insert(key.clone(), value.clone());
            report.changes.push(VarChange {
                scope: *scope,
                key: key.clone(),
                value: Some(value),
            });
        }
        Stmt::Unset(scope, key) => {
            ctx.vars.remove(key);
            report.changes.push(VarChange {
                scope: *scope,
                key: key.clone(),
                value: None,
            });
        }
        Stmt::Log(expr) => {
            let v = ctx.eval(expr)?.unwrap_or_default();
            report.logs.push(display(&v));
        }
        Stmt::Test(name, expr) => {
            let (passed, message) = match check_expr(expr, ctx) {
                Ok(r) => r,
                Err(e) => (false, e),
            };
            report.tests.push(TestResult {
                name: name.clone(),
                passed,
                message,
            });
        }
    }
    Ok(())
}

/// Evaluates a check; a failed comparison says what was compared.
fn check_expr(expr: &Expr, ctx: &mut Ctx<'_>) -> Result<(bool, String), String> {
    let passed = truthy(ctx.eval(expr)?.as_ref());
    if passed {
        return Ok((true, String::new()));
    }
    let message = match expr {
        Expr::Bin(op, l, r) if !matches!(op, Op::And | Op::Or | Op::Add) => {
            let l = ctx.eval(l)?;
            let r = ctx.eval(r)?;
            let side = |v: Option<serde_json::Value>| {
                v.map_or_else(|| "missing".to_owned(), |v| shown(&v))
            };
            format!("{} {} {} is false", side(l), op.text(), side(r))
        }
        Expr::Exists(_, true) => "missing".to_owned(),
        Expr::Is(inner, ty, want) => {
            let v = ctx.eval(inner)?;
            let not = if *want { "" } else { "not " };
            let got = v.map_or_else(|| "missing".to_owned(), |v| shown(&v));
            format!("expected {not}{ty}, got {got}")
        }
        _ => "is false".to_owned(),
    };
    Ok((false, message))
}

#[cfg(test)]
mod tests;
