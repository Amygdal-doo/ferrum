//! Read-only checks against the engine on this machine. Never creates,
//! stops or removes anything. Run with:
//! `cargo test -p ferrumdock-engine --test live_engine -- --ignored --nocapture`

use ferrumdock_engine::{LogOptions, compose, connect, context, endpoint, format, sample_stats};
use futures::StreamExt;

#[tokio::test]
#[ignore = "needs a running Docker-compatible engine"]
async fn reads_the_local_engine() {
    for c in context::detect() {
        println!(
            "context {}{}: {} ({})",
            c.name,
            if c.current { " (current)" } else { "" },
            c.host,
            if c.endpoint.is_some() {
                "usable"
            } else {
                "not supported"
            }
        );
    }
    for c in endpoint::detect() {
        println!("candidate: {} at {}", c.label(), c.endpoint);
    }
    let connection = match connect(None).await {
        Ok(c) => c,
        Err(e) => panic!("no engine reachable: {e}"),
    };
    let engine = connection.engine.clone();
    println!(
        "engine: {} via {} ({}) — version {} API {} {}/{}",
        connection.info.platform.as_deref().unwrap_or("?"),
        connection.candidate.source,
        connection.candidate.endpoint,
        connection.info.version,
        connection.info.api_version,
        connection.info.os,
        connection.info.arch
    );
    assert!(!connection.info.version.is_empty());

    let containers = engine
        .containers()
        .await
        .unwrap_or_else(|e| panic!("containers: {e}"));
    let running: Vec<String> = containers
        .iter()
        .filter(|c| c.state.is_up())
        .map(|c| c.id.clone())
        .collect();
    let stats = sample_stats(&engine, &running).await;
    for group in compose::group_by_project(containers.clone()) {
        println!(
            "[{}] {}/{} running",
            group.project.as_deref().unwrap_or("standalone"),
            group.running(),
            group.containers.len()
        );
        for c in &group.containers {
            let s = stats.get(&c.id);
            println!(
                "  {} {} {} | {} | cpu {} mem {}",
                c.name,
                c.image,
                c.state.label(),
                format::ports(&c.ports),
                s.map_or_else(|| "-".into(), |s| format::cpu(s.cpu_percent)),
                s.map_or_else(|| "-".into(), |s| format::memory(s.mem_usage, s.mem_limit)),
            );
            if let Some(s) = s {
                println!(
                    "    net {} in / {} out, disk {} read / {} written",
                    format::bytes(s.net_rx),
                    format::bytes(s.net_tx),
                    format::bytes(s.blk_read),
                    format::bytes(s.blk_write)
                );
            }
        }
    }
    // `docker compose ps` (read-only) for the first compose project.
    let cli = ferrumdock_engine::DockerCli::detect();
    let first = compose::group_by_project(containers.clone())
        .into_iter()
        .find_map(|g| g.compose_project());
    if let (Some(cli), Some(project)) = (cli, first) {
        let services =
            ferrumdock_engine::compose_ps::run_ps(&cli, &connection.candidate.endpoint, &project)
                .await
                .unwrap_or_else(|e| panic!("compose ps: {e}"));
        println!("compose ps {}: {} services", project.name, services.len());
        for s in &services {
            println!("  {} {} {} {:?}", s.service, s.name, s.status, s.ports);
        }
        assert!(!services.is_empty());
        // The project folder's compose files (read from disk) and container matching.
        if let Some(dir) = project.working_dir.as_deref().map(std::path::Path::new) {
            let files = ferrumdock_engine::project::load_compose_files(dir);
            let names: Vec<String> = files.iter().map(|f| f.project.clone()).collect();
            let members = containers
                .iter()
                .filter(|c| ferrumdock_engine::project::belongs_to(c, dir, &names))
                .count();
            println!(
                "project folder {}: {} compose file(s) {:?}, {members} containers",
                dir.display(),
                files.len(),
                files.iter().map(|f| &f.services).collect::<Vec<_>>()
            );
            assert!(members >= services.len());
        }
    }
    if let Some(c) = containers.first() {
        let detail = engine
            .inspect(&c.id)
            .await
            .unwrap_or_else(|e| panic!("inspect: {e}"));
        println!(
            "inspect {}: {} env, {} mounts, {} networks",
            detail.name,
            detail.env.len(),
            detail.mounts.len(),
            detail.networks.len()
        );
        let options = LogOptions {
            follow: false,
            tail: Some(3),
            timestamps: true,
            since: None,
        };
        let chunks: Vec<_> = engine.logs(&c.id, options).collect().await;
        println!("logs: {} chunks", chunks.len());
    }

    let images = engine
        .images()
        .await
        .unwrap_or_else(|e| panic!("images: {e}"));
    let total: u64 = images.iter().map(|i| i.size).sum();
    println!(
        "{} images, {} total, {} dangling",
        images.len(),
        format::bytes(total),
        images.iter().filter(|i| i.is_dangling()).count()
    );
    if let Some(image) = images.iter().find(|i| !i.is_dangling()) {
        let layers = engine
            .image_history(&image.id)
            .await
            .unwrap_or_else(|e| panic!("image history: {e}"));
        println!("history of {}: {} layers", image.tags[0], layers.len());
        assert!(!layers.is_empty());
    }
    let volumes = engine
        .volumes()
        .await
        .unwrap_or_else(|e| panic!("volumes: {e}"));
    println!(
        "{} volumes ({} with size)",
        volumes.len(),
        volumes.iter().filter(|v| v.size.is_some()).count()
    );
    let networks = engine
        .networks()
        .await
        .unwrap_or_else(|e| panic!("networks: {e}"));
    println!("{} networks", networks.len());
    assert!(networks.iter().any(|n| n.is_builtin()));
}

/// Browses the first running container's file system (read only: one exec
/// of the listing script, archive downloads), and checks that `/auth`
/// refuses made-up credentials.
#[tokio::test]
#[ignore = "needs a running Docker-compatible engine with a running container"]
async fn browses_a_container_and_checks_auth() {
    use ferrumdock_engine::files;
    use ferrumdock_engine::registry::{RegistryAuth, server_key};
    let connection = connect(None)
        .await
        .unwrap_or_else(|e| panic!("no engine reachable: {e}"));
    let engine = connection.engine.clone();
    let containers = engine
        .containers()
        .await
        .unwrap_or_else(|e| panic!("containers: {e}"));
    let Some(c) = containers.iter().find(|c| c.state.is_up()) else {
        println!("no running container: skipped");
        return;
    };
    let started = std::time::Instant::now();
    let root = files::list_dir(&engine, &c.id, "/")
        .await
        .unwrap_or_else(|e| panic!("list /: {e}"));
    println!(
        "{} ({:.1}s): / has {} entries{}: {}",
        c.name,
        started.elapsed().as_secs_f64(),
        root.entries.len(),
        if root.partial { " (partial)" } else { "" },
        root.entries
            .iter()
            .map(|e| format!("{} {}", e.mode_string(), e.name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(!root.entries.is_empty());
    let etc = files::list_dir(&engine, &c.id, "/etc")
        .await
        .unwrap_or_else(|e| panic!("list /etc: {e}"));
    println!(
        "/etc ({:.1}s): {} entries",
        started.elapsed().as_secs_f64(),
        etc.entries.len()
    );
    let started = std::time::Instant::now();
    let from_archive = files::list_dir_archive(&engine, &c.id, "/etc")
        .await
        .unwrap_or_else(|e| panic!("archive /etc: {e}"));
    println!(
        "/etc from the archive ({:.1}s): {} entries",
        started.elapsed().as_secs_f64(),
        from_archive.entries.len()
    );
    let names =
        |l: &files::DirListing| l.entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&etc), names(&from_archive), "exec and archive agree");
    let hosts = files::read_file(&engine, &c.id, "/etc/hosts")
        .await
        .unwrap_or_else(|e| panic!("read /etc/hosts: {e}"));
    println!(
        "/etc/hosts ({} bytes):\n{}",
        hosts.size,
        hosts.text().unwrap_or("<binary>")
    );
    assert!(hosts.text().is_some_and(|t| t.contains("localhost")));

    let made_up = RegistryAuth {
        server: server_key("docker.io"),
        username: "ferrumdock-live-check".into(),
        secret: "not-a-real-password".into(),
    };
    let started = std::time::Instant::now();
    let refused = engine.check_auth(&made_up).await;
    println!(
        "auth with made-up credentials ({:.1}s): {refused:?}",
        started.elapsed().as_secs_f64()
    );
    assert!(refused.is_err());
}

/// `docker compose config` on a temporary compose file, saved and unsaved
/// (stdin). Compose only reads it; nothing is created.
#[tokio::test]
#[ignore = "needs the docker CLI with the compose plugin"]
async fn validates_compose_files_with_the_cli() {
    use ferrumdock_engine::compose::ComposeProject;
    use ferrumdock_engine::compose_run::validate;
    let cli = ferrumdock_engine::DockerCli::detect().unwrap_or_else(|| panic!("no docker CLI"));
    let connection = connect(None)
        .await
        .unwrap_or_else(|e| panic!("no engine reachable: {e}"));
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let file = dir.path().join("compose.yaml");
    std::fs::write(&file, "services:\n  web:\n    image: nginx:alpine\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let project = ComposeProject {
        name: "ferrumdock-live-check".into(),
        working_dir: Some(dir.path().display().to_string()),
        config_files: vec![file.display().to_string()],
    };
    let endpoint = &connection.candidate.endpoint;
    let saved = validate(&cli, endpoint, &project, Some(&file), None).await;
    println!("saved: {saved:?}");
    assert!(saved.is_ok_and(|text| text.contains("nginx:alpine")));
    let broken = "services:\n  web:\n    image: nginx\n    ports: \"not a list\n";
    let unsaved = validate(&cli, endpoint, &project, Some(&file), Some(broken)).await;
    println!("unsaved, broken: {unsaved:?}");
    assert!(unsaved.is_err());
    let undefined = "services:\n  web:\n    image: nginx\n    volumes: [data:/d]\n";
    let unsaved = validate(&cli, endpoint, &project, Some(&file), Some(undefined)).await;
    println!("unsaved, undefined volume: {unsaved:?}");
    assert!(unsaved.is_err_and(|e| e.to_string().contains("data")));
}
