use crate::layout::Segment;
use crate::segments::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    let threshold = ctx.config.segments.command_duration.show_above_ms;
    if ctx.cmd_duration_ms < threshold {
        return None;
    }

    let content = format_duration(ctx.cmd_duration_ms);
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "command_duration".into(),
        content,
        compact_content: Some(format_duration_compact(ctx.cmd_duration_ms)),
        priority: 50,
        min_width: 4,
        preferred_width,
        hide_below_cols: 40,
        fg: ctx.palette.yellow.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let s = ms as f64 / 1000.0;
        format!("{s:.1}s")
    } else if ms < 3_600_000 {
        let m = ms / 60_000;
        let s = (ms % 60_000) / 1000;
        format!("{m}m{s}s")
    } else {
        let h = ms / 3_600_000;
        let m = (ms % 3_600_000) / 60_000;
        format!("{h}h{m}m")
    }
}

fn format_duration_compact(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{}s", ms / 1000)
    } else if ms < 3_600_000 {
        format!("{}m", ms / 60_000)
    } else {
        format!("{}h", ms / 3_600_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500ms");
        assert_eq!(format_duration(1500), "1.5s");
        assert_eq!(format_duration(65000), "1m5s");
        assert_eq!(format_duration(3_661_000), "1h1m");
    }
}
