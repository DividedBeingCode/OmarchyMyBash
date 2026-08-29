use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.python.enabled {
        return None;
    }

    let env_name = detect_python_env(ctx)?;
    let content = format!("🐍 {env_name}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "python_env".into(),
        content: content.clone(),
        compact_content: Some("🐍".to_string()),
        priority: 35,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.yellow.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn detect_python_env(ctx: &SegmentContext<'_>) -> Option<String> {
    if let Some(venv) = ctx.env_get("VIRTUAL_ENV") {
        let name = std::path::Path::new(&venv)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&venv);
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    if let Some(conda) = ctx.env_get("CONDA_DEFAULT_ENV") {
        if !conda.is_empty() {
            return Some(conda);
        }
    }

    None
}
