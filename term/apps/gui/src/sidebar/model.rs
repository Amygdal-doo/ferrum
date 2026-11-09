//! What the sidebar lists, built from the server's session tree: an "Agents"
//! section (blocks running Claude Code, Codex & co., or reporting a program
//! status) and one section per session with its tabs and their split panes.
//! Pure data so it can be tested without a window.

use super::agents::{is_agent, program_name};
use ferrumterm_protocol::{
    BlockId, BlockInfo, ProgramState, ProgramStatus, SessionId, SessionInfo, WindowInfo,
};
use std::collections::HashMap;

/// One clickable sidebar row; clicking focuses `block` in `session`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub session: SessionId,
    pub block: BlockId,
    pub title: String,
    /// Command and/or working directory, shortened.
    pub detail: String,
    pub status: Option<ProgramStatus>,
    /// Indented under its tab (a split pane).
    pub nested: bool,
    /// The pane on screen right now.
    pub active: bool,
    pub attention: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub session: SessionId,
    pub title: String,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Model {
    pub agents: Vec<Row>,
    pub sessions: Vec<Section>,
    /// Agents that are blocked, or finished / failed since you last looked.
    pub attention: usize,
}

/// `updated_ms` of the `done` / `error` status last shown on screen, per block.
#[derive(Debug, Default)]
pub struct Seen(HashMap<BlockId, u64>);

impl Seen {
    /// Remembers the status of every block in `window` (it is on screen).
    pub fn look_at(&mut self, window: &WindowInfo) {
        for b in &window.blocks {
            if let Some(s) = &b.status {
                self.0.insert(b.id, s.updated_ms);
            }
        }
    }

    /// Drops blocks that no longer exist.
    pub fn retain(&mut self, sessions: &[SessionInfo]) {
        self.0.retain(|id, _| {
            sessions.iter().flat_map(|s| &s.windows).flat_map(|w| &w.blocks).any(|b| b.id == *id)
        });
    }

    /// Blocked always needs you; done / error only until you have seen them.
    #[must_use]
    pub fn needs_attention(&self, b: &BlockInfo) -> bool {
        let Some(s) = &b.status else { return false };
        match s.state {
            ProgramState::Blocked => true,
            ProgramState::Done | ProgramState::Error => self.0.get(&b.id) != Some(&s.updated_ms),
            ProgramState::Working | ProgramState::Idle => false,
        }
    }
}

impl Model {
    /// `visible` is the window on screen (its focused pane is marked active).
    #[must_use]
    pub fn build(sessions: &[SessionInfo], visible: Option<&WindowInfo>, seen: &Seen) -> Self {
        let mut model = Self::default();
        for s in sessions {
            let mut section = Section { session: s.id, title: s.name.clone(), rows: Vec::new() };
            for w in &s.windows {
                let shown = visible.is_some_and(|v| v.id == w.id);
                for (i, b) in w.blocks.iter().enumerate() {
                    let mut row = Row {
                        session: s.id,
                        block: b.id,
                        title: if i == 0 { window_title(w, b) } else { block_title(b) },
                        detail: detail(b),
                        status: b.status.clone(),
                        nested: i > 0,
                        active: shown && w.focused == b.id,
                        attention: seen.needs_attention(b),
                    };
                    if is_agent(b) {
                        model.attention += usize::from(row.attention);
                        let mut agent = row.clone();
                        agent.title = block_title(b);
                        agent.nested = false;
                        agent.detail = format!("{} · {}", s.name, row.detail);
                        model.agents.push(agent);
                    }
                    // The Agents section carries the badge; don't double-flag the tab.
                    row.attention = false;
                    section.rows.push(row);
                }
            }
            model.sessions.push(section);
        }
        model
    }
}

/// The tab's name when it was given one, else its first pane's title.
fn window_title(w: &WindowInfo, b: &BlockInfo) -> String {
    if w.name.is_empty() || w.name.chars().all(|c| c.is_ascii_digit()) {
        block_title(b)
    } else {
        w.name.clone()
    }
}

/// Label (`-l`), else the terminal title, else the program name.
#[must_use]
pub fn block_title(b: &BlockInfo) -> String {
    if let Some(l) = b.label.as_ref().filter(|l| !l.is_empty()) {
        return l.clone();
    }
    if !b.title.is_empty() {
        return b.title.clone();
    }
    program_name(&b.command).map_or_else(|| "shell".into(), str::to_owned)
}

fn detail(b: &BlockInfo) -> String {
    let cwd = b.cwd.as_deref().map(tilde);
    let program = program_name(&b.command);
    let mut d = match (program, cwd) {
        (Some(p), Some(c)) => format!("{p} · {c}"),
        (Some(p), None) => p.to_owned(),
        (None, Some(c)) => c,
        (None, None) => String::new(),
    };
    if let Some(code) = b.exited {
        d = format!("exited {code} · {d}");
    }
    d
}

/// `/Users/me/src/x` → `~/src/x`.
fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match path.strip_prefix(home.as_str()) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => path.to_owned(),
        },
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::{Layout, WindowId};

    fn block(
        id: u32,
        command: &[&str],
        label: Option<&str>,
        state: Option<ProgramState>,
    ) -> BlockInfo {
        BlockInfo {
            id: BlockId(id),
            label: label.map(str::to_owned),
            title: String::new(),
            command: command.iter().map(|s| (*s).to_owned()).collect(),
            cwd: Some("/work/app".into()),
            pid: None,
            cols: 80,
            rows: 24,
            status: state.map(|s| ProgramStatus { updated_ms: 7, ..ProgramStatus::new(s) }),
            exited: None,
        }
    }

    fn session(blocks: Vec<BlockInfo>) -> SessionInfo {
        let windows = blocks
            .into_iter()
            .enumerate()
            .map(|(i, b)| WindowInfo {
                id: WindowId(i as u32 + 1),
                name: format!("{}", i + 1),
                layout: Layout::Block(b.id),
                focused: b.id,
                zoomed: false,
                cols: 80,
                rows: 24,
                blocks: vec![b],
            })
            .collect::<Vec<_>>();
        SessionInfo {
            id: SessionId(1),
            name: "main".into(),
            created_ms: 0,
            active_window: WindowId(1),
            windows,
        }
    }

    #[test]
    fn agents_get_their_own_section_and_badge() {
        let sessions = vec![session(vec![
            block(1, &[], None, None),
            block(2, &["claude"], Some("claude"), Some(ProgramState::Blocked)),
            block(3, &["codex"], None, Some(ProgramState::Working)),
            block(4, &["npm", "run", "dev"], Some("server"), None),
        ])];
        let m = Model::build(&sessions, Some(&sessions[0].windows[0]), &Seen::default());
        let agents: Vec<_> = m.agents.iter().map(|r| r.block).collect();
        assert_eq!(agents, vec![BlockId(2), BlockId(3)]);
        assert_eq!(m.attention, 1, "only the blocked agent needs you");
        assert_eq!(m.sessions[0].rows.len(), 4, "every tab is still listed");
        assert!(m.sessions[0].rows[0].active);
        assert_eq!(m.sessions[0].rows[0].title, "shell");
        assert_eq!(m.sessions[0].rows[3].title, "server");
        assert_eq!(m.agents[0].detail, "main · claude · /work/app");
    }

    #[test]
    fn done_needs_attention_until_seen() {
        let sessions = vec![session(vec![
            block(1, &["claude"], None, Some(ProgramState::Done)),
            block(2, &["codex"], None, Some(ProgramState::Error)),
        ])];
        let mut seen = Seen::default();
        assert_eq!(Model::build(&sessions, None, &seen).attention, 2);
        seen.look_at(&sessions[0].windows[0]);
        let m = Model::build(&sessions, None, &seen);
        assert_eq!(m.attention, 1);
        assert!(!m.agents[0].attention && m.agents[1].attention);

        // A newer `done` is unseen again.
        let mut again = sessions.clone();
        if let Some(s) = again[0].windows[0].blocks[0].status.as_mut() {
            s.updated_ms = 8;
        }
        assert_eq!(Model::build(&again, None, &seen).attention, 2);

        seen.retain(&[]);
        assert!(seen.0.is_empty());
    }

    #[test]
    fn new_agent_labels_skip_taken_ones() {
        use super::super::agents::unique_label;
        let sessions = vec![session(vec![
            block(1, &["claude"], Some("claude"), None),
            block(2, &["claude"], Some("claude-2"), None),
        ])];
        assert_eq!(unique_label("claude", &sessions), "claude-3");
        assert_eq!(unique_label("codex", &sessions), "codex");
    }

    #[test]
    fn split_panes_are_nested_under_their_tab() {
        let mut s = session(vec![block(1, &[], None, None)]);
        let w = &mut s.windows[0];
        w.name = "logs".into();
        w.blocks.push(block(2, &["tail", "-f", "x.log"], None, None));
        let m = Model::build(&[s], None, &Seen::default());
        let rows = &m.sessions[0].rows;
        assert_eq!((rows[0].title.as_str(), rows[0].nested), ("logs", false));
        assert_eq!((rows[1].title.as_str(), rows[1].nested), ("tail", true));
        assert_eq!(rows[1].detail, "tail · /work/app");
    }
}
