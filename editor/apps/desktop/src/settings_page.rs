//! Settings › Editor in the shared Settings window: switches for the
//! view (soft wrap, minimap, line numbers, indent guides, current line)
//! and editing (autocomplete, format on save). Each switch writes its key
//! to `config.toml` (comments kept); the config watcher then applies it
//! to every open editor, like an edit of the file by hand.

use ferrum_ui::components::IconName;
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section, Switch};
use ferrumeditor_core::config::{Config, set_bool_value};
use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window};

use crate::settings::Settings;

/// A boolean `config.toml` key shown as a switch.
struct Toggle {
    key: &'static str,
    title: &'static str,
    detail: &'static str,
    get: fn(&Config) -> bool,
    set: fn(&mut Config, bool),
}

const VIEW: &[Toggle] = &[
    Toggle {
        key: "soft_wrap",
        title: "Soft wrap",
        detail: "Wrap long lines at the window edge (Markdown wraps by default). \
                 Alt+Z toggles it for one file.",
        get: |c| c.soft_wrap,
        set: |c, on| c.soft_wrap = on,
    },
    Toggle {
        key: "minimap",
        title: "Minimap",
        detail: "A map of the whole file at the right of the text; click or drag in it to scroll.",
        get: |c| c.minimap,
        set: |c, on| c.minimap = on,
    },
    Toggle {
        key: "show_line_numbers",
        title: "Line numbers",
        detail: "",
        get: |c| c.show_line_numbers,
        set: |c, on| c.show_line_numbers = on,
    },
    Toggle {
        key: "show_indent_guides",
        title: "Indent guides",
        detail: "",
        get: |c| c.show_indent_guides,
        set: |c, on| c.show_indent_guides = on,
    },
    Toggle {
        key: "highlight_current_line",
        title: "Highlight the current line",
        detail: "",
        get: |c| c.highlight_current_line,
        set: |c, on| c.highlight_current_line = on,
    },
];

const EDITING: &[Toggle] = &[
    Toggle {
        key: "autocomplete",
        title: "Suggest while typing",
        detail: "Completions from the language server and the file; Ctrl+Space always asks.",
        get: |c| c.autocomplete,
        set: |c, on| c.autocomplete = on,
    },
    Toggle {
        key: "format_on_save",
        title: "Format on save",
        detail: "With the language server, for Rust, Go, TypeScript and JavaScript \
                 (per language under [languages.<id>]).",
        get: |c| c.format_on_save,
        set: |c, on| c.format_on_save = on,
    },
];

/// Sets a boolean key: in memory now, and in `config.toml` for the watcher
/// (which applies it to the open editors) and the next launch.
pub fn set_bool(key: &str, on: bool, cx: &mut App) -> Result<(), String> {
    let toggle = VIEW
        .iter()
        .chain(EDITING)
        .find(|t| t.key == key)
        .ok_or_else(|| format!("unknown setting `{key}`"))?;
    let settings = cx.global_mut::<Settings>();
    (toggle.set)(&mut settings.config, on);
    let Some(path) = settings.files.as_ref().map(|f| f.config.clone()) else {
        return Ok(());
    };
    let source = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(&path, set_bool_value(&source, key, on))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn section(title: &'static str, toggles: &'static [Toggle], config: &Config) -> Section {
    Section::new(title).rows(toggles.iter().map(|toggle| {
        let key = toggle.key;
        let row = Row::new(key, toggle.title).control(
            Switch::new(key, (toggle.get)(config)).on_change(move |on, _, cx| {
                if let Err(error) = set_bool(key, on, cx) {
                    tracing::warn!("settings: {error}");
                }
                cx.refresh_windows();
            }),
        );
        if toggle.detail.is_empty() {
            row
        } else {
            row.detail(toggle.detail)
        }
    }))
}

/// Settings › Editor.
pub struct EditorPage;

impl SettingsPage for EditorPage {
    fn id(&self) -> &'static str {
        "editor"
    }
    fn title(&self) -> &'static str {
        "Editor"
    }
    fn icon(&self) -> IconName {
        IconName::Code
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0xe8743b).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "wrap",
            "soft wrap",
            "minimap",
            "line numbers",
            "indent",
            "autocomplete",
            "format",
        ]
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let config = cx.global::<Settings>().config.clone();
        gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .gap_6()
            .child(section("View", VIEW, &config))
            .child(section("Editing", EDITING, &config))
            .into_any_element()
    }
}
