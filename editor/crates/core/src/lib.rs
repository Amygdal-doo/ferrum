//! FerrumIDE core: everything that does not need a window. The text
//! buffer, multi-cursor editing, undo, motions, search, syntax highlighting,
//! git line marks, the language server and debug adapter clients, AI agent
//! snapshots and diff review, and the user's config / keymap / theme /
//! snippets all live here and are unit-tested without GPUI.

pub mod agents;
pub mod buffer;
pub mod cli;
pub mod commands;
pub mod completion;
pub mod config;
pub mod dap;
pub mod display;
pub mod editor;
pub mod file_tree;
pub mod files;
pub mod find;
pub mod fuzzy;
pub mod git;
pub mod highlight;
pub mod history;
pub mod icons;
pub mod keymap;
pub mod language;
pub mod lsp;
pub mod markdown;
pub mod paths;
pub mod project_search;
pub mod selection;
pub mod session;
pub mod snippets;
pub mod tasks;
pub mod terminal;
pub mod text;
pub mod theme;
pub mod tool_layout;
