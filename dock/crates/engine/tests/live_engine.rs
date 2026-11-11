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
