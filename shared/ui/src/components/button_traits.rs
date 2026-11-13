//! The builder-trait impls every button kind shares.

/// Implements the shared builder traits for a button kind with a `base`.
macro_rules! button_traits {
    ($ty:ty) => {
        impl $crate::components::Clickable for $ty {
            fn on_click(
                mut self,
                handler: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
            ) -> Self {
                self.base.on_click = Some(std::rc::Rc::new(handler));
                self
            }

            fn cursor_style(mut self, cursor: gpui::CursorStyle) -> Self {
                self.base.cursor = cursor;
                self
            }
        }

        impl $crate::components::Disableable for $ty {
            fn disabled(mut self, disabled: bool) -> Self {
                self.base.disabled = disabled;
                self
            }
        }

        impl $crate::components::Toggleable for $ty {
            fn toggle_state(mut self, selected: bool) -> Self {
                self.base.selected = selected;
                self
            }
        }

        impl $crate::components::SelectableButton for $ty {
            fn selected_style(mut self, style: $crate::components::ButtonStyle) -> Self {
                self.base.selected_style = Some(style);
                self
            }
        }

        impl $crate::components::FixedWidth for $ty {
            fn width(mut self, width: impl Into<gpui::DefiniteLength>) -> Self {
                self.base.width = Some(width.into());
                self
            }

            fn full_width(mut self) -> Self {
                self.base.width = Some(gpui::relative(1.));
                self
            }
        }

        impl $crate::components::VisibleOnHover for $ty {
            fn visible_on_hover(mut self, group: impl Into<gpui::SharedString>) -> Self {
                self.base.hover_group = Some(group.into());
                self
            }
        }

        impl $crate::components::ButtonCommon for $ty {
            fn id(&self) -> &gpui::ElementId {
                &self.base.id
            }

            fn style(mut self, style: $crate::components::ButtonStyle) -> Self {
                self.base.style = style;
                self
            }

            fn size(mut self, size: $crate::components::ButtonSize) -> Self {
                self.base.size = size;
                self
            }

            fn tooltip(
                mut self,
                tooltip: impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static,
            ) -> Self {
                self.base.tooltip = Some(std::rc::Rc::new(tooltip));
                self
            }

            fn track_focus(mut self, focus: &gpui::FocusHandle) -> Self {
                self.base.focus = Some(focus.clone());
                self
            }
        }
    };
}

pub(crate) use button_traits;
