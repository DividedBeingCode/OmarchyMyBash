use std::sync::LazyLock;

use crate::layout::Segment;
use super::SegmentContext;
use super::util::{run_command, TtlCache};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Kubernetes context segment: TTL-cached `kubectl config current-context`
/// (15 s, 500 ms bounded wait). When kubectl is absent or hangs, the segment
/// degrades to hidden. The command is only attempted once per TTL window.
const CONTEXT_TTL: Duration = Duration::from_secs(15);
const CMD_TIMEOUT_MS: u64 = 500;

static CONTEXT_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(8));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.kubectl_context.enabled {
        return None;
    }

    // The current context is not a property of the cwd — it depends only on
    // the active kubeconfig. Keying on the cwd made every first prompt in a
    // new directory pay a fresh blocking spawn, and more than `max_entries`
    // directories inside one TTL window evicted the cache into permanent
    // misses. `KUBECONFIG` rides the env channel and can change mid-session,
    // so it is the key.
    let cache_key = ctx.env_get("KUBECONFIG").unwrap_or_default();
    let context = CONTEXT_CACHE
        .get_or(&cache_key, CONTEXT_TTL, || {
            run_command("kubectl", &["config", "current-context"], CMD_TIMEOUT_MS)
        })?;
    if context.is_empty() {
        return None;
    }

    let icon = &ctx.config.segments.kubectl_context.icon;
    let content = format!("{icon} {context}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "kubectl_context".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 37,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.cyan.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitStatus;
    use crate::terminal::TermCaps;
    use crate::theme::ThemePalette;
    use std::collections::HashMap;
    use std::sync::LazyLock;

    static THEME: LazyLock<ThemePalette> = LazyLock::new(ThemePalette::default);
    static CAPS: LazyLock<TermCaps> = LazyLock::new(TermCaps::detect);
    static GIT: LazyLock<GitStatus> = LazyLock::new(GitStatus::default);
    static CONFIG: LazyLock<Config> = LazyLock::new(Config::default);
    static EMPTY_ENV: LazyLock<HashMap<String, String>> = LazyLock::new(HashMap::new);

    fn make_ctx() -> SegmentContext<'static> {
        SegmentContext {
            cwd: "/tmp",
            home: "/home/u",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: &GIT,
            config: &CONFIG,
            palette: &THEME,
            term_caps: &CAPS,
            env: Some(&EMPTY_ENV),
        }
    }

    #[test]
    fn test_disabled_by_default() {
        // Default config keeps this segment off — zero behavior change.
        assert!(!Config::default().segments.kubectl_context.enabled);
        assert!(render(&make_ctx()).is_none());
    }

    #[test]
    fn test_missing_kubectl_degrades_to_hidden() {
        if crate::segments::util::on_path("kubectl") {
            return; // Can't simulate absence on a box that has it.
        }
        let mut config = Config::default();
        config.segments.kubectl_context.enabled = true;
        let mut ctx = make_ctx();
        ctx.config = &config;
        assert!(render(&ctx).is_none(), "missing kubectl must hide the segment");
    }
}
