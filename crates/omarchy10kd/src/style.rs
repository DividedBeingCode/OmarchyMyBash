use crate::config::Config;

#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    pub left_separator: String,
    pub right_separator: String,
    pub frame: FrameStyle,
    pub gap_char: Option<char>,
    pub left_cap_start: String,
    pub left_cap_end: String,
    pub right_cap_start: String,
    pub right_cap_end: String,
    pub segment_order: &'static [&'static str],
    pub force_single_line: bool,
}

#[derive(Debug, Clone)]
pub struct FrameStyle {
    pub enabled: bool,
    pub left: bool,
    pub right: bool,
    pub top_left: &'static str,
    pub bottom_left: &'static str,
    pub top_right: &'static str,
    pub bottom_right: &'static str,
}

impl Default for FrameStyle {
    fn default() -> Self {
        Self {
            enabled: false,
            left: false,
            right: false,
            top_left: "",
            bottom_left: "",
            top_right: "",
            bottom_right: "",
        }
    }
}

const FRAME_ENABLED: FrameStyle = FrameStyle {
    enabled: true,
    left: true,
    right: true,
    top_left: "\u{256d}\u{2500}",
    bottom_left: "\u{2570}\u{2500}",
    top_right: "\u{2500}\u{256e}",
    bottom_right: "\u{2500}\u{256f}",
};

const ALL_SEGMENTS: &[&str] = &[
    "os", "ssh", "container", "directory", "git", "python_env", "toolchain", "nix",
    "k8s", "exit_status", "command_duration", "jobs", "time", "battery",
];

const CLASSIC_SEGMENTS: &[&str] = &["ssh", "directory", "git", "exit_status"];
const PURE_SEGMENTS: &[&str] = &["directory", "git"];
const MINIMAL_SEGMENTS: &[&str] = &["directory"];
const DENSE_SEGMENTS: &[&str] = &[
    "os", "ssh", "directory", "git", "exit_status", "command_duration", "jobs",
];

struct PresetDefaults {
    left_separator: &'static str,
    right_separator: &'static str,
    frame: FrameStyle,
    gap_char: Option<char>,
    segment_order: &'static [&'static str],
    force_single_line: bool,
}

fn preset_defaults(name: &str) -> PresetDefaults {
    match name {
        "powerline" => PresetDefaults {
            left_separator: " \u{e0b0} ",
            right_separator: " \u{e0b2} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
        "rainbow" => PresetDefaults {
            left_separator: " \u{e0b0} ",
            right_separator: " \u{e0b2} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
        "framed" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FRAME_ENABLED,
            gap_char: Some('\u{2500}'),
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
        "classic" => PresetDefaults {
            left_separator: " \u{2502} ",
            right_separator: " \u{2502} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: CLASSIC_SEGMENTS,
            force_single_line: false,
        },
        "lean" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
        "dense" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: DENSE_SEGMENTS,
            force_single_line: true,
        },
        "minimal" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: MINIMAL_SEGMENTS,
            force_single_line: true,
        },
        "pure" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: PURE_SEGMENTS,
            force_single_line: false,
        },
        "slanted" => PresetDefaults {
            left_separator: " \u{e0bc} ",
            right_separator: " \u{e0be} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
        _ => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
        },
    }
}

pub struct StyleResolver;

impl StyleResolver {
    pub fn resolve(config: &Config) -> ResolvedStyle {
        let preset_name = Self::effective_preset(config);
        let defaults = preset_defaults(&preset_name);

        let left_sep = config.style.separators.left.as_deref()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(|s| GlyphCatalog::separator(s).to_string())
            .unwrap_or_else(|| defaults.left_separator.to_string());

        let right_sep = config.style.separators.right.as_deref()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(|s| GlyphCatalog::separator(s).to_string())
            .unwrap_or_else(|| defaults.right_separator.to_string());

        let frame = if let Some(enabled) = config.style.frame.enabled {
            if enabled {
                FrameStyle {
                    enabled: true,
                    left: config.style.frame.left.unwrap_or(true),
                    right: config.style.frame.right.unwrap_or(true),
                    ..FRAME_ENABLED
                }
            } else {
                FrameStyle::default()
            }
        } else {
            defaults.frame
        };

        let gap_char = config.style.frame.gap_char.as_deref()
            .and_then(|s| if s.is_empty() || s == "none" { None } else { s.chars().next() })
            .or(if frame.enabled { defaults.gap_char } else { None });

        let left_cap_start = config.style.caps.left_start.clone().unwrap_or_default();
        let left_cap_end = config.style.caps.left_end.clone().unwrap_or_default();
        let right_cap_start = config.style.caps.right_start.clone().unwrap_or_default();
        let right_cap_end = config.style.caps.right_end.clone().unwrap_or_default();

        let force_single = defaults.force_single_line;

        ResolvedStyle {
            left_separator: left_sep,
            right_separator: right_sep,
            frame,
            gap_char,
            left_cap_start,
            left_cap_end,
            right_cap_start,
            right_cap_end,
            segment_order: defaults.segment_order,
            force_single_line: force_single,
        }
    }

    /// Determines the effective preset, honoring legacy `prompt.layout` when
    /// `style.preset` hasn't been explicitly overridden from its default.
    fn effective_preset(config: &Config) -> String {
        let style_preset = config.style.preset.as_str();
        let layout = config.prompt.layout.as_str();

        if style_preset != "omarchy" {
            return style_preset.to_string();
        }

        match layout {
            "omarchy" | "" => style_preset.to_string(),
            other => other.to_string(),
        }
    }
}

pub struct GlyphCatalog;

impl GlyphCatalog {
    pub fn os_icon<'a>(key: &'a str) -> Option<&'a str> {
        match key {
            "arch" => Some("\u{f303}"),
            "ubuntu" => Some("\u{f31b}"),
            "debian" => Some("\u{f306}"),
            "fedora" => Some("\u{f30a}"),
            "nixos" => Some("\u{f313}"),
            "macos" | "apple" => Some("\u{f179}"),
            "windows" => Some("\u{f17a}"),
            "linux" => Some("\u{f17c}"),
            "omarchy" => Some("\u{f312}"),
            "alpine" => Some("\u{f300}"),
            "void" => Some("\u{f32e}"),
            "gentoo" => Some("\u{f30d}"),
            "manjaro" => Some("\u{f312}"),
            "opensuse" => Some("\u{f314}"),
            "centos" => Some("\u{f304}"),
            "raspberry_pi" => Some("\u{f315}"),
            "none" => None,
            _ => Some(key),
        }
    }

    pub fn separator(key: &str) -> &'static str {
        match key {
            "powerline" => " \u{e0b0} ",
            "powerline_thin" => " \u{e0b1} ",
            "slanted" => " \u{e0bc} ",
            "round" => " \u{e0b4} ",
            "vertical" => " \u{2502} ",
            "dot" => " \u{b7} ",
            "diamond" => " \u{25c6} ",
            "none" | "" => " ",
            _ => " ",
        }
    }

    pub fn prompt_char<'a>(key: &'a str) -> &'a str {
        match key {
            "chevron" => "\u{276f}",
            "arrow" => "\u{279c}",
            "lambda" => "\u{3bb}",
            "dollar" => "$",
            "angle" => ">",
            "percent" => "%",
            "triangle" => "\u{25b6}",
            "hash" => "#",
            _ => key,
        }
    }

    pub fn branch_icon<'a>(key: &'a str) -> &'a str {
        match key {
            "powerline" => "\u{e0a0}",
            "octicon" => "\u{f418}",
            "nerd" => "\u{f126}",
            "text" => "git:",
            "none" | "" => "",
            _ => key,
        }
    }
}

pub fn available_presets() -> &'static [&'static str] {
    &["omarchy", "powerline", "rainbow", "framed", "classic", "lean", "dense", "slanted", "minimal", "pure"]
}

pub fn available_os_icons() -> &'static [(&'static str, &'static str)] {
    &[
        ("arch", "\u{f303}"),
        ("ubuntu", "\u{f31b}"),
        ("debian", "\u{f306}"),
        ("fedora", "\u{f30a}"),
        ("nixos", "\u{f313}"),
        ("macos", "\u{f179}"),
        ("windows", "\u{f17a}"),
        ("linux", "\u{f17c}"),
        ("omarchy", "\u{f312}"),
        ("alpine", "\u{f300}"),
        ("void", "\u{f32e}"),
        ("gentoo", "\u{f30d}"),
        ("manjaro", "\u{f312}"),
        ("opensuse", "\u{f314}"),
        ("centos", "\u{f304}"),
        ("raspberry_pi", "\u{f315}"),
    ]
}

pub fn available_separators() -> &'static [(&'static str, &'static str)] {
    &[
        ("none", " "),
        ("powerline", " \u{e0b0} "),
        ("powerline_thin", " \u{e0b1} "),
        ("slanted", " \u{e0bc} "),
        ("round", " \u{e0b4} "),
        ("vertical", " \u{2502} "),
        ("dot", " \u{b7} "),
        ("diamond", " \u{25c6} "),
    ]
}

pub fn available_prompt_chars() -> &'static [(&'static str, &'static str)] {
    &[
        ("chevron", "\u{276f}"),
        ("arrow", "\u{279c}"),
        ("lambda", "\u{3bb}"),
        ("dollar", "$"),
        ("angle", ">"),
        ("percent", "%"),
        ("triangle", "\u{25b6}"),
        ("hash", "#"),
    ]
}

pub fn available_branch_icons() -> &'static [(&'static str, &'static str)] {
    &[
        ("powerline", "\u{e0a0}"),
        ("octicon", "\u{f418}"),
        ("nerd", "\u{f126}"),
        ("text", "git:"),
        ("none", ""),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_preset_resolves() {
        let config = Config::default();
        let style = StyleResolver::resolve(&config);
        assert_eq!(style.left_separator, " ");
        assert!(!style.frame.enabled);
        assert!(!style.force_single_line);
    }

    #[test]
    fn test_powerline_preset() {
        let mut config = Config::default();
        config.style.preset = "powerline".into();
        let style = StyleResolver::resolve(&config);
        assert!(style.left_separator.contains('\u{e0b0}'));
    }

    #[test]
    fn test_framed_preset_enables_frame() {
        let mut config = Config::default();
        config.style.preset = "framed".into();
        let style = StyleResolver::resolve(&config);
        assert!(style.frame.enabled);
        assert!(style.gap_char.is_some());
        assert_eq!(style.gap_char, Some('\u{2500}'));
    }

    #[test]
    fn test_separator_override() {
        let mut config = Config::default();
        config.style.separators.left = Some("vertical".into());
        let style = StyleResolver::resolve(&config);
        assert!(style.left_separator.contains('\u{2502}'));
    }

    #[test]
    fn test_glyph_catalog_os_icons() {
        assert_eq!(GlyphCatalog::os_icon("arch"), Some("\u{f303}"));
        assert_eq!(GlyphCatalog::os_icon("macos"), Some("\u{f179}"));
        assert_eq!(GlyphCatalog::os_icon("apple"), Some("\u{f179}"));
        assert_eq!(GlyphCatalog::os_icon("none"), None);
        assert_eq!(GlyphCatalog::os_icon("X"), Some("X"));
    }

    #[test]
    fn test_glyph_catalog_prompt_chars() {
        assert_eq!(GlyphCatalog::prompt_char("chevron"), "\u{276f}");
        assert_eq!(GlyphCatalog::prompt_char("lambda"), "\u{3bb}");
        assert_eq!(GlyphCatalog::prompt_char("custom!"), "custom!");
    }

    #[test]
    fn test_dense_forces_single_line() {
        let mut config = Config::default();
        config.style.preset = "dense".into();
        let style = StyleResolver::resolve(&config);
        assert!(style.force_single_line);
    }

    #[test]
    fn test_minimal_forces_single_line() {
        let mut config = Config::default();
        config.style.preset = "minimal".into();
        let style = StyleResolver::resolve(&config);
        assert!(style.force_single_line);
        assert_eq!(style.segment_order, &["directory"]);
    }

    #[test]
    fn test_frame_config_override() {
        let mut config = Config::default();
        config.style.frame.enabled = Some(true);
        config.style.frame.gap_char = Some("\u{b7}".into());
        let style = StyleResolver::resolve(&config);
        assert!(style.frame.enabled);
        assert_eq!(style.gap_char, Some('\u{b7}'));
    }

    #[test]
    fn test_legacy_layout_migration() {
        let mut config = Config::default();
        config.prompt.layout = "powerline".into();
        // style.preset is still default "omarchy", so layout should take precedence
        let style = StyleResolver::resolve(&config);
        assert!(style.left_separator.contains('\u{e0b0}'));
    }

    #[test]
    fn test_style_preset_overrides_legacy_layout() {
        let mut config = Config::default();
        config.prompt.layout = "powerline".into();
        config.style.preset = "framed".into();
        // style.preset is explicitly set, should take precedence over layout
        let style = StyleResolver::resolve(&config);
        assert!(style.frame.enabled);
    }

    #[test]
    fn test_branch_icon_catalog() {
        assert_eq!(GlyphCatalog::branch_icon("powerline"), "\u{e0a0}");
        assert_eq!(GlyphCatalog::branch_icon("octicon"), "\u{f418}");
        assert_eq!(GlyphCatalog::branch_icon("none"), "");
        assert_eq!(GlyphCatalog::branch_icon("text"), "git:");
        assert_eq!(GlyphCatalog::branch_icon("custom-icon"), "custom-icon");
    }
}
