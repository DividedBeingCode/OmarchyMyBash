use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::{LayoutEngine, ResolvedSegment};
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

        let segments = segments::collect_segments(&ctx);
        let layout = LayoutEngine::new(cols);
        let resolved = layout.resolve(&segments);

        let line1 = self.format_line1(&resolved);
        let line2 = self.format_line2(&ctx);

        let left = if self.config.prompt.newline {
            format!(
                "{OSC_133_PROMPT_START}{line1}\n{line2} {OSC_133_PROMPT_END}"
            )
        } else {
            format!(
                "{OSC_133_PROMPT_START}{line1} {line2} {OSC_133_PROMPT_END}"
            )
        };

        let transient = if self.config.prompt.transient {
            Some(format!(
                "{OSC_133_PROMPT_START}{} {OSC_133_PROMPT_END}",
                character::render_transient_char(&ctx)
            ))
        } else {
            None
        };

        PromptResponse {
            left,
            right: None,
            transient,
            git_stale: git_status.stale,
        }
    }

    fn format_line1(&self, segments: &[ResolvedSegment]) -> String {
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

        parts.join(" ")
    }

    fn format_line2(&self, ctx: &SegmentContext<'_>) -> String {
        character::render_prompt_char(ctx)
    }
}
