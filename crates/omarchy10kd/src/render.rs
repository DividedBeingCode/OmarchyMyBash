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
    /// Notification threshold in ms; `0` means notifications are OFF
    /// (protocol 0.4 — the old contract emitted null/absent on disable,
    /// which the adapter misread as "keep default": the no-op bug).
    pub notify_threshold_ms: u64,
    /// Adapter may emit OSC 133;C/D when true (from `[terminal.semantic_prompts]`).
    pub semantic_prompts: bool,
    /// Restrict notifications to unfocused terminals (bash-side gating).
    pub notify_unfocused_only: bool,
}

/// Claude Code statusLine stdin JSON (protocol 0.4 `statusline` message).
/// Parsing is deliberately tolerant: unknown fields are skipped by serde and
/// missing sections simply render fewer parts.
#[derive(Debug, Default, serde::Deserialize)]
pub struct StatuslinePayload {
    #[serde(default)]
    pub model: Option<StatuslineModel>,
    #[serde(default)]
    pub workspace: Option<StatuslineWorkspace>,
    #[serde(default)]
    pub cost: Option<StatuslineCost>,
    #[serde(default)]
    pub context_window: Option<StatuslineContext>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StatuslineModel {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StatuslineWorkspace {
    #[serde(default)]
    pub current_dir: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StatuslineCost {
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, serde::Deserialize)]
pub struct StatuslineContext {
    #[serde(default)]
    pub used: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    /// Claude Code's documented field for pre-computed context usage.
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub percentage: Option<f64>,
}

impl StatuslineContext {
    /// Fallback chain: `used_percentage` (documented Claude Code field)
    /// -> `percentage` -> computed from `used`/`total`.
    fn percentage(&self) -> Option<f64> {
        if let Some(p) = self.used_percentage {
            return Some(p);
        }
        if let Some(p) = self.percentage {
            return Some(p);
        }
        match (self.used, self.total) {
            (Some(u), Some(t)) if t > 0 => Some(u as f64 / t as f64 * 100.0),
            _ => None,
        }
    }
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
        env: Option<&std::collections::HashMap<String, String>>,
    ) -> PromptResponse {
        self.render_with_ssh(cwd, exit_code, cmd_duration_ms, cols, jobs, git_status, shell_integration, None, env)
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
        env: Option<&std::collections::HashMap<String, String>>,
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
            env,
        };

        let resolved_style = StyleResolver::resolve(self.config);

        let mut segments = segments::collect_segments(&ctx);
        // Filter segments by style preset's allowed list
        let allowed = resolved_style.segment_order;
        segments.retain(|s| allowed.contains(&s.name));

        // True powerline (v0.4 1.1): fill each segment with a background
        // color. Powerline uses the segment's own fg color as bg (fg flips to
        // the theme background for contrast); rainbow rotates through the
        // accent/red/green/yellow/blue palette. Layout width math is
        // unchanged — separators are already counted by LayoutEngine.
        if resolved_style.filled {
            let rainbow_colors = [
                &self.palette.accent,
                &self.palette.red,
                &self.palette.green,
                &self.palette.yellow,
                &self.palette.blue,
            ];
            for (i, seg) in segments.iter_mut().enumerate() {
                seg.bg = Some(if resolved_style.rainbow {
                    rainbow_colors[i % rainbow_colors.len()].bg_escape()
                } else {
                    seg.fg.replace("[38;2;", "[48;2;")
                });
                seg.fg = self.palette.background.fg_escape();
            }
        }

        let sep_display_width = UnicodeWidthStr::width(resolved_style.left_separator.as_str()) as u16;
        let layout = LayoutEngine::new_with_separator_width(cols, sep_display_width);
        let resolved = layout.resolve(&segments);

        let line1 = self.format_line1(&resolved, &resolved_style);
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

        let notifications = self.config.effective_notifications();
        let notify_threshold_ms = if notifications.enabled {
            notifications.threshold_ms
        } else {
            0
        };
        let semantic_prompts = self.config.terminal.semantic_prompts.enabled;

        let right_for_response = if resolved_style.frame.enabled { None } else { right };

        PromptResponse {
            left,
            right: right_for_response,
            transient,
            git_stale: git_status.stale,
            notify_threshold_ms,
            semantic_prompts,
            notify_unfocused_only: notifications.unfocused_only,
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

    fn format_line1(&self, segments: &[ResolvedSegment], style: &ResolvedStyle) -> String {
        if style.filled {
            return self.format_line1_filled(segments, style);
        }

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

        parts.join(&style.left_separator)
    }

    /// True powerline rendering (v0.4 1.1): each segment is painted with its
    /// background fill, separators flip fg to the previous segment's bg and
    /// blend into the next segment's bg, and the configured caps decorate the
    /// line ends. Separator glyphs stay inside the width the LayoutEngine
    /// already counted, so layout math is unchanged.
    fn format_line1_filled(&self, segments: &[ResolvedSegment], style: &ResolvedStyle) -> String {
        let mut out = String::new();

        if !style.left_cap_start.is_empty() {
            out.push_str(&style.left_cap_start);
        }

        for (i, seg) in segments.iter().enumerate() {
            let bg = seg.bg.clone().unwrap_or_else(|| seg.fg.clone());

            // Segment body: bg fill + contrasting fg.
            out.push_str(&wrap_np(&bg));
            out.push_str(&wrap_np(&seg.fg));
            if seg.bold {
                out.push_str(&wrap_np(BOLD));
            }
            out.push_str(&seg.content);

            if let Some(next) = segments.get(i + 1) {
                out.push_str(&wrap_np(&RESET));
                // Separator: fg flipped to this segment's bg, blending into
                // the next segment's bg.
                out.push_str(&wrap_np(&bg));
                if let Some(next_bg) = &next.bg {
                    out.push_str(&wrap_np(next_bg));
                }
                out.push_str(&wrap_np(&style.left_separator));
                out.push_str(&wrap_np(&RESET));
            } else {
                // Last segment: close the fill. An end cap (right_cap_end,
                // falling back to left_cap_end) hangs in the last bg color.
                out.push_str(&wrap_np(&RESET));
                let end_cap = if !style.right_cap_end.is_empty() {
                    Some(&style.right_cap_end)
                } else if !style.left_cap_end.is_empty() {
                    Some(&style.left_cap_end)
                } else {
                    None
                };
                if let Some(cap) = end_cap {
                    out.push_str(&wrap_np(&bg));
                    out.push_str(cap);
                    out.push_str(&wrap_np(&RESET));
                }
            }
        }

        out
    }

    /// Render the Claude Code statusline (v0.4 1.2) as a single left-only
    /// ANSI line: model name, context % (green/yellow/red), cost, cwd basename.
    pub fn render_statusline(&self, payload: &StatuslinePayload) -> String {
        let mut parts: Vec<String> = Vec::new();

        if let Some(model) = &payload.model {
            if let Some(name) = model.display_name.as_deref().filter(|s| !s.is_empty()) {
                parts.push(format!(
                    "{}{}{}",
                    wrap_np(&self.palette.accent.fg_escape()),
                    name,
                    wrap_np(RESET)
                ));
            }
        }

        if let Some(ctx) = &payload.context_window {
            if let Some(pct) = ctx.percentage() {
                let pct = pct.round().clamp(0.0, 100.0) as u8;
                let color = if pct >= self.config.statusline.context_critical_pct {
                    &self.palette.red
                } else if pct >= self.config.statusline.context_warning_pct {
                    &self.palette.yellow
                } else {
                    &self.palette.green
                };
                parts.push(format!(
                    "{}ctx {}%{}",
                    wrap_np(&color.fg_escape()),
                    pct,
                    wrap_np(RESET)
                ));
            }
        }

        if let Some(cost) = &payload.cost {
            if let Some(usd) = cost.total_cost_usd {
                parts.push(format!(
                    "{}${:.2}{}",
                    wrap_np(&self.palette.muted.fg_escape()),
                    usd,
                    wrap_np(RESET)
                ));
            }
        }

        if let Some(ws) = &payload.workspace {
            if let Some(dir) = ws.current_dir.as_deref().filter(|s| !s.is_empty()) {
                let base = std::path::Path::new(dir)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(dir);
                parts.push(format!(
                    "{}{}{}",
                    wrap_np(&self.palette.foreground.fg_escape()),
                    base,
                    wrap_np(RESET)
                ));
            }
        }

        parts.join(" ")
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

    // ── v0.4 helpers ─────────────────────────────────────

    fn render_resp(
        config: &Config,
        git_status: &GitStatus,
        env: Option<&std::collections::HashMap<String, String>>,
    ) -> PromptResponse {
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(config, &palette);
        renderer.render_with_ssh(
            "/home/u/project",
            0,
            0,
            120,
            0,
            git_status,
            false,
            Some(false),
            env,
        )
    }

    #[test]
    fn test_powerline_emits_bg_sgr() {
        let mut config = Config::default();
        config.style.preset = "powerline".into();
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),

            ..Default::default()
        };
        let resp = render_resp(&config, &git, None);
        assert!(
            resp.left.contains("\x1b[48;2;"),
            "powerline preset must emit SGR 48;2 background fills, got: {:?}",
            resp.left
        );
    }

    #[test]
    fn test_rainbow_distinct_from_powerline() {
        let palette = ThemePalette::default();
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            ..Default::default()
        };
        let mut config = Config::default();
        config.style.preset = "rainbow".into();
        let rainbow = render_resp(&config, &git, None);
        let mut config = Config::default();
        config.style.preset = "powerline".into();
        let powerline = render_resp(&config, &git, None);

        assert!(rainbow.left.contains("\x1b[48;2;"));
        // Rainbow rotates bg colors: the accent color appears as a bg fill.
        let accent_bg = format!("\x1b[48;2;{};{};{}m", palette.accent.r, palette.accent.g, palette.accent.b);
        assert!(
            rainbow.left.contains(&accent_bg),
            "rainbow should paint the first segment with the accent bg"
        );
        assert_ne!(
            rainbow.left,
            powerline.left,
            "rainbow and powerline renders must differ"
        );
    }

    #[test]
    fn test_non_filled_preset_has_no_bg() {
        let mut config = Config::default();
        config.style.preset = "lean".into();
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            ..Default::default()
        };
        let resp = render_resp(&config, &git, None);
        assert!(!resp.left.contains("\x1b[48;2;"), "lean must not emit bg fills");
    }

    #[test]
    fn test_notification_disabled_emits_zero_threshold() {
        let mut config = Config::default();
        config.segments.notification.enabled = false;
        let git = GitStatus::default();
        let resp = render_resp(&config, &git, None);
        assert_eq!(resp.notify_threshold_ms, 0, "disabled notifications must emit 0");


        config.segments.notification.enabled = true;
        config.segments.notification.threshold_ms = 12345;
        let resp = render_resp(&config, &git, None);
        assert_eq!(resp.notify_threshold_ms, 12345);
    }

    #[test]
    fn test_statusline_render_parts() {
        let config = Config::default();
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(&config, &palette);
        let payload: StatuslinePayload = serde_json::from_value(serde_json::json!({
            "model": {"id": "claude-x", "display_name": "Opus"},
            "workspace": {"current_dir": "/home/u/projects/my-app"},
            "cost": {"total_cost_usd": 0.034, "total_duration_ms": 1000},
            "context_window": {"used": 42000, "total": 100000},
            "unknown_future_field": {"nested": true}
        }))
        .unwrap();
        let left = renderer.render_statusline(&payload);
        assert!(left.contains("Opus"), "model name missing: {left}");
        assert!(left.contains("ctx 42%"), "context percent missing: {left}");
        assert!(left.contains("$0.03"), "cost missing: {left}");
        assert!(left.contains("my-app"), "cwd basename missing: {left}");
    }

    #[test]
    fn test_statusline_context_thresholds() {
        let config = Config::default();
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(&config, &palette);

        let mk = |pct: f64| {
            let payload: StatuslinePayload = serde_json::from_value(serde_json::json!({
                "context_window": {"percentage": pct}
            }))
            .unwrap();
            renderer.render_statusline(&payload)
        };

        let green = mk(10.0);
        let yellow = mk(70.0);
        let red = mk(95.0);
        assert!(green.contains(&palette.green.fg_escape()));
        assert!(yellow.contains(&palette.yellow.fg_escape()));
        assert!(red.contains(&palette.red.fg_escape()));
    }

    #[test]
    fn test_statusline_tolerates_empty_payload() {
        let config = Config::default();
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(&config, &palette);
        let payload: StatuslinePayload = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(renderer.render_statusline(&payload), "");
    }
    #[test]
    fn test_statusline_used_percentage_field() {
        let config = Config::default();
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(&config, &palette);
        // Documented Claude Code field: context_window.used_percentage.
        let payload: StatuslinePayload = serde_json::from_value(serde_json::json!({
            "context_window": {"used_percentage": 91.4}
        }))
        .unwrap();
        let left = renderer.render_statusline(&payload);
        assert!(left.contains("ctx 91%"), "used_percentage must drive the percent: {left}");
        assert!(left.contains(&palette.red.fg_escape()), "91% must render red");
    }
}
