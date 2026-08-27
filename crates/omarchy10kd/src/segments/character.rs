use crate::segments::SegmentContext;

pub fn render_prompt_char(ctx: &SegmentContext<'_>) -> String {
    let reset = "\x1b[0m";

    let (symbol, color) = if ctx.exit_code == 0 {
        (
            ctx.config.segments.character.success.as_str(),
            ctx.palette.accent.fg_escape(),
        )
    } else {
        (
            ctx.config.segments.character.error.as_str(),
            ctx.palette.red.fg_escape(),
        )
    };

    format!("{color}{symbol}{reset}")
}

pub fn render_transient_char(ctx: &SegmentContext<'_>) -> String {
    let reset = "\x1b[0m";
    let color = ctx.palette.muted.fg_escape();
    format!("{color}❯{reset}")
}
