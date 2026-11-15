//! FerrumPorts product page. Claims follow the FerrumPorts README
//! ("What works today" / "Known limitations") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMPORTS;

const FEATURES: &[(&str, &str)] = &[
    (
        "Every listening port",
        "TCP listeners with port, address, process, full command line, user, start time and \
         working directory. IPv4 and IPv6 sockets of one process share a row, and a UDP \
         toggle adds bound UDP sockets.",
    ),
    (
        "Grouped by project",
        "Each process's working directory is mapped to its git root, so you see \
         \"shop: 3000 web, 5432 postgres\". This project shows only the ports of the project \
         in the strip at the top.",
    ),
    (
        "Search and refresh",
        "The table refreshes every 2 s, with Pause and ⌘R. ⌘F searches ports, pids, \
         processes, commands, projects and addresses; 30 finds 3000.",
    ),
    (
        "Stop a process",
        "A confirmation shows the full command line, then sends SIGTERM. If the process is \
         still running 2 s later, a second dialog offers SIGKILL. Pids 0 and 1 are refused.",
    ),
    (
        "Is port N free?",
        "Type a port and FerrumPorts tries to bind it. If it's taken, you see which process \
         holds it and the nearest free port.",
    ),
    (
        "Details and hints",
        "Addresses and exposure (local, one interface or all), every port of the same pid, \
         and hints for about 45 well-known ports such as 5173 Vite or 6379 Redis. Open the \
         project in FerrumIDE, FerrumGIT or FerrumTerm.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "TCP listeners and bound UDP sockets, with process, command, user and working directory",
    "Group by project and This project",
    "Auto-refresh every 2 s, search, and well-known port hints",
    "Stop with SIGTERM, then SIGKILL if needed, after a confirmation",
    "Is port N free?, with the nearest free port",
    "The project strip, with ⌥⌘O to switch projects",
];

const NOT_BUILT_YET: &[&str] = &[
    "Column sorting and keyboard navigation in the table",
    "A context menu on rows (actions are on the row icons and in the detail pane)",
    "UDP in the free-port check (it tests TCP only)",
    "Windows support",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMPORTS,
    meta_description: "FerrumPorts is a native Rust app that shows which process listens on \
                       which port, groups them by project, and stops them after a \
                       confirmation.",
    intro: "\"Who is using port 3000?\" answered in one window. FerrumPorts reads the system \
            with lsof, ps and ss, and writes nothing but the shared list of recent Ferrum \
            projects.",
    features_lead: "The scanners, parsers and kill logic are unit-tested; the window is covered \
                    by headless tests.",
    features: FEATURES,
    screenshots: &[
        "listening ports grouped by project",
        "stopping a dev server after a confirmation",
    ],
    status_summary: "FerrumPorts is in alpha, as a first working version. Its tests are \
                     automated; the app hasn't been clicked through by hand yet, and the row \
                     and detail-pane actions are wired but not hand-tested.",
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
        ("Shell command", "fep, turned on in FerrumLauncher"),
        (
            "Platforms",
            "macOS with lsof; Linux with ss (written and tested, not yet run on Linux)",
        ),
        ("Built with", "Rust, GPUI, lsof, ps and ss"),
        (
            "Tested with",
            "Automated unit tests and headless window tests, not yet by hand",
        ),
    ],
    specs_note: "Without root, lsof on macOS only shows your own processes' sockets, so a \
                 port held by root or another user may be missing. On Linux, ss lists them \
                 without the pid or process name.",
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
        assert!(html.contains(">FerrumPorts</h1>"));
        assert!(html.contains("Animated preview of FerrumPorts"));
        assert!(html.contains("/assets/img/brand/ports.svg"));
        assert!(html.contains("href=\"/download\""));
    }
}
