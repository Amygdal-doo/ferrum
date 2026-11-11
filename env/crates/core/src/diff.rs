//! Comparing environments: the "staging lacks STRIPE_KEY" report, and the
//! added/changed/removed preview shown before a bundle is imported.

use crate::bundle::BundleContents;
use crate::model::{Project, Variable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Changed,
    Removed,
    Unchanged,
}

/// One key in an import preview (keys only: values stay out of reports).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChange {
    pub key: String,
    pub kind: ChangeKind,
}

/// What importing `incoming` over `current` does to each key, in
/// `incoming` order followed by keys only `current` has.
#[must_use]
pub fn diff(current: &[Variable], incoming: &[Variable]) -> Vec<KeyChange> {
    let mut out: Vec<KeyChange> = incoming
        .iter()
        .map(|new| {
            let kind = match current.iter().find(|v| v.key == new.key) {
                None => ChangeKind::Added,
                Some(old) if old.value != new.value || old.comment != new.comment => {
                    ChangeKind::Changed
                }
                Some(_) => ChangeKind::Unchanged,
            };
            KeyChange {
                key: new.key.clone(),
                kind,
            }
        })
        .collect();
    out.extend(
        current
            .iter()
            .filter(|old| !incoming.iter().any(|v| v.key == old.key))
            .map(|old| KeyChange {
                key: old.key.clone(),
                kind: ChangeKind::Removed,
            }),
    );
    out
}

/// Two environments side by side.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    pub only_left: Vec<String>,
    pub only_right: Vec<String>,
    /// In both, with different values.
    pub differing: Vec<String>,
    pub same: usize,
}

#[must_use]
pub fn compare(left: &[Variable], right: &[Variable]) -> Comparison {
    let mut out = Comparison::default();
    for l in left {
        match right.iter().find(|r| r.key == l.key) {
            None => out.only_left.push(l.key.clone()),
            Some(r) if r.value != l.value => out.differing.push(l.key.clone()),
            Some(_) => out.same += 1,
        }
    }
    out.only_right = right
        .iter()
        .filter(|r| !left.iter().any(|l| l.key == r.key))
        .map(|r| r.key.clone())
        .collect();
    out
}

impl Comparison {
    /// Sentences such as "staging lacks STRIPE_KEY".
    #[must_use]
    pub fn missing_report(&self, left: &str, right: &str) -> Vec<String> {
        let lacks = |env: &str, keys: &[String]| {
            keys.iter()
                .map(|k| format!("{env} lacks {k}"))
                .collect::<Vec<_>>()
        };
        let mut out = lacks(right, &self.only_left);
        out.extend(lacks(left, &self.only_right));
        out
    }
}

/// The preview of importing one bundled environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentPlan {
    pub name: String,
    /// The project has no environment of this name yet.
    pub is_new: bool,
    pub changes: Vec<KeyChange>,
}

impl EnvironmentPlan {
    #[must_use]
    pub fn count(&self, kind: ChangeKind) -> usize {
        self.changes.iter().filter(|c| c.kind == kind).count()
    }
}

#[must_use]
pub fn plan_import(project: &Project, contents: &BundleContents) -> Vec<EnvironmentPlan> {
    contents
        .environments
        .iter()
        .map(|incoming| {
            let current = project
                .environment_index(&incoming.name)
                .map(|i| project.environments[i].variables.as_slice());
            EnvironmentPlan {
                name: incoming.name.clone(),
                is_new: current.is_none(),
                changes: diff(current.unwrap_or_default(), &incoming.variables),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportStats {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
}

/// Imports the bundle's environments into `project`: adds and updates
/// keys, and with `remove_missing` also deletes keys the bundle lacks
/// (only in the environments it carries).
pub fn apply_import(
    project: &mut Project,
    contents: &BundleContents,
    remove_missing: bool,
) -> ImportStats {
    let mut stats = ImportStats::default();
    for plan in plan_import(project, contents) {
        stats.added += plan.count(ChangeKind::Added);
        stats.changed += plan.count(ChangeKind::Changed);
        let Some(incoming) = contents.environments.iter().find(|e| e.name == plan.name) else {
            continue;
        };
        let env = project.environment_mut(&incoming.name);
        for var in &incoming.variables {
            env.upsert(var.clone());
        }
        if remove_missing {
            let before = env.variables.len();
            env.variables
                .retain(|v| incoming.variables.iter().any(|n| n.key == v.key));
            stats.removed += before - env.variables.len();
        }
    }
    project.touch();
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Environment;

    fn vars(pairs: &[(&str, &str)]) -> Vec<Variable> {
        pairs.iter().map(|(k, v)| Variable::new(*k, *v)).collect()
    }

    #[test]
    fn diff_classifies_keys() {
        let current = vars(&[("A", "1"), ("B", "2"), ("C", "3")]);
        let incoming = vars(&[("B", "2"), ("A", "9"), ("D", "4")]);
        let kinds: Vec<_> = diff(&current, &incoming)
            .into_iter()
            .map(|c| (c.key, c.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("B".to_string(), ChangeKind::Unchanged),
                ("A".to_string(), ChangeKind::Changed),
                ("D".to_string(), ChangeKind::Added),
                ("C".to_string(), ChangeKind::Removed),
            ]
        );
    }

    #[test]
    fn compare_reports_missing_keys() {
        let prod = vars(&[("STRIPE_KEY", "x"), ("PORT", "80"), ("DEBUG", "0")]);
        let staging = vars(&[("PORT", "8080"), ("DEBUG", "0"), ("SENTRY", "y")]);
        let c = compare(&prod, &staging);
        assert_eq!(c.only_left, ["STRIPE_KEY"]);
        assert_eq!(c.only_right, ["SENTRY"]);
        assert_eq!(c.differing, ["PORT"]);
        assert_eq!(c.same, 1);
        assert_eq!(
            c.missing_report("production", "staging"),
            ["staging lacks STRIPE_KEY", "production lacks SENTRY"]
        );
    }

    #[test]
    fn import_merges_or_replaces() {
        let mut project = Project::new("shop", None);
        project
            .environment_mut("staging")
            .variables
            .extend(vars(&[("A", "1"), ("OLD", "x")]));
        let mut staging = Environment::new("staging");
        staging.variables = vars(&[("A", "2"), ("NEW", "y")]);
        let mut qa = Environment::new("qa");
        qa.variables = vars(&[("Q", "1")]);
        let contents = BundleContents {
            project: "shop".into(),
            environments: vec![staging, qa],
            created_at: chrono::Utc::now(),
            note: String::new(),
        };
        let plan = plan_import(&project, &contents);
        assert!(!plan[0].is_new);
        assert!(plan[1].is_new);
        assert_eq!(plan[0].count(ChangeKind::Removed), 1);

        let mut merged = project.clone();
        let stats = apply_import(&mut merged, &contents, false);
        assert_eq!((stats.added, stats.changed, stats.removed), (2, 1, 0));
        let env = &merged.environments[merged.environment_index("staging").expect("staging")];
        assert_eq!(env.get("A").expect("A").value, "2");
        assert!(env.get("OLD").is_some());
        assert!(merged.environment_index("qa").is_some());

        let stats = apply_import(&mut project, &contents, true);
        assert_eq!(stats.removed, 1);
        let env = &project.environments[project.environment_index("staging").expect("staging")];
        assert!(env.get("OLD").is_none());
        // Environments not in the bundle are untouched.
        assert!(project.environment_index("production").is_some());
    }
}
