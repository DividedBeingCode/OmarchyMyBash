use crate::segments::SegmentContext;
use crate::terminal::TermCaps;

const UNDERCURL_ON: &str = "\x1b[4:3m";
const UNDERCURL_OFF: &str = "\x1b[4:0m";

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

    if ctx.exit_code != 0 && TermCaps::detect().has_undercurl {
        format!("{color}{UNDERCURL_ON}{symbol}{UNDERCURL_OFF}{reset}")
    } else {
        format!("{color}{symbol}{reset}")
    }
}

pub fn render_transient_char(ctx: &SegmentContext<'_>) -> String {
    let reset = "\x1b[0m";
    let color = ctx.palette.muted.fg_escape();
    format!("{color}❯{reset}")
}
