//! The bundled `material` icon theme: solid 16×16 glyphs in the spirit of
//! Material Design file icons (filled shapes with knocked-out details,
//! coloured per file type, folders coloured by their role). Drawn for
//! Ferrum Editor — no third-party icon files. GPUI paints an SVG as a
//! mask, so each icon is one shape in one tint.

/// A solid icon.
macro_rules! filled {
    ($($body:expr),* $(,)?) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="#000">"##,
            $($body,)*
            "</svg>"
        )
    };
}

/// A bold line icon (for shapes that read better as strokes).
macro_rules! bold {
    ($($body:expr),* $(,)?) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">"##,
            $($body,)*
            "</svg>"
        )
    };
}

/// A solid `shape` with `cut` (strokes, or shapes with `fill="#000"`)
/// knocked out of it.
macro_rules! knockout {
    ($shape:expr, $cut:expr $(,)?) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><defs><mask id="k"><rect width="16" height="16" fill="#fff"/><g fill="none" stroke="#000" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">"##,
            $cut,
            r##"</g></mask></defs><g fill="#000" mask="url(#k)">"##,
            $shape,
            "</g></svg>"
        )
    };
}

/// A page with rounded corners.
macro_rules! page {
    () => {
        r##"<path d="M3.5 1h6l3.5 3.5V14a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V2a1 1 0 0 1 1-1z"/>"##
    };
}

macro_rules! square {
    () => {
        r##"<rect x="1.5" y="1.5" width="13" height="13" rx="2.5"/>"##
    };
}

macro_rules! shield {
    () => {
        r##"<path d="M2 1h12l-1.1 12.3L8 15l-4.9-1.7z"/>"##
    };
}

const FILE: &str = knockout!(page!(), r##"<path d="M9.5 1v3.5H13"/>"##);
const TEXT: &str = knockout!(page!(), r##"<path d="M5.5 7.5h5M5.5 10h5M5.5 12.5h3"/>"##);
const CODE: &str = knockout!(
    page!(),
    r##"<path d="M6.8 7.5 5.3 9.5l1.5 2M9.2 7.5l1.5 2-1.5 2"/>"##
);
const PDF: &str = knockout!(
    page!(),
    r##"<path d="M5.5 12.5v-5h1.8a1.3 1.3 0 0 1 0 2.6H5.5"/>"##
);
const ARCHIVE: &str = knockout!(
    page!(),
    r##"<path d="M8 2.5v.5M8 4.5v.5M8 6.5v.5"/><rect x="6.8" y="8.5" width="2.4" height="3" rx=".5"/>"##
);
const FOLDER: &str = filled!(
    r##"<path d="M1 3.5a1 1 0 0 1 1-1h4.2l1.6 1.6H14a1 1 0 0 1 1 1v7.4a1 1 0 0 1-1 1H2a1 1 0 0 1-1-1z"/>"##
);
const FOLDER_OPEN: &str = filled!(
    r##"<path d="M1 3.5a1 1 0 0 1 1-1h4.2l1.6 1.6H13a1 1 0 0 1 1 1v1.4H4.3a1 1 0 0 0-.9.6L1 12.5z"/>"##,
    r##"<path d="M4.4 7.5h10.8a.5.5 0 0 1 .46.7l-2.2 5.2a1 1 0 0 1-.92.6H1.8a.5.5 0 0 1-.46-.7l2.2-5.2a1 1 0 0 1 .86-.6z"/>"##
);
const RUST: &str = knockout!(
    r##"<circle cx="8" cy="8" r="5.2"/><path fill="none" stroke="#000" stroke-width="2.2" stroke-linecap="round" d="M8 1.3v1.6M8 13.1v1.6M1.3 8h1.6M13.1 8h1.6M3.3 3.3l1.1 1.1M11.6 11.6l1.1 1.1M3.3 12.7l1.1-1.1M11.6 4.4l1.1-1.1"/>"##,
    r##"<circle cx="8" cy="8" r="2" fill="#000" stroke="none"/>"##
);
const JS: &str = knockout!(
    square!(),
    r##"<path d="M7.4 7v4.2a1.6 1.6 0 0 1-3 .8M12.2 7.6c-.5-.7-2.6-.8-2.6.6 0 1.4 2.6.7 2.6 2.2 0 1.4-2.1 1.5-2.8.5"/>"##
);
const TS: &str = knockout!(
    square!(),
    r##"<path d="M3.6 7h4M5.6 7v5.2M12.2 7.6c-.5-.7-2.6-.8-2.6.6 0 1.4 2.6.7 2.6 2.2 0 1.4-2.1 1.5-2.8.5"/>"##
);
const REACT: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.2">"##,
    r##"<ellipse cx="8" cy="8" rx="7" ry="2.6"/><ellipse cx="8" cy="8" rx="7" ry="2.6" transform="rotate(60 8 8)"/><ellipse cx="8" cy="8" rx="7" ry="2.6" transform="rotate(120 8 8)"/>"##,
    r##"<circle cx="8" cy="8" r="1.5" fill="#000" stroke="none"/></svg>"##
);
const JSON: &str = bold!(
    r##"<path d="M5.5 2c-1.4 0-2 .6-2 2v1.8c0 .9-.5 1.6-1.5 1.7v1c1 .1 1.5.8 1.5 1.7V12c0 1.4.6 2 2 2M10.5 2c1.4 0 2 .6 2 2v1.8c0 .9.5 1.6 1.5 1.7v1c-1 .1-1.5.8-1.5 1.7V12c0 1.4-.6 2-2 2"/>"##
);
const SETTINGS: &str = knockout!(
    r##"<circle cx="8" cy="8" r="5"/><path fill="none" stroke="#000" stroke-width="2.4" stroke-linecap="round" d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4"/>"##,
    r##"<circle cx="8" cy="8" r="2.2" fill="#000" stroke="none"/>"##
);
const TOML: &str = knockout!(page!(), r##"<path d="M5 6.5h6M8 6.5v6"/>"##);
const YAML: &str = knockout!(
    page!(),
    r##"<path d="M5 6.5l1.5 2.2L8 6.5M6.5 8.7v3.3M9.5 12l1-5.5 1 5.5"/>"##
);
const MARKDOWN: &str = knockout!(
    r##"<rect x=".5" y="3" width="15" height="10" rx="2"/>"##,
    r##"<path d="M3.3 10.5v-5l2 2.5 2-2.5v5M11.3 5.5v5M9.6 8.7l1.7 1.8 1.7-1.8"/>"##
);
const PYTHON: &str = filled!(
    r##"<path d="M7.9 1C5 1 5.2 2.3 5.2 2.3V4h2.8v.6H4.1S2 4.3 2 7.2 3.8 10 3.8 10h1.1V8.6S4.8 6.8 6.7 6.8h2.8s1.8 0 1.8-1.7V2.8S11.6 1 7.9 1zM6.4 1.9a.5.5 0 1 1 0 1 .5.5 0 0 1 0-1z"/>"##,
    r##"<path d="M8.1 15c2.9 0 2.7-1.3 2.7-1.3V12H8v-.6h3.9S14 11.7 14 8.8 12.2 6 12.2 6h-1.1v1.4s.1 1.8-1.8 1.8H6.5s-1.8 0-1.8 1.7v2.3S4.4 15 8.1 15zm1.5-.9a.5.5 0 1 1 0-1 .5.5 0 0 1 0 1z"/>"##
);
const GO: &str = knockout!(
    r##"<rect x=".5" y="3.5" width="15" height="9" rx="4.5"/>"##,
    r##"<path d="M6 6.3a2 2 0 1 0 .6 2H5.3"/><circle cx="10.4" cy="8" r="1.8"/>"##
);
const HTML: &str = knockout!(
    shield!(),
    r##"<path d="M10.8 4.5H5.3l.3 3h4.9l-.4 3.6L8 11.8l-2.1-.7-.1-1.3"/>"##
);
const CSS: &str = knockout!(
    shield!(),
    r##"<path d="M5.2 4.5h5.6l-.5 6.6L8 11.8l-2.3-.7M5.6 8h4.8"/>"##
);
const SHELL: &str = knockout!(
    r##"<rect x=".5" y="2" width="15" height="12" rx="2"/>"##,
    r##"<path d="M3.5 6l2.5 2-2.5 2M7.5 11h4.5"/>"##
);
const IMAGE: &str = knockout!(
    r##"<rect x="1" y="2" width="14" height="12" rx="2"/>"##,
    r##"<path d="M1 12.5l4-4 3 3 2-2 5 5"/><circle cx="10.8" cy="5.8" r="1"/>"##
);
const LOCK: &str = knockout!(
    r##"<rect x="2.5" y="7" width="11" height="8" rx="1.5"/><path fill="none" stroke="#000" stroke-width="1.8" d="M5 7V5a3 3 0 0 1 6 0v2"/>"##,
    r##"<path d="M8 10v2"/>"##
);
const GIT: &str = knockout!(
    r##"<path d="M8 .8 15.2 8 8 15.2.8 8z"/>"##,
    r##"<path d="M6.2 4.6v6.8M6.2 6.4l3.4 3.4"/>"##
);
const DOCKER: &str = filled!(
    r##"<path d="M.8 8.2h13.6c.4-1 1.3-1.3 1.8-1.2-.2 4.6-3.6 7.2-8.2 7.2C4 14.2 1.2 12 .8 8.2z"/>"##,
    r##"<path d="M2.6 5.6h2v2h-2zM5 5.6h2v2H5zM7.4 5.6h2v2h-2zM5 3.2h2v2H5zM7.4 3.2h2v2h-2z"/>"##
);
const LICENSE: &str = knockout!(
    r##"<circle cx="8" cy="6.2" r="5"/><path d="M5 10l-1.2 5L8 13l4.2 2L11 10z"/>"##,
    r##"<circle cx="8" cy="6.2" r="2"/>"##
);
const README: &str = knockout!(
    r##"<circle cx="8" cy="8" r="7"/>"##,
    r##"<path d="M8 7.3v4.2M8 4.7v.1"/>"##
);
const CRATE: &str = knockout!(
    r##"<path d="M8 .8 14.8 4.2v7.6L8 15.2 1.2 11.8V4.2z"/>"##,
    r##"<path d="M1.2 4.2 8 7.6l6.8-3.4M8 7.6v7.6"/>"##
);
const NPM: &str = knockout!(square!(), r##"<path d="M4.5 11.5v-7h7v7M8 7v4.5"/>"##);
const DATABASE: &str = filled!(
    r##"<ellipse cx="8" cy="3.6" rx="6" ry="2.4"/>"##,
    r##"<path d="M2 5.6C2 6.9 4.7 8 8 8s6-1.1 6-2.4V8c0 1.3-2.7 2.4-6 2.4S2 9.3 2 8zM2 10.2c0 1.3 2.7 2.4 6 2.4s6-1.1 6-2.4v2.2c0 1.3-2.7 2.4-6 2.4s-6-1.1-6-2.4z"/>"##
);
const FONT: &str = knockout!(square!(), r##"<path d="M4.5 12 8 4l3.5 8M5.8 9.5h4.4"/>"##);
const AUDIO: &str = filled!(
    r##"<path d="M6 3.2 14 1.5v9.3a2.2 2.2 0 1 1-1.5-2.1V4.3L7.5 5.4v7.4A2.2 2.2 0 1 1 6 10.7z"/>"##
);
const VIDEO: &str = knockout!(
    r##"<rect x=".5" y="2.5" width="15" height="11" rx="2"/>"##,
    r##"<path d="M6.5 5.5v5l4-2.5z" fill="#000"/>"##
);
const KEY: &str = knockout!(
    r##"<circle cx="5" cy="8" r="4"/><path d="M8 7h7v2h-1.5v2h-2V9H8z"/>"##,
    r##"<circle cx="5" cy="8" r="1.2"/>"##
);
const BUILD: &str = filled!(
    r##"<path d="M10.6 1.2a3.8 3.8 0 0 0-3.7 4.9L1.6 11.4a1.7 1.7 0 0 0 2.4 2.4l5.3-5.3a3.8 3.8 0 0 0 4.9-3.7l-2.3 2.3-2.2-.6-.6-2.2z"/>"##
);

/// (id, SVG, tint).
pub(super) const ICONS: &[(&str, &str, &str)] = &[
    ("file", FILE, "#90a4ae"),
    ("folder", FOLDER, "#90a4ae"),
    ("folder-open", FOLDER_OPEN, "#90a4ae"),
    ("text", TEXT, "#90a4ae"),
    ("code", CODE, "#7986cb"),
    ("rust", RUST, "#ff7043"),
    ("javascript", JS, "#ffca28"),
    ("typescript", TS, "#0288d1"),
    ("react", REACT, "#00bcd4"),
    ("react_ts", REACT, "#0288d1"),
    ("json", JSON, "#fbc02d"),
    ("toml", TOML, "#9e9e9e"),
    ("yaml", YAML, "#ff5252"),
    ("settings", SETTINGS, "#42a5f5"),
    ("markdown", MARKDOWN, "#42a5f5"),
    ("python", PYTHON, "#3d7ab3"),
    ("go", GO, "#00acc1"),
    ("html", HTML, "#e44d26"),
    ("css", CSS, "#42a5f5"),
    ("shell", SHELL, "#ff7043"),
    ("image", IMAGE, "#26a69a"),
    ("lock", LOCK, "#ffd54f"),
    ("git", GIT, "#e64a19"),
    ("docker", DOCKER, "#0288d1"),
    ("license", LICENSE, "#ffca28"),
    ("readme", README, "#42a5f5"),
    ("cargo", CRATE, "#ff7043"),
    ("npm", NPM, "#e53935"),
    ("archive", ARCHIVE, "#afb42b"),
    ("database", DATABASE, "#ffca28"),
    ("font", FONT, "#f44336"),
    ("audio", AUDIO, "#ef5350"),
    ("video", VIDEO, "#ff9800"),
    ("pdf", PDF, "#ef5350"),
    ("env", KEY, "#fbc02d"),
    ("build", BUILD, "#8bc34a"),
];

/// Extensions that differ from the `ferrum` set (the rest are shared).
pub(super) const EXTENSIONS: &[(&str, &str)] = &[("jsx", "react"), ("tsx", "react_ts")];

/// Folder roles: (names, tint). Each gets a closed and an open icon.
pub(super) const FOLDERS: &[(&[&str], &str)] = &[
    (&["src", "source", "sources"], "#4caf50"),
    (
        &["lib", "libs", "crates", "packages", "pkg", "vendor"],
        "#c0ca33",
    ),
    (
        &[
            "test",
            "tests",
            "__tests__",
            "spec",
            "specs",
            "e2e",
            "testdata",
            "fixtures",
        ],
        "#00bfa5",
    ),
    (&["docs", "doc", "documentation"], "#2196f3"),
    (&["node_modules"], "#8bc34a"),
    (&[".github", ".gitlab", ".circleci", "ci"], "#78909c"),
    (&[".vscode", ".idea", ".zed", ".cargo"], "#29b6f6"),
    (
        &[
            "assets",
            "images",
            "img",
            "icons",
            "static",
            "media",
            "resources",
        ],
        "#26a69a",
    ),
    (&["components", "widgets", "ui"], "#c0ca33"),
    (&["config", "configs", "settings", ".config"], "#00bcd4"),
    (
        &[
            "dist", "build", "out", "target", "release", "debug", ".next", "coverage",
        ],
        "#e57373",
    ),
    (&["scripts", "tools", "bin"], "#5c6bc0"),
    (&["public", "www", "web"], "#039be5"),
    (&["api", "server", "backend", "routes"], "#ffb300"),
    (&["styles", "css", "scss", "theme", "themes"], "#ab47bc"),
    (&["examples", "example", "demo", "samples"], "#43a047"),
    (&[".git"], "#f4511e"),
];

/// The folder shapes, for [`FOLDERS`].
pub(super) const FOLDER_SHAPES: (&str, &str) = (FOLDER, FOLDER_OPEN);
