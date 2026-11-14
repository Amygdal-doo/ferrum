//! What can go on a diagram: component kinds (with each cloud's service
//! for them), technologies (languages, frameworks, databases, AI tools)
//! and the clouds themselves.

mod kinds;
mod techs;

use serde::{Deserialize, Serialize};

pub use kinds::KINDS;
pub use techs::TECHS;

/// Whose service names the diagram uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    Generic,
    Aws,
    Azure,
    Gcp,
    Hetzner,
    DigitalOcean,
    OpenSource,
}

impl Provider {
    pub const ALL: [Self; 7] = [
        Self::Generic,
        Self::Aws,
        Self::Azure,
        Self::Gcp,
        Self::Hetzner,
        Self::DigitalOcean,
        Self::OpenSource,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Aws => "AWS",
            Self::Azure => "Azure",
            Self::Gcp => "GCP",
            Self::Hetzner => "Hetzner",
            Self::DigitalOcean => "DigitalOcean",
            Self::OpenSource => "Open source",
        }
    }

    /// Brand colour, `0xRRGGBB`.
    pub fn color(self) -> u32 {
        match self {
            Self::Generic => 0x8b8b90,
            Self::Aws => 0xff9900,
            Self::Azure => 0x0078d4,
            Self::Gcp => 0x4285f4,
            Self::Hetzner => 0xd50c2d,
            Self::DigitalOcean => 0x0080ff,
            Self::OpenSource => 0x3ddc84,
        }
    }
}

/// Groups of the palette.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Client,
    Mobile,
    Frontend,
    #[default]
    Backend,
    Database,
    Messaging,
    Ai,
    Infra,
    Cloud,
    Custom,
}

impl Category {
    pub const ALL: [Self; 10] = [
        Self::Client,
        Self::Mobile,
        Self::Frontend,
        Self::Backend,
        Self::Database,
        Self::Messaging,
        Self::Ai,
        Self::Infra,
        Self::Cloud,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Client => "Clients & edge",
            Self::Mobile => "Mobile",
            Self::Frontend => "Frontend",
            Self::Backend => "Backend",
            Self::Database => "Data",
            Self::Messaging => "Messaging",
            Self::Ai => "AI",
            Self::Infra => "Infrastructure",
            Self::Cloud => "Clouds",
            Self::Custom => "Custom",
        }
    }
}

/// A component kind and what each provider calls it. `services` follows
/// [`Provider::ALL`] without `Generic` (the kind's own name).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// AWS, Azure, GCP, Hetzner, DigitalOcean, open source.
    pub services: [&'static str; 6],
}

impl Kind {
    pub fn find(id: &str) -> Option<&'static Kind> {
        KINDS.iter().find(|k| k.id == id)
    }

    /// The name of this kind's service on `provider`.
    pub fn service(&self, provider: Provider) -> &'static str {
        match provider {
            Provider::Generic => self.name,
            Provider::Aws => self.services[0],
            Provider::Azure => self.services[1],
            Provider::Gcp => self.services[2],
            Provider::Hetzner => self.services[3],
            Provider::DigitalOcean => self.services[4],
            Provider::OpenSource => self.services[5],
        }
    }
}

/// A technology with a badge: brand colour and a short mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tech {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// `0xRRGGBB`.
    pub color: u32,
    /// Up to three characters for the badge (`Go`, `K8s`).
    pub mark: &'static str,
    /// Component kind it usually is (for the palette).
    pub kind: &'static str,
}

impl Tech {
    pub fn find(id: &str) -> Option<&'static Tech> {
        TECHS.iter().find(|t| t.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_known() {
        let kinds: HashSet<_> = KINDS.iter().map(|k| k.id).collect();
        assert_eq!(kinds.len(), KINDS.len());
        let techs: HashSet<_> = TECHS.iter().map(|t| t.id).collect();
        assert_eq!(techs.len(), TECHS.len());
        for tech in TECHS {
            assert!(kinds.contains(tech.kind), "{} -> {}", tech.id, tech.kind);
            assert!(
                !tech.mark.is_empty() && tech.mark.chars().count() <= 3,
                "{}",
                tech.id
            );
        }
    }

    #[test]
    fn every_kind_names_a_service_everywhere() {
        for kind in KINDS {
            for provider in Provider::ALL {
                assert!(
                    !kind.service(provider).is_empty(),
                    "{} {provider:?}",
                    kind.id
                );
            }
        }
        let lb = Kind::find("load-balancer").expect("kind");
        assert_eq!(lb.service(Provider::Aws), "Elastic Load Balancing");
    }
}
