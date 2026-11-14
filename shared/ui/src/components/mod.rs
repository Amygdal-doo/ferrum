//! Ferrum's widget set: buttons, labels, icons, checkboxes, tabs, list
//! rows, tooltips and menus, all drawn from the active [`crate::theme`].

mod button;
mod button_traits;
mod buttons;
mod color;
mod context_menu;
mod divider;
mod dropdown;
mod icon;
pub(crate) mod icon_name;
mod label;
mod list_item;
mod popover;
mod tab;
#[cfg(test)]
mod tests;
mod toggle;
mod tooltip;
mod traits;

pub use button::{ButtonSize, ButtonStyle};
pub use buttons::{Button, ContentButton, IconButton};
pub use color::{Color, TintColor};
pub use context_menu::ContextMenu;
pub use divider::{Divider, ProgressBar};
pub use dropdown::{DropdownMenu, DropdownStyle};
pub use icon::{Icon, IconPosition, IconSize};
pub use icon_name::{IconName, UnknownIcon};
pub use label::{
    Headline, HeadlineSize, Label, LabelCommon, LabelProps, LabelSize, LineHeightStyle,
    LoadingLabel,
};
pub use list_item::{ListItem, ListItemSpacing};
pub use popover::{PopoverMenu, PopoverTrigger, RightClickMenu, right_click_menu};
pub use tab::{TAB_HEIGHT, Tab, TabBar};
pub use toggle::Checkbox;
pub use tooltip::{Tooltip, format_keystroke};
pub use traits::{
    ButtonCommon, Clickable, Disableable, FixedWidth, SelectableButton, StyledExt,
    StyledTypography, TextSize, ToggleState, Toggleable, VisibleOnHover, h_flex, rems_from_px,
    v_flex,
};

pub use crate::theme::ActiveTheme;

/// Everything a view usually needs: the components, their builder traits,
/// and GPUI's element prelude.
pub mod prelude {
    pub use gpui::prelude::*;
    pub use gpui::{
        AbsoluteLength, AnyElement, App, Context, DefiniteLength, Div, Element, ElementId,
        InteractiveElement, ParentElement, Pixels, Rems, RenderOnce, SharedString, Styled, Window,
        div, px, relative, rems,
    };

    pub use super::{
        ActiveTheme, Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Disableable,
        FixedWidth, Headline, HeadlineSize, Icon, IconButton, IconName, IconPosition, IconSize,
        Label, LabelCommon, LabelSize, LineHeightStyle, LoadingLabel, SelectableButton, StyledExt,
        StyledTypography, TextSize, Toggleable, VisibleOnHover, h_flex, rems_from_px, v_flex,
    };
}
