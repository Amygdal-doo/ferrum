//! Bodies of the posts on `/updates`, one module per post. Each exports a
//! `POST: crate::updates::Post`. Claims must come from the app READMEs,
//! `catalog.json` and the product pages: no invented plans, numbers or dates.

pub mod agentic_workspace;
pub mod editor_panes_agentic;
pub mod glass_and_cards;
pub mod launcher_settings_compact;
pub mod onboarding_settings;
pub mod per_app_themes;
pub mod public_alpha;
pub mod rex_layout_and_fec;
pub mod system_and_vibe;
pub mod term_menu_worktrees;
pub mod three_new_names;

use maud::{Markup, html};

use crate::components::card;
use crate::products::Product;

/// A product row: logo, linked name, kind and one line about it.
pub fn product_row(p: &Product, text: &str) -> Markup {
    html! {
        li class="post-app" style=(p.accent_style()) {
            (card::app_logo(p, "app-logo--sm", true))
            div class="post-app__copy" {
                p class="post-app__name" {
                    a href=(p.path()) { (p.name) }
                    span class="post-app__kind" { (p.kind) }
                }
                p class="post-app__text" { (text) }
            }
        }
    }
}
