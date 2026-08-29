use crate::git::GitStatus;
use crate::layout::Segment;
use crate::render::wrap_np;
use crate::segments::SegmentContext;
use crate::style::GlyphCatalog;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    let git = ctx.git_status;
    if !git.is_repo {
        return None;
    }

    let mode = ctx.config.git.mode.as_str();

    let (content, compact) = match mode {
        "hidden" => return None,
        "compact" => (format_compact(git, ctx), None),
        "expanded" => {
            let expanded = format_expanded(git, ctx);
            let compact = format_compact(git, ctx);
            (expanded, Some(compact))
        }
        _ => {
            // "adaptive" — expanded when dirty, compact when clean
            if git.is_dirty() {
                let expanded = format_expanded(git, ctx);
                let compact = format_compact(git, ctx);
                (expanded, Some(compact))
            } else {
                (format_compact(git, ctx), None)
            }
        }
    };

    let show_stale = git.stale && ctx.config.git.stale_display;
    let (content, compact) = if show_stale {
        // Stale data (large repo / cold cache): honest placeholder instead of
        // possibly wrong counts — `<stale_icon> branch` in muted style.
        (format!("{} {}", ctx.config.git.stale_icon, git.branch), None)
    } else {
        (content, compact)
    };

    let fg = if show_stale {
        ctx.palette.muted.fg_escape()
    } else if git.conflicted > 0 {
        ctx.palette.red.fg_escape()
    } else if git.is_dirty() {
        ctx.palette.yellow.fg_escape()
    } else {
        ctx.palette.green.fg_escape()
    };

    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
    let compact_width = compact
        .as_ref()
        .map(|c| UnicodeWidthStr::width(c.as_str()) as u16)
        .unwrap_or(preferred_width);

    // OSC 8: link the segment to the normalized origin URL when the
    // terminal supports hyperlinks. Widths above are computed from the
    // plain text before wrapping, exactly as directory.rs does.
    let (content, compact) = if ctx.term_caps.has_osc8 {
        match git.remote.as_deref().and_then(normalize_remote_url) {
            Some(url) => (
                hyperlink(&content, &url),
                compact.map(|c| hyperlink(&c, &url)),
            ),
            None => (content, compact),
        }
    } else {
        (content, compact)
    };

    Some(Segment {
        name: "git",
        content,
        compact_content: compact,
        priority: 20,
        min_width: compact_width,
        preferred_width,
        hide_below_cols: 30,
        fg,
        bg: None,
        bold: false,
        separator: None,
    })
}


fn format_compact(git: &GitStatus, ctx: &SegmentContext<'_>) -> String {
    let mut parts = Vec::new();

    let branch_icon = GlyphCatalog::branch_icon(&ctx.config.git.branch_icon);
    let icon_prefix = if branch_icon.is_empty() {
        String::new()
    } else {
        format!("{branch_icon} ")
    };

    let branch_display = if git.is_detached {
        format!(":{}", &git.commit)
    } else if git.branch.is_empty() {
        "\u{2026}".to_string()
    } else {
        truncate_branch(&git.branch, 20)
    };
    parts.push(format!("{icon_prefix}{branch_display}"));

    if let Some(ref wt) = git.worktree {
        parts.push(format!(" {wt}"));
    }

    // Action state
    if let Some(ref action) = git.action {
        parts.push(format!("{action}"));
    }

    // Dirty indicator
    if git.conflicted > 0 {
        parts.push(format!("×{}", git.conflicted));
    } else if git.is_dirty() {
        let mut dirty_parts = Vec::new();
        if git.staged > 0 {
            dirty_parts.push(format!("+{}", git.staged));
        }
        if git.unstaged > 0 {
            dirty_parts.push(format!("!{}", git.unstaged));
        }
        if git.untracked > 0 {
            dirty_parts.push(format!("?{}", git.untracked));
        }
        parts.push(dirty_parts.join(""));
    } else {
        parts.push("✓".into());
    }

    format!(" {}", parts.join(" "))
}

fn format_expanded(git: &GitStatus, ctx: &SegmentContext<'_>) -> String {
    let mut parts = Vec::new();

    let branch_icon = GlyphCatalog::branch_icon(&ctx.config.git.branch_icon);
    let icon_prefix = if branch_icon.is_empty() {
        String::new()
    } else {
        format!("{branch_icon} ")
    };

    let branch_display = if git.is_detached {
        format!(":{}", &git.commit)
    } else if git.branch.is_empty() {
        "\u{2026}".to_string()
    } else {
        truncate_branch(&git.branch, 30)
    };
    parts.push(format!("{icon_prefix}{branch_display}"));

    if let Some(ref wt) = git.worktree {
        parts.push(format!(" {wt}"));
    }

    if let Some(ref action) = git.action {
        parts.push(format!("{action}"));
    }

    if git.ahead > 0 || git.behind > 0 {
        let mut ab = Vec::new();
        if git.ahead > 0 {
            ab.push(format!("⇡{}", git.ahead));
        }
        if git.behind > 0 {
            ab.push(format!("⇣{}", git.behind));
        }
        parts.push(ab.join(""));
    }

    if git.staged > 0 {
        parts.push(format!("+{}", git.staged));
    }
    if git.unstaged > 0 {
        parts.push(format!("!{}", git.unstaged));
    }
    if git.untracked > 0 {
        parts.push(format!("?{}", git.untracked));
    }
    if git.conflicted > 0 {
        parts.push(format!("×{}", git.conflicted));
    }
    if git.stashes > 0 {
        parts.push(format!("≡{}", git.stashes));
    }

    if !git.is_dirty() && git.ahead == 0 && git.behind == 0 && git.action.is_none() {
        parts.push("✓".into());
    }

    format!(" {}", parts.join(" "))
}

fn truncate_branch(branch: &str, max_len: usize) -> String {
    if branch.chars().count() <= max_len {
        branch.to_string()
    } else {
        let end = branch
            .char_indices()
            .nth(max_len - 1)
            .map(|(i, _)| i)
            .unwrap_or(branch.len());
        format!("{}…", &branch[..end])
    }
}

/// Normalize a git remote URL into a browsable https URL for OSC 8
/// hyperlinks:
/// - scp-like ssh (`[user@]host:path`) and `ssh://[user@]host[:port]/path`
///   become `https://host/path` (credentials and port dropped);
/// - `http(s)://` URLs pass through;
/// - a trailing `.git` suffix is stripped when a repo name remains;
/// - anything else (`git://`, `file://`, local paths) yields None, so the
///   branch renders plain.
fn normalize_remote_url(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_matches('"').trim_matches('\'');
    if raw.is_empty() {
        return None;
    }

    let host_path = if let Some(rest) = raw.strip_prefix("ssh://") {
        let rest = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
        let (host, path) = rest.split_once('/')?;
        // Drop a :port suffix if present.
        let host = host.split(':').next().unwrap_or(host);
        if host.is_empty() {
            return None;
        }
        format!("{host}/{path}")
    } else if raw.starts_with("https://") || raw.starts_with("http://") {
        let (_, rest) = raw.split_once("://")?;
        rest.to_string()
    } else if !raw.contains("://") {
        // scp-like ssh syntax. Requiring an explicit user@ keeps ordinary
        // local paths like "docs/notes" or "C:\\repo" from matching.
        let (user_host, path) = raw.split_once(':')?;
        let host = user_host.rsplit_once('@')?.1;
        if host.is_empty() || path.is_empty() {
            return None;
        }
        format!("{host}/{path}")
    } else {
        return None;
    };

    let host_path = match host_path.strip_suffix(".git") {
        Some(stripped) if !stripped.is_empty() && !stripped.ends_with('/') => {
            stripped.to_string()
        }
        // "host/.git" or ".git": stripping leaves no repo name, so there is
        // no browsable repo page — not linkable.
        Some(_) => return None,
        None => host_path,
    };
    // A remote with no path component ("https://host", "host/.git") has no
    // browsable repo page to link to.
    let has_path = host_path
        .split_once('/')
        .is_some_and(|(_, path)| !path.is_empty());
    if !has_path {
        return None;
    }
    Some(format!("https://{host_path}"))
}

/// Wrap `text` in an OSC 8 hyperlink to `url`, with readline non-printing
/// delimiters around both escapes. Bytes outside printable ASCII in `url`
/// are percent-encoded so nothing can truncate the escape sequence.
fn hyperlink(text: &str, url: &str) -> String {
    let mut safe = String::with_capacity(url.len());
    for byte in url.bytes() {
        match byte {
            0x21..=0x7E => safe.push(byte as char),
            _ => safe.push_str(&format!("%{byte:02X}")),
        }
    }
    format!(
        "{}{}{}",
        wrap_np(&format!("\x1b]8;;{safe}\x1b\\")),
        text,
        wrap_np("\x1b]8;;\x1b\\")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_ascii() {
        assert_eq!(truncate_branch("main", 20), "main");
        assert_eq!(truncate_branch("a-very-long-branch-name-here", 10), "a-very-lo…");
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        let branch = "功能/新しい機能ブランチ";
        let result = truncate_branch(branch, 6);
        assert_eq!(result, "功能/新し…");
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_emoji_branch() {
        let branch = "🚀🎉🔥feature";
        let result = truncate_branch(branch, 5);
        assert_eq!(result, "🚀🎉🔥f…");
    }

    #[test]
    fn test_empty_branch_display_compact() {
        let git = GitStatus {
            is_repo: true,
            branch: String::new(),
            stale: true,
            ..Default::default()
        };
        let ctx_config = crate::config::Config::default();
        let palette = crate::theme::ThemePalette::default();
        let term_caps = crate::terminal::TermCaps::default();
        let ctx = SegmentContext {
            cwd: "/tmp",
            home: "/home/test",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 80,
            jobs: 0,
            in_ssh: false,
            git_status: &git,
            config: &ctx_config,
            palette: &palette,
            term_caps: &term_caps,
            env: None,
        };
        let result = format_compact(&git, &ctx);
        assert!(result.contains("…"), "empty branch should show ellipsis, got: {result}");
    }

    #[test]
    fn test_stale_icon_renders_branch_placeholder() {
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            stale: true,
            ..Default::default()
        };
        let ctx_config = crate::config::Config::default();
        let palette = crate::theme::ThemePalette::default();
        let term_caps = crate::terminal::TermCaps::default();
        let ctx = SegmentContext {
            cwd: "/tmp",
            home: "/home/test",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 80,
            jobs: 0,
            in_ssh: false,
            git_status: &git,
            config: &ctx_config,
            palette: &palette,
            term_caps: &term_caps,
            env: None,
        };
        let seg = render(&ctx).expect("stale repo should still render the git segment");
        assert!(
            seg.content.starts_with("⟳ main"),
            "stale segment should render '<stale_icon> branch', got: {}",
            seg.content
        );
        assert_eq!(seg.fg, ctx.palette.muted.fg_escape());
    }

    #[test]
    fn test_fresh_repo_has_no_stale_icon() {
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            ..Default::default()
        };
        let ctx_config = crate::config::Config::default();
        let palette = crate::theme::ThemePalette::default();
        let term_caps = crate::terminal::TermCaps::default();
        let ctx = SegmentContext {
            cwd: "/tmp",
            home: "/home/test",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 80,
            jobs: 0,
            in_ssh: false,
            git_status: &git,
            config: &ctx_config,
            palette: &palette,
            term_caps: &term_caps,
            env: None,
        };
        let seg = render(&ctx).expect("fresh repo should render the git segment");
        assert!(!seg.content.contains('⟳'), "fresh segment must not carry the stale icon");
    }

    struct OscFixture {
        config: crate::config::Config,
        palette: crate::theme::ThemePalette,
        term_caps: crate::terminal::TermCaps,
    }

    impl OscFixture {
        fn new(has_osc8: bool) -> Self {
            Self {
                config: crate::config::Config::default(),
                palette: crate::theme::ThemePalette::default(),
                term_caps: crate::terminal::TermCaps {
                    has_osc8,
                    ..Default::default()
                },
            }
        }

        fn ctx<'a>(&'a self, git: &'a GitStatus) -> SegmentContext<'a> {
            SegmentContext {
                cwd: "/tmp",
                home: "/home/test",
                exit_code: 0,
                cmd_duration_ms: 0,
                cols: 120,
                jobs: 0,
                in_ssh: false,
                git_status: git,
                config: &self.config,
                palette: &self.palette,
                term_caps: &self.term_caps,
                env: None,
            }
        }
    }

    #[test]
    fn test_normalize_scp_like_ssh() {
        assert_eq!(
            normalize_remote_url("git@github.com:user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
        assert_eq!(
            normalize_remote_url("git@gitlab.com:group/sub/repo"),
            Some("https://gitlab.com/group/sub/repo".into())
        );
    }

    #[test]
    fn test_normalize_ssh_url_drops_user_and_port() {
        assert_eq!(
            normalize_remote_url("ssh://git@github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
        assert_eq!(
            normalize_remote_url("ssh://github.com:2222/user/repo"),
            Some("https://github.com/user/repo".into())
        );
    }

    #[test]
    fn test_normalize_https_passthrough_and_git_suffix() {
        assert_eq!(
            normalize_remote_url("https://github.com/user/repo.git"),
            Some("https://github.com/user/repo".into())
        );
        assert_eq!(
            normalize_remote_url("https://github.com/user/repo"),
            Some("https://github.com/user/repo".into())
        );
        assert_eq!(
            normalize_remote_url("https://github.com/user/repo.wiki.git"),
            Some("https://github.com/user/repo.wiki".into())
        );
    }

    #[test]
    fn test_normalize_rejects_unsupported_and_degenerate() {
        assert_eq!(normalize_remote_url("git://github.com/user/repo.git"), None);
        assert_eq!(normalize_remote_url("/srv/git/repo.git"), None);
        assert_eq!(normalize_remote_url("docs/notes"), None);
        assert_eq!(normalize_remote_url(""), None);
        assert_eq!(normalize_remote_url("git@github.com:"), None);
        assert_eq!(normalize_remote_url("https://github.com/.git"), None);
    }

    #[test]
    fn test_hyperlink_uses_st_terminator() {
        let out = hyperlink("main", "https://github.com/user/repo");
        let open = format!("\x01\x1b]8;;https://github.com/user/repo\x1b\\\x02");
        let close = format!("\x01\x1b]8;;\x1b\\\x02");
        assert!(
            out.starts_with(&open),
            "must open with OSC 8 params + ST, got: {out:?}"
        );
        assert!(
            out.ends_with(&close),
            "must close with empty OSC 8 + ST, got: {out:?}"
        );
        assert!(out.contains(&format!("{open}main{close}")));
    }

    #[test]
    fn test_hyperlink_percent_encodes_unsafe_bytes() {
        let out = hyperlink("main", "https://example.com/a b");
        assert!(
            out.contains("\x1b]8;;https://example.com/a%20b\x1b\\"),
            "space must be percent-encoded, got: {out:?}"
        );
    }

    #[test]
    fn test_render_links_branch_when_osc8_and_remote() {
        let fixture = OscFixture::new(true);
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            remote: Some("git@github.com:user/repo.git".into()),
            ..Default::default()
        };
        let ctx = fixture.ctx(&git);
        let seg = render(&ctx).expect("repo should render the git segment");
        assert!(
            seg.content.contains("\x1b]8;;https://github.com/user/repo\x1b\\"),
            "segment must carry the normalized remote link, got: {:?}",
            seg.content
        );
        // Widths are measured on the plain text, not the escape bytes: the
        // embedded hyperlink escapes must not inflate the reported width.
        assert!(
            (seg.preferred_width as usize) < seg.content.len(),
            "escape bytes must not be counted as display width: width={}, len={}",
            seg.preferred_width,
            seg.content.len()
        );
    }

    #[test]
    fn test_render_plain_without_remote_or_osc8() {
        let fixture = OscFixture::new(true);
        let git = GitStatus {
            is_repo: true,
            branch: "main".into(),
            ..Default::default()
        };
        let ctx = fixture.ctx(&git);
        let seg = render(&ctx).expect("repo should render the git segment");
        assert!(
            !seg.content.contains("\x1b]8;"),
            "no remote must mean no hyperlink, got: {:?}",
            seg.content
        );

        let git_linked = GitStatus {
            remote: Some("git@github.com:user/repo.git".into()),
            ..git.clone()
        };
        let plain_caps = OscFixture::new(false);
        let ctx = plain_caps.ctx(&git_linked);
        let seg = render(&ctx).expect("repo should render the git segment");
        assert!(
            !seg.content.contains("\x1b]8;"),
            "no OSC 8 capability must mean no hyperlink, got: {:?}",
            seg.content
        );
    }
}
