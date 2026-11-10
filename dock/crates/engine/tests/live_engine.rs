//! Read-only checks against the engine on this machine. Never creates,
//! stops or removes anything. Run with:
//! `cargo test -p ferrumdock-engine --test live_engine -- --ignored --nocapture`

use ferrumdock_engine::{LogOptions, compose, connect, format, sample_stats};
use futures::StreamExt;

#[tokio::test]
#[ignore = "needs a running Docker-compatible engine"]
async fn reads_the_local_engine() {
    let connection = match connect().await {
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
