
pub mod directory;
pub mod git;
pub mod exit_status;
pub mod command_duration;
pub mod character;
pub mod os;
pub mod ssh;
pub mod jobs;
pub mod container;
pub mod python_env;
pub mod toolchain;
pub mod nix;
pub mod k8s;
pub mod time;
pub mod battery;
pub mod ai;

use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::Segment;
use crate::terminal::TermCaps;
use crate::theme::ThemePalette;

pub struct SegmentContext<'a> {
    pub cwd: &'a str,
    pub home: &'a str,
    pub exit_code: i32,
    pub cmd_duration_ms: u64,
    pub cols: u16,
    pub jobs: u32,
    pub in_ssh: bool,
    pub git_status: &'a GitStatus,
    pub config: &'a Config,
    pub palette: &'a ThemePalette,
    pub term_caps: &'a TermCaps,
    /// Environment values carried by the prompt request (protocol 0.4).
    /// `None` for legacy clients and previews.
    pub env: Option<&'a std::collections::HashMap<String, String>>,
}

impl SegmentContext<'_> {
    /// Read an environment variable, preferring the env channel from the
    /// prompt request over the daemon's own process environment. Falls back
    /// to `std::env` when the channel is absent or doesn't carry `key`.
    pub fn env_get(&self, key: &str) -> Option<String> {
        if let Some(map) = self.env {
            if let Some(v) = map.get(key) {
                return Some(v.clone());
            }
        }
        std::env::var(key).ok()
    }
}

pub fn collect_segments(ctx: &SegmentContext<'_>) -> Vec<Segment> {
    let mut segments = Vec::new();

    if let Some(seg) = os::render(ctx) {
        segments.push(seg);
    }

    if let Some(seg) = ssh::render(ctx) {
        segments.push(seg);
    }

    if ctx.config.segments.container.enabled {
        if let Some(seg) = container::render(ctx) {
            segments.push(seg);
        }
    }

    if let Some(seg) = directory::render(ctx) {
        segments.push(seg);
    }

    if ctx.config.git.enabled {
        if let Some(seg) = git::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.python.enabled {
        if let Some(seg) = python_env::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.toolchain.enabled {
        if let Some(seg) = toolchain::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.nix.enabled {
        if let Some(seg) = nix::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.k8s.enabled {
        if let Some(seg) = k8s::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.exit_status.enabled {
        if let Some(seg) = exit_status::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.ai.enabled {
        if let Some(seg) = ai::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.command_duration.enabled {
        if let Some(seg) = command_duration::render(ctx) {
            segments.push(seg);
        }
    }

    if let Some(seg) = jobs::render(ctx) {
        segments.push(seg);
    }

    if ctx.config.segments.time.enabled {
        if let Some(seg) = time::render(ctx) {
            segments.push(seg);
        }
    }

    if ctx.config.segments.battery.enabled {
        if let Some(seg) = battery::render(ctx) {
            segments.push(seg);
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitStatus;
    use crate::terminal::TermCaps;
    use crate::theme::ThemePalette;
    use std::collections::HashMap;

    fn make_ctx<'a>(
        env: Option<&'a HashMap<String, String>>,
        config: &'a Config,
        git: &'a GitStatus,
    ) -> SegmentContext<'a> {
        SegmentContext {
            cwd: "/tmp",
            home: "/home/u",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: git,
            config,
            palette: &THEME,
            term_caps: &CAPS,
            env,
        }
    }

    static THEME: std::sync::LazyLock<ThemePalette> = std::sync::LazyLock::new(ThemePalette::default);
    static CAPS: std::sync::LazyLock<TermCaps> = std::sync::LazyLock::new(TermCaps::detect);

    #[test]
    fn test_env_get_prefers_request_env() {
        let mut map = HashMap::new();
        map.insert("VIRTUAL_ENV".to_string(), "/somewhere/venv".to_string());
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(Some(&map), &config, &git);
        assert_eq!(ctx.env_get("VIRTUAL_ENV").as_deref(), Some("/somewhere/venv"));
    }

    #[test]
    fn test_env_get_falls_back_to_process_env() {
        let map = HashMap::new();
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(Some(&map), &config, &git);
        // Key missing from the request map: falls back to std::env (present
        // in the test process since we set it).
        unsafe { std::env::set_var("O10K_TEST_FALLBACK_VAR", "process-value") };
        assert_eq!(
            ctx.env_get("O10K_TEST_FALLBACK_VAR").as_deref(),
            Some("process-value")
        );
    }

    #[test]
    fn test_env_get_without_channel_reads_process_env() {
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(None, &config, &git);
        unsafe { std::env::set_var("O10K_TEST_NO_CHANNEL_VAR", "legacy-value") };
        assert_eq!(
            ctx.env_get("O10K_TEST_NO_CHANNEL_VAR").as_deref(),
            Some("legacy-value")
        );
    }
}
