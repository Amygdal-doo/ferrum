//! Which project a process belongs to, from its working directory.
//!
//! Same rule as `ferrum_ui::project::project_root` (the git repository),
//! reimplemented here so the core stays free of GPUI. Daemons running in
//! `/` or `~` get no project rather than a meaningless one.

use std::path::{Path, PathBuf};

/// The top of the git repository containing `path`, if any.
#[must_use]
pub fn git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The project of a process working in `cwd`: its git repository, else
/// the folder itself when it is somewhere under the home folder (but not
/// the home folder or `~/Library`).
#[must_use]
pub fn project_for(cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(root) = git_root(cwd) {
        // A home folder kept in git (dotfiles) is not a project.
        if Some(root.as_path()) != home && root.parent().is_some() {
            return Some(root);
        }
    }
    let home = home?;
    let rest = cwd.strip_prefix(home).ok()?;
    let first = rest.components().next()?;
    let first = first.as_os_str().to_string_lossy();
    if first == "Library" || first.starts_with('.') {
        return None;
    }
    Some(cwd.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_folders_to_projects() {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path();
        let repo = home.join("code/shop");
        std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
        std::fs::create_dir_all(repo.join("apps/web")).expect("mkdir");
        std::fs::create_dir_all(home.join("scratch/x")).expect("mkdir");
        assert_eq!(project_for(&repo.join("apps/web"), Some(home)), Some(repo));
        assert_eq!(
            project_for(&home.join("scratch/x"), Some(home)),
            Some(home.join("scratch/x"))
        );
        assert_eq!(project_for(home, Some(home)), None);
        assert_eq!(project_for(&home.join("Library/Caches"), Some(home)), None);
        assert_eq!(project_for(&home.join(".cache/x"), Some(home)), None);
        assert_eq!(project_for(Path::new("/"), Some(home)), None);
        assert_eq!(project_for(Path::new("/"), None), None);
    }
}
