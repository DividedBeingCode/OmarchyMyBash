use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.toolchain.enabled {
        return None;
    }

    let parts = collect_tool_versions();
    if parts.is_empty() {
        return None;
    }

    let content = parts.join(" ");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "toolchain",
        content: content.clone(),
        compact_content: Some(parts.join("")),
        priority: 40,
        min_width: 2,
        preferred_width,
        hide_below_cols: 60,
        fg: ctx.palette.foreground.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn collect_tool_versions() -> Vec<String> {
    let tools: [(&str, &str, &str); 5] = [
        ("MISE_NODE_VERSION", "⬢", "node"),
        ("MISE_PYTHON_VERSION", "🐍", "python"),
        ("MISE_RUBY_VERSION", "💎", "ruby"),
        ("MISE_GO_VERSION", "🐹", "go"),
        ("MISE_RUST_VERSION", "🦀", "rust"),
    ];

    let mut parts = Vec::new();
    for (var, icon, _name) in tools {
        if let Ok(version) = std::env::var(var) {
            if !version.is_empty() {
                parts.push(format!("{icon} {version}"));
            }
        }
    }
    parts
}
