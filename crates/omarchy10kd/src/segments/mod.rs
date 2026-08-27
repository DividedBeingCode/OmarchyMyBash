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

use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::Segment;
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
