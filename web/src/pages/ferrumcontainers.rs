//! FerrumContainers product page. Claims follow the FerrumContainers README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMCONTAINERS;

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
        "Bring a Compose project up, down or restart it in one click, start one straight \
         from its compose file, and follow all its services' logs together.",
    ),
    (
        "Your project first",
        "Open a project and see only its containers and compose files. Run a new container \
         with ports, env, volumes and a restart policy from a dialog.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Engine detection: DOCKER_HOST, Docker Desktop, OrbStack, Colima",
    "Containers grouped by Compose project with live stats and actions",
    "Live logs, inspect and Open shell",
    "Images, volumes and networks with remove and prune",
    "Compose up, down and restart; Up from a compose file; per-project ps and combined logs",
    "Project bar: the open project's containers and compose files",
    "Run dialog for new containers",
    "Shell choice (auto, bash, zsh, sh), log save, regex search and since filter",
    "CPU and memory charts, network and disk I/O",
    "Docker contexts with an engine picker; image tag and layer history",
];

const NOT_BUILT_YET: &[&str] = &[
    "Kubernetes contexts",
    "A file browser inside containers",
    "Building images from a Dockerfile",
    "Pulling from private registries (no registry sign-in yet)",
    "ssh:// engines and TLS certificates",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMCONTAINERS,
    meta_description: "FerrumContainers is a native Rust container manager for Docker Desktop, \
                       OrbStack, Colima and Podman: containers, logs, images, volumes and \
                       Compose.",
    intro: "A native window on the engine you already run. FerrumContainers talks to it over the \
            Docker Engine API; it doesn't ship or replace an engine.",
    features_lead: "The engine client is unit-tested against a fake engine and read-checked \
                    against Docker Desktop; the window is new in this alpha.",
    features: FEATURES,
    screenshots: &[
        "containers grouped by Compose project",
        "live logs of a container",
    ],
    status_summary: "FerrumContainers is in alpha. Expect rough edges in the window while it gets \
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
        ("Install", "FerrumLauncher (free, see Download)"),
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
        assert!(html.contains(">FerrumContainers</h1>"));
        assert!(html.contains("Animated preview of FerrumContainers"));
        assert!(html.contains("/assets/img/brand/dock.svg"));
        assert!(html.contains("href=\"/download\""));
    }
}
