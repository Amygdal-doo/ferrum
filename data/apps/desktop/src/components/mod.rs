pub mod connection;
pub mod dialogs;
pub mod explorer;
pub mod result_grid;
pub mod rust_ui;
pub mod shell;
pub mod sql_editor;
pub mod table_view;

use ferrum_domain::EnvironmentTag;

/// Tailwind classes for the environment colour dot. Literal strings so the
/// Tailwind scanner keeps them.
#[must_use]
pub const fn env_dot_class(env: EnvironmentTag) -> &'static str {
    match env {
        EnvironmentTag::Local => "bg-zinc-400",
        EnvironmentTag::Development => "bg-sky-500",
        EnvironmentTag::Staging => "bg-amber-500",
        EnvironmentTag::Production => "bg-red-600",
    }
}
