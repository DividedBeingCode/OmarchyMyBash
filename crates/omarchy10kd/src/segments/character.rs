use crate::render::wrap_np;
use crate::segments::SegmentContext;
use crate::style::GlyphCatalog;

const RESET: &str = "\x1b[0m";
const UNDERCURL_ON: &str = "\x1b[4:3m";
const UNDERCURL_OFF: &str = "\x1b[4:0m";

pub fn render_prompt_char(ctx: &SegmentContext<'_>) -> String {
    // Vi NORMAL mode (opt-in via config): bash/ble.sh reports KEYMAP through
    // the env channel as "vi_mode"; anything starting with 'n' is NORMAL.
    let in_vi_normal = ctx.config.segments.character.vi_mode
        && ctx
            .env_get("vi_mode")
            .is_some_and(|v| v.starts_with('n') || v.starts_with('N'));
    let (symbol, color) = if ctx.exit_code == 0 {
        (
            if in_vi_normal {
                GlyphCatalog::prompt_char_normal()
            } else {
                GlyphCatalog::prompt_char(ctx.config.segments.character.success.as_str())
            },
            ctx.palette.accent.fg_escape(),
        )
    } else {
        (
            if in_vi_normal {
                GlyphCatalog::prompt_char_normal()
            } else {
                GlyphCatalog::prompt_char(ctx.config.segments.character.error.as_str())
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitStatus;
    use crate::terminal::TermCaps;
    use crate::theme::ThemePalette;
    use std::collections::HashMap;

    static GIT: std::sync::LazyLock<GitStatus> = std::sync::LazyLock::new(GitStatus::default);
    static THEME: std::sync::LazyLock<ThemePalette> = std::sync::LazyLock::new(ThemePalette::default);
    static CAPS: std::sync::LazyLock<TermCaps> = std::sync::LazyLock::new(TermCaps::default);

    fn make_ctx<'a>(
        env: Option<&'a HashMap<String, String>>,
        config: &'a Config,
    ) -> SegmentContext<'a> {
        SegmentContext {
            cwd: "/tmp",
            home: "/home/u",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: &GIT,
            config,
            palette: &THEME,
            term_caps: &CAPS,
            env,
        }
    }

    fn env_with(key: &str, value: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert(key.to_string(), value.to_string());
        map
    }

    #[test]
    fn test_vi_mode_off_renders_default_char() {
        let config = Config::default();
        let env = env_with("vi_mode", "normal");
        let out = render_prompt_char(&make_ctx(Some(&env), &config));
        assert!(
            out.contains('\u{276f}'),
            "vi_mode off must keep the default prompt char, got: {out}"
        );
        assert!(!out.contains('\u{276e}'), "unexpected normal glyph, got: {out}");
    }

    #[test]
    fn test_vi_mode_normal_renders_normal_glyph() {
        let mut config = Config::default();
        config.segments.character.vi_mode = true;
        let env = env_with("vi_mode", "normal");
        let out = render_prompt_char(&make_ctx(Some(&env), &config));
        assert!(
            out.contains('\u{276e}'),
            "vi NORMAL should render the normal glyph, got: {out}"
        );
        assert!(!out.contains('\u{276f}'), "unexpected insert glyph, got: {out}");
    }

    #[test]
    fn test_vi_mode_insert_or_absent_keeps_default_char() {
        let mut config = Config::default();
        config.segments.character.vi_mode = true;

        let insert_env = env_with("vi_mode", "insert");
        let out = render_prompt_char(&make_ctx(Some(&insert_env), &config));
        assert!(
            out.contains('\u{276f}'),
            "vi INSERT should keep the default char, got: {out}"
        );

        let empty = HashMap::new();
        let out = render_prompt_char(&make_ctx(Some(&empty), &config));
        assert!(
            out.contains('\u{276f}') && !out.contains('\u{276e}'),
            "missing vi_mode env should keep the default char, got: {out}"
        );
    }
}
