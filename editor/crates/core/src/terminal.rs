//! Integrated terminal logic that needs no window: the panel's tab state,
//! keystroke → terminal input translation and the shell to launch. The
//! emulator itself is FerrumTerm's (`ferrumterm-emulator`), used by the
//! desktop app.

use std::path::PathBuf;

/// Panel height limits in pixels.
pub const MIN_HEIGHT: f32 = 100.0;
pub const MAX_HEIGHT: f32 = 1200.0;
pub const DEFAULT_HEIGHT: f32 = 260.0;

/// The bottom panel: whether it is shown, its height and its tabs (ids of
/// terminal sessions, owned by the app). Sessions keep running while the
/// panel is hidden.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelState {
    pub open: bool,
    pub height: f32,
    tabs: Vec<u64>,
    active: usize,
    next_id: u64,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            open: false,
            height: DEFAULT_HEIGHT,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }
}

/// What the app must do after [`PanelState::toggle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    /// Shown, and there is no terminal yet: spawn one.
    OpenedEmpty,
    Opened,
    Closed,
}

impl PanelState {
    pub fn tabs(&self) -> &[u64] {
        &self.tabs
    }

    pub fn active_index(&self) -> Option<usize> {
        (!self.tabs.is_empty()).then_some(self.active)
    }

    pub fn active_id(&self) -> Option<u64> {
        self.tabs.get(self.active).copied()
    }

    pub fn toggle(&mut self) -> Toggle {
        self.open = !self.open;
        match (self.open, self.tabs.is_empty()) {
            (false, _) => Toggle::Closed,
            (true, true) => Toggle::OpenedEmpty,
            (true, false) => Toggle::Opened,
        }
    }

    /// Adds a tab after the active one, makes it active, opens the panel.
    pub fn add_tab(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let at = if self.tabs.is_empty() {
            0
        } else {
            self.active + 1
        };
        self.tabs.insert(at, id);
        self.active = at;
        self.open = true;
        id
    }

    /// Removes a tab; the neighbour to the left becomes active. Closing the
    /// last tab hides the panel. Returns whether `id` was present.
    pub fn close_tab(&mut self, id: u64) -> bool {
        let Some(index) = self.tabs.iter().position(|t| *t == id) else {
            return false;
        };
        self.tabs.remove(index);
        if self.active > index || (self.active == index && index > 0) {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if self.tabs.is_empty() {
            self.open = false;
            self.active = 0;
        }
        true
    }

    pub fn select(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// Cycles the active tab by `delta`.
    pub fn cycle(&mut self, delta: isize) {
        if self.tabs.is_empty() {
            return;
        }
        let len = self.tabs.len() as isize;
        self.active = ((self.active as isize + delta).rem_euclid(len)) as usize;
    }

    /// Sets the height, clamped to the limits and to `available` (the
    /// window area minus room for the editor).
    pub fn set_height(&mut self, height: f32, available: f32) {
        let max = MAX_HEIGHT.min((available - 120.0).max(MIN_HEIGHT));
        self.height = height.clamp(MIN_HEIGHT, max);
    }
}

/// What a keystroke sends to the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermKey {
    /// A key spec for `ferrumterm_emulator::keys` (`Enter`, `C-c`, `M-f`, `S-Up`…).
    Key(String),
    /// Literal text.
    Text(String),
}

fn named(key: &str) -> Option<&'static str> {
    Some(match key {
        "enter" => "Enter",
        "tab" => "Tab",
        "escape" => "Escape",
        "backspace" => "BSpace",
        "space" => "Space",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        "delete" => "Delete",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        _ => return None,
    })
}

/// Modifier state of a keystroke.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Cmd on macOS, Super/Win elsewhere.
    pub platform: bool,
}

/// Translates a keystroke (GPUI key name, lower case) into terminal input.
/// `None` leaves it to text input (plain characters, so IME works) or to
/// app bindings (anything with the platform modifier). Alt acts as Meta.
pub fn translate(key: &str, mods: Mods) -> Option<TermKey> {
    if mods.platform {
        return None;
    }
    let key = key.to_lowercase();
    let mut prefix = String::new();
    if mods.ctrl {
        prefix.push_str("C-");
    }
    if mods.alt {
        prefix.push_str("M-");
    }
    if let Some(name) = named(&key) {
        let is_space = key == "space";
        if prefix.is_empty() && is_space {
            return Some(TermKey::Text(" ".into()));
        }
        let shift = if mods.shift && !is_space { "S-" } else { "" };
        return Some(TermKey::Key(format!("{prefix}{shift}{name}")));
    }
    if !prefix.is_empty() && key.chars().count() == 1 {
        return Some(TermKey::Key(format!("{prefix}{key}")));
    }
    None
}

/// The shell to launch: `configured` if set, else `$SHELL`, else a
/// platform default. Runs as a login shell (see the desktop app).
pub fn shell(configured: Option<&str>) -> PathBuf {
    if let Some(shell) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        return PathBuf::from(shell);
    }
    if let Some(shell) = std::env::var_os("SHELL").filter(|s| !s.is_empty()) {
        return PathBuf::from(shell);
    }
    PathBuf::from(if cfg!(windows) {
        "powershell.exe"
    } else if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    })
}

/// Tab label: the program's title, else the shell's file name.
pub fn tab_title(title: &str, shell: &std::path::Path) -> String {
    let title = title.trim();
    if !title.is_empty() {
        return title.chars().take(40).collect();
    }
    shell
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "shell".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_and_tabs() {
        let mut panel = PanelState::default();
        assert_eq!(panel.toggle(), Toggle::OpenedEmpty);
        let a = panel.add_tab();
        assert_eq!(panel.toggle(), Toggle::Closed);
        assert_eq!(panel.toggle(), Toggle::Opened);
        let b = panel.add_tab();
        let c = panel.add_tab();
        assert_eq!(panel.tabs(), &[a, b, c]);
        assert_eq!(panel.active_id(), Some(c));
        panel.select(0);
        let d = panel.add_tab();
        assert_eq!(panel.tabs(), &[a, d, b, c], "inserted after the active tab");
        panel.cycle(-2);
        assert_eq!(panel.active_id(), Some(c));
        panel.cycle(1);
        assert_eq!(panel.active_id(), Some(a));
    }

    #[test]
    fn closing_tabs_moves_left_and_hides_when_empty() {
        let mut panel = PanelState::default();
        let a = panel.add_tab();
        let b = panel.add_tab();
        let c = panel.add_tab();
        panel.select(1);
        assert!(panel.close_tab(b));
        assert_eq!(panel.active_id(), Some(a));
        assert!(panel.close_tab(c));
        assert_eq!(panel.active_id(), Some(a));
        assert!(!panel.close_tab(99));
        assert!(panel.open);
        assert!(panel.close_tab(a));
        assert!(!panel.open);
        assert_eq!(panel.active_index(), None);
        assert_eq!(panel.toggle(), Toggle::OpenedEmpty);
    }

    #[test]
    fn height_is_clamped() {
        let mut panel = PanelState::default();
        panel.set_height(20.0, 900.0);
        assert_eq!(panel.height, MIN_HEIGHT);
        panel.set_height(5000.0, 900.0);
        assert_eq!(panel.height, 780.0);
        panel.set_height(300.0, 150.0);
        assert_eq!(panel.height, MIN_HEIGHT);
    }

    #[test]
    fn translates_keys() {
        let none = Mods::default();
        let ctrl = Mods { ctrl: true, ..none };
        let alt = Mods { alt: true, ..none };
        let shift = Mods {
            shift: true,
            ..none
        };
        let cmd = Mods {
            platform: true,
            ..none
        };
        let key = |k: &str| Some(TermKey::Key(k.into()));
        assert_eq!(translate("enter", none), key("Enter"));
        assert_eq!(translate("c", ctrl), key("C-c"));
        assert_eq!(translate("f", alt), key("M-f"));
        assert_eq!(translate("up", ctrl), key("C-Up"));
        assert_eq!(translate("tab", shift), key("S-Tab"));
        assert_eq!(translate("backspace", none), key("BSpace"));
        assert_eq!(translate("space", none), Some(TermKey::Text(" ".into())));
        assert_eq!(translate("space", ctrl), key("C-Space"));
        assert_eq!(translate("a", none), None);
        assert_eq!(translate("a", shift), None);
        assert_eq!(translate("c", cmd), None);
    }

    #[test]
    fn shell_choice_and_titles() {
        assert_eq!(shell(Some("/bin/fish")), PathBuf::from("/bin/fish"));
        assert!(!shell(Some("  ")).as_os_str().is_empty());
        assert_eq!(tab_title("", std::path::Path::new("/bin/zsh")), "zsh");
        assert_eq!(tab_title(" vim ", std::path::Path::new("/bin/zsh")), "vim");
    }
}
