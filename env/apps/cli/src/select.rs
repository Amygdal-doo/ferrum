//! Picking the project and environment a command means, and the text
//! `fenv list` prints (names only unless values are asked for).

use std::path::{Path, PathBuf};

use ferrumenv_core::dotenv;
use ferrumenv_core::model::{Environment, Project, Vault};

/// `--project NAME`, or the project linked to the current folder (or the
/// closest parent folder that has one).
pub fn project<'a>(
    vault: &'a Vault,
    name: Option<&str>,
    cwd: &Path,
) -> Result<&'a Project, String> {
    if let Some(name) = name {
        return vault
            .project_by_name(name)
            .map(|i| &vault.projects[i])
            .ok_or_else(|| format!("no project named {name} (fenv list shows them)"));
    }
    // Compare resolved paths too (macOS: /var is /private/var).
    let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let cwd_real = real(cwd);
    let folders: Vec<(usize, PathBuf)> = vault
        .projects
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.path.as_deref().map(|path| (i, real(path))))
        .collect();
    cwd.ancestors()
        .find_map(|dir| vault.project_by_path(dir))
        .or_else(|| {
            cwd_real
                .ancestors()
                .find_map(|dir| folders.iter().find(|(_, f)| f == dir).map(|(i, _)| *i))
        })
        .map(|i| &vault.projects[i])
        .ok_or_else(|| {
            format!(
                "{} is not linked to a project: pass --project NAME",
                cwd.display()
            )
        })
}

/// An environment by exact name, else by unique prefix (`dev` →
/// `development`), case-insensitively.
pub fn environment<'a>(project: &'a Project, name: &str) -> Result<&'a Environment, String> {
    if let Some(i) = project.environment_index(name) {
        return Ok(&project.environments[i]);
    }
    let wanted = name.to_lowercase();
    let matches: Vec<&Environment> = project
        .environments
        .iter()
        .filter(|e| e.name.to_lowercase().starts_with(&wanted))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "{} has no environment {name} (it has: {})",
            project.name,
            names(project)
        )),
        _ => Err(format!(
            "{name} matches several environments of {}: {}",
            project.name,
            matches
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn names(project: &Project) -> String {
    project
        .environments
        .iter()
        .map(|e| e.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every project with its folder and environment sizes.
pub fn list_projects(vault: &Vault) -> String {
    let mut out = String::new();
    for p in &vault.projects {
        out.push_str(&p.name);
        if let Some(path) = &p.path {
            out.push_str(&format!("  ({})", path.display()));
        }
        out.push('\n');
        for e in &p.environments {
            out.push_str(&format!("  {}: {} variables\n", e.name, e.variables.len()));
        }
    }
    if out.is_empty() {
        out.push_str("The vault has no projects yet.\n");
    }
    out
}

/// Variable names of `envs`, one per line under each environment; with
/// `show`, `.env` lines with the values instead.
pub fn list_variables(envs: &[&Environment], show: bool) -> String {
    let mut out = String::new();
    for env in envs {
        if envs.len() > 1 {
            out.push_str(&format!("[{}]\n", env.name));
        }
        if show {
            out.push_str(&dotenv::to_dotenv(&env.variables));
        } else {
            for var in &env.variables {
                out.push_str(&var.key);
                out.push('\n');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumenv_core::Variable;
    use std::path::PathBuf;

    fn vault() -> Vault {
        let mut vault = Vault::default();
        let i = vault
            .add_project("shop", Some(PathBuf::from("/code/shop")))
            .expect("project");
        let env = vault.projects[i].environment_mut("development");
        env.upsert(Variable::new("API_KEY", "s3cret value"));
        env.upsert(Variable::new("PORT", "8080"));
        vault.add_project("other", None).expect("project");
        vault
    }

    #[test]
    fn projects_by_name_or_folder() {
        let v = vault();
        let cwd = Path::new("/somewhere");
        assert_eq!(project(&v, Some("SHOP"), cwd).expect("name").name, "shop");
        assert!(project(&v, Some("nope"), cwd).is_err());
        let inside = Path::new("/code/shop/src/bin");
        assert_eq!(project(&v, None, inside).expect("folder").name, "shop");
        assert!(project(&v, None, cwd).is_err());
    }

    #[test]
    fn environments_by_prefix() {
        let v = vault();
        let p = &v.projects[0];
        assert_eq!(environment(p, "dev").expect("dev").name, "development");
        assert_eq!(
            environment(p, "PRODUCTION").expect("prod").name,
            "production"
        );
        assert!(environment(p, "x").is_err());
        let mut q = p.clone();
        q.add_environment("devtools").expect("env");
        assert!(
            environment(&q, "dev")
                .expect_err("ambiguous")
                .contains("several")
        );
        assert_eq!(
            environment(&q, "development").expect("exact").name,
            "development"
        );
    }

    #[test]
    fn listing_hides_values_unless_shown() {
        let v = vault();
        let dev = &v.projects[0].environments[0];
        let names = list_variables(&[dev], false);
        assert_eq!(names, "API_KEY\nPORT\n");
        assert!(!names.contains("s3cret"));
        let shown = list_variables(&[dev], true);
        assert!(shown.contains("API_KEY='s3cret value'"));
        let all: Vec<&Environment> = v.projects[0].environments.iter().collect();
        assert!(list_variables(&all, false).starts_with("[development]\nAPI_KEY\n"));
        let projects = list_projects(&v);
        assert!(projects.contains("shop  (/code/shop)"));
        assert!(projects.contains("development: 2 variables"));
        assert!(!projects.contains("s3cret"));
    }
}
