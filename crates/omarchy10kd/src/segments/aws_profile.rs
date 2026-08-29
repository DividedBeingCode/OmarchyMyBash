use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

/// Active AWS identity segment: pure env-tier detection over the 0.4 env
/// channel, so it cannot spawn anything and degrades to hidden.
/// Precedence: AWS_PROFILE > AWS_VAULT > AWS_DEFAULT_PROFILE.
pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.aws_profile.enabled {
        return None;
    }

    let profile = detect_profile(ctx)?;
    let icon = &ctx.config.segments.aws_profile.icon;
    let content = format!("{icon} {profile}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "aws_profile".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
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

fn detect_profile(ctx: &SegmentContext<'_>) -> Option<String> {
    for key in ["AWS_PROFILE", "AWS_VAULT", "AWS_DEFAULT_PROFILE"] {
        if let Some(v) = ctx.env_get(key) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
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
            env: Some(env),
        }
    }

    static GIT: LazyLock<GitStatus> = LazyLock::new(GitStatus::default);
    static CONFIG: LazyLock<Config> = LazyLock::new(Config::default);

    fn env_with(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_aws_profile_precedence() {
        let config = Config::default();
        let env = env_with(&[
            ("AWS_DEFAULT_PROFILE", "fallback"),
            ("AWS_VAULT", "vaultname"),
            ("AWS_PROFILE", "primary"),
        ]);
        assert_eq!(detect_profile(&make_ctx(&env, &config)).as_deref(), Some("primary"));
        let env = env_with(&[("AWS_DEFAULT_PROFILE", "fallback"), ("AWS_VAULT", "vaultname")]);
        assert_eq!(detect_profile(&make_ctx(&env, &config)).as_deref(), Some("vaultname"));
        let env = env_with(&[("AWS_DEFAULT_PROFILE", "fallback")]);
        assert_eq!(detect_profile(&make_ctx(&env, &config)).as_deref(), Some("fallback"));
    }

    #[test]
    fn test_hidden_without_aws_env() {
        let env = env_with(&[]);
        let config = Config::default();
        let ctx = make_ctx(&env, &config);
        assert!(render(&ctx).is_none());
    }

    #[test]
    fn test_enabled_renders_profile() {
        let env = env_with(&[("AWS_PROFILE", "prod")]);
        let mut config = Config::default();
        config.segments.aws_profile.enabled = true;
        let ctx = make_ctx(&env, &config);
        let seg = render(&ctx).expect("enabled + env must render");
        assert_eq!(&*seg.name, "aws_profile");
        assert!(seg.content.contains("prod"));
    }
}
