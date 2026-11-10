//! Key tab (Redis): summary (type, length, TTL, memory), TTL / delete
//! actions, and the value: an editor for text strings, the results grid for
//! everything else.

use crate::app::FerrumData;
use crate::commands::keys::KeyInputs;
use crate::state::{Dialog, KeyViewLoad};
use ferrum_ui::prelude::*;
use ferrum_ui::ui::TintColor;
use ferrumdata_domain::{EditorTabId, KeyKind, KeyTtl};

/// What the view needs from the loaded value (avoids cloning its rows).
struct Loaded {
    summary: String,
    kind: KeyKind,
    has_ttl: bool,
    /// Text string value the editor can show and save.
    editable_text: bool,
    truncated: bool,
}

impl FerrumData {
    /// Value editor and TTL box of a key tab, created on first display.
    fn key_inputs(
        &mut self,
        tab_id: EditorTabId,
        text: Option<String>,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<TextInput>, Entity<TextInput>) {
        if let Some(i) = self.key_inputs.get(&tab_id) {
            return (i.value.clone(), i.ttl.clone());
        }
        let value = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .multiline(6)
                .borderless()
                .read_only(read_only)
                .with_text(text.unwrap_or_default())
        });
        let ttl = cx.new(|cx| TextInput::new(window, cx).placeholder("TTL, e.g. 90 / 15m / 2h"));
        self.key_inputs.insert(
            tab_id,
            KeyInputs {
                value: value.clone(),
                ttl: ttl.clone(),
            },
        );
        (value, ttl)
    }

    pub fn render_key_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let Some(tab) = self.state.active_tab() else {
            return div().into_any_element();
        };
        let (Some(key), Some(profile_id)) = (tab.key.clone(), tab.profile_id) else {
            return div().into_any_element();
        };
        let tab_id = tab.id;
        let read_only = self.state.profile(profile_id).is_none_or(|p| p.read_only);
        let mut text = None;
        let load = match self.state.key_views.get(&tab_id) {
            None | Some(KeyViewLoad::Loading) => Err(None),
            Some(KeyViewLoad::Failed(e)) => Err(Some(e.clone())),
            Some(KeyViewLoad::Loaded(v)) => {
                text.clone_from(&v.text);
                Ok(Loaded {
                    summary: v.summary(),
                    kind: v.kind.clone(),
                    has_ttl: matches!(v.ttl, KeyTtl::ExpiresIn(_)),
                    editable_text: v.kind == KeyKind::String && v.text.is_some(),
                    truncated: v.truncated,
                })
            }
        };
        let (value_input, ttl_input) = self.key_inputs(tab_id, text, read_only, window, cx);
        let crumb = |text: String| Label::new(text).size(LabelSize::Small).color(Color::Muted);
        let exists = load
            .as_ref()
            .is_ok_and(|l| !matches!(l.kind, KeyKind::Missing));
        let ro_tip = |what: &'static str| -> SharedString {
            if read_only {
                "Read-only profile: editing is disabled".into()
            } else {
                what.into()
            }
        };

        let header = h_flex()
            .flex_none()
            .gap_3()
            .px_4()
            .pt_3()
            .child(
                h_flex()
                    .gap_2()
                    .min_w_0()
                    .child(crumb("Keys".into()))
                    .child(crumb("/".into()))
                    .child(
                        div()
                            .truncate()
                            .child(Label::new(key.clone()).size(LabelSize::Small)),
                    ),
            )
            .children(load.as_ref().ok().map(|l| {
                div()
                    .truncate()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(l.summary.clone())
            }))
            .child(div().flex_1())
            .child(
                Button::new("refresh-key", "Refresh")
                    .start_icon(Icon::new(IconName::RotateCw).size(IconSize::Small))
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text("Reload the value"))
                    .on_click(cx.listener(move |this, _, _, cx| this.load_key_view(tab_id, cx))),
            );

        let has_ttl = load.as_ref().is_ok_and(|l| l.has_ttl);
        let delete_profile = self.state.profile(profile_id).cloned();
        let delete_key = key.clone();
        let actions = h_flex()
            .flex_none()
            .gap_2()
            .px_4()
            .pt_2()
            .child(div().w(px(170.)).child(ttl_input))
            .child(
                Button::new("key-set-ttl", "Set TTL")
                    .style(ButtonStyle::Outlined)
                    .disabled(read_only || !exists)
                    .tooltip(Tooltip::text(ro_tip(
                        "Expire the key after this time (PEXPIRE)",
                    )))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_key_ttl(tab_id, cx))),
            )
            .child(
                Button::new("key-persist", "Remove TTL")
                    .style(ButtonStyle::Outlined)
                    .disabled(read_only || !has_ttl)
                    .tooltip(Tooltip::text(ro_tip("Keep the key forever (PERSIST)")))
                    .on_click(cx.listener(move |this, _, _, cx| this.persist_key(tab_id, cx))),
            )
            .child(div().flex_1())
            .when(read_only, |this| {
                this.child(
                    Label::new("Read-only profile: editing is disabled")
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .child(
                Button::new("key-delete", "Delete…")
                    .style(ButtonStyle::Tinted(TintColor::Error))
                    .disabled(read_only || !exists)
                    .tooltip(Tooltip::text(ro_tip("Delete this key (asks first)")))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(profile) = delete_profile.clone() {
                            this.open_dialog(
                                Dialog::DeleteKey {
                                    profile,
                                    key: delete_key.clone(),
                                },
                                window,
                                cx,
                            );
                        }
                    })),
            );

        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        let body = match load {
            Err(None) => message("Loading value…".into()),
            Err(Some(e)) => {
                h_flex()
                    .gap_2()
                    .p_4()
                    .text_sm()
                    .text_color(status.error)
                    .child(e)
                    .child(Button::new("key-retry", "Retry").on_click(
                        cx.listener(move |this, _, _, cx| this.load_key_view(tab_id, cx)),
                    ))
                    .into_any_element()
            }
            Ok(l) if l.kind == KeyKind::Missing => {
                message("This key no longer exists (deleted or expired).".into())
            }
            Ok(Loaded {
                kind: KeyKind::Other(name),
                ..
            }) => message(format!(
                "Values of type “{name}” are not shown here; use the command console \
                 (e.g. a module command such as JSON.GET)."
            )),
            Ok(l) if l.editable_text => {
                let can_save = !read_only && !l.truncated;
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id(SharedString::from(format!("key-value-{tab_id}")))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_2()
                            .bg(colors.editor_background)
                            .font_buffer(cx)
                            .text_buffer(cx)
                            .child(value_input),
                    )
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_3()
                            .px_4()
                            .py_2()
                            .border_t_1()
                            .border_color(colors.border)
                            .child(
                                Button::new("key-save", "Save value")
                                    .style(ButtonStyle::Filled)
                                    .disabled(!can_save)
                                    .tooltip(Tooltip::text(if l.truncated {
                                        "Only part of this value was loaded".into()
                                    } else {
                                        ro_tip("SET … KEEPTTL: replaces the value, keeps the TTL")
                                    }))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.save_key_value(tab_id, cx);
                                    })),
                            )
                            .children(
                                self.state
                                    .runs
                                    .get(&tab_id)
                                    .and_then(|r| r.note.clone())
                                    .map(|note| {
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .text_color(status.warning)
                                            .child(note)
                                    }),
                            ),
                    )
                    .into_any_element()
            }
            // Hashes, lists, sets, sorted sets, streams and binary strings.
            Ok(_) => self.render_results(window, cx),
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(actions)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .mt_3()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(body),
            )
            .into_any_element()
    }
}
