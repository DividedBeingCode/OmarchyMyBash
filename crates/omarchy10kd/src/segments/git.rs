use crate::git::GitStatus;
use crate::layout::Segment;
use crate::segments::SegmentContext;
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
            if is_dirty(git) {
                let expanded = format_expanded(git, ctx);
                let compact = format_compact(git, ctx);
                (expanded, Some(compact))
            } else {
                (format_compact(git, ctx), None)
            }
        }
    };

    let fg = if git.stale {
        ctx.palette.muted.fg_escape()
    } else if git.conflicted > 0 {
        ctx.palette.red.fg_escape()
    } else if is_dirty(git) {
        ctx.palette.yellow.fg_escape()
    } else {
        ctx.palette.green.fg_escape()
    };

    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
    let compact_width = compact
        .as_ref()
        .map(|c| UnicodeWidthStr::width(c.as_str()) as u16)
        .unwrap_or(preferred_width);

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

fn is_dirty(git: &GitStatus) -> bool {
    git.staged > 0 || git.unstaged > 0 || git.untracked > 0 || git.conflicted > 0
}

fn format_compact(git: &GitStatus, _ctx: &SegmentContext<'_>) -> String {
    let mut parts = Vec::new();

    // Branch or detached HEAD
    let branch_display = if git.is_detached {
        format!(":{}", &git.commit)
    } else {
        truncate_branch(&git.branch, 20)
    };
    parts.push(branch_display);

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
    } else if is_dirty(git) {
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

fn format_expanded(git: &GitStatus, _ctx: &SegmentContext<'_>) -> String {
    let mut parts = Vec::new();

    let branch_display = if git.is_detached {
        format!(":{}", &git.commit)
    } else {
        truncate_branch(&git.branch, 30)
    };
    parts.push(branch_display);

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

    if !is_dirty(git) && git.ahead == 0 && git.behind == 0 && git.action.is_none() {
        parts.push("✓".into());
    }

    format!(" {}", parts.join(" "))
}

fn truncate_branch(branch: &str, max_len: usize) -> String {
    if branch.len() <= max_len {
        branch.to_string()
    } else {
        format!("{}…", &branch[..max_len - 1])
    }
}
