//! What the sidebar lists, built from the server's session tree: an "Agents"
//! section (blocks running Claude Code, Codex & co., or reporting a program
//! status) grouped by status — Needs you, Done, Working, Idle — and one
//! section per session with its tabs and their split panes. Pure data so it
//! can be tested without a window.

use super::agents::{agent_name, is_agent, program_name};
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
    /// The coding agent it runs, if it has a logo.
    pub logo: Option<ferrum_ui::AgentLogo>,
    /// Where it sits in the Agents section.
    pub group: AgentGroup,
    /// Agents: the project directory or the agent's own title, short.
    pub subtitle: String,
    /// Unix millis of the last status change.
    pub changed_ms: Option<u64>,
}

/// The Agents section's groups, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentGroup {
    /// Asking a question or waiting for permission.
    NeedsYou,
    /// Finished (or failed) a turn you have not looked at yet.
    Done,
    Working,
    Idle,
}

impl AgentGroup {
    pub const ALL: [Self; 4] = [Self::NeedsYou, Self::Done, Self::Working, Self::Idle];

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::Done => "Done",
            Self::Working => "Working",
            Self::Idle => "Idle",
        }
    }
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
    /// `agents` by group (non-empty groups only), most recent change first.
    pub groups: Vec<(AgentGroup, Vec<Row>)>,
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
            sessions
                .iter()
                .flat_map(|s| &s.windows)
                .flat_map(|w| &w.blocks)
                .any(|b| b.id == *id)
        });
    }

    /// Whether the block's current status was on screen.
    #[must_use]
    pub fn has_seen(&self, b: &BlockInfo) -> bool {
        b.status
            .as_ref()
            .is_some_and(|s| self.0.get(&b.id) == Some(&s.updated_ms))
    }

    /// The Agents group of `b`: done / error become idle once seen.
    #[must_use]
    pub fn group(&self, b: &BlockInfo) -> AgentGroup {
        match b.status.as_ref().map(|s| s.state) {
            Some(ProgramState::Blocked) => AgentGroup::NeedsYou,
            Some(ProgramState::Done | ProgramState::Error) if !self.has_seen(b) => AgentGroup::Done,
            Some(ProgramState::Working) => AgentGroup::Working,
            _ => AgentGroup::Idle,
        }
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
            let mut section = Section {
                session: s.id,
                title: s.name.clone(),
                rows: Vec::new(),
            };
            for w in &s.windows {
                let shown = visible.is_some_and(|v| v.id == w.id);
                for (i, b) in w.blocks.iter().enumerate() {
                    let mut row = Row {
                        session: s.id,
                        block: b.id,
                        title: if i == 0 {
                            window_title(w, b)
                        } else {
                            block_title(b)
                        },
                        detail: detail(b),
                        status: b.status.clone(),
                        nested: i > 0,
                        active: shown && w.focused == b.id,
                        attention: seen.needs_attention(b),
                        logo: agent_name(b)
                            .or_else(|| program_name(&b.command))
                            .and_then(ferrum_ui::AgentLogo::for_program),
                        group: seen.group(b),
                        subtitle: subtitle(b),
                        changed_ms: b.status.as_ref().map(|s| s.updated_ms).filter(|ms| *ms > 0),
                    };
                    if is_agent(b) {
                        model.attention += usize::from(row.attention);
                        let mut agent = row.clone();
                        agent.title = agent_title(w, b);
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
        for group in AgentGroup::ALL {
            let mut rows: Vec<Row> = model
                .agents
                .iter()
                .filter(|r| r.group == group)
                .cloned()
                .collect();
            rows.sort_by_key(|r| std::cmp::Reverse(r.changed_ms.unwrap_or(0)));
            if !rows.is_empty() {
                model.groups.push((group, rows));
            }
        }
        model
    }
}

/// An agent's second line: its own title (minus a leading spinner or
/// status glyph) when it is not just the row's title, else its directory.
fn subtitle(b: &BlockInfo) -> String {
    let title = clean_title(&b.title);
    let row_title = block_title(b);
    if !title.is_empty() && title != row_title && clean_title(&row_title) != title {
        return title.to_owned();
    }
    b.cwd.as_deref().map(tilde).unwrap_or_default()
}

/// `✳ Fix the tests` → `Fix the tests`.
fn clean_title(title: &str) -> &str {
    let t = title.trim();
    match t.split_once(' ') {
        Some((first, rest)) if !first.chars().any(char::is_alphanumeric) => rest.trim(),
        _ => t,
    }
}

/// `<1m`, `12m`, `3h`, `1d`: time from `then_ms` to `now_ms`.
#[must_use]
pub fn relative_time(now_ms: u64, then_ms: u64) -> String {
    let minutes = now_ms.saturating_sub(then_ms) / 60_000;
    match minutes {
        0 => "<1m".into(),
        m if m < 60 => format!("{m}m"),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m => format!("{}d", m / (60 * 24)),
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

/// An agent's name in the Agents section and its notifications: its label
/// (`-l`), else its tab's name, else the agent (`claude`), else
/// [`block_title`].
#[must_use]
pub fn agent_title(w: &WindowInfo, b: &BlockInfo) -> String {
    if let Some(l) = b.label.as_ref().filter(|l| !l.is_empty()) {
        return l.clone();
    }
    if !w.name.is_empty() && !w.name.chars().all(|c| c.is_ascii_digit()) && w.blocks.len() == 1 {
        return w.name.clone();
    }
    agent_name(b).map_or_else(|| block_title(b), str::to_owned)
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
            status: state.map(|s| ProgramStatus {
                updated_ms: 7,
                ..ProgramStatus::new(s)
            }),
            exited: None,
            agent: None,
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
    fn agents_are_grouped_by_status_in_order() {
        let mut idle = block(5, &["/bin/zsh"], None, None);
        idle.agent = Some("claude".into());
        let mut sessions = vec![session(vec![
            block(1, &["codex"], None, Some(ProgramState::Working)),
            block(2, &["claude"], None, Some(ProgramState::Done)),
            block(3, &["gemini"], None, Some(ProgramState::Blocked)),
            block(4, &["npm", "run", "dev"], None, None),
            idle,
        ])];
        // A newer change sorts first inside its group.
        if let Some(s) = sessions[0].windows[0].blocks[0].status.as_mut() {
            s.updated_ms = 9;
        }
        sessions[0].windows[0]
            .blocks
            .push(block(6, &["aider"], None, Some(ProgramState::Working)));
        let mut seen = Seen::default();
        let m = Model::build(&sessions, None, &seen);
        let order: Vec<(AgentGroup, Vec<BlockId>)> = m
            .groups
            .iter()
            .map(|(g, rows)| (*g, rows.iter().map(|r| r.block).collect()))
            .collect();
        assert_eq!(
            order,
            vec![
                (AgentGroup::NeedsYou, vec![BlockId(3)]),
                (AgentGroup::Done, vec![BlockId(2)]),
                (AgentGroup::Working, vec![BlockId(1), BlockId(6)]),
                (AgentGroup::Idle, vec![BlockId(5)]),
            ]
        );
        assert_eq!(m.groups[3].1[0].logo, Some(ferrum_ui::AgentLogo::Claude));
        // Looking at the finished agent makes it idle.
        seen.look_at(&sessions[0].windows[1]);
        let m = Model::build(&sessions, None, &seen);
        assert!(!m.groups.iter().any(|(g, _)| *g == AgentGroup::Done));
        assert_eq!(m.groups.last().map(|(_, r)| r.len()), Some(2));
    }

    #[test]
    fn subtitles_and_relative_times() {
        let mut b = block(1, &["claude"], Some("claude"), None);
        assert_eq!(subtitle(&b), "/work/app");
        b.title = "✳ Fix the flaky test".into();
        assert_eq!(subtitle(&b), "Fix the flaky test");
        b.title = "claude".into();
        assert_eq!(subtitle(&b), "/work/app");
        assert_eq!(relative_time(100_000, 90_000), "<1m");
        assert_eq!(relative_time(13 * 60_000, 0), "13m");
        assert_eq!(relative_time(3 * 3_600_000 + 5, 0), "3h");
        assert_eq!(relative_time(26 * 3_600_000, 0), "1d");
        assert_eq!(relative_time(0, 5_000), "<1m", "clock skew");
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
        w.blocks
            .push(block(2, &["tail", "-f", "x.log"], None, None));
        let m = Model::build(&[s], None, &Seen::default());
        let rows = &m.sessions[0].rows;
        assert_eq!((rows[0].title.as_str(), rows[0].nested), ("logs", false));
        assert_eq!((rows[1].title.as_str(), rows[1].nested), ("tail", true));
        assert_eq!(rows[1].detail, "tail · /work/app");
    }
}
