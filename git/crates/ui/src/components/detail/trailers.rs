//! Hides git trailers (`Co-Authored-By:`, `Signed-off-by:`, …) from the
//! commit message shown in the details pane. The commit itself is untouched.

/// The message body without its trailing block of `Key: value` trailers.
pub(super) fn without_trailers(body: &str) -> &str {
    let body = body.trim_end();
    let start = body.rfind("\n\n").map_or(0, |i| i + 2);
    let last = &body[start..];
    if !last.is_empty() && last.lines().all(is_trailer) {
        body[..start].trim_end()
    } else {
        body
    }
}

fn is_trailer(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(key, value)| {
        !key.is_empty()
            && !value.trim().is_empty()
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::without_trailers;

    #[test]
    fn drops_the_trailer_block() {
        let body = "- one\n- two\n\nCo-Authored-By: A <a@b.c>\nClaude-Session: https://x/y\n";
        assert_eq!(without_trailers(body), "- one\n- two");
    }

    #[test]
    fn a_body_of_only_trailers_is_empty() {
        assert_eq!(without_trailers("Signed-off-by: A <a@b.c>"), "");
    }

    #[test]
    fn keeps_prose_and_lists() {
        let body = "Fixes the crash.\n\nNote: this also renames the flag, see below.\nMore text";
        assert_eq!(without_trailers(body), body);
        assert_eq!(without_trailers("- a\n- b"), "- a\n- b");
    }
}
