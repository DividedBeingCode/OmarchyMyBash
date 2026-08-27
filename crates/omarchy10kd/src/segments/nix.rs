use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.nix.enabled {
        return None;
    }

    let shell_type = std::env::var("IN_NIX_SHELL").ok()?;
    let label = match shell_type.as_str() {
        "pure" | "impure" => shell_type,
        _ => return None,
    };

    let content = format!("❄ {label}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "nix",
        content: content.clone(),
        compact_content: Some("❄".to_string()),
        priority: 36,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.blue.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}
