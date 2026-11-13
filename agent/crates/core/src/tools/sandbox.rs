//! Keeps every tool path inside the project root: relative paths resolve
//! against it, `..` may not climb out, absolute paths must point inside,
//! and symlinks are resolved so a link cannot lead outside either. Writes
//! are also refused inside `.git`.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    #[error("path is empty")]
    Empty,
    #[error("{0} is outside the project")]
    Outside(String),
    #[error("{0} is inside .git; the agent may not write there")]
    GitDir(String),
    #[error("{0}")]
    Io(String),
}

#[derive(Debug, Clone)]
pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    /// `root` is canonicalized (it must exist).
    pub fn new(root: &Path) -> Result<Self, SandboxError> {
        let root = root
            .canonicalize()
            .map_err(|e| SandboxError::Io(format!("{}: {e}", root.display())))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The absolute path for `input` (relative to the root, or absolute),
    /// if it stays inside the root once `..` and symlinks are resolved.
    pub fn resolve(&self, input: &str) -> Result<PathBuf, SandboxError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(SandboxError::Empty);
        }
        let raw = Path::new(input);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.root.join(raw)
        };
        // Lexical normalization first: `..` can never climb above `/`.
        let mut normal = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::ParentDir => {
                    if !normal.pop() {
                        return Err(SandboxError::Outside(input.into()));
                    }
                }
                Component::CurDir => {}
                other => normal.push(other.as_os_str()),
            }
        }
        // Absolute input may name the root through a symlinked prefix
        // (e.g. /tmp vs /private/tmp): compare the real location of the
        // deepest existing ancestor.
        let real = real_path(&normal);
        if !real.starts_with(&self.root) {
            return Err(SandboxError::Outside(input.into()));
        }
        Ok(real)
    }

    /// Like [`Sandbox::resolve`], and not inside `.git`.
    pub fn resolve_for_write(&self, input: &str) -> Result<PathBuf, SandboxError> {
        let path = self.resolve(input)?;
        if self
            .relative(&path)
            .components()
            .any(|c| c.as_os_str() == ".git")
        {
            return Err(SandboxError::GitDir(input.into()));
        }
        if path == self.root {
            return Err(SandboxError::Outside(input.into()));
        }
        Ok(path)
    }

    /// `path` relative to the root (`path` must come from `resolve`).
    pub fn relative(&self, path: &Path) -> PathBuf {
        path.strip_prefix(&self.root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf())
    }

    /// Display form: relative with `/`.
    pub fn display(&self, path: &Path) -> String {
        let rel = self.relative(path);
        let text = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        if text.is_empty() { ".".into() } else { text }
    }
}

/// `path` with its deepest existing ancestor canonicalized (symlinks
/// resolved) and the missing tail appended.
fn real_path(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (
            existing.file_name().map(|n| n.to_os_string()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("project");
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(dir.path().join("secret.txt"), "s").expect("write");
        let sandbox = Sandbox::new(&root).expect("sandbox");
        let real_root = root.canonicalize().expect("canonical");

        assert_eq!(
            sandbox.resolve("src/main.rs").expect("inside"),
            real_root.join("src/main.rs")
        );
        assert_eq!(
            sandbox.resolve("./src/../src/x").expect("inside"),
            real_root.join("src/x")
        );
        assert_eq!(sandbox.resolve(".").expect("root"), real_root);
        for bad in [
            "../secret.txt",
            "src/../../secret.txt",
            "../../../../../../etc/passwd",
            "/etc/passwd",
        ] {
            assert!(
                matches!(sandbox.resolve(bad), Err(SandboxError::Outside(_))),
                "{bad} escaped"
            );
        }
        let absolute_inside = root.join("src").to_string_lossy().to_string();
        assert!(sandbox.resolve(&absolute_inside).is_ok());
        assert_eq!(sandbox.resolve(" "), Err(SandboxError::Empty));
        assert!(matches!(
            sandbox.resolve_for_write(".git/config"),
            Err(SandboxError::GitDir(_))
        ));
        assert!(sandbox.resolve_for_write(".").is_err(), "the root itself");
        assert_eq!(sandbox.display(&real_root.join("src/a.rs")), "src/a.rs");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_lead_outside() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("project");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&root).expect("mkdir");
        std::fs::create_dir_all(&outside).expect("mkdir");
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("symlink");
        std::os::unix::fs::symlink(dir.path().join("x.txt"), root.join("file-link"))
            .expect("symlink");
        let sandbox = Sandbox::new(&root).expect("sandbox");
        assert!(matches!(
            sandbox.resolve("link/new.txt"),
            Err(SandboxError::Outside(_))
        ));
        std::fs::write(dir.path().join("x.txt"), "x").expect("write");
        assert!(matches!(
            sandbox.resolve("file-link"),
            Err(SandboxError::Outside(_))
        ));
    }
}
