//! Reusable editing widgets.

mod auth_editor;
mod kv_table;
mod secret_input;
mod vars_table;

pub use auth_editor::AuthEditor;
pub use kv_table::KvTable;
pub use secret_input::SecretInput;
pub use vars_table::VarsTable;

/// Borderless table cell input.
pub const CELL: &str =
    "h-7 w-full bg-transparent px-2 font-mono text-xs outline-none focus:bg-accent/40 select-text";
/// Cell referencing an unresolved `{{variable}}`.
pub const CELL_BAD: &str = "h-7 w-full bg-transparent px-2 font-mono text-xs outline-none focus:bg-accent/40 select-text text-destructive underline decoration-destructive decoration-dotted";
/// Standard bordered field.
pub const FIELD: &str = "h-8 w-full rounded-md border bg-background px-2 text-sm outline-none focus:border-ring select-text";

/// Tooltip listing unresolved variables (empty when none).
#[must_use]
pub fn unresolved_title(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("Unresolved: {}", names.join(", "))
    }
}
