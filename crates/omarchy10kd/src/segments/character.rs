use crate::render::wrap_np;
use crate::segments::SegmentContext;
use crate::style::GlyphCatalog;

const RESET: &str = "\x1b[0m";
const UNDERCURL_ON: &str = "\x1b[4:3m";
const UNDERCURL_OFF: &str = "\x1b[4:0m";

pub fn render_prompt_char(ctx: &SegmentContext<'_>) -> String {
    let (symbol, color) = if ctx.exit_code == 0 {
        (
            GlyphCatalog::prompt_char(ctx.config.segments.character.success.as_str()),
            ctx.palette.accent.fg_escape(),
        )
    } else {
        (
            GlyphCatalog::prompt_char(ctx.config.segments.character.error.as_str()),
            ctx.palette.red.fg_escape(),
        )
    };

    if ctx.exit_code != 0 && ctx.term_caps.has_undercurl {
        format!(
            "{}{}{}{}{}",
            wrap_np(&color),
            wrap_np(UNDERCURL_ON),
            symbol,
            wrap_np(UNDERCURL_OFF),
            wrap_np(RESET)
        )
    } else {
        format!("{}{}{}", wrap_np(&color), symbol, wrap_np(RESET))
    }
}

pub fn render_transient_char(ctx: &SegmentContext<'_>) -> String {
    let symbol = GlyphCatalog::prompt_char(ctx.config.segments.character.transient.as_str());
    let color = ctx.palette.muted.fg_escape();
    format!("{}{}{}", wrap_np(&color), symbol, wrap_np(RESET))
}
