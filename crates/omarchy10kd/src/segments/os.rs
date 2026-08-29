use crate::layout::Segment;
use crate::style::GlyphCatalog;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.os.enabled {
        return None;
    }

    let icon = match GlyphCatalog::os_icon(ctx.config.segments.os.icon.as_str()) {
        Some(i) => i,
        None => return None,
    };

    Some(Segment {
        name: "os".into(),
        content: icon.to_string(),
        compact_content: Some(icon.to_string()),
        priority: 5,
        min_width: 2,
        preferred_width: UnicodeWidthStr::width(icon) as u16,
        hide_below_cols: 40,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}
