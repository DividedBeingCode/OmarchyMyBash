use crate::layout::Segment;
use crate::segments::SegmentContext;
use unicode_width::UnicodeWidthStr;
use std::path::Path;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    let path = ctx.cwd;
    let home = ctx.home;

    let display_path = if !home.is_empty() && path.starts_with(home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };

    let strategy = ctx.config.directory.strategy.as_str();
    let max_len = ctx.config.directory.max_length;

    let (content, compact) = match strategy {
        "full" => (display_path.clone(), display_path.clone()),
        "truncate" => {
            let truncated = truncate_path(&display_path, max_len);
            (truncated.clone(), truncated)
        }
        _ => {
            let compact = smart_truncate(&display_path, max_len, ctx.config.directory.repo_root_style.as_str());
            (display_path.clone(), compact)
        }
    };

    let bold = strategy != "truncate"
        || ctx.config.directory.repo_root_style == "bold";

    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
    let compact_width = UnicodeWidthStr::width(compact.as_str()) as u16;

    Some(Segment {
        name: "directory",
        content,
        compact_content: Some(compact),
        priority: 10,
        min_width: compact_width.min(10),
        preferred_width,
        hide_below_cols: 0,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold,
        separator: None,
    })
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    format!("{first}/\u{2026}/{last}")
}

fn smart_truncate(path: &str, max_len: usize, _repo_root_style: &str) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return path.to_string();
    }

    // Keep first component (~ or /) and last component, truncate middle
    let first = parts[0]; // "~" or ""
    let last = parts[parts.len() - 1];

    let mut result_parts = vec![first.to_string()];

    // Check if any middle component is a repo root
    let mut current_path = if first == "~" {
        std::env::var("HOME").unwrap_or_default()
    } else {
        String::new()
    };

    for (i, part) in parts.iter().enumerate().skip(1) {
        if i == parts.len() - 1 {
            result_parts.push(part.to_string());
            break;
        }

        current_path = format!("{current_path}/{part}");
        let is_repo_root = Path::new(&current_path).join(".git").exists();

        if is_repo_root {
            result_parts.push(part.to_string());
        } else {
            // Truncate to first unique character
            let truncated = unique_prefix(part, &parts[1..i], &parts[i + 1..parts.len() - 1]);
            result_parts.push(truncated);
        }
    }

    let result = result_parts.join("/");
    if result.len() <= max_len {
        result
    } else {
        // Last resort: keep just first + last
        format!("{first}/…/{last}")
    }
}

fn unique_prefix(target: &str, before: &[&str], after: &[&str]) -> String {
    let siblings: Vec<&&str> = before.iter().chain(after.iter()).collect();

    for len in 1..=target.len() {
        let prefix = &target[..len];
        let is_unique = siblings.iter().all(|s| !s.starts_with(prefix) || **s == target);
        if is_unique {
            return prefix.to_string();
        }
    }

    target.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_substitution() {
        let path = "/home/ian/Code/omarchy10k";
        let home = "/home/ian";
        let display = if path.starts_with(home) {
            format!("~{}", &path[home.len()..])
        } else {
            path.to_string()
        };
        assert_eq!(display, "~/Code/omarchy10k");
    }

    #[test]
    fn test_short_path_no_truncation() {
        assert_eq!(smart_truncate("~/Code", 40, "bold"), "~/Code");
    }
}
