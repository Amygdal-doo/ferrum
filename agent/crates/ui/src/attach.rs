//! Adding context from the composer: the + menu (files, a folder, the git
//! diff, terminal output, a web page, @-mentions), the 📎 file picker and
//! the microphone (macOS Dictation into the composer).

use std::path::Path;

use ferrum_ui::Notice;
use ferrum_ui::components::ContextMenu;
use ferrum_ui::prelude::*;
use ferrumagent_core::context::Mention;
use ferrumagent_core::thread::Block;
use gpui::{Context, Entity, PathPromptOptions, Window};

use crate::actions::*;
use crate::view::ChatView;

/// Text read from a file outside the project, at most.
const OUTSIDE_LIMIT: usize = 100 * 1024;

/// How to dictate, shown by the microphone button. Starting Dictation
/// ourselves needs AppKit calls that are `unsafe` (`sendAction:to:from:`),
/// and recording needs a microphone entitlement the apps don't have, so
/// the button focuses the composer and points at the system shortcut.
pub(crate) const DICTATION_HINT: &str = if cfg!(target_os = "macos") {
    "Dictate: press Fn twice (or Edit › Start Dictation…) and speak — the text goes into the composer."
} else {
    "Dictate with your system's speech input — the text goes into the composer."
};

/// A picked path as a mention: project files and folders by their path
/// relative to `root`; text files elsewhere by their content.
pub(crate) fn path_mention(root: &Path, path: &Path) -> Result<Mention, String> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let name = path.display().to_string();
    if let Ok(rel) = path.strip_prefix(&root) {
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            return Ok(Mention::Folder { path: ".".into() });
        }
        return Ok(if path.is_dir() {
            Mention::Folder { path: rel }
        } else {
            Mention::File { path: rel }
        });
    }
    if path.is_dir() {
        return Err(format!(
            "{name}: only folders inside the project can be added"
        ));
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{name}: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!(
            "{name}: binary files (images included) can't be attached yet"
        ));
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut end = text.len().min(OUTSIDE_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = text[..end].to_string();
    Ok(Mention::Selection {
        path: name,
        start_line: 1,
        end_line: text.lines().count().max(1),
        text,
    })
}

impl ChatView {
    pub(crate) fn attach_files(&mut self, _: &AttachFiles, _: &mut Window, cx: &mut Context<Self>) {
        self.pick_paths(true, cx);
    }

    /// The system picker: several files, or one folder.
    pub(crate) fn pick_paths(&mut self, files: bool, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files,
            directories: !files,
            multiple: files,
            prompt: Some(if files { "Attach" } else { "Add folder" }.into()),
        });
        let task = cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            this.update(cx, |this, cx| this.add_context_paths(paths, cx))
                .ok();
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
    }

    /// Adds files and folders as context chips (a host's file tree or a
    /// Finder drop): project paths by reference, text files elsewhere by
    /// their content; the others are refused with a notice. Read off the
    /// UI thread.
    pub fn add_context_paths(&mut self, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let root = self.project.clone();
        let task = cx.spawn(async move |this, cx| {
            let mentions = cx
                .background_spawn(async move {
                    paths
                        .iter()
                        .map(|p| path_mention(&root, p))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |this, cx| {
                for mention in mentions {
                    match mention {
                        Ok(mention) => this.add_mention(mention, cx),
                        Err(e) => this.show_notice(Notice::error(e), cx),
                    }
                }
            })
            .ok();
        });
        self.tasks.retain(|t| !t.is_ready());
        self.tasks.push(task);
    }

    /// The context chips waiting in the composer.
    #[must_use]
    pub fn mentions(&self) -> &[Mention] {
        &self.mentions
    }

    /// The output of the thread's last command.
    fn add_terminal_output(&mut self, cx: &mut Context<Self>) {
        let last = self
            .thread
            .messages
            .iter()
            .rev()
            .flat_map(|m| m.blocks.iter().rev())
            .find_map(|b| match b {
                Block::Tool { call } if call.name == "run_command" => {
                    call.output.clone().map(|text| (call.title.clone(), text))
                }
                _ => None,
            });
        match last {
            Some((title, text)) => self.add_mention(Mention::Terminal { title, text }, cx),
            None => self.show_notice(
                Notice::info("No command output in this thread yet. A terminal pane can add its output to the chat."),
                cx,
            ),
        }
    }

    fn insert_in_composer(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| {
            input.focus(window, cx);
            input.insert(text, window, cx);
        });
    }

    pub(crate) fn dictate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.show_notice(Notice::info(DICTATION_HINT), cx);
    }

    /// The + menu, built when it opens.
    pub(crate) fn add_context_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        move |window, cx| {
            let this = this.clone();
            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                let run = |f: fn(&mut ChatView, &mut Window, &mut Context<ChatView>)| {
                    let this = this.clone();
                    move |window: &mut Window, cx: &mut App| {
                        this.update(cx, |view, cx| f(view, window, cx)).ok();
                    }
                };
                menu.icon_entry(
                    "Files…",
                    IconName::File,
                    run(|v, _, cx| v.pick_paths(true, cx)),
                )
                .icon_entry(
                    "Folder…",
                    IconName::Folder,
                    run(|v, _, cx| v.pick_paths(false, cx)),
                )
                .separator()
                .icon_entry(
                    "Git diff",
                    IconName::GitBranch,
                    run(|v, _, cx| v.add_mention(Mention::GitDiff, cx)),
                )
                .icon_entry(
                    "Terminal output",
                    IconName::Terminal,
                    run(|v, _, cx| v.add_terminal_output(cx)),
                )
                .icon_entry(
                    "Web page…",
                    IconName::Link,
                    run(|v, window, cx| v.insert_in_composer("@https://", window, cx)),
                )
                .separator()
                .icon_entry(
                    "Mention with @",
                    IconName::Hash,
                    run(|v, window, cx| v.insert_in_composer("@", window, cx)),
                )
            }))
        }
    }
}

/// A small microphone drawn with boxes (the icon set has none).
pub(crate) fn mic_glyph(color: gpui::Hsla) -> impl IntoElement {
    v_flex()
        .items_center()
        .child(
            div()
                .w(px(6.))
                .h(px(9.))
                .rounded_full()
                .border(px(1.5))
                .border_color(color),
        )
        .child(
            div()
                .w(px(10.))
                .h(px(3.5))
                .mt(px(-2.))
                .rounded_b(px(5.))
                .border_b(px(1.5))
                .border_l(px(1.5))
                .border_r(px(1.5))
                .border_color(color),
        )
        .child(div().w(px(1.5)).h(px(2.)).bg(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picked_paths_become_mentions() {
        let project = tempfile::tempdir().expect("tempdir");
        let other = tempfile::tempdir().expect("tempdir");
        let root = project.path();
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(root.join("src/a.rs"), "fn a() {}\n").expect("write");
        std::fs::write(other.path().join("notes.md"), "one\ntwo\n").expect("write");
        std::fs::write(other.path().join("logo.png"), [0x89, b'P', 0, 1]).expect("write");

        assert_eq!(
            path_mention(root, &root.join("src/a.rs")),
            Ok(Mention::File {
                path: "src/a.rs".into()
            })
        );
        assert_eq!(
            path_mention(root, &root.join("src")),
            Ok(Mention::Folder { path: "src".into() })
        );
        match path_mention(root, &other.path().join("notes.md")) {
            Ok(Mention::Selection { end_line, text, .. }) => {
                assert_eq!((end_line, text.as_str()), (2, "one\ntwo\n"));
            }
            other => panic!("{other:?}"),
        }
        assert!(path_mention(root, &other.path().join("logo.png")).is_err());
        assert!(path_mention(root, other.path()).is_err());
    }
}
