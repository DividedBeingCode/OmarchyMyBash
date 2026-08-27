use crate::layout::Segment;
use crate::render::wrap_np;
use crate::segments::SegmentContext;
use unicode_width::UnicodeWidthStr;
use std::path::Path;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    let path = ctx.cwd;
    let home = ctx.home;

    let display_path = if !home.is_empty()
        && std::path::Path::new(path).starts_with(std::path::Path::new(home))
    {
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

    let bold = ctx.config.directory.repo_root_style == "bold";

    let display_content = if ctx.term_caps.has_osc8 {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_default();
        let abs_path = ctx.cwd;
        let osc_open = format!("\x1b]8;;file://{hostname}{abs_path}\x1b\\");
        let osc_close = "\x1b]8;;\x1b\\";
        format!(
            "{}{}{}",
            wrap_np(&osc_open),
            content,
            wrap_np(osc_close)
        )
    } else {
        content.clone()
    };

    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
    let compact_width = UnicodeWidthStr::width(compact.as_str()) as u16;

    Some(Segment {
        name: "directory",
        content: display_content,
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
    let chars: Vec<char> = target.chars().collect();

    for len in 1..=chars.len() {
        let prefix: String = chars[..len].iter().collect();
        let is_unique = siblings.iter().all(|s| !s.starts_with(&prefix) || **s == target);
        if is_unique {
            return prefix;
        }
    }

    target.to_string()
}

mod hostname {
    use std::ffi::OsString;

    pub fn get() -> std::io::Result<OsString> {
        let mut buf = vec![0u8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(len);
        Ok(OsString::from(String::from_utf8_lossy(&buf).into_owned()))
    }

    mod libc {
        unsafe extern "C" {
            pub fn gethostname(name: *mut std::ffi::c_char, len: usize) -> std::ffi::c_int;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_substitution() {
        let path = "/home/ian/Code/omarchy10k";
        let home = "/home/ian";
        let display = if std::path::Path::new(path).starts_with(std::path::Path::new(home)) {
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

    #[test]
    fn test_home_prefix_not_path_aware_false_positive() {
        let path = "/home/ian2/projects";
        let home = "/home/ian";
        let matches = std::path::Path::new(path).starts_with(std::path::Path::new(home));
        assert!(!matches, "/home/ian2 should NOT match /home/ian");
    }

    #[test]
    fn test_unique_prefix_multibyte() {
        let result = unique_prefix("données", &["docs"], &[]);
        assert!(result.is_char_boundary(result.len()), "prefix must be valid UTF-8");
        assert!(!result.is_empty());
    }
}
