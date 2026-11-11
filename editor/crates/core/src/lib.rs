//! Ferrum Editor core: everything that does not need a window. The text
//! buffer, multi-cursor editing, undo, motions, search, syntax highlighting,
//! git line marks and the user's config / keymap / theme / snippets all live
//! here and are unit-tested without GPUI.

pub mod buffer;
pub mod cli;
pub mod commands;
pub mod config;
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
pub mod markdown;
pub mod paths;
pub mod project_search;
pub mod selection;
pub mod session;
pub mod snippets;
pub mod terminal;
pub mod text;
pub mod theme;
