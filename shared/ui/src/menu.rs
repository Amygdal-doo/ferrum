//! Keyboard actions shared by menus, pickers and dialogs. `ferrum_ui::run`
//! binds Escape to [`Cancel`] everywhere and Enter / ↑ / ↓ inside the
//! `menu` key context.

gpui::actions!(
    menu,
    [
        /// Close the menu or dialog.
        Cancel,
        /// Run the selected item.
        Confirm,
        /// Run the selected item with the alternate behaviour.
        SecondaryConfirm,
        /// Move the selection down.
        SelectNext,
        /// Move the selection up.
        SelectPrevious,
        /// Select the first item.
        SelectFirst,
        /// Select the last item.
        SelectLast,
    ]
);
