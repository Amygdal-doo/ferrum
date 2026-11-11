//! The built-in `ferrum` icon set: small 16×16 line drawings made for
//! Ferrum Editor (no third-party icon pack). GPUI paints SVGs as a mask,
//! so each icon is one shape tinted with the colour listed here.

/// Wraps path data in a 16×16 stroked SVG.
macro_rules! svg {
    ($($body:expr),* $(,)?) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">"##,
            $($body,)*
            "</svg>"
        )
    };
}

/// A page with a folded corner, the base of document icons.
macro_rules! page {
    () => {
        r#"<path d="M4 1.5h5.5l3 3v10H4z"/><path d="M9.5 1.5v3h3"/>"#
    };
}

const FILE: &str = svg!(page!());
const TEXT: &str = svg!(page!(), r#"<path d="M6 7.5h4.5M6 10h4.5M6 12.5h2.5"/>"#);
const CODE: &str = svg!(
    page!(),
    r#"<path d="M7.3 8 5.8 9.75l1.5 1.75M9.2 8l1.5 1.75-1.5 1.75"/>"#
);
const FOLDER: &str = svg!(r#"<path d="M1.5 3.5h4.5l1.5 1.5h7v8.5h-13z"/>"#);
const FOLDER_OPEN: &str =
    svg!(r#"<path d="M1.5 13.5v-10h4.5l1.5 1.5h6v2.5"/><path d="M1.5 13.5l2.2-6h11l-2.2 6z"/>"#);
const RUST: &str = svg!(
    r#"<circle cx="8" cy="8" r="4.5"/><circle cx="8" cy="8" r="1.6"/>"#,
    r#"<path d="M12.16 9.72L13.82 10.41M9.72 12.16L10.41 13.82M6.28 12.16L5.59 13.82M3.84 9.72L2.18 10.41M3.84 6.28L2.18 5.59M6.28 3.84L5.59 2.18M9.72 3.84L10.41 2.18M12.16 6.28L13.82 5.59"/>"#,
);
const JS: &str = svg!(
    r#"<rect x="1.5" y="1.5" width="13" height="13" rx="2"/>"#,
    r#"<path d="M7.5 7v4a1.5 1.5 0 0 1-3 0M12 7.6c-.5-.7-2.6-.8-2.6.6 0 1.4 2.6.7 2.6 2.2 0 1.4-2.1 1.4-2.8.5"/>"#,
);
const TS: &str = svg!(
    r#"<rect x="1.5" y="1.5" width="13" height="13" rx="2"/>"#,
    r#"<path d="M3.8 7h4M5.8 7v5M12 7.6c-.5-.7-2.6-.8-2.6.6 0 1.4 2.6.7 2.6 2.2 0 1.4-2.1 1.4-2.8.5"/>"#,
);
const JSON: &str = svg!(
    r#"<path d="M6 2.5c-1.5 0-2 .5-2 2v2c0 1-.5 1.5-1.5 1.5 1 0 1.5.5 1.5 1.5v2c0 1.5.5 2 2 2M10 2.5c1.5 0 2 .5 2 2v2c0 1 .5 1.5 1.5 1.5-1 0-1.5.5-1.5 1.5v2c0 1.5-.5 2-2 2"/>"#,
);
const SETTINGS: &str = svg!(
    r#"<path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11"/>"#,
    r##"<circle cx="5" cy="4.5" r="1.4" fill="#000"/><circle cx="10.5" cy="8" r="1.4" fill="#000"/><circle cx="6.5" cy="11.5" r="1.4" fill="#000"/>"##,
);
const MARKDOWN: &str = svg!(
    r#"<rect x="1" y="3.5" width="14" height="9" rx="1.5"/>"#,
    r#"<path d="M3.8 10V6l2 2.5 2-2.5v4M11.3 6v4M9.8 8.5l1.5 1.5 1.5-1.5"/>"#,
);
const PYTHON: &str = svg!(
    r#"<path d="M8.5 7.5h-3c-1 0-1.5.5-1.5 1.5v1.5H3c-1 0-1.5-1-1.5-2.5S2 5.5 3 5.5h4.5V5H5V3c0-1 1-1.5 3-1.5S11 2 11 3v3c0 1-.5 1.5-1.5 1.5"/>"#,
    r#"<path d="M7.5 8.5h3c1 0 1.5-.5 1.5-1.5V5.5h1c1 0 1.5 1 1.5 2.5s-.5 2.5-1.5 2.5H8.5v.5H11v2c0 1-1 1.5-3 1.5s-3-.5-3-1.5v-3c0-1 .5-1.5 1.5-1.5"/>"#,
);
const GO: &str = svg!(
    r#"<path d="M1.5 6h3.5M1 8.5h3.5M2 11h2.5"/><circle cx="10" cy="8.5" r="3.8"/><circle cx="10" cy="8.5" r=".6"/>"#,
);
const HTML: &str =
    svg!(r#"<path d="M5 4.5 1.5 8 5 11.5M11 4.5 14.5 8 11 11.5M9.2 3.5 6.8 12.5"/>"#);
const CSS: &str = svg!(r#"<path d="M6.5 2.5 5.3 13.5M11 2.5l-1.2 11M2.5 6h11M2 10h11"/>"#);
const SHELL: &str = svg!(
    r#"<rect x="1.5" y="2.5" width="13" height="11" rx="1.5"/>"#,
    r#"<path d="M4 6.5l2 1.5-2 1.5M7.5 10.5h4.5"/>"#,
);
const IMAGE: &str = svg!(
    r#"<rect x="1.5" y="2.5" width="13" height="11" rx="1.5"/>"#,
    r#"<path d="M1.5 11.5l4-4 3 3 2-2 4 4"/><circle cx="10.5" cy="6" r="1.2"/>"#,
);
const LOCK: &str = svg!(
    r#"<rect x="3" y="7" width="10" height="7.5" rx="1.5"/>"#,
    r#"<path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2M8 10v1.5"/>"#,
);
const GIT: &str = svg!(
    r#"<circle cx="4.5" cy="3.5" r="1.6"/><circle cx="4.5" cy="12.5" r="1.6"/><circle cx="11.5" cy="5.5" r="1.6"/>"#,
    r#"<path d="M4.5 5.1v5.8M11.5 7.1c0 2.6-3.5 2.6-5.8 4.2"/>"#,
);
const DOCKER: &str = svg!(
    r#"<path d="M1.5 8.5h12.5c0 3-2.5 5-6.5 5-4 0-6-2-6-5z"/>"#,
    r#"<path d="M3.5 6h2.2v2.5H3.5zM5.7 6h2.2v2.5H5.7zM7.9 6h2.2v2.5H7.9zM5.7 3.5h2.2V6H5.7z"/>"#,
);
const LICENSE: &str = svg!(
    r#"<circle cx="8" cy="6.5" r="4"/><circle cx="8" cy="6.5" r="1.5"/>"#,
    r#"<path d="M5.6 9.7 4.7 14.5 8 12.8l3.3 1.7-.9-4.8"/>"#,
);
const README: &str = svg!(
    r#"<path d="M8 4C6.5 2.8 4.5 2.5 1.5 2.5v10c3 0 5 .3 6.5 1.5 1.5-1.2 3.5-1.5 6.5-1.5v-10c-3 0-5 .3-6.5 1.5zM8 4v10"/>"#,
);
const CRATE: &str = svg!(r#"<path d="M8 1.5 14 4.5v7L8 14.5 2 11.5v-7zM2 4.5l6 3 6-3M8 7.5v7"/>"#);
const PACKAGE: &str = svg!(r#"<path d="M2.5 5.5h11v8h-11zM1.5 2.5h13v3h-13zM6.5 8h3"/>"#);
const ARCHIVE: &str = svg!(page!(), r#"<path d="M8 3v1M8 5v1M7 7.5h2V10H7z"/>"#);
const DATABASE: &str = svg!(
    r#"<ellipse cx="8" cy="4" rx="5.5" ry="2"/>"#,
    r#"<path d="M2.5 4v8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2V4M2.5 8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2"/>"#,
);
const FONT: &str = svg!(r#"<path d="M3 13.5 8 2.5l5 11M5 9.5h6"/>"#);
const AUDIO: &str = svg!(
    r#"<path d="M6 12V3.5l7-1.5v8.5"/><circle cx="4.5" cy="12" r="1.5"/><circle cx="11.5" cy="10.5" r="1.5"/>"#,
);
const VIDEO: &str =
    svg!(r#"<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><path d="M6.5 5.5v5l4-2.5z"/>"#,);
const KEY: &str = svg!(r#"<circle cx="5" cy="8" r="3"/><path d="M8 8h6.5M12 8v2.5M14.2 8v2"/>"#);
const BUILD: &str = svg!(
    r#"<path d="M10.5 2a3 3 0 0 0-2.8 4L2.5 11.2a1.25 1.25 0 0 0 1.8 1.8L9.5 7.8a3 3 0 0 0 4-2.8l-1.8 1.8-1.8-.5-.5-1.8z"/>"#,
);

/// (id, SVG, tint). `None` tints with the theme's muted text colour.
pub(super) const ICONS: &[(&str, &str, Option<&str>)] = &[
    ("file", FILE, None),
    ("folder", FOLDER, None),
    ("folder-open", FOLDER_OPEN, None),
    ("text", TEXT, Some("#9aa2b1")),
    ("code", CODE, Some("#8a9ba8")),
    ("rust", RUST, Some("#dea584")),
    ("javascript", JS, Some("#e8d44d")),
    ("typescript", TS, Some("#3d8fd1")),
    ("json", JSON, Some("#cbcb41")),
    ("toml", SETTINGS, Some("#c9845e")),
    ("yaml", SETTINGS, Some("#e06c75")),
    ("settings", SETTINGS, Some("#8a9ba8")),
    ("markdown", MARKDOWN, Some("#519aba")),
    ("python", PYTHON, Some("#4b8bbe")),
    ("go", GO, Some("#00add8")),
    ("html", HTML, Some("#e8663d")),
    ("css", CSS, Some("#8d6fd8")),
    ("shell", SHELL, Some("#89e051")),
    ("image", IMAGE, Some("#a074c4")),
    ("lock", LOCK, Some("#8a8f98")),
    ("git", GIT, Some("#f05133")),
    ("docker", DOCKER, Some("#2496ed")),
    ("license", LICENSE, Some("#d4af37")),
    ("readme", README, Some("#42a5f5")),
    ("cargo", CRATE, Some("#dea584")),
    ("npm", PACKAGE, Some("#cb3837")),
    ("archive", ARCHIVE, Some("#b58b5a")),
    ("database", DATABASE, Some("#e8a838")),
    ("font", FONT, Some("#c678dd")),
    ("audio", AUDIO, Some("#e57373")),
    ("video", VIDEO, Some("#ff7043")),
    ("pdf", TEXT, Some("#e53935")),
    ("env", KEY, Some("#ecd53f")),
    ("build", BUILD, Some("#6d8086")),
];

/// Exact file names (lower case) → icon id.
pub(super) const FILE_NAMES: &[(&str, &str)] = &[
    ("cargo.toml", "cargo"),
    ("cargo.lock", "lock"),
    ("package.json", "npm"),
    ("package-lock.json", "lock"),
    ("yarn.lock", "lock"),
    ("pnpm-lock.yaml", "lock"),
    ("bun.lockb", "lock"),
    ("poetry.lock", "lock"),
    ("gemfile.lock", "lock"),
    ("composer.lock", "lock"),
    ("dockerfile", "docker"),
    ("containerfile", "docker"),
    ("docker-compose.yml", "docker"),
    ("docker-compose.yaml", "docker"),
    ("compose.yaml", "docker"),
    ("compose.yml", "docker"),
    (".dockerignore", "docker"),
    (".gitignore", "git"),
    (".gitattributes", "git"),
    (".gitmodules", "git"),
    (".gitkeep", "git"),
    ("readme", "readme"),
    ("readme.md", "readme"),
    ("readme.txt", "readme"),
    ("readme.rst", "readme"),
    ("license", "license"),
    ("license.md", "license"),
    ("license.txt", "license"),
    ("licence", "license"),
    ("copying", "license"),
    ("tsconfig.json", "typescript"),
    ("jsconfig.json", "javascript"),
    (".env", "env"),
    (".env.local", "env"),
    (".env.example", "env"),
    (".env.development", "env"),
    (".env.production", "env"),
    ("makefile", "build"),
    ("justfile", "build"),
    ("cmakelists.txt", "build"),
    ("build.rs", "build"),
    (".editorconfig", "settings"),
    (".prettierrc", "settings"),
    (".eslintrc", "settings"),
    ("rust-toolchain", "rust"),
    ("rust-toolchain.toml", "rust"),
];

/// Extensions (lower case, without the dot) → icon id.
pub(super) const EXTENSIONS: &[(&str, &str)] = &[
    ("rs", "rust"),
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("jsx", "javascript"),
    ("ts", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("tsx", "typescript"),
    ("d.ts", "typescript"),
    ("json", "json"),
    ("jsonc", "json"),
    ("json5", "json"),
    ("toml", "toml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("ini", "settings"),
    ("cfg", "settings"),
    ("conf", "settings"),
    ("md", "markdown"),
    ("markdown", "markdown"),
    ("mdx", "markdown"),
    ("py", "python"),
    ("pyi", "python"),
    ("go", "go"),
    ("html", "html"),
    ("htm", "html"),
    ("xml", "html"),
    ("vue", "html"),
    ("svelte", "html"),
    ("css", "css"),
    ("scss", "css"),
    ("sass", "css"),
    ("less", "css"),
    ("sh", "shell"),
    ("bash", "shell"),
    ("zsh", "shell"),
    ("fish", "shell"),
    ("ps1", "shell"),
    ("png", "image"),
    ("jpg", "image"),
    ("jpeg", "image"),
    ("gif", "image"),
    ("webp", "image"),
    ("bmp", "image"),
    ("ico", "image"),
    ("icns", "image"),
    ("svg", "image"),
    ("avif", "image"),
    ("lock", "lock"),
    ("zip", "archive"),
    ("tar", "archive"),
    ("gz", "archive"),
    ("tgz", "archive"),
    ("xz", "archive"),
    ("bz2", "archive"),
    ("7z", "archive"),
    ("rar", "archive"),
    ("sql", "database"),
    ("db", "database"),
    ("sqlite", "database"),
    ("sqlite3", "database"),
    ("ttf", "font"),
    ("otf", "font"),
    ("woff", "font"),
    ("woff2", "font"),
    ("mp3", "audio"),
    ("wav", "audio"),
    ("flac", "audio"),
    ("ogg", "audio"),
    ("mp4", "video"),
    ("mov", "video"),
    ("mkv", "video"),
    ("webm", "video"),
    ("pdf", "pdf"),
    ("txt", "text"),
    ("log", "text"),
    ("csv", "text"),
    ("env", "env"),
    ("pem", "env"),
    ("key", "env"),
    ("c", "code"),
    ("h", "code"),
    ("cc", "code"),
    ("cpp", "code"),
    ("hpp", "code"),
    ("java", "code"),
    ("kt", "code"),
    ("swift", "code"),
    ("rb", "code"),
    ("php", "code"),
    ("lua", "code"),
    ("dart", "code"),
    ("zig", "code"),
    ("cs", "code"),
    ("ex", "code"),
    ("exs", "code"),
    ("hs", "code"),
    ("scala", "code"),
    ("wasm", "code"),
];

/// Language ids → icon id (files the tables above miss, e.g. `.bashrc`).
pub(super) const LANGUAGES: &[(&str, &str)] = &[
    ("rust", "rust"),
    ("javascript", "javascript"),
    ("typescript", "typescript"),
    ("tsx", "typescript"),
    ("json", "json"),
    ("toml", "toml"),
    ("yaml", "yaml"),
    ("markdown", "markdown"),
    ("python", "python"),
    ("go", "go"),
    ("html", "html"),
    ("css", "css"),
    ("shell", "shell"),
];

/// Folder names (lower case) → icon id when closed / open.
pub(super) const FOLDER_NAMES: &[(&str, &str, &str)] = &[(".git", "git", "git")];
