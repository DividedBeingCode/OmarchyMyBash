use crate::config::Config;
use crate::git::GitStatus;
use crate::layout::{LayoutEngine, ResolvedSegment, Segment};
use crate::plugins;
use crate::segments::{self, SegmentContext, character};
use crate::style::{GapGradient, GlyphCatalog, ResolvedStyle, StyleResolver};
use crate::theme::AnsiColor;
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
        plugin_segments: Vec<Segment>,
    ) -> PromptResponse {
        self.render_with_ssh(cwd, exit_code, cmd_duration_ms, cols, jobs, git_status, shell_integration, None, env, plugin_segments)
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
        plugin_segments: Vec<Segment>,
    ) -> PromptResponse {
        let home = std::env::var("HOME").unwrap_or_default();
        let in_ssh = force_ssh.unwrap_or_else(|| {
            std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok()
        });
        // The SHELL's answer, not the daemon's environment. The daemon has no
        // controlling terminal and outlives the shell that spawned it, so its
        // own env names whichever terminal started it first -- and this gates
        // OSC 8 hyperlinks and undercurl, so getting it wrong is visible.
        let term_caps = TermCaps::for_kind(crate::terminal::kind_from_channel(env));

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
        // Plugin segments join the built-in pipeline here: same Vec, same
        // layout/priority/filter path. The preset filter passes
        // `plugin.`-prefixed names through.
        segments.extend(plugin_segments);
        let allowed = resolved_style.segment_order;
        segments.retain(|s| allowed.contains(&&*s.name) || s.name.starts_with(plugins::PLUGIN_SEGMENT_PREFIX));
        let use_newline = self.config.prompt.newline && !resolved_style.force_single_line;

        // True powerline (v0.4 1.1): fill each segment with a background
        // color. Powerline uses the segment's own fg color as bg (fg flips to
        // the theme background for contrast); rainbow uses p10k's semantic
        // fills — dir blue, git green (yellow when dirty), duration dark with
        // yellow text, jobs cyan, time white — with per-fill contrast text
        // instead of one global dark fg. Layout width math is unchanged —
        // separators are already counted by LayoutEngine.
        if resolved_style.filled {
            let ramp_len = segments.len();
            for (i, seg) in segments.iter_mut().enumerate() {
                // p10k pads every segment one space per side inside its
                // colored fill (LEFT/RIGHT_{LEFT,RIGHT}_WHITESPACE) — without
                // it text sits on the block edges. Applied before the layout
                // pass so width math sees the padded content.
                seg.content = format!(" {} ", seg.content);
                if let Some(cc) = &seg.compact_content {
                    seg.compact_content = Some(format!(" {} ", cc));
                }
                if resolved_style.gradient_ramp {
                    // Wave 1 gradient preset: stepped accent→magenta→cyan
                    // ramp across the segment run, dark text for contrast.
                    let t = if ramp_len <= 1 {
                        0.0
                    } else {
                        i as f32 / (ramp_len - 1) as f32
                    };
                    seg.bg = Some(self.palette.ramp_color(t).bg_escape());
                    seg.fg = self.palette.background.fg_escape();
                } else if resolved_style.rainbow {
                    let (bg, fg) = semantic_fill(&self.palette, &seg.name, &ctx);
                    seg.bg = Some(bg.bg_escape());
                    seg.fg = fg.fg_escape();
                } else {
                    seg.bg = Some(seg.fg.replace("[38;2;", "[48;2;"));
                    seg.fg = self.palette.background.fg_escape();
                }
            }
        }

        let sep_display_width = UnicodeWidthStr::width(resolved_style.left_separator.as_str()) as u16;
        // Frame-aware layout budget: the frame glyphs, boundary spaces and a
        // minimum gap live OUTSIDE the segments — reserve them, or the right
        // cap wraps past the terminal edge (p10k budgets the frame first).
        let frame_reserve: u16 = if resolved_style.frame.enabled && use_newline {
            (resolved_style.frame.left as u16) * 2
                + (resolved_style.frame.right as u16) * 2
                + 2
        } else {
            0
        };
        let layout = LayoutEngine::new_with_separator_width(
            cols.saturating_sub(frame_reserve),
            sep_display_width,
        );
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

        let left_segment_names: std::collections::HashSet<String> = resolved
            .iter()
            .map(|r| segments[r.original_index].name.to_string())
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
            // Cursor sits directly after the prompt char, p10k-style.
            format!("{title_escape}{prompt_start}{line1}\n{line2}{prompt_end}")
        } else {
            format!("{title_escape}{prompt_start}{line1} {line2}{prompt_end}")
        };

        // p10k PROMPT_ADD_NEWLINE: one blank line before each prompt so
        // consecutive commands breathe.
        let left = if self.config.prompt.blank_line {
            format!("\n{left}")
        } else {
            left
        };

        // Cursor shape per vi mode (DECSCUSR). Prepended to the prompt rather
        // than carried as a new protocol field: it is a zero-width control
        // sequence that must land before the user types, which is exactly
        // where the prompt already goes. Off by default.
        //
        // Not gated on a capability flag -- DECSCUSR predates all of these
        // terminals and every one of them honours it; a terminal that does
        // not simply ignores the sequence.
        let left = match self
            .config
            .terminal
            .cursor_shape
            .sequence_for_keymap(&ctx.env_get("vi_mode").unwrap_or_default())
        {
            // wrap_np, NOT raw. The adapter assigns this verbatim to PS1, and
            // readline counts any byte not inside \x01..\x02 as printable --
            // so an unwrapped 5-byte sequence puts the cursor column out by
            // five, wrapping long lines wrongly and corrupting Ctrl-R redraw.
            Some(seq) => format!("{}{left}", wrap_np(&seq)),
            None => left,
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

        // p10k renders the frame glyphs with no padding — ╰─❯ sits tight, and
        // the vertical bars align on every line.
        let frame_prefix = if style.frame.left {
            format!("{frame_fg}{top_left}{reset}")
        } else {
            String::new()
        };

        let bottom_prefix = if style.frame.left {
            format!("{frame_fg}{bottom_left}{reset}")
        } else {
            String::new()
        };

        if let (Some(gap_char), true) = (style.gap_char, style.frame.right) {
            let left_visible = strip_ansi_width(line1);
            let right_text = right_content.unwrap_or("");
            let right_visible = strip_ansi_width(right_text);

            // Fixed composition: prefix(2) + boundary space(1) + suffix(2)
            // + one trailing indent column. The right prompt adds its width
            // plus a separating space only when it exists — reserving width
            // for an absent right prompt shifted the top cap one column
            // left of the bottom cap.
            let right_width = if right_text.is_empty() { 0 } else { right_visible + 1 };
            let gap_width = (cols as usize)
                .saturating_sub(6 + left_visible + right_width);

            let gap_str: String = std::iter::repeat(gap_char).take(gap_width).collect();
            let gap_styled = match style.gap_gradient {
                GapGradient::Off => format!("{frame_fg}{gap_str}{reset}"),
                mode => self.gradient_gap(&gap_str, mode),
            };

            let right_frame = if style.frame.right {
                format!("{frame_fg}{top_right}{reset}")
            } else {
                String::new()
            };

            let right_part = if !right_text.is_empty() {
                format!(" {right_text}")
            } else {
                String::new()
            };

            // The typing line carries only the ╰─ prefix and the prompt
            // char — no bottom border, no gap fill. A full-width bottom
            // border puts the cursor past the terminal edge after PS1,
            // wrapping typed input onto the next line.
            format!(
                "{title_escape}{prompt_start}{frame_prefix}{line1} {gap_styled}{right_part}{right_frame}\n{bottom_prefix}{line2}{prompt_end}"
            )
        } else {
            let right_part = match right_content {
                Some(r) if !r.is_empty() => format!("  {r}"),
                _ => String::new(),
            };

            format!(
                "{title_escape}{prompt_start}{frame_prefix}{line1}{right_part}\n{bottom_prefix}{line2}{prompt_end}"
            )
        }
    }

    /// Wave 1: interpolate the gap fill between palette-derived endpoints,
    /// one truecolor SGR per 8-cell block. Gaps under 8 cells render solid
    /// accent. Foreground-only — frame budget math is untouched.
    fn gradient_gap(&self, gap_str: &str, mode: GapGradient) -> String {
        let width = gap_str.chars().count();
        let accent = self.palette.accent.fg_escape();
        if width < 8 {
            return format!("{accent}{gap_str}{}", RESET);
        }
        let (a, b) = self.palette.gap_gradient_endpoints(mode);
        let blocks = (width + 7) / 8;
        let mut out = String::new();
        for bi in 0..blocks {
            let start = bi * 8;
            let end = ((bi + 1) * 8).min(width);
            let t = ((start + end) as f32 / 2.0) / width as f32;
            out.push_str(&AnsiColor::lerp(&a, &b, t).fg_escape());
            out.push_str(&gap_str.chars().skip(start).take(end - start).collect::<String>());
        }
        out.push_str(&RESET.to_string());
        out
    }

    fn render_right(
        &self,
        ctx: &SegmentContext<'_>,
        left_names: &std::collections::HashSet<String>,
    ) -> Option<String> {
        let mut parts = Vec::new();

        // Configurable right rail ([prompt].right_segments); the default
        // ["command_duration", "git"] preserves the historical hardcoded pair.
        for seg in crate::layout::resolve_right_rail(&ctx.config.prompt.right_segments) {
            match seg {
                crate::layout::RightSegment::CommandDuration => {
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
                }
                crate::layout::RightSegment::Git => {
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
                }
                crate::layout::RightSegment::Time
                | crate::layout::RightSegment::Battery
                | crate::layout::RightSegment::Jobs => {
                    // Aux segments reuse their left-rail renderers for the
                    // gating and content; the rail styles them muted like
                    // command_duration.
                    let name = seg.name();
                    if left_names.contains(name) {
                        continue;
                    }
                    let built = match seg {
                        crate::layout::RightSegment::Time => crate::segments::time::render(ctx),
                        crate::layout::RightSegment::Battery => crate::segments::battery::render(ctx),
                        _ => crate::segments::jobs::render(ctx),
                    };
                    if let Some(s) = built {
                        parts.push(format!(
                            "{}{}{}",
                            wrap_np(&ctx.palette.muted.fg_escape()),
                            s.content,
                            wrap_np(RESET)
                        ));
                    }
                }
            }
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
        left_names: &std::collections::HashSet<String>,
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

/// p10k-rainbow semantic fills: (background, contrasting text color) per
/// segment, mirroring p10k's config/p10k-rainbow.zsh — dir blue with
/// near-white text, git green (yellow when dirty) with dark text, duration
/// dark with yellow text, jobs cyan, time white with dark text.
fn semantic_fill<'a>(
    p: &'a ThemePalette,
    name: &str,
    ctx: &crate::segments::SegmentContext,
) -> (&'a crate::theme::AnsiColor, &'a crate::theme::AnsiColor) {
    match name {
        "os" => (&p.muted, &p.bright_foreground),
        "ssh" => (&p.yellow, &p.background),
        "container" => (&p.cyan, &p.background),
        "directory" => (&p.blue, &p.bright_foreground),
        "git" => {
            if ctx.git_status.is_dirty() {
                (&p.yellow, &p.background)
            } else {
                (&p.green, &p.background)
            }
        }
        "python_env" | "toolchain" | "nix" => (&p.orange, &p.background),
        "ai" | "k8s" => (&p.magenta, &p.background),
        "exit_status" => (&p.red, &p.background),
        // Duration floats yellow text on the terminal background — p10k's
        // quiet treatment (bg 0 / fg 3).
        "command_duration" => (&p.background, &p.yellow),
        "jobs" => (&p.cyan, &p.background),
        "time" => (&p.bright_foreground, &p.background),
        "battery" => (&p.orange, &p.background),
        _ => (&p.accent, &p.background),
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
            Vec::new(),
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

#[cfg(test)]
mod cursor_shape_render_tests {
    use super::*;

    fn env_with(k: &str, v: &str) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert(k.to_string(), v.to_string());
        m
    }

    fn render(config: &Config, env: Option<&std::collections::HashMap<String, String>>) -> String {
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(config, &palette);
        let git = crate::git::GitStatus { is_repo: false, branch: "main".into(), ..Default::default() };
        renderer
            .render_with_ssh("/home/u/p", 0, 0, 120, 0, &git, false, Some(false), env, Vec::new())
            .left
    }

    #[test]
    fn nothing_is_emitted_while_disabled() {
        // The default. A prompt must not change the user's cursor style
        // just by being installed.
        let c = Config::default();
        let out = render(&c, Some(&env_with("vi_mode", "vi-command")));
        assert!(!out.contains(" q"), "unexpected DECSCUSR in: {out:?}");
    }

    #[test]
    fn normal_mode_gets_the_block_cursor() {
        let mut c = Config::default();
        c.terminal.cursor_shape.enabled = true;
        let out = render(&c, Some(&env_with("vi_mode", "vi-command")));
        assert!(out.contains("\x1b[2 q"), "expected steady block, got {out:?}");
    }

    #[test]
    fn insert_mode_gets_the_bar_cursor() {
        let mut c = Config::default();
        c.terminal.cursor_shape.enabled = true;
        let out = render(&c, Some(&env_with("vi_mode", "vi-insert")));
        assert!(out.contains("\x1b[6 q"), "expected steady bar, got {out:?}");
    }

    #[test]
    fn a_shell_with_no_vi_mode_still_gets_the_insert_shape() {
        // No vi_mode key at all -- emacs mode, or vanilla bash outside a
        // bind callback. The bar is the right default, not "no sequence".
        let mut c = Config::default();
        c.terminal.cursor_shape.enabled = true;
        let out = render(&c, None);
        assert!(out.contains("\x1b[6 q"), "got {out:?}");
    }

    #[test]
    fn the_sequence_is_wrapped_for_readline() {
        // Every escape in PS1 must sit inside \x01..\x02 or readline counts
        // its bytes as printable and mis-tracks the cursor column.
        let mut c = Config::default();
        c.terminal.cursor_shape.enabled = true;
        let out = render(&c, Some(&env_with("vi_mode", "vi-command")));
        assert!(
            out.starts_with("\x01\x1b[2 q\x02"),
            "DECSCUSR must be wrapped as non-printing, got {out:?}"
        );
    }

    #[test]
    fn the_sequence_precedes_the_prompt_itself() {
        // It must land before the user types, which means before everything.
        let mut c = Config::default();
        c.terminal.cursor_shape.enabled = true;
        let out = render(&c, Some(&env_with("vi_mode", "vi-command")));
        let seq_at = out.find("\x1b[2 q").expect("sequence present");
        assert_eq!(seq_at, 1, "DECSCUSR must be first, just inside \\x01");
    }
}

#[cfg(test)]
mod term_caps_plumbing_tests {
    use super::*;

    fn env_with(k: &str, v: &str) -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert(k.to_string(), v.to_string());
        m
    }

    fn render_with(env: Option<&std::collections::HashMap<String, String>>) -> String {
        let mut config = Config::default();
        // The directory segment carries the OSC 8 hyperlink that term_caps
        // gates, so this is the observable difference.
        config.directory.enabled = true;
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(&config, &palette);
        let git = crate::git::GitStatus { is_repo: false, branch: "main".into(), ..Default::default() };
        renderer
            .render_with_ssh("/home/u/p", 0, 0, 120, 0, &git, false, Some(false), env, Vec::new())
            .left
    }

    /// The regression this plumbing exists for. `TermCaps` was detected from
    /// the DAEMON's environment, which names whichever terminal happened to
    /// start it -- so a foot shell talking to a daemon spawned elsewhere got
    /// the wrong capability profile, and lost OSC 8 hyperlinks.
    #[test]
    fn capabilities_follow_the_shells_terminal_not_the_daemons() {
        let foot = render_with(Some(&env_with("O10K_TERM", "foot")));
        let unknown = render_with(Some(&env_with("O10K_TERM", "unknown")));

        // foot supports OSC 8; the conservative profile does not.
        assert!(
            foot.contains("\x1b]8;;"),
            "foot should get an OSC 8 hyperlink, got {foot:?}"
        );
        assert!(
            !unknown.contains("\x1b]8;;"),
            "an unidentified terminal must not get OSC 8, got {unknown:?}"
        );
    }

    #[test]
    fn ghostty_also_gets_hyperlinks() {
        let out = render_with(Some(&env_with("O10K_TERM", "ghostty")));
        assert!(out.contains("\x1b]8;;"), "got {out:?}");
    }

    #[test]
    fn a_request_without_the_channel_value_still_renders() {
        // Older adapters send no O10K_TERM; this must fall back, not panic.
        let _ = render_with(None);
        let _ = render_with(Some(&env_with("VIRTUAL_ENV", "/x")));
    }
}
