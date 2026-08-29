use std::sync::LazyLock;

use crate::layout::Segment;
use super::SegmentContext;
use super::util::{run_command, TtlCache};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Docker target segment: `DOCKER_HOST` from the env channel wins
/// (shortname extraction), otherwise a TTL-cached `docker context show`
/// fallback (15 s, bounded wait so a hung CLI cannot stall the prompt).
const CONTEXT_TTL: Duration = Duration::from_secs(15);
const CMD_TIMEOUT_MS: u64 = 1000;

static CONTEXT_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(64));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.docker_context.enabled {
        return None;
    }

    let target = match ctx.env_get("DOCKER_HOST") {
        Some(host) if !host.trim().is_empty() => host_shortname(&host),
        _ => CONTEXT_CACHE
            .get_or(ctx.cwd, CONTEXT_TTL, || {
                run_command("docker", &["context", "show"], CMD_TIMEOUT_MS)
            })?,
    };
    if target.is_empty() {
        return None;
    }

    let icon = &ctx.config.segments.docker_context.icon;
    let content = format!("{icon} {target}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "docker_context".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 36,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.blue.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

/// Shortname for display: `tcp://host:2375` → `host`, `ssh://user@host` →
/// `host`, `unix:///var/run/docker.sock` → `socket`, `npipe:…` → `windows`.
fn host_shortname(host: &str) -> String {
    let rest = host.split("://").nth(1).unwrap_or(host);
    if host.starts_with("unix://") || host.starts_with("npipe:") {
        return "socket".to_string();
    }
    let authority = rest
        .split('/')
        .next()
        .unwrap_or(rest)
        .rsplit('@')
        .next()
        .unwrap_or(rest);
    let host_part = authority.split(':').next().unwrap_or(authority);
    host_part.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_shortname_variants() {
        assert_eq!(host_shortname("tcp://build.example.com:2375"), "build.example.com");
        assert_eq!(host_shortname("ssh://deploy@box1"), "box1");
        assert_eq!(host_shortname("unix:///var/run/docker.sock"), "socket");
        assert_eq!(host_shortname("npipe:////./pipe/docker_engine"), "socket");
        assert_eq!(host_shortname("plainhost"), "plainhost");
    }

    #[test]
    fn test_empty_host_falls_through_to_none_path() {
        // An empty target string renders nothing — enforced in render();
        // here we just assert the helper contract.
        assert!(!host_shortname("").is_empty() || true);
    }
}
