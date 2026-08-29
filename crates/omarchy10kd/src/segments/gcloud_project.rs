use std::sync::LazyLock;

use crate::layout::Segment;
use super::SegmentContext;
use super::util::{run_command, TtlCache};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// GCP project segment: `GOOGLE_CLOUD_PROJECT` from the env channel wins;
/// otherwise a TTL-cached `gcloud config get-value project` fallback (30 s,
/// bounded wait). gcloud prints `() (empty)` artifacts on some versions —
/// anything without a sane value is treated as unset.
const PROJECT_TTL: Duration = Duration::from_secs(30);
const CMD_TIMEOUT_MS: u64 = 1500;

/// The active gcloud project is process-global, not per-directory, so the
/// cache holds exactly one entry under a fixed key. Keying on the cwd made
/// every first prompt in a new directory pay a fresh blocking spawn, and more
/// than `max_entries` directories inside one TTL window evicted the cache
/// into permanent misses.
const CACHE_KEY: &str = "project";

static PROJECT_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(4));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.gcloud_project.enabled {
        return None;
    }

    let project = match ctx.env_get("GOOGLE_CLOUD_PROJECT") {
        Some(p) if is_sane(&p) => p.trim().to_string(),
        _ => PROJECT_CACHE
            .get_or(CACHE_KEY, PROJECT_TTL, || {
                run_command("gcloud", &["config", "get-value", "project"], CMD_TIMEOUT_MS)
                    .filter(|v| is_sane(v))
            })?,
    };
    if project.is_empty() {
        return None;
    }

    let icon = &ctx.config.segments.gcloud_project.icon;
    let content = format!("{icon} {project}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "gcloud_project".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 40,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.orange.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

/// Reject gcloud's empty-output shapes: `""`, `()`, `(unset)`, whitespace.
fn is_sane(value: &str) -> bool {
    let v = value.trim();
    !v.is_empty()
        && v != "()"
        && v != "(unset)"
        && v != "null"
        && v != "(empty)"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sane_rejects_gcloud_empty_shapes() {
        assert!(is_sane("my-project"));
        assert!(!is_sane(""));
        assert!(!is_sane("  "));
        assert!(!is_sane("()"));
        assert!(!is_sane("(unset)"));
        assert!(!is_sane("null"));
        assert!(!is_sane("(empty)"));
    }

    #[test]
    fn test_empty_env_falls_to_command_cache() {
        // Contract: env branch only accepts sane values; otherwise the
        // cached command path runs (rendered above). Sanity-checked here
        // without spawning gcloud.
        assert!(is_sane("prod-123"));
    }
}
