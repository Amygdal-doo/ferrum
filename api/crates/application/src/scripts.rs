//! Running a request's pre-request and test scripts around a send.
//! Scripts are pure (`ferrumapi_domain::script`): they see variables and the
//! response only; the request itself is the only network traffic.

use crate::Prepared;
use ferrumapi_domain::script::{self, ScriptReport, VarChange};
use ferrumapi_domain::{HttpResponse, VariableSet};

/// Runs the pre-request script; its `set` / `unset` apply to the
/// variables the request is sent with.
#[must_use]
pub fn pre_request(mut prepared: Prepared) -> (Prepared, ScriptReport) {
    let source = prepared.request.scripts.pre_request.clone();
    if source.trim().is_empty() {
        return (prepared, ScriptReport::default());
    }
    let report = script::run_pre_request(&source, &mut prepared.variables);
    (prepared, report)
}

/// Runs the test script (with the variables as the pre-request script left
/// them) and stores both reports on the response. Requests without
/// scripts get no report.
pub fn finish(response: &mut HttpResponse, pre: ScriptReport, tests: &str, vars: &mut VariableSet) {
    let mut report = pre;
    if !tests.trim().is_empty() {
        report.extend(script::run_tests(tests, vars, response));
    }
    if !report.is_empty() || !tests.trim().is_empty() {
        response.script = Some(report);
    }
}

/// Applies earlier scripts' changes (any scope) to the variables of a
/// later request in the same collection run.
pub fn apply_changes(vars: &mut VariableSet, changes: &[VarChange]) {
    for c in changes {
        match &c.value {
            Some(v) => vars.insert(c.key.clone(), v.clone()),
            None => vars.remove(&c.key),
        }
    }
}
