use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::{LayoutEngine, ResolvedSegment};
use crate::segments::{self, SegmentContext, character};
use crate::style::{GlyphCatalog, ResolvedStyle, StyleResolver};
use crate::terminal::TermCaps;
use crate::theme::ThemePalette;
use unicode_width::UnicodeWidthStr;

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

        let resolved_style = StyleResolver::resolve(self.config);

        let mut segments = segments::collect_segments(&ctx);
        // Filter segments by style preset's allowed list
        let allowed = resolved_style.segment_order;
        segments.retain(|s| allowed.contains(&s.name));

        let sep_display_width = UnicodeWidthStr::width(resolved_style.left_separator.as_str()) as u16;
        let layout = LayoutEngine::new_with_separator_width(cols, sep_display_width);
        let resolved = layout.resolve(&segments);

        let line1 = self.format_line1(&resolved, &resolved_style.left_separator);
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

        let use_newline = self.config.prompt.newline && !resolved_style.force_single_line;

        let left_segment_names: std::collections::HashSet<&str> = resolved
            .iter()
            .map(|r| segments[r.original_index].name)
            .collect();

        let right = if self.config.prompt.right_prompt && !resolved_style.frame.enabled {
            self.render_right(&ctx, &left_segment_names)
        } else {
            None
        };

        let left = if resolved_style.frame.enabled && use_newline {
            let right_content = if self.config.prompt.right_prompt {
                self.render_right_raw(&ctx, &left_segment_names)
            } else {
                None
            };
            self.render_framed(
                &title_escape, &line1, &line2, right_content.as_deref(),
                cols, &resolved_style, prompt_start, prompt_end,
            )
        } else if use_newline {
            format!("{title_escape}{prompt_start}{line1}\n{line2} {prompt_end}")
        } else {
            format!("{title_escape}{prompt_start}{line1} {line2} {prompt_end}")
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

        let right_for_response = if resolved_style.frame.enabled { None } else { right };

        PromptResponse {
            left,
            right: right_for_response,
            transient,
            git_stale: git_status.stale,
            notify_threshold_ms,
        }
    }

    fn render_framed(
        &self,
        title_escape: &str,
        line1: &str,
        line2: &str,
        right_content: Option<&str>,
        cols: u16,
        style: &ResolvedStyle,
        prompt_start: &str,
        prompt_end: &str,
    ) -> String {
        let frame_fg = wrap_np(&self.palette.muted.fg_escape());
        let reset = wrap_np(RESET);

        let top_left = style.frame.top_left;
        let bottom_left = style.frame.bottom_left;
        let top_right = style.frame.top_right;
        let bottom_right = style.frame.bottom_right;

        let frame_prefix = if style.frame.left {
            format!("{frame_fg}{top_left}{reset} ")
        } else {
            String::new()
        };

        let bottom_prefix = if style.frame.left {
            format!("{frame_fg}{bottom_left}{reset} ")
        } else {
            String::new()
        };

        if let (Some(gap_char), true) = (style.gap_char, style.frame.right) {
            let left_visible = strip_ansi_width(line1);
            let right_text = right_content.unwrap_or("");
            let right_visible = strip_ansi_width(right_text);

            let frame_overhead = if style.frame.left { 3 } else { 0 }
                + if style.frame.right { 3 } else { 0 };
            let content_width = left_visible + right_visible;
            let gap_width = (cols as usize).saturating_sub(content_width + frame_overhead + 1);

            let gap_str: String = std::iter::repeat(gap_char).take(gap_width).collect();
            let gap_styled = format!("{frame_fg}{gap_str}{reset}");

            let right_frame = if style.frame.right {
                format!(" {frame_fg}{top_right}{reset}")
            } else {
                String::new()
            };

            let bottom_frame = if style.frame.right {
                format!("{frame_fg}{bottom_right}{reset}")
            } else {
                String::new()
            };

            let right_part = if !right_text.is_empty() {
                format!(" {right_text}")
            } else {
                String::new()
            };

            format!(
                "{title_escape}{prompt_start}{frame_prefix}{line1} {gap_styled}{right_part}{right_frame}\n{bottom_prefix}{line2} {bottom_frame}{prompt_end}"
            )
        } else {
            let right_part = match right_content {
                Some(r) if !r.is_empty() => format!("  {r}"),
                _ => String::new(),
            };

            let bottom_frame = if style.frame.right {
                format!("{frame_fg}{bottom_right}{reset}")
            } else {
                String::new()
            };

            format!(
                "{title_escape}{prompt_start}{frame_prefix}{line1}{right_part}\n{bottom_prefix}{line2} {bottom_frame}{prompt_end}"
            )
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
            && (!ctx.git_status.stale || ctx.config.git.stale_display)
        {
            let branch_icon = GlyphCatalog::branch_icon(&ctx.config.git.branch_icon);
            let icon_part = if branch_icon.is_empty() {
                String::new()
            } else {
                format!("{branch_icon} ")
            };
            parts.push(format!(
                "{}{}{}{}",
                wrap_np(&if ctx.git_status.stale {
                    ctx.palette.muted.fg_escape()
                } else {
                    ctx.palette.accent.fg_escape()
                }),
                icon_part,
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

    fn render_right_raw(
        &self,
        ctx: &SegmentContext<'_>,
        left_names: &std::collections::HashSet<&str>,
    ) -> Option<String> {
        self.render_right(ctx, left_names)
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

fn strip_ansi_width(s: &str) -> usize {
    let mut clean = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x01' | '\x02' => {}
            '\x1b' => {
                match chars.peek() {
                    Some('[') => {
                        chars.next();
                        while let Some(&nc) = chars.peek() {
                            chars.next();
                            if nc.is_ascii_alphabetic() { break; }
                        }
                    }
                    Some(']') => {
                        chars.next();
                        for nc in chars.by_ref() {
                            if nc == '\x07' { break; }
                            if nc == '\x1b' {
                                if chars.peek() == Some(&'\\') { chars.next(); }
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => clean.push(c),
        }
    }
    UnicodeWidthStr::width(clean.as_str())
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

    #[test]
    fn test_strip_ansi_width() {
        assert_eq!(strip_ansi_width("hello"), 5);
        assert_eq!(strip_ansi_width("\x1b[31mhello\x1b[0m"), 5);
        assert_eq!(strip_ansi_width("\x01\x1b[31m\x02hello\x01\x1b[0m\x02"), 5);
    }
}
