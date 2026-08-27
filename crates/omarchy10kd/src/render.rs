use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::{LayoutEngine, LayoutPreset, ResolvedSegment};
use crate::segments::{self, SegmentContext, character};
use crate::terminal::TermCaps;
use crate::theme::ThemePalette;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

/// Wrap an ANSI escape in readline non-printing delimiters
pub fn wrap_np(esc: &str) -> String {
    format!("\x01{esc}\x02")
}
const OSC_133_PROMPT_START: &str = "\x01\x1b]133;A\x07\x02";
const OSC_133_PROMPT_END: &str = "\x01\x1b]133;B\x07\x02";

#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptResponse {
    pub left: String,
    pub right: Option<String>,
    pub transient: Option<String>,
    pub git_stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_threshold_ms: Option<u64>,
}

pub struct PromptRenderer<'a> {
    config: &'a Config,
    palette: &'a ThemePalette,
}

impl<'a> PromptRenderer<'a> {
    pub fn new(config: &'a Config, palette: &'a ThemePalette) -> Self {
        Self { config, palette }
    }

    pub fn render(
        &self,
        cwd: &str,
        exit_code: i32,
        cmd_duration_ms: u64,
        cols: u16,
        jobs: u32,
        git_status: &GitStatus,
        shell_integration: bool,
    ) -> PromptResponse {
        self.render_with_ssh(cwd, exit_code, cmd_duration_ms, cols, jobs, git_status, shell_integration, None)
    }

    pub fn render_with_ssh(
        &self,
        cwd: &str,
        exit_code: i32,
        cmd_duration_ms: u64,
        cols: u16,
        jobs: u32,
        git_status: &GitStatus,
        shell_integration: bool,
        force_ssh: Option<bool>,
    ) -> PromptResponse {
        let home = std::env::var("HOME").unwrap_or_default();
        let in_ssh = force_ssh.unwrap_or_else(|| {
            std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok()
        });
        let term_caps = TermCaps::detect();

        let ctx = SegmentContext {
            cwd,
            home: &home,
            exit_code,
            cmd_duration_ms,
            cols,
            jobs,
            in_ssh,
            git_status,
            config: self.config,
            palette: self.palette,
            term_caps: &term_caps,
        };

        let mut segments = segments::collect_segments(&ctx);
        LayoutPreset::apply_filter(&mut segments, &self.config.prompt.layout);
        let layout = LayoutEngine::new(cols);
        let resolved = layout.resolve(&segments);

        let separator = LayoutPreset::separator(&self.config.prompt.layout);
        let line1 = self.format_line1(&resolved, separator);
        let line2 = self.format_line2(&ctx);

        let (prompt_start, prompt_end) = if shell_integration {
            (OSC_133_PROMPT_START, OSC_133_PROMPT_END)
        } else {
            ("", "")
        };

        let title_escape = if self.config.terminal.title.enabled {
            let short_cwd = if !home.is_empty()
                && std::path::Path::new(cwd).starts_with(std::path::Path::new(&home))
            {
                format!("~{}", &cwd[home.len()..])
            } else {
                cwd.to_string()
            };
            let title_text = self.expand_title_format(&self.config.terminal.title.format, &short_cwd, git_status);
            format!("\x01\x1b]2;{title_text}\x07\x02")
        } else {
            String::new()
        };

        let force_single = LayoutPreset::is_single_line(&self.config.prompt.layout);
        let use_newline = self.config.prompt.newline && !force_single;

        let left = if use_newline {
            format!("{title_escape}{prompt_start}{line1}\n{line2} {prompt_end}")
        } else {
            format!("{title_escape}{prompt_start}{line1} {line2} {prompt_end}")
        };

        let left_segment_names: std::collections::HashSet<&str> = resolved
            .iter()
            .map(|r| segments[r.original_index].name)
            .collect();

        let right = if self.config.prompt.right_prompt {
            self.render_right(&ctx, &left_segment_names)
        } else {
            None
        };

        let transient = if self.config.prompt.transient {
            Some(format!(
                "{prompt_start}{} {prompt_end}",
                character::render_transient_char(&ctx)
            ))
        } else {
            None
        };

        let notify_threshold_ms = if self.config.segments.notification.enabled {
            Some(self.config.segments.notification.threshold_ms)
        } else {
            None
        };

        PromptResponse {
            left,
            right,
            transient,
            git_stale: git_status.stale,
            notify_threshold_ms,
        }
    }

    fn render_right(
        &self,
        ctx: &SegmentContext<'_>,
        left_names: &std::collections::HashSet<&str>,
    ) -> Option<String> {
        let mut parts = Vec::new();

        if !left_names.contains("command_duration")
            && ctx.config.segments.command_duration.enabled
            && ctx.cmd_duration_ms >= ctx.config.segments.command_duration.show_above_ms
        {
            let secs = ctx.cmd_duration_ms / 1000;
            let ms = ctx.cmd_duration_ms % 1000;
            let time_str = if secs >= 60 {
                format!("{}m{}s", secs / 60, secs % 60)
            } else if secs > 0 {
                format!("{secs}.{:01}s", ms / 100)
            } else {
                format!("{ms}ms")
            };
            parts.push(format!(
                "{}{}{}",
                wrap_np(&ctx.palette.muted.fg_escape()),
                time_str,
                wrap_np(RESET)
            ));
        }

        if !left_names.contains("git")
            && ctx.git_status.is_repo
            && !ctx.git_status.branch.is_empty()
        {
            parts.push(format!(
                "{}{} {}{}",
                wrap_np(&if ctx.git_status.stale {
                    ctx.palette.muted.fg_escape()
                } else {
                    ctx.palette.accent.fg_escape()
                }),
                "\u{e0a0}",
                ctx.git_status.branch,
                wrap_np(RESET)
            ));
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    fn format_line1(&self, segments: &[ResolvedSegment], separator: &str) -> String {
        let mut parts = Vec::new();

        for seg in segments {
            let mut styled = String::new();

            styled.push_str(&wrap_np(&seg.fg));
            if seg.bold {
                styled.push_str(&wrap_np(BOLD));
            }
            styled.push_str(&seg.content);
            styled.push_str(&wrap_np(RESET));

            parts.push(styled);
        }

        parts.join(separator)
    }

    fn format_line2(&self, ctx: &SegmentContext<'_>) -> String {
        character::render_prompt_char(ctx)
    }

    fn expand_title_format(&self, format: &str, short_cwd: &str, git_status: &GitStatus) -> String {
        if format.is_empty() {
            return short_cwd.to_string();
        }
        let user = std::env::var("USER").unwrap_or_default();
        let host = gethostname_string();
        let branch = if git_status.is_repo && !git_status.branch.is_empty() {
            &git_status.branch
        } else {
            ""
        };
        format
            .replace("{dir}", short_cwd)
            .replace("{user}", &user)
            .replace("{host}", &host)
            .replace("{branch}", branch)
    }
}

fn gethostname_string() -> String {
    let mut buf = vec![0u8; 256];
    let ret = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if ret != 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(len);
    String::from_utf8_lossy(&buf).into_owned()
}

mod libc {
    unsafe extern "C" {
        pub fn gethostname(name: *mut std::ffi::c_char, len: usize) -> std::ffi::c_int;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_np() {
        let esc = "\x1b[31m";
        let wrapped = wrap_np(esc);
        assert_eq!(wrapped, "\x01\x1b[31m\x02");
        assert!(wrapped.starts_with('\x01'));
        assert!(wrapped.ends_with('\x02'));
    }

    #[test]
    fn test_wrap_np_reset() {
        assert_eq!(wrap_np(RESET), "\x01\x1b[0m\x02");
    }

    #[test]
    fn test_wrap_np_bold() {
        assert_eq!(wrap_np(BOLD), "\x01\x1b[1m\x02");
    }
}
