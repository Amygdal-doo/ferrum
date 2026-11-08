//! Rebuilds a patch containing one hunk (optionally only some of its lines)
//! for `git apply --cached` (stage), `--cached --reverse` (unstage) or
//! `--reverse` (discard). Line counts are fixed up by `git apply --recount`.

use ferrumgit_domain::{FileDiff, LineKind};

/// Header lines `git apply` needs: `diff --git`, mode lines, `---`, `+++`.
fn header(file: &FileDiff) -> String {
    let mut out = String::new();
    for l in &file.header {
        if l.starts_with("index ")
            || l.starts_with("similarity index")
            || l.starts_with("dissimilarity")
        {
            continue;
        }
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Patch for hunk `index`. `lines` selects line indexes inside the hunk;
/// `None` = the whole hunk. `reverse` must match how the patch is applied,
/// because unselected lines are neutralised differently in each direction:
/// forward drops unselected `+` and turns unselected `-` into context;
/// reverse drops unselected `-` and turns unselected `+` into context.
#[must_use]
pub fn hunk_patch(
    file: &FileDiff,
    index: usize,
    lines: Option<&[usize]>,
    reverse: bool,
) -> Option<String> {
    let hunk = file.hunks.get(index)?;
    let mut body = String::new();
    let mut changed = false;
    for (i, l) in hunk.lines.iter().enumerate() {
        let selected = lines.is_none_or(|sel| sel.contains(&i));
        let marker = match (l.kind, selected, reverse) {
            (LineKind::Context, ..) => Some(' '),
            (LineKind::NoNewline, ..) => {
                body.push_str(&l.content);
                body.push('\n');
                continue;
            }
            (LineKind::Added, true, _) => Some('+'),
            (LineKind::Removed, true, _) => Some('-'),
            (LineKind::Added, false, false) | (LineKind::Removed, false, true) => None,
            (LineKind::Added, false, true) | (LineKind::Removed, false, false) => Some(' '),
        };
        let Some(m) = marker else { continue };
        changed |= m != ' ';
        body.push(m);
        body.push_str(&l.content);
        body.push('\n');
    }
    if !changed {
        return None;
    }
    // Counts are recomputed by `git apply --recount`; keep the original starts.
    let h = format!(
        "@@ -{},{} +{},{} @@\n",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    );
    Some(format!("{}{h}{body}", header(file)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_diff;

    const P: &str = "diff --git a/f.txt b/f.txt
index 1..2 100644
--- a/f.txt
+++ b/f.txt
@@ -1,3 +1,3 @@
 a
-b
+B
 c
@@ -10,2 +10,3 @@
 x
+y
 z
";

    #[test]
    fn whole_hunk() {
        let f = &parse_diff(P)[0];
        let p = hunk_patch(f, 1, None, false).expect("patch");
        assert_eq!(
            p,
            "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -10,2 +10,3 @@\n x\n+y\n z\n"
        );
    }

    #[test]
    fn selected_lines_forward_and_reverse() {
        let f = &parse_diff(P)[0];
        // Only the "+B" line (index 2) selected.
        let fwd = hunk_patch(f, 0, Some(&[2]), false).expect("fwd");
        assert!(fwd.ends_with(" a\n b\n+B\n c\n"), "{fwd}");
        let rev = hunk_patch(f, 0, Some(&[2]), true).expect("rev");
        assert!(rev.ends_with(" a\n+B\n c\n"), "{rev}");
    }

    #[test]
    fn nothing_selected_is_none() {
        let f = &parse_diff(P)[0];
        assert!(hunk_patch(f, 0, Some(&[0]), false).is_none());
        assert!(hunk_patch(f, 5, None, false).is_none());
    }
}
