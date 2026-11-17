//! `~/.ferrum/term/config.toml`: the everyday terminal settings as a plain
//! file to read and edit by hand. Settings › Terminal and this file stay in
//! sync: what the file sets wins over `settings.json` (see `settings`),
//! Settings rewrites the file whenever it changes something, and the
//! windows notice edits within a second and apply them (see
//! [`crate::settings::reload_if_changed`]). Keys the file leaves out keep
//! their Settings value; an invalid file is ignored (and logged).

use crate::settings::{Bell, Blink, CursorStyle, OptionKey, TermSettings};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// `~/.ferrum/term/config.toml` (or under `$FERRUM_HOME`).
#[must_use]
pub fn path() -> Option<PathBuf> {
    ferrum_ui::prefs::path("term/config.toml")
}

/// The file's keys; each is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub cursor: Option<CursorStyle>,
    pub cursor_blink: Option<Blink>,
    pub cursor_force: Option<bool>,
    pub scrollback: Option<u32>,
    pub option_as_meta: Option<Meta>,
    pub copy_on_select: Option<bool>,
    pub bell: Option<Bell>,
    pub theme: Option<String>,
    pub shell_integration: Option<bool>,
}

/// `option_as_meta`: `true` / `false`, or `"auto"`, `"left"`, `"right"`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Meta {
    On(bool),
    Named(String),
}

impl Meta {
    fn key(&self) -> Option<OptionKey> {
        Some(match self {
            Self::On(true) => OptionKey::Alt,
            Self::On(false) => OptionKey::Characters,
            Self::Named(name) => match name.trim().to_ascii_lowercase().as_str() {
                "auto" | "automatic" => OptionKey::Automatic,
                "alt" | "true" | "yes" | "on" => OptionKey::Alt,
                "characters" | "false" | "no" | "off" => OptionKey::Characters,
                "left" | "left_alt" => OptionKey::LeftAlt,
                "right" | "right_alt" => OptionKey::RightAlt,
                _ => return None,
            },
        })
    }
}

/// Parses the file's text.
pub fn parse(text: &str) -> Result<ConfigFile, String> {
    toml::from_str(text).map_err(|e| e.message().to_owned())
}

impl ConfigFile {
    /// Puts the keys the file sets into `settings`.
    pub fn apply(&self, settings: &mut TermSettings) {
        if let Some(family) = &self.font_family {
            let family = family.trim();
            settings.font_family = (!family.is_empty()).then(|| family.to_owned());
        }
        if let Some(size) = self.font_size.filter(|s| s.is_finite()) {
            settings.font_size = size.clamp(8., 40.);
        }
        if let Some(cursor) = self.cursor {
            settings.cursor_style = cursor;
        }
        if let Some(blink) = self.cursor_blink {
            settings.cursor_blink = blink;
        }
        if let Some(force) = self.cursor_force {
            settings.force_cursor_style = force;
        }
        if let Some(lines) = self.scrollback {
            settings.scrollback = lines.min(ferrumterm_emulator::MAX_SCROLLBACK as u32);
        }
        if let Some(key) = self.option_as_meta.as_ref().and_then(Meta::key) {
            settings.option = Some(key);
        }
        if let Some(copy) = self.copy_on_select {
            settings.copy_on_select = copy;
        }
        if let Some(bell) = self.bell {
            settings.bell = bell;
        }
        if let Some(theme) = &self.theme {
            settings.theme = crate::theme::by_id(theme).id.to_owned();
        }
        if let Some(on) = self.shell_integration {
            settings.shell_integration = on;
        }
    }
}

/// Reads `path` (when it exists) into `settings`.
pub fn overlay(path: &Path, settings: &mut TermSettings) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    match parse(&text) {
        Ok(file) => file.apply(settings),
        Err(error) => tracing::warn!(%error, "ignoring {}", path.display()),
    }
}

fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

/// The file for `settings`, with a comment on each key.
#[must_use]
pub fn render(settings: &TermSettings) -> String {
    let themes: Vec<&str> = crate::theme::THEMES.iter().map(|t| t.id).collect();
    let meta = match settings.option_key() {
        OptionKey::Automatic => quoted("auto"),
        OptionKey::Alt => "true".to_owned(),
        OptionKey::Characters => "false".to_owned(),
        OptionKey::LeftAlt => quoted("left"),
        OptionKey::RightAlt => quoted("right"),
    };
    let lines = [
        "# FerrumTerm settings. Settings › Terminal writes this file too, and edits".to_owned(),
        "# here apply to open terminals within a second.".to_owned(),
        String::new(),
        "# Any installed monospace font; \"\" for the bundled Lilex.".to_owned(),
        format!(
            "font_family = {}",
            quoted(settings.font_family.as_deref().unwrap_or_default())
        ),
        format!("font_size = {}", settings.font_size),
        String::new(),
        "# block | beam | underline; blink: off | slow | normal | fast.".to_owned(),
        format!("cursor = {}", quoted(&name(settings.cursor_style))),
        format!("cursor_blink = {}", quoted(&name(settings.cursor_blink))),
        "# Use this cursor even when a program asks for another shape.".to_owned(),
        format!("cursor_force = {}", settings.force_cursor_style),
        String::new(),
        "# Lines kept per terminal (new terminals).".to_owned(),
        format!("scrollback = {}", settings.scrollback),
        "# Option as Alt (Meta): true, false, \"auto\" (on US / ABC layouts),".to_owned(),
        "# \"left\" or \"right\" (that Option key only).".to_owned(),
        format!("option_as_meta = {meta}"),
        "# Copy the selection when the mouse is released.".to_owned(),
        format!("copy_on_select = {}", settings.copy_on_select),
        "# visual (the pane flashes) | system (alert sound) | off.".to_owned(),
        format!("bell = {}", quoted(&name(settings.bell))),
        String::new(),
        format!("# {}.", themes.join(" | ")),
        "# ferrum follows the Ferrum app theme (light or dark).".to_owned(),
        format!("theme = {}", quoted(&settings.theme)),
        String::new(),
        "# Mark prompts and commands in new zsh and fish shells (command blocks);".to_owned(),
        "# for bash: eval \"$(fterm shell-integration bash)\" in ~/.bashrc.".to_owned(),
        format!("shell_integration = {}", settings.shell_integration),
    ];
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Writes `render(settings)` to `path`, creating its folder.
pub fn write(path: &Path, settings: &TermSettings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("toml.partial");
    std::fs::write(&partial, render(settings))?;
    std::fs::rename(&partial, path)
}

/// A settings enum's name in the file (its serde name).
fn name(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rendered_file_reads_back_as_the_same_settings() {
        let settings = TermSettings {
            font_family: Some("JetBrains \"Mono\"".into()),
            font_size: 15.5,
            cursor_style: CursorStyle::Underline,
            cursor_blink: Blink::Fast,
            force_cursor_style: true,
            scrollback: 50_000,
            option: Some(OptionKey::RightAlt),
            copy_on_select: true,
            bell: Bell::System,
            theme: "nord".into(),
            shell_integration: false,
            ..TermSettings::default()
        };
        let text = render(&settings);
        assert!(text.contains("theme = \"nord\""), "{text}");
        assert!(text.contains("option_as_meta = \"right\""), "{text}");
        let mut back = TermSettings::default();
        parse(&text).expect("valid toml").apply(&mut back);
        assert_eq!(back, settings);
    }

    #[test]
    fn keys_left_out_keep_their_value_and_values_are_checked() {
        let mut settings = TermSettings {
            copy_on_select: true,
            ..TermSettings::default()
        };
        parse(
            "font_size = 99\ncursor = \"bar\"\noption_as_meta = true\nscrollback = 999999999\n\
             theme = \"Dracula\"\nfont_family = \"\"\n",
        )
        .expect("valid")
        .apply(&mut settings);
        assert!(settings.copy_on_select, "not in the file");
        assert_eq!(settings.font_size, 40.);
        assert_eq!(settings.cursor_style, CursorStyle::Beam);
        assert_eq!(settings.option_key(), OptionKey::Alt);
        assert_eq!(
            settings.scrollback,
            ferrumterm_emulator::MAX_SCROLLBACK as u32
        );
        assert_eq!(settings.theme, "dracula");
        assert_eq!(settings.font_family, None);
        parse("option_as_meta = \"auto\"")
            .expect("valid")
            .apply(&mut settings);
        assert_eq!(settings.option_key(), OptionKey::Automatic);
        assert!(parse("bell = \"loud\"").is_err());
        assert!(parse("font_size = ").is_err());
    }

    #[test]
    fn edits_reach_the_settings_and_settings_reach_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("term").join("config.toml");
        let mut settings = TermSettings::default();
        write(&path, &settings).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, text.replace("bell = \"visual\"", "bell = \"off\"")).expect("edit");
        overlay(&path, &mut settings);
        assert_eq!(settings.bell, Bell::Off);
        // A broken file changes nothing.
        std::fs::write(&path, "bell = ").expect("break");
        overlay(&path, &mut settings);
        assert_eq!(settings.bell, Bell::Off);
    }
}
