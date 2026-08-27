use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::{LayoutEngine, LayoutPreset, ResolvedSegment};
use crate::segments::{self, SegmentContext, character};
use crate::theme::ThemePalette;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const OSC_133_PROMPT_START: &str = "\x01\x1b]133;A\x07\x02";
const OSC_133_PROMPT_END: &str = "\x01\x1b]133;B\x07\x02";

#[derive(Debug, Clone, serde::Serialize)]
pub struct PromptResponse {
    pub left: String,
    pub right: Option<String>,
    pub transient: Option<String>,
    pub git_stale: bool,
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
        let home = std::env::var("HOME").unwrap_or_default();
        let in_ssh = std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok();

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
            let short_cwd = if !home.is_empty() && cwd.starts_with(&home) {
                format!("~{}", &cwd[home.len()..])
            } else {
                cwd.to_string()
            };
            format!("\x1b]2;{short_cwd}\x07")
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

        let right = if self.config.prompt.right_prompt {
            self.render_right(&ctx)
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

        PromptResponse {
            left,
            right,
            transient,
            git_stale: git_status.stale,
        }
    }

    fn render_right(&self, ctx: &SegmentContext<'_>) -> Option<String> {
        let mut parts = Vec::new();

        if ctx.config.segments.command_duration.enabled && ctx.cmd_duration_ms >= ctx.config.segments.command_duration.show_above_ms {
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
                ctx.palette.muted.fg_escape(),
                time_str,
                RESET
            ));
        }

        if ctx.git_status.is_repo && !ctx.git_status.branch.is_empty() {
            parts.push(format!(
                "{}{} {}{}",
                if ctx.git_status.stale { ctx.palette.muted.fg_escape() } else { ctx.palette.accent.fg_escape() },
                "\u{e0a0}",
                ctx.git_status.branch,
                RESET
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

            styled.push_str(&seg.fg);
            if seg.bold {
                styled.push_str(BOLD);
            }
            styled.push_str(&seg.content);
            styled.push_str(RESET);

            parts.push(styled);
        }

        parts.join(separator)
    }

    fn format_line2(&self, ctx: &SegmentContext<'_>) -> String {
        character::render_prompt_char(ctx)
    }
}
