//! A component's icon: the project's custom icon for its technology
//! (`.architecture/icons/<id>.png`), the real logo for AI models Ferrum
//! ships, else a badge in the technology's brand colour with its short
//! mark, else a glyph for the component kind.

use std::path::Path;

use ferrum_ui::gpui::{Hsla, ObjectFit, Pixels, Rgba, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrum_ui::{AgentLogo, agent_logo};
use ferrumsystem_core::{Category, Kind, System, Tech, store};

/// `0xRRGGBB` → colour.
pub fn rgb(hex: u32) -> Hsla {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.,
        g: ((hex >> 8) & 0xff) as f32 / 255.,
        b: (hex & 0xff) as f32 / 255.,
        a: 1.,
    }
    .into()
}

/// A brand colour made readable on the current background.
fn readable(color: Hsla, dark: bool) -> Hsla {
    let mut c = color;
    if dark {
        c.l = c.l.max(0.62);
    } else {
        c.l = c.l.min(0.42);
    }
    c
}

/// Name, colour and mark of technology `id` (catalogue or custom).
pub fn tech_info(system: &System, id: &str) -> Option<(String, u32, String)> {
    if let Some(t) = Tech::find(id) {
        return Some((t.name.into(), t.color, t.mark.into()));
    }
    system.custom_techs.iter().find(|t| t.id == id).map(|t| {
        let mark: String = t
            .name
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(2)
            .collect();
        (t.name.clone(), t.color, mark)
    })
}

/// The logo of a catalogue AI technology: a model maker or provider
/// (`claude`, `openai`, `ollama`, `qwen`…) or a coding agent (`claude-code`).
fn agent(id: &str) -> Option<AgentLogo> {
    if !Tech::find(id).is_some_and(|t| t.category == Category::Ai) {
        return None;
    }
    AgentLogo::for_provider(id).or_else(|| AgentLogo::for_program(id))
}

/// Glyph for a kind without a technology.
pub fn kind_glyph(kind: &str) -> IconName {
    match kind {
        "web-client" | "static-site" => IconName::Public,
        "mobile-client" | "desktop-client" => IconName::Monitor,
        "dns" | "cdn" => IconName::Cloud,
        "waf" | "auth" | "secrets" => IconName::Lock,
        "load-balancer" | "api-gateway" => IconName::ArrowRightLeft,
        "queue" | "stream" | "pubsub" | "realtime" => IconName::Zap,
        "email" => IconName::Bell,
        "cache" | "sql-db" | "nosql-db" | "wide-column" | "time-series" | "warehouse" => {
            IconName::DatabaseZap
        }
        "object-storage" => IconName::Archive,
        "search" | "vector-db" => IconName::MagnifyingGlass,
        "llm" | "ml" | "agent" => IconName::Sparkle,
        "cluster" | "registry" => IconName::Blocks,
        "vm" => IconName::Cpu,
        "monitoring" | "logging" => IconName::Eye,
        "ci" | "cron" | "etl" => IconName::RotateCw,
        "external" => IconName::ExternalLink,
        _ => match Kind::find(kind).map(|k| k.category) {
            Some(Category::Database) => IconName::DatabaseZap,
            _ => IconName::Server,
        },
    }
}

/// The icon for technology `tech` (or kind `kind`), `size` square.
pub fn icon(
    system: &System,
    root: Option<&Path>,
    tech: Option<&str>,
    kind: &str,
    size: Pixels,
    cx: &App,
) -> AnyElement {
    let dark = !cx.theme().appearance().is_light();
    if let Some(id) = tech {
        if let Some(path) = root.and_then(|r| store::icon(r, id)) {
            return img(path)
                .flex_none()
                .size(size)
                .rounded(size * 0.22)
                .object_fit(ObjectFit::Contain)
                .into_any_element();
        }
        if let Some(logo) = agent(id) {
            return div()
                .flex_none()
                .size(size)
                .flex()
                .items_center()
                .justify_center()
                .child(agent_logo(logo, size * 0.86, cx))
                .into_any_element();
        }
        if let Some((_, color, mark)) = tech_info(system, id) {
            let color = rgb(color);
            let text = readable(color, dark);
            return div()
                .flex_none()
                .size(size)
                .flex()
                .items_center()
                .justify_center()
                .rounded(size * 0.24)
                .bg(color.opacity(if dark { 0.2 } else { 0.14 }))
                .border_1()
                .border_color(color.opacity(0.55))
                .text_color(text)
                .text_size(size * if mark.chars().count() > 2 { 0.34 } else { 0.42 })
                .font_weight(gpui::FontWeight::BOLD)
                .child(mark)
                .into_any_element();
        }
    }
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .rounded(size * 0.24)
        .bg(colors.text.opacity(0.07))
        .border_1()
        .border_color(colors.text.opacity(0.14))
        .child(
            Icon::new(kind_glyph(kind))
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::agent;
    use ferrum_ui::AgentLogo;

    #[test]
    fn ai_technologies_show_their_logo() {
        assert_eq!(agent("claude"), Some(AgentLogo::Claude));
        assert_eq!(agent("claude-code"), Some(AgentLogo::Claude));
        assert_eq!(agent("openai"), Some(AgentLogo::OpenAi));
        assert_eq!(agent("ollama"), Some(AgentLogo::Ollama));
        assert_eq!(agent("qwen"), Some(AgentLogo::Qwen));
        assert_eq!(agent("glm"), Some(AgentLogo::Glm));
        assert_eq!(agent("mistral"), Some(AgentLogo::Mistral));
        assert_eq!(agent("muse"), Some(AgentLogo::Muse));
        assert_eq!(agent("vllm"), None);
        assert_eq!(agent("postgres"), None);
    }
}
