//! Window preferences kept between launches in `<state dir>/gui-state`:
//! sidebar open/closed and width, and recently used custom agent commands.
//! Plain `key=value` lines; unknown keys are ignored, so older and newer
//! builds can share the file.

use std::path::PathBuf;

pub const MIN_SIDEBAR: f32 = 180.0;
pub const MAX_SIDEBAR: f32 = 480.0;
const MAX_RECENT: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct UiState {
    pub sidebar_open: bool,
    pub sidebar_width: f32,
    /// Custom agent command lines, most recent first.
    pub recent_agents: Vec<String>,
    /// Share of the sidebar's height the Agents section takes (bottom).
    pub agents_fraction: f32,
    /// Saved "New tab" configurations: (name, command line; empty = shell).
    pub tab_configs: Vec<(String, String)>,
    /// Icon and colour chosen for a tab, by window id (`w3`).
    pub tab_styles: std::collections::BTreeMap<String, crate::tab_style::TabStyle>,
}

/// Default and limits of [`UiState::agents_fraction`].
pub const AGENTS_FRACTION: f32 = 0.3;
const AGENTS_FRACTION_RANGE: (f32, f32) = (0.12, 0.85);

/// `fraction` kept inside the allowed range.
#[must_use]
pub fn clamp_agents_fraction(fraction: f32) -> f32 {
    fraction.clamp(AGENTS_FRACTION_RANGE.0, AGENTS_FRACTION_RANGE.1)
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            sidebar_open: true,
            sidebar_width: 240.0,
            recent_agents: Vec::new(),
            agents_fraction: AGENTS_FRACTION,
            tab_configs: Vec::new(),
            tab_styles: std::collections::BTreeMap::new(),
        }
    }
}

fn path() -> PathBuf {
    ferrumterm_client::paths::state_dir().join("gui-state")
}

impl UiState {
    #[must_use]
    pub fn load() -> Self {
        std::fs::read_to_string(path()).map_or_else(|_| Self::default(), |s| Self::parse(&s))
    }

    pub fn save(&self) {
        let p = path();
        let written = p
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&p, self.to_text()));
        if let Err(error) = written {
            tracing::warn!(%error, path = %p.display(), "could not save window state");
        }
    }

    /// Saves only the recent agent commands, keeping the saved sidebar
    /// state (an embedded view shares the file with the desktop window).
    pub fn save_recent_agents(&self) {
        let mut saved = Self::load();
        if saved.recent_agents != self.recent_agents
            || saved.tab_configs != self.tab_configs
            || saved.tab_styles != self.tab_styles
        {
            saved.recent_agents.clone_from(&self.recent_agents);
            saved.tab_configs.clone_from(&self.tab_configs);
            saved.tab_styles.clone_from(&self.tab_styles);
            saved.save();
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "sidebar" => s.sidebar_open = value != "closed",
                "sidebar_width" => {
                    if let Ok(w) = value.parse::<f32>() {
                        s.sidebar_width = w.clamp(MIN_SIDEBAR, MAX_SIDEBAR);
                    }
                }
                "agent" if !value.is_empty() && s.recent_agents.len() < MAX_RECENT => {
                    s.recent_agents.push(value.to_owned());
                }
                "agents_fraction" => {
                    if let Ok(f) = value.parse::<f32>() {
                        s.agents_fraction = clamp_agents_fraction(f);
                    }
                }
                "tab_style" => {
                    let mut parts = value.split('\t');
                    if let (Some(id), Some(style)) = (
                        parts.next(),
                        crate::tab_style::TabStyle::parse(parts.next(), parts.next()),
                    ) {
                        s.tab_styles.insert(id.to_owned(), style);
                    }
                }
                "tab" => {
                    if let Some((name, command)) = value.split_once('\t')
                        && !name.trim().is_empty()
                    {
                        s.tab_configs
                            .push((name.trim().to_owned(), command.trim().to_owned()));
                    }
                }
                _ => {}
            }
        }
        s
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "sidebar={}\nsidebar_width={}\n",
            if self.sidebar_open { "open" } else { "closed" },
            self.sidebar_width.round()
        );
        for a in &self.recent_agents {
            out.push_str(&format!("agent={a}\n"));
        }
        out.push_str(&format!("agents_fraction={:.3}\n", self.agents_fraction));
        for (name, command) in &self.tab_configs {
            out.push_str(&format!("tab={name}\t{command}\n"));
        }
        for (id, style) in &self.tab_styles {
            out.push_str(&format!(
                "tab_style={id}\t{}\t{}\n",
                style.icon, style.color
            ));
        }
        out
    }

    /// Adds (or replaces) the tab configuration `name`.
    pub fn save_tab_config(&mut self, name: &str, command: &str) {
        let name = name.trim().replace(['\n', '\t'], " ");
        let command = command.trim().replace(['\n', '\t'], " ");
        self.tab_configs.retain(|(n, _)| *n != name);
        self.tab_configs.push((name, command));
    }

    /// Moves `command_line` to the front of the recent list.
    pub fn remember_agent(&mut self, command_line: &str) {
        let line = command_line.trim().replace('\n', " ");
        self.recent_agents.retain(|a| *a != line);
        self.recent_agents.insert(0, line);
        self.recent_agents.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_junk() {
        let mut s = UiState {
            sidebar_open: false,
            sidebar_width: 300.0,
            recent_agents: Vec::new(),
            agents_fraction: 0.4,
            tab_configs: Vec::new(),
            tab_styles: std::collections::BTreeMap::new(),
        };
        s.tab_styles.insert(
            "w3".into(),
            crate::tab_style::TabStyle {
                icon: "server".into(),
                color: "green".into(),
            },
        );
        s.save_tab_config("dev server", "npm run dev");
        s.remember_agent("aider --model sonnet");
        s.remember_agent("gemini");
        s.remember_agent("aider --model sonnet");
        assert_eq!(s.recent_agents, vec!["aider --model sonnet", "gemini"]);
        assert_eq!(UiState::parse(&s.to_text()), s);

        let odd = UiState::parse("junk\nsidebar_width=9000\nfuture_key=1\nsidebar=open\n");
        assert!(odd.sidebar_open);
        assert!((odd.sidebar_width - MAX_SIDEBAR).abs() < f32::EPSILON);
        assert_eq!(UiState::parse(""), UiState::default());
    }

    #[test]
    fn recent_agents_are_capped() {
        let mut s = UiState::default();
        for i in 0..8 {
            s.remember_agent(&format!("tool{i}"));
        }
        assert_eq!(s.recent_agents.len(), MAX_RECENT);
        assert_eq!(s.recent_agents[0], "tool7");
    }
}
