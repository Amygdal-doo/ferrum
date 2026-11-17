//! The rows the table shows: search, "only this project", grouping.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::Listener;

/// What the table is asked to show.
#[derive(Debug, Clone, Copy, Default)]
pub struct View<'a> {
    /// Search text; every word must match the port, process, command,
    /// project, address, user or port hint.
    pub query: &'a str,
    /// Only listeners of this project.
    pub project: Option<&'a Path>,
    pub grouped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// `ferrum — 3000 web, 5432 postgres`; `project` is `None` for "Other".
    Group {
        project: Option<PathBuf>,
        name: String,
        summary: String,
    },
    /// Index into the listener slice.
    Listener { index: usize, grouped: bool },
}

/// Whether `listener` matches every word of `query`.
#[must_use]
pub fn matches(listener: &Listener, query: &str) -> bool {
    let port = listener.port.to_string();
    let project = listener.project_name().unwrap_or_default();
    let hint = listener.hint().map(|h| h.description).unwrap_or_default();
    let container = listener
        .container
        .as_ref()
        .map(|c| format!("{} {}", c.name, c.image))
        .unwrap_or_default();
    let fields = [
        port.as_str(),
        listener.process.as_str(),
        listener.command.as_deref().unwrap_or_default(),
        project.as_str(),
        &listener.address_label(),
        listener.user.as_deref().unwrap_or_default(),
        hint,
        listener.protocol.label(),
        container.as_str(),
    ]
    .map(str::to_lowercase);
    query.split_whitespace().all(|word| {
        let word = word.trim_start_matches(':').to_lowercase();
        // A number matches ports by prefix only, so `30` finds 3000 but
        // not every pid containing 30.
        if word.chars().all(|c| c.is_ascii_digit()) && !word.is_empty() {
            return port.starts_with(&word) || listener.pid.is_some_and(|p| p.to_string() == word);
        }
        fields.iter().any(|f| f.contains(&word))
    })
}

/// Comma-separated `short_label`s of a group, e.g. `3000 web, 5432 postgres`.
#[must_use]
pub fn summary(listeners: &[&Listener]) -> String {
    listeners
        .iter()
        .map(|l| l.short_label())
        .collect::<Vec<_>>()
        .join(", ")
}

#[must_use]
pub fn rows(listeners: &[Listener], view: View<'_>) -> Vec<Row> {
    let visible: Vec<usize> = listeners
        .iter()
        .enumerate()
        .filter(|(_, l)| view.project.is_none_or(|p| l.project.as_deref() == Some(p)))
        .filter(|(_, l)| matches(l, view.query))
        .map(|(i, _)| i)
        .collect();
    if !view.grouped {
        return visible
            .into_iter()
            .map(|index| Row::Listener {
                index,
                grouped: false,
            })
            .collect();
    }
    // Projects by name; listeners without a project last, as "Other".
    let mut groups: BTreeMap<(bool, String, Option<PathBuf>), Vec<usize>> = BTreeMap::new();
    for index in visible {
        let l = &listeners[index];
        let name = l.project_name().unwrap_or_else(|| "Other".to_string());
        groups
            .entry((l.project.is_none(), name.to_lowercase(), l.project.clone()))
            .or_default()
            .push(index);
    }
    let mut rows = Vec::new();
    for ((_, _, project), indices) in groups {
        let members: Vec<&Listener> = indices.iter().map(|i| &listeners[*i]).collect();
        rows.push(Row::Group {
            name: members[0]
                .project_name()
                .unwrap_or_else(|| "Other".to_string()),
            summary: summary(&members),
            project,
        });
        rows.extend(indices.into_iter().map(|index| Row::Listener {
            index,
            grouped: true,
        }));
    }
    rows
}

/// Every listener of `pid` (the detail pane's "all ports").
#[must_use]
pub fn ports_of(listeners: &[Listener], pid: u32) -> Vec<&Listener> {
    listeners.iter().filter(|l| l.pid == Some(pid)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Protocol;

    fn listener(port: u16, process: &str, pid: u32, project: Option<&str>) -> Listener {
        Listener {
            protocol: Protocol::Tcp,
            port,
            addresses: vec!["127.0.0.1".into()],
            pid: Some(pid),
            process: process.into(),
            user: Some("me".into()),
            command: Some(format!("/usr/bin/{process} --serve")),
            started: None,
            cwd: None,
            project: project.map(PathBuf::from),
            container: None,
        }
    }

    fn sample() -> Vec<Listener> {
        vec![
            listener(3000, "node", 10, Some("/code/ferrum")),
            listener(5037, "adb", 11, None),
            listener(5432, "postgres", 12, Some("/code/ferrum")),
            listener(8080, "java", 13, Some("/code/Api")),
        ]
    }

    #[test]
    fn searches_fields() {
        let l = sample();
        assert!(matches(&l[0], ""));
        assert!(matches(&l[0], "30"));
        assert!(matches(&l[0], ":3000"));
        assert!(!matches(&l[0], "00"));
        assert!(matches(&l[0], "10"), "exact pid");
        assert!(matches(&l[0], "NODE ferrum"));
        assert!(matches(&l[0], "--serve"));
        assert!(matches(&l[2], "postgresql"), "hint description");
        assert!(!matches(&l[0], "node java"));
    }

    #[test]
    fn groups_by_project_with_summaries() {
        let l = sample();
        let rows = rows(
            &l,
            View {
                grouped: true,
                ..View::default()
            },
        );
        let names: Vec<String> = rows
            .iter()
            .map(|r| match r {
                Row::Group { name, summary, .. } => format!("[{name}: {summary}]"),
                Row::Listener { index, .. } => l[*index].port.to_string(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "[Api: 8080 http-alt]",
                "8080",
                "[ferrum: 3000 web, 5432 postgres]",
                "3000",
                "5432",
                "[Other: 5037 adb]",
                "5037"
            ]
        );
    }

    #[test]
    fn filters_to_one_project() {
        let l = sample();
        let project = PathBuf::from("/code/ferrum");
        let rows = rows(
            &l,
            View {
                query: "post",
                project: Some(&project),
                grouped: false,
            },
        );
        assert_eq!(
            rows,
            [Row::Listener {
                index: 2,
                grouped: false
            }]
        );
        assert_eq!(ports_of(&l, 12).len(), 1);
    }
}
