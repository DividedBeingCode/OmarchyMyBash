use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

/// Agent signal segment (v0.4 1.3): one glyph + tool name while an AI coding
/// agent is active in the session. Detection is env-var-only via the 0.1 env
/// channel (`ctx.env_get`), so it degrades to hidden and cannot break.
///
/// Note: `CLAUDE_CODE_ENTRYPOINT` and the `CODEX_*` keys are not part of the
/// frozen 12-key default allowlist — add them to `[env.watch] keys` (or rely
/// on the daemon-environment fallback) for reliable detection.
pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.ai.enabled {
        return None;
    }

    let (glyph, tool) = detect_agent(ctx)?;
    let content = format!("{glyph} {tool}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "ai".into(),
        content: content.clone(),
        compact_content: Some(glyph.to_string()),
        priority: 38,
        min_width: 2,
        preferred_width,
        hide_below_cols: ctx.config.segments.ai.hide_below_cols,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn detect_agent(ctx: &SegmentContext<'_>) -> Option<(char, &'static str)> {
    // Claude Code exports the entrypoint used to launch it.
    if ctx.env_get("CLAUDE_CODE_ENTRYPOINT").is_some() {
        return Some(('\u{2726}', "claude"));
    }
    // OpenAI Codex CLI: sandbox/home vars only exist inside a codex session.
    if ctx.env_get("CODEX_SANDBOX").is_some() || ctx.env_get("CODEX_HOME").is_some() {
        return Some(('\u{2733}', "codex"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitStatus;
    use crate::terminal::TermCaps;
    use crate::theme::ThemePalette;
    use std::collections::HashMap;
    use std::sync::LazyLock;

    static THEME: LazyLock<ThemePalette> = LazyLock::new(ThemePalette::default);
    static CAPS: LazyLock<TermCaps> = LazyLock::new(TermCaps::detect);

    fn make_ctx<'a>(
        env: &'a HashMap<String, String>,
        config: &'a Config,
        git_status: &'a GitStatus,
    ) -> SegmentContext<'a> {
        SegmentContext {
            cwd: "/tmp",
            home: "/home/u",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status,
            config,
            palette: &THEME,
            term_caps: &CAPS,
            env: Some(env),
        }
    }

    fn env_with(key: &str) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert(key.to_string(), "1".to_string());
        env
    }

    #[test]
    fn test_claude_entrypoint_detected() {
        let env = env_with("CLAUDE_CODE_ENTRYPOINT");
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(&env, &config, &git);
        let seg = render(&ctx).expect("claude env should activate the segment");
        assert_eq!(&*seg.name, "ai");
        assert!(seg.content.contains("claude"));
    }

    #[test]
    fn test_codex_detected() {
        let env = env_with("CODEX_SANDBOX");
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(&env, &config, &git);
        let seg = render(&ctx).expect("codex env should activate the segment");
        assert!(seg.content.contains("codex"));
    }

    #[test]
    fn test_hidden_without_agent_env() {
        let env = HashMap::new();
        let config = Config::default();
        let git = GitStatus::default();
        let ctx = make_ctx(&env, &config, &git);
        assert!(render(&ctx).is_none());
    }

    #[test]
    fn test_disabled_config_hides_segment() {
        let env = env_with("CLAUDE_CODE_ENTRYPOINT");
        let mut config = Config::default();
        config.segments.ai.enabled = false;
        let git = GitStatus::default();
        let ctx = make_ctx(&env, &config, &git);
        assert!(render(&ctx).is_none());
    }
}
