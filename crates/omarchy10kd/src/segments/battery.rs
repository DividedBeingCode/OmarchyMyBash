use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.battery.enabled {
        return None;
    }

    let (capacity, charging) = read_battery()?;
    let show_above = ctx.config.segments.battery.show_above;
    if capacity > show_above {
        return None;
    }

    let icon = if charging { "🔌" } else { "🔋" };
    let content = format!("{icon} {capacity}%");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    let cfg = &ctx.config.segments.battery;
    let fg = if capacity <= cfg.threshold_critical {
        ctx.palette.red.fg_escape()
    } else if capacity <= cfg.threshold_warning {
        ctx.palette.yellow.fg_escape()
    } else {
        ctx.palette.green.fg_escape()
    };

    Some(Segment {
        name: "battery".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 56,
        min_width: 3,
        preferred_width,
        hide_below_cols: 40,
        fg,
        bg: None,
        bold: false,
        separator: None,
    })
}

pub fn read_battery() -> Option<(u32, bool)> {
    for bat in ["BAT0", "BAT1"] {
        let base = format!("/sys/class/power_supply/{bat}");
        let capacity_path = format!("{base}/capacity");
        let status_path = format!("{base}/status");

        if !std::path::Path::new(&capacity_path).exists() {
            continue;
        }

        let capacity_str = std::fs::read_to_string(&capacity_path).ok()?;
        let capacity: u32 = capacity_str.trim().parse().ok()?;

        let charging = std::fs::read_to_string(&status_path)
            .map(|s| s.trim().eq_ignore_ascii_case("Charging"))
            .unwrap_or(false);

        return Some((capacity, charging));
    }
    None
}
