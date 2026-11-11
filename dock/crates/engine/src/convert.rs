//! bollard (OpenAPI) models → FerrumContainers models. Missing fields become
//! empty values instead of errors: engines differ in what they fill in.

use crate::model::{
    Container, ContainerDetail, ContainerState, EngineInfo, Image, ImageLayer, Labels, Mount,
    Network, NetworkAttachment, PortMapping, PullEvent, Stats, Volume,
};
use crate::stats::{CpuSample, block_io, cpu_percent, memory_usage};
use bollard::models as m;

/// Negative sizes (`-1` = unknown) become 0.
#[must_use]
pub fn size(value: Option<i64>) -> u64 {
    value.and_then(|v| u64::try_from(v).ok()).unwrap_or(0)
}

fn labels(map: Option<std::collections::HashMap<String, String>>) -> Labels {
    map.unwrap_or_default().into_iter().collect()
}

#[must_use]
pub fn engine_info(v: m::SystemVersion) -> EngineInfo {
    EngineInfo {
        version: v.version.unwrap_or_default(),
        api_version: v.api_version.unwrap_or_default(),
        os: v.os.unwrap_or_default(),
        arch: v.arch.unwrap_or_default(),
        platform: v.platform.map(|p| p.name).filter(|n| !n.is_empty()),
    }
}

#[must_use]
pub fn container(c: m::ContainerSummary) -> Container {
    let name = c
        .names
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|n| n.trim_start_matches('/').to_owned())
        .unwrap_or_default();
    let state = c.state.map_or(ContainerState::Unknown, |s| {
        ContainerState::parse(s.as_ref())
    });
    let ports = c
        .ports
        .unwrap_or_default()
        .into_iter()
        .map(|p| PortMapping {
            ip: p.ip.filter(|ip| !ip.is_empty()),
            private: p.private_port,
            public: p.public_port,
            protocol: p
                .typ
                .map(|t| t.to_string())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "tcp".to_owned()),
        })
        .collect();
    let id = c.id.unwrap_or_default();
    Container {
        name: if name.is_empty() {
            crate::format::short_id(&id).to_owned()
        } else {
            name
        },
        id,
        image: c.image.unwrap_or_default(),
        state,
        status: c.status.unwrap_or_default(),
        ports,
        labels: labels(c.labels),
        created: c.created.unwrap_or(0),
    }
}

fn cpu_sample(s: Option<&m::ContainerCpuStats>) -> (CpuSample, u32) {
    let Some(s) = s else {
        return (CpuSample::default(), 0);
    };
    let usage = s.cpu_usage.as_ref();
    let online = s.online_cpus.unwrap_or_else(|| {
        usage
            .and_then(|u| u.percpu_usage.as_ref())
            .map_or(1, |p| u32::try_from(p.len()).unwrap_or(1))
    });
    (
        CpuSample {
            total_usage: usage.and_then(|u| u.total_usage).unwrap_or(0),
            system_usage: s.system_cpu_usage.unwrap_or(0),
        },
        online,
    )
}

#[must_use]
pub fn stats(s: &m::ContainerStatsResponse) -> Stats {
    let (current, online) = cpu_sample(s.cpu_stats.as_ref());
    let (previous, _) = cpu_sample(s.precpu_stats.as_ref());
    let memory = s.memory_stats.as_ref();
    let (net_rx, net_tx) =
        s.networks
            .iter()
            .flat_map(|n| n.values())
            .fold((0u64, 0u64), |(rx, tx), n| {
                (
                    rx.saturating_add(n.rx_bytes.unwrap_or(0)),
                    tx.saturating_add(n.tx_bytes.unwrap_or(0)),
                )
            });
    let (blk_read, blk_write) = block_io(
        s.blkio_stats
            .as_ref()
            .and_then(|b| b.io_service_bytes_recursive.as_ref())
            .into_iter()
            .flatten()
            .filter_map(|e| Some((e.op.as_deref()?, e.value?))),
    );
    Stats {
        cpu_percent: cpu_percent(previous, current, online),
        mem_usage: memory.map_or(0, |m| memory_usage(m.usage.unwrap_or(0), m.stats.as_ref())),
        mem_limit: memory.and_then(|m| m.limit).unwrap_or(0),
        net_rx,
        net_tx,
        blk_read,
        blk_write,
    }
}

#[must_use]
pub fn detail(d: m::ContainerInspectResponse) -> ContainerDetail {
    let config = d.config.unwrap_or_default();
    let state = d.state.unwrap_or_default();
    let env = config
        .env
        .unwrap_or_default()
        .into_iter()
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (k.to_owned(), v.to_owned()),
            None => (kv, String::new()),
        })
        .collect();
    let mounts = d
        .mounts
        .unwrap_or_default()
        .into_iter()
        .map(|mp| Mount {
            kind: mp.typ.unwrap_or_default(),
            source: mp
                .name
                .filter(|n| !n.is_empty())
                .or(mp.source)
                .unwrap_or_default(),
            destination: mp.destination.unwrap_or_default(),
            read_write: mp.rw.unwrap_or(true),
        })
        .collect();
    let mut networks: Vec<NetworkAttachment> = d
        .network_settings
        .and_then(|n| n.networks)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, e)| NetworkAttachment {
            network: name,
            ip_address: e.ip_address.unwrap_or_default(),
            gateway: e.gateway.unwrap_or_default(),
            aliases: e.aliases.unwrap_or_default(),
        })
        .collect();
    networks.sort_by(|a, b| a.network.cmp(&b.network));
    let command = config
        .entrypoint
        .unwrap_or_default()
        .into_iter()
        .chain(config.cmd.unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ");
    ContainerDetail {
        id: d.id.unwrap_or_default(),
        name: d
            .name
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_owned(),
        image: config.image.unwrap_or_default(),
        state: state.status.map_or(ContainerState::Unknown, |s| {
            ContainerState::parse(s.as_ref())
        }),
        created: d.created.map(|c| c.to_string()).unwrap_or_default(),
        started_at: state.started_at.filter(|s| !s.starts_with("0001-")),
        command,
        working_dir: config.working_dir.filter(|w| !w.is_empty()),
        tty: config.tty.unwrap_or(false),
        restart_count: d.restart_count.unwrap_or(0),
        env,
        mounts,
        networks,
        labels: labels(config.labels),
    }
}

#[must_use]
pub fn image(i: m::ImageSummary) -> Image {
    Image {
        id: i.id,
        tags: i
            .repo_tags
            .into_iter()
            .filter(|t| t != "<none>:<none>")
            .collect(),
        size: size(Some(i.size)),
        created: i.created,
        containers: i.containers,
    }
}

#[must_use]
pub fn volume(v: m::Volume) -> Volume {
    Volume {
        name: v.name,
        driver: v.driver,
        mountpoint: v.mountpoint,
        created: v.created_at.map(|c| c.to_string()),
        labels: v.labels.into_iter().collect(),
        size: v
            .usage_data
            .as_ref()
            .and_then(|u| u64::try_from(u.size).ok()),
        ref_count: v.usage_data.map(|u| u.ref_count).filter(|n| *n >= 0),
    }
}

/// Fills size / ref count from a `/system/df?type=volume` body: newer engines
/// nest items under `VolumeUsage.Items`, older ones list them in `Volumes`.
pub fn merge_volume_usage(volumes: &mut [Volume], df: &serde_json::Value) {
    let items = df
        .pointer("/VolumeUsage/Items")
        .or_else(|| df.get("Volumes"))
        .and_then(serde_json::Value::as_array);
    for item in items.into_iter().flatten() {
        let Some(name) = item.get("Name").and_then(|n| n.as_str()) else {
            continue;
        };
        let Some(volume) = volumes.iter_mut().find(|v| v.name == name) else {
            continue;
        };
        let data = item.get("UsageData");
        let field = |key: &str| {
            data.and_then(|d| d.get(key))
                .and_then(serde_json::Value::as_i64)
        };
        if let Some(size) = field("Size").and_then(|s| u64::try_from(s).ok()) {
            volume.size = Some(size);
        }
        if let Some(count) = field("RefCount").filter(|n| *n >= 0) {
            volume.ref_count = Some(count);
        }
    }
}

#[must_use]
pub fn network(n: m::Network) -> Network {
    Network {
        id: n.id.unwrap_or_default(),
        name: n.name.unwrap_or_default(),
        driver: n.driver.unwrap_or_default(),
        scope: n.scope.unwrap_or_default(),
        internal: n.internal.unwrap_or(false),
        created: n.created.map(|c| c.to_string()),
        labels: labels(n.labels),
    }
}

#[must_use]
pub fn image_layer(h: m::ImageHistoryResponseItem) -> ImageLayer {
    ImageLayer {
        id: h.id,
        created: h.created,
        created_by: h.created_by,
        size: size(Some(h.size)),
        tags: h.tags,
        comment: h.comment,
    }
}

#[must_use]
pub fn pull_event(info: m::CreateImageInfo) -> PullEvent {
    let detail = info.progress_detail.unwrap_or_default();
    PullEvent {
        id: info.id,
        status: info.status.unwrap_or_default(),
        current: detail.current.and_then(|c| u64::try_from(c).ok()),
        total: detail.total.and_then(|t| u64::try_from(t).ok()),
        error: info.error_detail.and_then(|e| e.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_container_summary_from_json() {
        let json = serde_json::json!({
            "Id": "0123456789abcdef",
            "Names": ["/web-db-1"],
            "Image": "postgres:16",
            "State": "running",
            "Status": "Up 2 hours",
            "Created": 1_700_000_000,
            "Ports": [
                {"IP": "0.0.0.0", "PrivatePort": 5432, "PublicPort": 5432, "Type": "tcp"},
                {"PrivatePort": 9187, "Type": "tcp"}
            ],
            "Labels": {"com.docker.compose.project": "web"}
        });
        let summary: m::ContainerSummary =
            serde_json::from_value(json).unwrap_or_else(|e| panic!("valid summary: {e}"));
        let c = container(summary);
        assert_eq!(c.name, "web-db-1");
        assert_eq!(c.state, ContainerState::Running);
        assert_eq!(c.project(), Some("web"));
        assert_eq!(c.ports.len(), 2);
        assert_eq!(c.ports[1].public, None);
        assert_eq!(c.ports[0].ip.as_deref(), Some("0.0.0.0"));
    }

    #[test]
    fn converts_stats_and_volume_usage() {
        let json = serde_json::json!({
            "cpu_stats": {"cpu_usage": {"total_usage": 6000}, "system_cpu_usage": 200_000, "online_cpus": 2},
            "precpu_stats": {"cpu_usage": {"total_usage": 1000}, "system_cpu_usage": 100_000},
            "memory_stats": {"usage": 1000, "limit": 4000, "stats": {"inactive_file": 200}},
            "networks": {"eth0": {"rx_bytes": 10, "tx_bytes": 20}, "eth1": {"rx_bytes": 1}},
            "blkio_stats": {"io_service_bytes_recursive": [
                {"major": 8, "minor": 0, "op": "read", "value": 4096},
                {"major": 8, "minor": 0, "op": "write", "value": 512}
            ]}
        });
        let response: m::ContainerStatsResponse =
            serde_json::from_value(json).unwrap_or_else(|e| panic!("valid stats: {e}"));
        let s = stats(&response);
        assert!((s.cpu_percent - 10.0).abs() < 1e-9);
        assert_eq!((s.mem_usage, s.mem_limit), (800, 4000));
        assert_eq!((s.net_rx, s.net_tx), (11, 20));
        assert_eq!((s.blk_read, s.blk_write), (4096, 512));

        let mut volumes = vec![Volume {
            name: "data".into(),
            driver: "local".into(),
            mountpoint: String::new(),
            created: None,
            labels: Labels::new(),
            size: None,
            ref_count: None,
        }];
        let new_layout = serde_json::json!({"VolumeUsage": {"Items": [
            {"Name": "data", "UsageData": {"Size": 2048, "RefCount": 1}},
            {"Name": "other", "UsageData": {"Size": 1, "RefCount": 0}}
        ]}});
        merge_volume_usage(&mut volumes, &new_layout);
        assert_eq!(volumes[0].size, Some(2048));
        assert_eq!(volumes[0].ref_count, Some(1));
        let old_layout = serde_json::json!({"Volumes": [
            {"Name": "data", "UsageData": {"Size": -1, "RefCount": 2}}
        ]});
        merge_volume_usage(&mut volumes, &old_layout);
        assert_eq!(
            (volumes[0].size, volumes[0].ref_count),
            (Some(2048), Some(2))
        );
        merge_volume_usage(&mut volumes, &serde_json::json!({"message": "error"}));
        assert_eq!(size(Some(-1)), 0);
    }
}
