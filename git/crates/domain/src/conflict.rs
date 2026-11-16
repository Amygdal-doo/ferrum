//! A conflicted file split into the parts both sides agree on and the
//! conflicts, from `git merge-file --diff3` output, plus the result of the
//! user's choice per conflict.

/// Conflict markers are read at this length (`git merge-file
/// --marker-size`), so a file that itself contains 7-character markers
/// cannot be mistaken for a conflict.
pub const PARSE_MARKER_SIZE: usize = 20;

/// The marker length written back for conflicts left unresolved (git's).
const MARKER_SIZE: usize = 7;

/// How one conflict is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictChoice {
    #[default]
    Unresolved,
    Ours,
    Theirs,
    /// Our lines, then theirs.
    Both,
    /// Their lines, then ours.
    BothTheirsFirst,
    /// The common ancestor's lines (neither change).
    Base,
}

impl ConflictChoice {
    #[must_use]
    pub const fn is_resolved(self) -> bool {
        !matches!(self, Self::Unresolved)
    }
}

/// A stretch of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSegment {
    /// Lines both sides agree on (or merged cleanly), with their line endings.
    Common(String),
    Conflict {
        ours: String,
        /// The ancestor's lines (`None` when the file was added on both sides).
        base: Option<String>,
        theirs: String,
    },
}

/// The conflicted file, split up.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConflictFile {
    pub path: String,
    pub segments: Vec<ConflictSegment>,
}

impl ConflictFile {
    /// Parses `git merge-file -p --diff3 --marker-size=PARSE_MARKER_SIZE`
    /// output. Text after an unterminated conflict stays a conflict whose
    /// sides are what was read.
    #[must_use]
    pub fn parse(path: &str, merged: &str) -> Self {
        let marker = |c: char| c.to_string().repeat(PARSE_MARKER_SIZE);
        let (start, base_m, sep, end) = (marker('<'), marker('|'), marker('='), marker('>'));
        let is = |line: &str, m: &str| {
            let line = line.trim_end_matches(['\n', '\r']);
            line == m
                || line
                    .strip_prefix(m)
                    .is_some_and(|rest| rest.starts_with(' '))
        };
        #[derive(PartialEq)]
        enum Part {
            Common,
            Ours,
            Base,
            Theirs,
        }
        let mut segments = Vec::new();
        let mut part = Part::Common;
        let (mut common, mut ours, mut theirs) = (String::new(), String::new(), String::new());
        let mut base: Option<String> = None;
        for line in merged.split_inclusive('\n') {
            match part {
                Part::Common if is(line, &start) => {
                    if !common.is_empty() {
                        segments.push(ConflictSegment::Common(std::mem::take(&mut common)));
                    }
                    part = Part::Ours;
                }
                Part::Common => common.push_str(line),
                Part::Ours if is(line, &base_m) => {
                    base = Some(String::new());
                    part = Part::Base;
                }
                Part::Ours | Part::Base if is(line, &sep) => part = Part::Theirs,
                Part::Ours => ours.push_str(line),
                Part::Base => base.get_or_insert_default().push_str(line),
                Part::Theirs if is(line, &end) => {
                    segments.push(ConflictSegment::Conflict {
                        ours: std::mem::take(&mut ours),
                        base: base.take(),
                        theirs: std::mem::take(&mut theirs),
                    });
                    part = Part::Common;
                }
                Part::Theirs => theirs.push_str(line),
            }
        }
        if part != Part::Common {
            segments.push(ConflictSegment::Conflict { ours, base, theirs });
        } else if !common.is_empty() {
            segments.push(ConflictSegment::Common(common));
        }
        Self {
            path: path.to_owned(),
            segments,
        }
    }

    /// Number of conflicts.
    #[must_use]
    pub fn conflicts(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| matches!(s, ConflictSegment::Conflict { .. }))
            .count()
    }

    /// The resolved file, once every conflict has a choice (`choices` holds
    /// one per conflict, in order).
    #[must_use]
    pub fn result(&self, choices: &[ConflictChoice]) -> Option<String> {
        (choices.len() == self.conflicts() && choices.iter().all(|c| c.is_resolved()))
            .then(|| self.draft(choices))
    }

    /// The file with the choices made so far; unresolved conflicts keep
    /// git's markers (for finishing by hand in an editor).
    #[must_use]
    pub fn draft(&self, choices: &[ConflictChoice]) -> String {
        let mut out = String::new();
        let mut n = 0;
        for segment in &self.segments {
            match segment {
                ConflictSegment::Common(text) => out.push_str(text),
                ConflictSegment::Conflict { ours, base, theirs } => {
                    let choice = choices.get(n).copied().unwrap_or_default();
                    n += 1;
                    let resolved = side(ours, base.as_deref(), theirs, choice);
                    match resolved {
                        Some(text) => out.push_str(&text),
                        None => {
                            let marker = |c: char, label: &str| {
                                let m = c.to_string().repeat(MARKER_SIZE);
                                if label.is_empty() {
                                    format!("{m}\n")
                                } else {
                                    format!("{m} {label}\n")
                                }
                            };
                            out.push_str(&marker('<', "ours"));
                            push_line(&mut out, ours);
                            if let Some(b) = base {
                                out.push_str(&marker('|', "base"));
                                push_line(&mut out, b);
                            }
                            out.push_str(&marker('=', ""));
                            push_line(&mut out, theirs);
                            out.push_str(&marker('>', "theirs"));
                        }
                    }
                }
            }
        }
        out
    }
}

/// The text of `choice`; `None` while unresolved.
fn side(ours: &str, base: Option<&str>, theirs: &str, choice: ConflictChoice) -> Option<String> {
    let joined = |a: &str, b: &str| {
        let mut s = String::new();
        push_line(&mut s, a);
        s.push_str(b);
        s
    };
    Some(match choice {
        ConflictChoice::Unresolved => return None,
        ConflictChoice::Ours => ours.to_owned(),
        ConflictChoice::Theirs => theirs.to_owned(),
        ConflictChoice::Both => joined(ours, theirs),
        ConflictChoice::BothTheirsFirst => joined(theirs, ours),
        ConflictChoice::Base => base.unwrap_or_default().to_owned(),
    })
}

/// Appends `text`, ending it with a newline when it has none (markers and
/// a following side must start on their own line).
fn push_line(out: &mut String, text: &str) {
    out.push_str(text);
    if !text.is_empty() && !text.ends_with('\n') {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(c: char) -> String {
        c.to_string().repeat(PARSE_MARKER_SIZE)
    }

    fn merged() -> String {
        format!(
            "a\n{} ours\nB1\n{} base\nb\n{}\nB2\n{} theirs\nc\n{} ours\nX\n{}\nY\n{} theirs\n",
            m('<'),
            m('|'),
            m('='),
            m('>'),
            m('<'),
            m('='),
            m('>')
        )
    }

    #[test]
    fn splits_common_text_and_conflicts() {
        let f = ConflictFile::parse("f.txt", &merged());
        assert_eq!(f.conflicts(), 2);
        assert_eq!(f.segments[0], ConflictSegment::Common("a\n".into()));
        assert_eq!(
            f.segments[1],
            ConflictSegment::Conflict {
                ours: "B1\n".into(),
                base: Some("b\n".into()),
                theirs: "B2\n".into(),
            }
        );
        assert_eq!(f.segments[2], ConflictSegment::Common("c\n".into()));
        assert!(matches!(
            &f.segments[3],
            ConflictSegment::Conflict { base: None, .. }
        ));
    }

    #[test]
    fn short_markers_in_the_file_are_content() {
        let text = format!(
            "<<<<<<< not a conflict\n{} ours\nx\n{}\ny\n{} theirs\n",
            m('<'),
            m('='),
            m('>')
        );
        let f = ConflictFile::parse("f", &text);
        assert_eq!(f.conflicts(), 1);
        assert_eq!(
            f.segments[0],
            ConflictSegment::Common("<<<<<<< not a conflict\n".into())
        );
    }

    #[test]
    fn result_needs_every_choice() {
        use ConflictChoice::*;
        let f = ConflictFile::parse("f.txt", &merged());
        assert_eq!(f.result(&[Ours]), None);
        assert_eq!(f.result(&[Ours, Unresolved]), None);
        assert_eq!(f.result(&[Ours, Theirs]).as_deref(), Some("a\nB1\nc\nY\n"));
        assert_eq!(
            f.result(&[Both, BothTheirsFirst]).as_deref(),
            Some("a\nB1\nB2\nc\nY\nX\n")
        );
        assert_eq!(f.result(&[Base, Ours]).as_deref(), Some("a\nb\nc\nX\n"));
    }

    #[test]
    fn draft_keeps_git_markers_for_open_conflicts() {
        use ConflictChoice::*;
        let f = ConflictFile::parse("f.txt", &merged());
        let draft = f.draft(&[Theirs, Unresolved]);
        assert_eq!(
            draft,
            "a\nB2\nc\n<<<<<<< ours\nX\n=======\nY\n>>>>>>> theirs\n"
        );
        // A side without a final newline still leaves markers on their own line.
        let f = ConflictFile {
            path: "f".into(),
            segments: vec![ConflictSegment::Conflict {
                ours: "x".into(),
                base: None,
                theirs: "y".into(),
            }],
        };
        assert_eq!(
            f.draft(&[]),
            "<<<<<<< ours\nx\n=======\ny\n>>>>>>> theirs\n"
        );
        assert_eq!(f.draft(&[Both]), "x\ny");
    }
}
