//! Command handlers by name. The catalogue of names, scopes and default
//! keys lives in `ferrumeditor_core::commands`; this registry pairs names
//! with workspace-level handlers.
//!
//! **Extension seam.** A future package system implements [`Package`]:
//! on activation it registers handlers here and contributes
//! [`CommandInfo`]s (which makes them appear in the palette and bindable
//! in `keymap.toml`). No packages exist yet.

use std::collections::HashMap;
use std::rc::Rc;

use ferrumeditor_core::commands::CommandInfo;
use gpui::{Context, Window};

use crate::workspace::Workspace;

pub type Handler = Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

#[derive(Default, Clone)]
pub struct CommandRegistry {
    handlers: HashMap<String, Handler>,
}

impl CommandRegistry {
    pub fn register(
        &mut self,
        name: &str,
        handler: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    ) {
        self.handlers.insert(name.to_string(), Rc::new(handler));
    }

    pub fn get(&self, name: &str) -> Option<Handler> {
        self.handlers.get(name).cloned()
    }
}

/// What a package would implement to add commands.
#[allow(dead_code)]
pub trait Package {
    fn name(&self) -> &str;
    /// Command metadata (name, scope, default keys) for palette + keymap.
    fn commands(&self) -> Vec<CommandInfo>;
    /// Registers a handler for each command returned by [`Package::commands`].
    fn activate(&self, registry: &mut CommandRegistry);
}
