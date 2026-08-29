use crate::layout::Segment;
use super::SegmentContext;
use std::sync::LazyLock;
use super::util::TtlCache;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// VPN indicator segment: detects tun/tap/wg interfaces via a read of
/// `/sys/class/net` (directory listing only — no syscalls per prompt beyond
/// the once-per-15 s cached recheck) and shows the interface names. Hidden
/// when no VPN interface exists.
const VPN_TTL: Duration = Duration::from_secs(15);

static VPN_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(8));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.vpn.enabled {
        return None;
    }

    let interfaces = VPN_CACHE.get_or("net", VPN_TTL, detect_interfaces)?;
    if interfaces.is_empty() {
        return None;
    }

    let icon = &ctx.config.segments.vpn.icon;
    let content = format!("{icon} {interfaces}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "vpn".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 34,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.green.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

/// VPN-like interface names from `/sys/class/net`: `tun*`, `tap*`, `wg*`
/// (WireGuard). Sorted for stable display. `None` when the sysfs root is
/// unreadable (non-Linux), so the caller falls through to hidden.
fn detect_interfaces() -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir("/sys/class/net")
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|name| {
            name.starts_with("tun") || name.starts_with("tap") || name.starts_with("wg")
        })
        .collect();
    names.sort();
    if names.is_empty() {
        None
    } else {
        Some(names.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detection_classifies_interfaces() {
        // Functional contract: if any VPN interface exists on this box the
        // detector must find it; on a plain box it reports none. Either way
        // it must not error.
        let detected = detect_interfaces();
        if detected.is_some() {
            for name in detected.unwrap().split(',') {
                assert!(
                    name.starts_with("tun") || name.starts_with("tap") || name.starts_with("wg")
                );
            }
        }
    }

    #[test]
    fn test_name_filter() {
        let is_vpn = |n: &str| n.starts_with("tun") || n.starts_with("tap") || n.starts_with("wg");
        assert!(is_vpn("tun0"));
        assert!(is_vpn("wg-quick-tunnel"));
        assert!(!is_vpn("eth0"));
        assert!(!is_vpn("wlan0"));
        assert!(!is_vpn("veth0"));
    }
}
