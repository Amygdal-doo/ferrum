//! Modal dialogs (new file / folder, rename, about) and the tree view's
//! file operations, including Move to Trash.

use std::path::{Path, PathBuf};

use ferrum_ui::components::{Button, ButtonCommon, ButtonStyle, Clickable};
use ferrum_ui::prelude::*;
use ferrum_ui::{Modal, TextInput, TextInputEvent};
use ferrumeditor_core::file_tree::{create_dir, create_file, rename};
use gpui::PromptLevel;

use super::Workspace;
use crate::tree_view::TreeEvent;

pub enum DialogKind {
    NewFile,
    NewFolder,
    Rename(PathBuf),
}

pub enum Dialog {
    Input {
        kind: DialogKind,
        input: Entity<TextInput>,
        _subscription: Subscription,
    },
    About,
}

impl Workspace {
    pub(super) fn on_tree_event(
        &mut self,
        _: &Entity<crate::tree_view::TreeView>,
        event: &TreeEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            TreeEvent::Open(path) => {
                self.open_file(path, window, cx);
            }
            TreeEvent::NewFile { dir } => {
                self.open_input_dialog(DialogKind::NewFile, dir, window, cx)
            }
            TreeEvent::NewFolder { dir } => {
                self.open_input_dialog(DialogKind::NewFolder, dir, window, cx)
            }
            TreeEvent::Rename(path) => {
                self.open_input_dialog(DialogKind::Rename(path.clone()), path, window, cx)
            }
            TreeEvent::Delete(path) => self.trash(path.clone(), window, cx),
            TreeEvent::RevealInFinder(path) => cx.reveal_path(path),
        }
    }

    fn open_input_dialog(
        &mut self,
        kind: DialogKind,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = match &kind {
            DialogKind::Rename(_) => path.to_string_lossy().to_string(),
            _ => format!("{}/", path.to_string_lossy().trim_end_matches('/')),
        };
        let input = cx.new(|cx| TextInput::new(window, cx).with_text(initial.clone()));
        input.update(cx, |input, cx| {
            if let DialogKind::Rename(_) = &kind {
                // Select the file name without its extension, like Finder.
                let name_start = initial.rfind('/').map_or(0, |i| i + 1);
                let name_end = initial[name_start..]
                    .rfind('.')
                    .filter(|i| *i > 0)
                    .map_or(initial.len(), |i| name_start + i);
                input.select_range(name_start..name_end, cx);
            }
            input.focus(window, cx);
        });
        let subscription =
            cx.subscribe_in(&input, window, |this: &mut Self, _, event, window, cx| {
                if *event == TextInputEvent::Submit {
                    this.submit_dialog(window, cx);
                }
            });
        self.dialog = Some(Dialog::Input {
            kind,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn show_about(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::About);
        cx.notify();
    }

    fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.focus_active(window, cx);
        cx.notify();
    }

    fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Input { kind, input, .. }) = &self.dialog else {
            return;
        };
        let target = PathBuf::from(input.read(cx).text().trim());
        let result = match kind {
            DialogKind::NewFile => create_file(&target).map(|()| Some(target.clone())),
            DialogKind::NewFolder => create_dir(&target).map(|()| None),
            DialogKind::Rename(from) => {
                let from = from.clone();
                rename(&from, &target).map(|()| {
                    self.retarget_editors(&from, &target, cx);
                    None
                })
            }
        };
        match result {
            Ok(open) => {
                self.dialog = None;
                if let Some(tree) = self.tree.clone() {
                    tree.update(cx, |tree, cx| {
                        tree.refresh(cx);
                        tree.reveal(&target, cx);
                    });
                }
                self.file_list = None;
                match open {
                    Some(path) => {
                        self.open_file(&path, window, cx);
                    }
                    None => self.focus_active(window, cx),
                }
            }
            Err(error) => self.error(format!("{}: {error}", target.display()), cx),
        }
        cx.notify();
    }

    /// Points open editors at a renamed file (or files inside a renamed folder).
    fn retarget_editors(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            editor.update(cx, |editor, cx| {
                let Some(path) = editor.path.clone() else {
                    return;
                };
                if let Ok(rest) = path.strip_prefix(from) {
                    editor.path = Some(if rest.as_os_str().is_empty() {
                        to.to_path_buf()
                    } else {
                        to.join(rest)
                    });
                    editor.refresh_disk_stamp();
                    cx.notify();
                }
            });
        }
    }

    fn trash(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Move “{name}” to the Trash?"),
            Some("Open tabs for it will be closed."),
            &["Move to Trash", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                if let Err(error) = trash::delete(&path) {
                    this.error(format!("Could not move to Trash: {error}"), cx);
                    return;
                }
                for p in 0..this.panes.len() {
                    let doomed: Vec<usize> = this.panes[p]
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| item.path(cx).is_some_and(|f| f.starts_with(&path)))
                        .map(|(i, _)| i)
                        .collect();
                    for i in doomed.into_iter().rev() {
                        this.remove_item(p, i, window, cx);
                    }
                }
                if let Some(tree) = this.tree.clone() {
                    tree.update(cx, |tree, cx| tree.refresh(cx));
                }
                this.file_list = None;
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn render_dialog(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dismiss = cx.listener(|this, _, window, cx| this.close_dialog(window, cx));
        match self.dialog.as_ref()? {
            Dialog::Input { kind, input, .. } => {
                let (title, action) = match kind {
                    DialogKind::NewFile => ("New File", "Create"),
                    DialogKind::NewFolder => ("New Folder", "Create"),
                    DialogKind::Rename(_) => ("Rename", "Rename"),
                };
                Some(
                    Modal::new(title)
                        .width(px(560.))
                        .on_dismiss(dismiss)
                        .child(div().on_action(cx.listener(|this, _: &ferrum_ui::menu::Cancel, window, cx| this.close_dialog(window, cx))).child(input.clone()))
                        .footer(Button::new("dialog-cancel", "Cancel").on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))))
                        .footer(
                            Button::new("dialog-ok", action)
                                .style(ButtonStyle::Filled)
                                .on_click(cx.listener(|this, _, window, cx| this.submit_dialog(window, cx))),
                        )
                        .into_any_element(),
                )
            }
            Dialog::About => Some(
                Modal::new("FerrumIDE")
                    .on_dismiss(dismiss)
                    .dismiss_on_backdrop(true)
                    .child(Label::new(format!("Version {}", crate::VERSION)))
                    .child(Label::new("A small, hackable text editor written in Rust on GPUI.").color(Color::Muted))
                    .child(
                        Label::new("Settings, keymap, theme and snippets live in ~/.ferrum/editor/ and apply as you save.")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            ),
        }
    }
}
