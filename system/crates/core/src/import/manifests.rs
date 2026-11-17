//! Package manifests: what a folder is built with (`package.json`,
//! `Cargo.toml`, `pyproject.toml` / `requirements.txt`, `go.mod`,
//! `Gemfile`, `pom.xml`) and which datastores its dependencies talk to.

use std::path::Path;

use super::{Builder, node};

const MANIFESTS: [&str; 7] = [
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "Gemfile",
    "pom.xml",
];

/// Dependency → (kind, technology) of the app, first match wins.
const APPS: &[(&str, &str, &str)] = &[
    ("next", "web-client", "nextjs"),
    ("nuxt", "web-client", "nuxt"),
    ("@angular/core", "web-client", "angular"),
    ("@sveltejs/kit", "web-client", "svelte"),
    ("svelte", "web-client", "svelte"),
    ("vue", "web-client", "vue"),
    ("react-native", "mobile-client", "react-native"),
    ("react", "web-client", "react"),
    ("@nestjs/core", "service", "nestjs"),
    ("express", "service", "nodejs"),
    ("fastify", "service", "nodejs"),
    ("hono", "service", "nodejs"),
    ("koa", "service", "nodejs"),
    ("tauri", "desktop-client", "tauri"),
    ("axum", "service", "rust"),
    ("actix-web", "service", "rust"),
    ("rocket", "service", "rust"),
    ("warp", "service", "rust"),
    ("poem", "service", "rust"),
    ("django", "service", "django"),
    ("fastapi", "service", "python"),
    ("flask", "service", "python"),
    ("rails", "service", "ruby"),
];

/// Dependency → (component id, kind, technology) of a datastore it uses.
const STORES: &[(&str, &str, &str, &str)] = &[
    ("pg", "postgres", "sql-db", "postgresql"),
    ("postgres", "postgres", "sql-db", "postgresql"),
    ("tokio-postgres", "postgres", "sql-db", "postgresql"),
    ("psycopg", "postgres", "sql-db", "postgresql"),
    ("psycopg2", "postgres", "sql-db", "postgresql"),
    ("psycopg2-binary", "postgres", "sql-db", "postgresql"),
    ("asyncpg", "postgres", "sql-db", "postgresql"),
    ("mysql", "mysql", "sql-db", "mysql"),
    ("mysql2", "mysql", "sql-db", "mysql"),
    ("mongodb", "mongodb", "nosql-db", "mongodb"),
    ("mongoose", "mongodb", "nosql-db", "mongodb"),
    ("pymongo", "mongodb", "nosql-db", "mongodb"),
    ("redis", "redis", "cache", "redis"),
    ("ioredis", "redis", "cache", "redis"),
    ("kafkajs", "kafka", "stream", "kafka"),
    ("rdkafka", "kafka", "stream", "kafka"),
    ("kafka-python", "kafka", "stream", "kafka"),
    ("amqplib", "rabbitmq", "queue", "rabbitmq"),
    ("lapin", "rabbitmq", "queue", "rabbitmq"),
    ("pika", "rabbitmq", "queue", "rabbitmq"),
];

/// The names of the dependencies the manifest at `path` lists, `None` when
/// it is not one (or is only a workspace root).
fn dependencies(path: &Path) -> Option<Vec<String>> {
    let file = path.file_name()?.to_str()?;
    let text = std::fs::read_to_string(path).ok()?;
    let mut deps: Vec<String> = match file {
        "package.json" => {
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            ["dependencies", "devDependencies"]
                .iter()
                .filter_map(|k| json.get(k)?.as_object())
                .flat_map(|m| m.keys().cloned())
                .collect()
        }
        "Cargo.toml" => {
            let toml: toml::Value = toml::from_str(&text).ok()?;
            toml.get("package")?;
            toml.get("dependencies")
                .and_then(toml::Value::as_table)
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default()
        }
        "pyproject.toml" => {
            let toml: toml::Value = toml::from_str(&text).ok()?;
            let project = toml
                .get("project")
                .and_then(|p| p.get("dependencies"))
                .and_then(toml::Value::as_array)
                .map(|deps| {
                    deps.iter()
                        .filter_map(toml::Value::as_str)
                        .map(requirement)
                        .collect()
                })
                .unwrap_or_default();
            let poetry: Vec<String> = toml
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("dependencies"))
                .and_then(toml::Value::as_table)
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default();
            [project, poetry].concat()
        }
        "requirements.txt" => text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(['#', '-']))
            .map(requirement)
            .collect(),
        "go.mod" | "Gemfile" | "pom.xml" => text
            .split(|c: char| c.is_whitespace() || "\"'<>/,".contains(c))
            .map(str::to_owned)
            .collect(),
        _ => return None,
    };
    deps.iter_mut().for_each(|d| *d = d.to_lowercase());
    Some(deps)
}

/// `Django>=5.0` → `django`.
fn requirement(line: &str) -> String {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
        .next()
        .unwrap_or(line)
        .to_lowercase()
}

/// What the app in `dir` is: (kind, technology), from its manifest.
pub(super) fn tech_of(dir: &Path) -> Option<(&'static str, &'static str)> {
    MANIFESTS.iter().find_map(|m| {
        let path = dir.join(m);
        let deps = dependencies(&path)?;
        app(m, &deps)
    })
}

fn app(manifest: &str, deps: &[String]) -> Option<(&'static str, &'static str)> {
    let found = APPS
        .iter()
        .find(|(dep, _, _)| deps.iter().any(|d| d == dep))
        .map(|(_, kind, tech)| (*kind, *tech));
    found.or(match manifest {
        "package.json" => Some(("service", "nodejs")),
        "Cargo.toml" => Some(("service", "rust")),
        "pyproject.toml" | "requirements.txt" => Some(("service", "python")),
        "go.mod" => Some(("service", "go")),
        "Gemfile" => Some(("service", "ruby")),
        "pom.xml" => Some(("service", "java")),
        _ => None,
    })
}

/// Adds the app the manifest at `path` describes, and the datastores it
/// uses; `false` when it is not a manifest.
pub(super) fn read(path: &Path, b: &mut Builder) -> bool {
    let Some(file) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    if !MANIFESTS.contains(&file) {
        return false;
    }
    let Some(deps) = dependencies(path) else {
        return false;
    };
    let Some((kind, tech)) = app(file, &deps) else {
        return false;
    };
    let name = path
        .parent()
        .and_then(Path::file_name)
        .map_or_else(|| "app".to_owned(), |n| n.to_string_lossy().into_owned());
    let n = node(
        &name,
        kind,
        crate::Tech::find(tech).map(|t| t.id.to_owned()),
    );
    let id = n.id.clone();
    b.add(n);
    for (dep, store, kind, tech) in STORES {
        if deps.iter().any(|d| d == dep) {
            let tech = crate::Tech::find(tech);
            let mut s = node(
                tech.map_or(*store, |t| t.name),
                kind,
                tech.map(|t| t.id.to_owned()),
            );
            s.id = (*store).to_owned();
            b.add(s);
            b.connect(&id, store);
        }
    }
    true
}
