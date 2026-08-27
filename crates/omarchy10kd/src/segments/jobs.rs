use crate::layout::Segment;
use super::SegmentContext;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.jobs.enabled {
        return None;
    }

    if ctx.jobs == 0 {
        return None;
    }

    let content = format!("\u{f013}\u{00d7}{}", ctx.jobs);
    let width = content.len() as u16;

    Some(Segment {
        name: "jobs",
        content,
        compact_content: Some(format!("\u{f013}{}", ctx.jobs)),
        priority: 45,
        min_width: 3,
        preferred_width: width,
        hide_below_cols: 50,
        fg: ctx.palette.blue.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}
