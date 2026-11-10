//! FerrumDock product page. Claims follow the FerrumDock README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMDOCK;

const FEATURES: &[(&str, &str)] = &[
    (
        "Works with your engine",
        "Finds Docker Desktop, OrbStack or Colima on its own, and any engine set in \
         DOCKER_HOST, such as Podman. It tells you what to start when none is running.",
    ),
    (
        "Containers by project",
        "Containers grouped by Compose project, with status, ports and live CPU and memory. \
         Start, stop, restart, pause or remove them.",
    ),
    (
        "Logs and inspect",
        "Follow logs live with search and timestamps, and see environment, mounts, networks \
         and labels.",
    ),
    (
        "Shell in FerrumTerm",
        "Open a shell inside a container in a FerrumTerm tab, or in Terminal if FerrumTerm \
         isn't installed.",
    ),
    (
        "Images, volumes, networks",
        "Pull images with progress, remove what you don't need and prune unused data, with \
         the space you get back.",
    ),
    (
        "Compose projects",
        "Bring a whole Compose project up, down or restart it in one click.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Engine detection: DOCKER_HOST, Docker Desktop, OrbStack, Colima",
    "Containers grouped by Compose project with live stats and actions",
    "Live logs, inspect and Open shell",
    "Images, volumes and networks with remove and prune",
    "Compose up, down and restart",
];

const NOT_BUILT_YET: &[&str] = &[
    "Kubernetes contexts",
    "A file browser inside containers",
    "Building images from a Dockerfile",
    "Pulling from private registries (no registry sign-in yet)",
    "Docker contexts (only DOCKER_HOST and the usual sockets)",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMDOCK,
    meta_description: "FerrumDock is a native Rust container manager for Docker Desktop, \
                       OrbStack, Colima and Podman: containers, logs, images, volumes and \
                       Compose.",
    intro: "A native window on the engine you already run. FerrumDock talks to it over the \
            Docker Engine API; it doesn't ship or replace an engine.",
    features_lead: "The engine client is unit-tested against a fake engine and read-checked \
                    against Docker Desktop; the window is new in this alpha.",
    features: FEATURES,
    screenshots: &[
        "containers grouped by Compose project",
        "live logs of a container",
    ],
    status_summary: "FerrumDock is in alpha. Expect rough edges in the window while it gets \
                     day-to-day use.",
    status_groups: &[
        BulletGroup {
            heading: "Works today",
            items: WORKS_TODAY,
        },
        BulletGroup {
            heading: "Not built yet",
            items: NOT_BUILT_YET,
        },
    ],
    specs: &[
        ("Install", "Ferrum Launcher (free, see Download)"),
        (
            "Needs",
            "A running Docker-compatible engine: Docker Desktop, OrbStack, Colima or Podman",
        ),
        (
            "Builds",
            "macOS (Apple Silicon and Intel), Linux x86_64 and Windows",
        ),
        ("Built with", "Rust, GPUI, the Docker Engine API, Tokio"),
        ("Tested on", "macOS"),
    ],
    specs_note: "Removing containers, images and volumes asks first. Pruning volumes also \
                 removes unused named volumes; the confirmation says so.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_status_and_demo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumDock</h1>"));
        assert!(html.contains("Animated preview of FerrumDock"));
        assert!(html.contains("/assets/img/brand/dock.svg"));
        assert!(html.contains("href=\"/download\""));
    }
}
