//! Display formatting for sizes, ports, ids, CPU and ages.

use crate::model::PortMapping;

/// First 12 hex digits of an id, without the `sha256:` prefix.
#[must_use]
pub fn short_id(id: &str) -> &str {
    let id = id.strip_prefix("sha256:").unwrap_or(id);
    id.get(..12).unwrap_or(id)
}

/// Binary-unit size, like Docker Desktop: `0 B`, `512 B`, `1.5 KB`, `42.0 MB`, `1.21 GB`.
#[must_use]
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if n < 1024 {
        return format!("{n} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// `12.3%` (one decimal; `docker stats` style, can exceed 100 % on multi-core).
#[must_use]
pub fn cpu(percent: f64) -> String {
    if percent.is_finite() && percent > 0.0 {
        format!("{percent:.1}%")
    } else {
        "0.0%".to_owned()
    }
}

/// `128 MB / 7.7 GB`; just the usage when the limit is unknown.
#[must_use]
pub fn memory(usage: u64, limit: u64) -> String {
    if limit == 0 {
        bytes(usage)
    } else {
        format!("{} / {}", bytes(usage), bytes(limit))
    }
}

/// One port: `0.0.0.0:8080→80/tcp`, `80/tcp` when only exposed.
#[must_use]
pub fn port(p: &PortMapping) -> String {
    match p.public {
        Some(public) => {
            let host = match p.ip.as_deref() {
                None | Some("" | "0.0.0.0" | "::") => String::new(),
                Some(ip) if ip.contains(':') => format!("[{ip}]:"),
                Some(ip) => format!("{ip}:"),
            };
            format!("{host}{public}→{}/{}", p.private, p.protocol)
        }
        None => format!("{}/{}", p.private, p.protocol),
    }
}

/// Published ports first, IPv4/IPv6 duplicates of the same mapping folded.
#[must_use]
pub fn ports(list: &[PortMapping]) -> String {
    let mut seen = Vec::new();
    let mut sorted: Vec<&PortMapping> = list.iter().collect();
    sorted.sort_by_key(|p| (p.public.is_none(), p.public, p.private, p.protocol.clone()));
    for p in sorted {
        let text = port(&PortMapping {
            ip: p.ip.as_deref().filter(|ip| *ip != "::").map(str::to_owned),
            ..p.clone()
        });
        if !seen.contains(&text) {
            seen.push(text);
        }
    }
    seen.join(", ")
}

/// `http://localhost:<port>` for a published TCP port reachable from this machine.
#[must_use]
pub fn browser_url(p: &PortMapping) -> Option<String> {
    let public = p.public?;
    if p.protocol != "tcp" {
        return None;
    }
    let host = match p.ip.as_deref() {
        None | Some("" | "0.0.0.0" | "::" | "127.0.0.1" | "::1") => "localhost".to_owned(),
        Some(ip) if ip.contains(':') => format!("[{ip}]"),
        Some(ip) => ip.to_owned(),
    };
    let scheme = if p.private == 443 || public == 443 {
        "https"
    } else {
        "http"
    };
    Some(format!("{scheme}://{host}:{public}"))
}

/// Distinct browser URLs of a container's published ports, lowest port first.
#[must_use]
pub fn browser_urls(list: &[PortMapping]) -> Vec<String> {
    let mut sorted: Vec<&PortMapping> = list.iter().collect();
    sorted.sort_by_key(|p| p.public);
    let mut urls: Vec<String> = Vec::new();
    for url in sorted.into_iter().filter_map(browser_url) {
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

/// Age of a Unix timestamp relative to `now`: `just now`, `5 minutes ago`, `3 days ago`.
#[must_use]
pub fn age(created: i64, now: i64) -> String {
    let secs = (now - created).max(0);
    let (n, unit) = match secs {
        0..60 => return "just now".to_owned(),
        60..3600 => (secs / 60, "minute"),
        3600..86_400 => (secs / 3600, "hour"),
        86_400..2_592_000 => (secs / 86_400, "day"),
        2_592_000..31_536_000 => (secs / 2_592_000, "month"),
        _ => (secs / 31_536_000, "year"),
    };
    format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(ip: Option<&str>, private: u16, public: Option<u16>, proto: &str) -> PortMapping {
        PortMapping {
            ip: ip.map(str::to_owned),
            private,
            public,
            protocol: proto.to_owned(),
        }
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KB");
        assert_eq!(bytes(1536), "1.50 KB");
        assert_eq!(bytes(42 * 1024 * 1024), "42.0 MB");
        assert_eq!(bytes(187 * 1024 * 1024), "187 MB");
        assert_eq!(bytes(1_300_000_000), "1.21 GB");
        assert_eq!(bytes(u64::MAX), "16384 PB");
    }

    #[test]
    fn shortens_ids() {
        assert_eq!(short_id("sha256:0123456789abcdef0123"), "0123456789ab");
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn formats_ports_and_folds_ipv6_duplicates() {
        let list = vec![
            mapping(None, 443, None, "tcp"),
            mapping(Some("0.0.0.0"), 80, Some(8080), "tcp"),
            mapping(Some("::"), 80, Some(8080), "tcp"),
            mapping(Some("127.0.0.1"), 5432, Some(5432), "tcp"),
            mapping(Some("0.0.0.0"), 53, Some(5353), "udp"),
        ];
        assert_eq!(
            ports(&list),
            "5353→53/udp, 127.0.0.1:5432→5432/tcp, 8080→80/tcp, 443/tcp"
        );
        assert_eq!(ports(&[]), "");
    }

    #[test]
    fn builds_browser_urls_for_published_tcp_ports_only() {
        assert_eq!(
            browser_url(&mapping(Some("0.0.0.0"), 80, Some(8080), "tcp")).as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(
            browser_url(&mapping(Some("192.168.1.5"), 443, Some(8443), "tcp")).as_deref(),
            Some("https://192.168.1.5:8443")
        );
        assert_eq!(browser_url(&mapping(None, 80, None, "tcp")), None);
        assert_eq!(browser_url(&mapping(None, 53, Some(53), "udp")), None);
        let urls = browser_urls(&[
            mapping(Some("::"), 80, Some(8080), "tcp"),
            mapping(Some("0.0.0.0"), 80, Some(8080), "tcp"),
            mapping(Some("0.0.0.0"), 3000, Some(3000), "tcp"),
        ]);
        assert_eq!(urls, ["http://localhost:3000", "http://localhost:8080"]);
    }

    #[test]
    fn formats_cpu_memory_and_age() {
        assert_eq!(cpu(12.345), "12.3%");
        assert_eq!(cpu(f64::NAN), "0.0%");
        assert_eq!(memory(1024 * 1024, 0), "1.00 MB");
        assert_eq!(
            memory(1024 * 1024, 2 * 1024 * 1024 * 1024),
            "1.00 MB / 2.00 GB"
        );
        assert_eq!(age(100, 130), "just now");
        assert_eq!(age(0, 60), "1 minute ago");
        assert_eq!(age(0, 7200), "2 hours ago");
        assert_eq!(age(0, 86_400 * 3), "3 days ago");
        assert_eq!(age(10, 0), "just now");
    }
}
