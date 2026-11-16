//! Saving the variables that pre-request / test scripts set.

use crate::app::FerrumApi;
use ferrumapi_domain::script::{VarChange, VarScope};
use ferrumapi_domain::{Environment, EnvironmentId, Variable};
use gpui::Context;

/// Applies `changes` to `env`'s variables (a set updates the first row with
/// that key, or adds one; an unset removes every row with it).
pub fn apply_to(env: &mut Environment, changes: &[&VarChange]) {
    for c in changes {
        let key = c.key.trim();
        match &c.value {
            Some(value) => match env.variables.iter_mut().find(|v| v.key.trim() == key) {
                Some(v) => {
                    v.value.clone_from(value);
                    v.enabled = true;
                }
                None => env.variables.push(Variable::new(key, value.clone())),
            },
            None => env.variables.retain(|v| v.key.trim() != key),
        }
    }
}

impl FerrumApi {
    /// Saves script changes to the active environment (`env.`; the globals
    /// when none is active) and the globals (`global.`). `var.` changes
    /// only live for the send or run that made them.
    pub fn apply_script_changes(&mut self, changes: &[VarChange], cx: &mut Context<Self>) {
        let env_target = self.state.active_env.unwrap_or(EnvironmentId::GLOBALS);
        let targets = [
            (env_target, VarScope::Environment),
            (EnvironmentId::GLOBALS, VarScope::Global),
        ];
        let mut touched: Vec<Environment> = Vec::new();
        for (id, scope) in targets {
            let picked: Vec<&VarChange> = changes.iter().filter(|c| c.scope == scope).collect();
            if picked.is_empty() {
                continue;
            }
            let existing = touched.iter_mut().find(|e| e.id == id);
            match existing {
                Some(env) => apply_to(env, &picked),
                None => {
                    let Some(mut env) = self.environment(id) else {
                        continue;
                    };
                    apply_to(&mut env, &picked);
                    touched.push(env);
                }
            }
        }
        for env in touched {
            self.save_environment(env, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_update_or_add_and_unsets_remove() {
        let mut env = Environment::new("e");
        let mut off = Variable::new("token", "old");
        off.enabled = false;
        env.variables = vec![off, Variable::new("gone", "x")];
        let changes = [
            VarChange {
                scope: VarScope::Environment,
                key: "token".into(),
                value: Some("new".into()),
            },
            VarChange {
                scope: VarScope::Environment,
                key: "added".into(),
                value: Some("1".into()),
            },
            VarChange {
                scope: VarScope::Environment,
                key: "gone".into(),
                value: None,
            },
        ];
        apply_to(&mut env, &changes.iter().collect::<Vec<_>>());
        let got: Vec<(&str, &str, bool)> = env
            .variables
            .iter()
            .map(|v| (v.key.as_str(), v.value.as_str(), v.enabled))
            .collect();
        assert_eq!(got, [("token", "new", true), ("added", "1", true)]);
    }
}
