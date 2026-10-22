//! Small pure text helpers shared by the UI and services.

/// Pretty-prints JSON with two-space indent, preserving key order.
/// `None` if `text` is not valid JSON.
#[must_use]
pub fn pretty_json(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// `512 B`, `1.5 KB`, `2.3 MB`.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

/// `850 ms`, `1.24 s`.
#[must_use]
pub fn human_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        #[allow(clippy::cast_precision_loss)]
        let s = ms as f64 / 1000.0;
        format!("{s:.2} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_json_keeps_order() {
        assert_eq!(
            pretty_json(r#"{"b":1,"a":[1]}"#).as_deref(),
            Some("{\n  \"b\": 1,\n  \"a\": [\n    1\n  ]\n}")
        );
        assert_eq!(pretty_json("nope"), None);
    }

    #[test]
    fn sizes_and_durations() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_duration_ms(1240), "1.24 s");
    }
}
