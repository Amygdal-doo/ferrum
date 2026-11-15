//! Settings › Notifications: which corner toasts appear in, which channels
//! are on, sound volume and per-event channels with a sound preview (see
//! [`crate::notify`]).

use gpui::{
    AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window, div, prelude::*, px,
};

use super::widgets::{Row, Section, Segmented, Switch, icon_tile};
use super::{SettingsPage, tint};
use crate::app_info::app_info;
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Disableable, Icon, IconName, IconSize, TintColor,
    h_flex, v_flex,
};
use crate::notify::{self, Channels, ToastPosition};
use crate::theme::ActiveTheme;

pub(super) struct NotificationsPage;

/// A miniature window with one pill per corner: top left, top right,
/// bottom left, bottom right.
fn position_picker(current: ToastPosition, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let cell = move |position: ToastPosition| {
        let selected = position == current;
        div()
            .id(("toast-position", position as usize))
            .w(px(36.))
            .h(px(14.))
            .rounded_full()
            .cursor_pointer()
            .bg(if selected {
                colors.text_accent
            } else {
                colors.element_active
            })
            .on_click(move |_, _, cx| notify::update(cx, |p| p.position = position))
    };
    let [top_left, top_right, bottom_left, bottom_right] = ToastPosition::CORNERS;
    v_flex()
        .p_2()
        .gap_6()
        .rounded_lg()
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.background)
        .child(
            h_flex()
                .gap_12()
                .child(cell(top_left))
                .child(cell(top_right)),
        )
        .child(
            h_flex()
                .gap_12()
                .child(cell(bottom_left))
                .child(cell(bottom_right)),
        )
}

/// "Bottom right" for the row's detail.
fn position_text(position: ToastPosition) -> String {
    let label = position.label();
    let mut chars = label.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn master(
    id: &'static str,
    title: &'static str,
    on: bool,
    set: fn(&mut notify::NotificationPrefs, bool),
) -> Row {
    Row::new(id, title)
        .control(Switch::new(id, on).on_change(move |on, _, cx| notify::update(cx, |p| set(p, on))))
}

impl SettingsPage for NotificationsPage {
    fn id(&self) -> &'static str {
        "notifications"
    }
    fn title(&self) -> &'static str {
        "Notifications"
    }
    fn icon(&self) -> IconName {
        IconName::Bell
    }
    fn tint(&self) -> Hsla {
        tint(0xf2555a)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["toast", "sound", "badge", "dock", "alerts"]
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let prefs = notify::load(cx);
        let info = app_info(cx);
        let volume_steps = [0.25_f32, 0.5, 0.75, 1.0];
        let volume_index = volume_steps
            .iter()
            .position(|v| (prefs.volume - v).abs() < 0.13)
            .unwrap_or(2);

        let in_app = Section::new("In-App Notifications").row(
            Row::new("toast-position", "Notification corner")
                .detail(position_text(prefs.position))
                .control(position_picker(prefs.position, cx)),
        );

        let channels = Section::new("Delivery")
            .detail(format!(
                "How {} lets you know something happened",
                info.name
            ))
            .row(master(
                "in-app",
                "Show in-app notifications",
                prefs.in_app,
                |p, on| p.in_app = on,
            ))
            .row(master(
                "system",
                "Show system notifications",
                prefs.system,
                |p, on| p.system = on,
            ))
            .row(master("badge", "Badge Dock icon", prefs.badge, |p, on| {
                p.badge = on
            }))
            .row(master("sound", "Play sounds", prefs.sound, |p, on| {
                p.sound = on
            }))
            .row(
                Row::new("volume", "Sound volume")
                    .disabled(!prefs.sound)
                    .control(
                        Segmented::new("volume-steps", volume_index)
                            .item("Low")
                            .item("Medium")
                            .item("High")
                            .item("Max")
                            .on_select(move |i, _, cx| {
                                notify::update(cx, |p| p.volume = volume_steps[i]);
                                notify::play_sound(volume_steps[i]);
                            }),
                    ),
            );

        let mut page = div()
            .flex()
            .flex_col()
            .gap_6()
            .child(in_app)
            .child(channels);

        if !info.events.is_empty() {
            let mut events =
                Section::new("Events").detail("Choose the channels for each kind of event.");
            for event in &info.events {
                let current = prefs
                    .events
                    .get(event.id)
                    .copied()
                    .unwrap_or(event.defaults);
                let id = event.id;
                let volume = prefs.volume;
                let play = Button::new(channel_id(id, "play"), "Play")
                    .style(ButtonStyle::Subtle)
                    .start_icon(Icon::new(IconName::PlayFilled).size(IconSize::XSmall))
                    .disabled(!prefs.sound || !current.sound)
                    .on_click(move |_, _, _| notify::play_event_sound(id, volume));
                let toggle = |name: &'static str, on: bool, set: fn(&mut Channels, bool)| {
                    Button::new(channel_id(id, name), name)
                        .style(if on {
                            ButtonStyle::Tinted(TintColor::Accent)
                        } else {
                            ButtonStyle::Subtle
                        })
                        .on_click(move |_, _, cx| {
                            notify::update(cx, |p| {
                                let mut c = p.events.get(id).copied().unwrap_or(current);
                                set(&mut c, !on);
                                p.events.insert(id.to_owned(), c);
                            })
                        })
                };
                events = events.row(
                    Row::new(("event", event.id.len() + event.title.len()), event.title)
                        .detail(current.summary())
                        .icon(icon_tile(event.icon, tint(event.tint), px(28.)))
                        .control(
                            h_flex()
                                .gap_1()
                                .child(toggle("In app", current.in_app, |c, on| c.in_app = on))
                                .child(toggle("System", current.system, |c, on| c.system = on))
                                .child(toggle("Badge", current.badge, |c, on| c.badge = on))
                                .child(toggle("Sound", current.sound, |c, on| c.sound = on))
                                .child(play),
                        ),
                );
            }
            page = page.child(events);
        }

        page.child(
            Section::new("Test").row(
                Row::new("test", "Send a test notification").control(
                    Button::new("test-notification", "Send")
                        .style(ButtonStyle::Subtle)
                        .on_click(|_, _, cx| {
                            let event = app_info(cx).events.first().map_or("settings", |e| e.id);
                            notify::notify(event, "Test notification", "This is how it looks.", cx);
                        }),
                ),
            ),
        )
        .into_any_element()
    }
}

/// Element id made of an event id and a channel name.
fn channel_id(event: &str, channel: &str) -> gpui::ElementId {
    gpui::ElementId::Name(format!("{event}-{channel}").into())
}
