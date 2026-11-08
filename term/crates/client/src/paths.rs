//! Default locations. Override with `FTERM_STATE_DIR` / `FTERM_SOCKET`.

use std::path::PathBuf;

/// State directory: token, pid, logs, hosts. `~/Library/Application Support/dev.FerrumTerm.FerrumTerm` on macOS.
#[must_use]
pub fn state_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("FTERM_STATE_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("dev", "FerrumTerm", "FerrumTerm").map_or_else(
        || std::env::temp_dir().join("ferrumterm"),
        |d| d.data_dir().to_path_buf(),
    )
}

/// Unix socket. Kept short (`$TMPDIR/ferrumterm-<user>/server.sock`) because
/// socket paths are limited to ~104 bytes on macOS.
#[must_use]
pub fn socket() -> PathBuf {
    if let Some(s) = std::env::var_os("FTERM_SOCKET") {
        return PathBuf::from(s);
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
    let preferred = match std::env::var_os("FTERM_STATE_DIR") {
        Some(d) => PathBuf::from(d).join("server.sock"),
        None => std::env::temp_dir()
            .join(format!("ferrumterm-{user}"))
            .join("server.sock"),
    };
    short_enough(preferred, &user)
}

/// Unix socket paths must fit `sun_path` (104 bytes on macOS). Long paths
/// are replaced by a stable short one derived from them.
fn short_enough(path: PathBuf, user: &str) -> PathBuf {
    const LIMIT: usize = 100;
    if path.as_os_str().len() < LIMIT {
        return path;
    }
    // FNV-1a: stable across runs (unlike `DefaultHasher`), no dependency.
    let hash = path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
        });
    PathBuf::from("/tmp")
        .join(format!("fterm-{user}-{:08x}", hash as u32))
        .join("server.sock")
}

#[must_use]
pub fn log_file() -> PathBuf {
    state_dir().join("server.log")
}

#[must_use]
pub fn hosts_file() -> PathBuf {
    state_dir().join("hosts.json")
}

#[cfg(test)]
mod tests {
    use super::short_enough;
    use std::path::PathBuf;

    #[test]
    fn long_socket_paths_are_shortened_stably() {
        let short = PathBuf::from("/tmp/x/server.sock");
        assert_eq!(short_enough(short.clone(), "me"), short);
        let long = PathBuf::from(format!("/{}/server.sock", "a".repeat(150)));
        let a = short_enough(long.clone(), "me");
        assert_eq!(a, short_enough(long, "me"));
        assert!(a.as_os_str().len() < 100 && a.starts_with("/tmp"));
    }
}
