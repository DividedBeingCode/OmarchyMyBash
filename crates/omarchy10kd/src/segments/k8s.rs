use crate::layout::Segment;
use super::SegmentContext;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.k8s.enabled {
        return None;
    }

    let display = parse_kube_context(ctx.home, ctx.config.segments.k8s.show_namespace)?;
    let content = format!("⎈ {display}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "k8s",
        content: content.clone(),
        compact_content: Some("⎈".to_string()),
        priority: 42,
        min_width: 2,
        preferred_width,
        hide_below_cols: 60,
        fg: ctx.palette.blue.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn kubeconfig_path(home: &str) -> PathBuf {
    std::env::var("KUBECONFIG")
        .ok()
        .and_then(|paths| {
            paths
                .split(':')
                .map(|p| expand_home(p.trim(), home))
                .find(|p| std::path::Path::new(p).exists())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".kube/config"))
}

fn expand_home(path: &str, home: &str) -> String {
    if path == "~" {
        home.to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        path.to_string()
    }
}

fn parse_kube_context(home: &str, show_namespace: bool) -> Option<String> {
    let path = kubeconfig_path(home);
    let content = std::fs::read_to_string(path).ok()?;

    let current_context = content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("current-context:")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })?;

    if !show_namespace {
        return Some(current_context);
    }

    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("name:") {
            if name.trim() == current_context {
                for j in (0..i).rev() {
                    let t = lines[j].trim();
                    if t.starts_with("- ") || t == "contexts:" {
                        break;
                    }
                    if let Some(ns) = t.strip_prefix("namespace:") {
                        let ns = ns.trim();
                        if !ns.is_empty() {
                            return Some(format!("{current_context}/{ns}"));
                        }
                    }
                }
                break;
            }
        }
    }

    Some(current_context)
}
