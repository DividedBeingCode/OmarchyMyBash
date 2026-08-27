use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.container.enabled {
        return None;
    }

    let container_type = detect_container()?;
    let prefix = if ctx.config.segments.container.icon == "auto" {
        "⬡"
    } else {
        ctx.config.segments.container.icon.as_str()
    };

    let content = format!("{prefix} {container_type}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "container",
        content: content.clone(),
        compact_content: Some(prefix.to_string()),
        priority: 7,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn detect_container() -> Option<&'static str> {
    if std::env::var_os("DISTROBOX_ENTER_PATH").is_some() {
        return Some("distrobox");
    }
    if std::path::Path::new("/.dockerenv").exists() {
        return Some("docker");
    }
    if std::path::Path::new("/run/.containerenv").exists() {
        return Some("podman");
    }
    if std::env::var_os("container").is_some() {
        return Some("toolbox");
    }
    None
}
