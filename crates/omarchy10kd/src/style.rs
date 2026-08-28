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
    /// True powerline rendering: fill each segment with a background color.
    pub filled: bool,
    /// Rainbow flavor of `filled`: bg colors rotate accent/red/green/yellow/blue
    /// instead of using each segment's own color.
    pub rainbow: bool,
    /// Smooth-ramp flavor of `filled`: bg colors sample a two-stage
    /// accent → magenta → cyan lerp across the segment run.
    pub gradient_ramp: bool,
    /// Gap fill interpolation for framed prompts.
    pub gap_gradient: GapGradient,
}

/// Gap fill interpolation mode (Wave 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GapGradient {
    #[default]
    Off,
    Subtle,
    Full,
}

impl GapGradient {
    fn parse(v: Option<&str>) -> Self {
        match v {
            Some("subtle") => GapGradient::Subtle,
            Some("full") => GapGradient::Full,
            _ => GapGradient::Off,
        }
    }
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
    "ai", "k8s", "exit_status", "command_duration", "jobs", "time", "battery", "load",
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
    filled: bool,
    rainbow: bool,
    gradient_ramp: bool,
}

const PLAIN: (bool, bool) = (false, false);

fn preset_defaults(name: &str) -> PresetDefaults {
    match name {
        "powerline" => PresetDefaults {
            left_separator: " \u{e0b0} ",
            right_separator: " \u{e0b2} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: true,
            rainbow: false,
            gradient_ramp: false,
        },
        "rainbow" => PresetDefaults {
            left_separator: " \u{e0b0} ",
            right_separator: " \u{e0b2} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: true,
            rainbow: true,
            gradient_ramp: false,
        },
        "gradient" => PresetDefaults {
            left_separator: " \u{e0b1} ",
            right_separator: " \u{e0b3} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: true,
            rainbow: false,
            gradient_ramp: true,
        },
        "framed" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FRAME_ENABLED,
            gap_char: Some('\u{2500}'),
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "classic" => PresetDefaults {
            left_separator: " \u{2502} ",
            right_separator: " \u{2502} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: CLASSIC_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "lean" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "dense" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: DENSE_SEGMENTS,
            force_single_line: true,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "minimal" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: MINIMAL_SEGMENTS,
            force_single_line: true,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "pure" => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: PURE_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        "slanted" => PresetDefaults {
            left_separator: " \u{e0bc} ",
            right_separator: " \u{e0be} ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
        _ => PresetDefaults {
            left_separator: " ",
            right_separator: " ",
            frame: FrameStyle::default(),
            gap_char: None,
            segment_order: ALL_SEGMENTS,
            force_single_line: false,
            filled: PLAIN.0,
            rainbow: PLAIN.1,
            gradient_ramp: PLAIN.1,
        },
    }
}

pub struct StyleResolver;

impl StyleResolver {
    pub fn resolve(config: &Config) -> ResolvedStyle {
        let preset_name = Self::effective_preset(config);
        let defaults = preset_defaults(&preset_name);

        // Separator geometry family: a set `shape` drives both directions
        // together; explicit left/right keys still win for custom mixes.
        let shape = config
            .style
            .separators
            .shape
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "auto");
        let left_sep = config
            .style
            .separators
            .left
            .as_deref()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(|s| GlyphCatalog::separator(s).to_string())
            .or_else(|| shape.map(|k| GlyphCatalog::separator(k).to_string()))
            .unwrap_or_else(|| defaults.left_separator.to_string());

        let right_sep = config
            .style
            .separators
            .right
            .as_deref()
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(|s| GlyphCatalog::separator(s).to_string())
            .or_else(|| shape.map(|k| GlyphCatalog::separator(k).to_string()))
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
            filled: defaults.filled,
            rainbow: defaults.rainbow,
            gradient_ramp: defaults.gradient_ramp,
            gap_gradient: GapGradient::parse(config.style.frame.gap_gradient.as_deref()),
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
            // p10k's "blurred" look: brightness-stepped block shades drawn in
            // the previous segment's bg — an ink-coverage fade, no truecolor.
            "fade" => " \u{2593}\u{2592}\u{2591} ",
            "fade_rev" => " \u{2591}\u{2592}\u{2593} ",
            "trapezoid" => " \u{e0d2} ",
            "trapezoid_rev" => " \u{e0d5} ",
            "flame" => " \u{e0c0} ",
            "dither" => " \u{e0c4} ",
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
            // Animals (Nerd Font MDI/fa; verified against the NF v3 cmap).
            "cat" => "\u{f0b58}",
            "penguin" => "\u{f0752}",
            "fox" => "\u{f0f86}",
            "owl" => "\u{f1041}",
            "duck" => "\u{f095f}",
            "butterfly" => "\u{f10a9}",
            "ladybug" => "\u{f0828}",
            "bee" => "\u{f0fa1}",
            "cow" => "\u{f01e4}",
            "horse" => "\u{f0f12}",
            "pig" => "\u{f1045}",
            "sheep" => "\u{f1077}",
            "dog" => "\u{f094c}",
            "rabbit" => "\u{f0810}",
            "turtle" => "\u{f0be0}",
            "paw" => "\u{f02f2}",
            "fish" => "\u{f0143}",
            "frog" => "\u{ed01}",
            "dragon" => "\u{ee01}",
            "panda" => "\u{f02e3}",
            "koala" => "\u{f1648}",
            "unicorn" => "\u{f14cb}",
            "teddy" => "\u{f1804}",
            // Kaomoji (plain Unicode; width-ambiguous in some terminals —
            // the panel marks these experimental).
            "kaomoji_bear" => "\u{295}\u{2022}\u{1f425}\u{2022}\u{295}",
            "kaomoji_smile" => "(\u{25d5}\u{203f}\u{25d5})",
            "kaomoji_rage" => "(\u{256f}\u{00b0}\u{25a1}\u{00b0})\u{256f}",
            "kaomoji_relaxed" => "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}",
            "kaomoji_smirk" => "(\u{00ac}\u{203f}\u{00ac})",
            "kaomoji_disapprove" => "\u{ca0}_\u{ca0}",
            _ => key,
        }
    }

    pub fn branch_icon<'a>(key: &'a str) -> &'a str {
        match key {
            "powerline" => "\u{e0a0}",
            "octicon" => "\u{f418}",
            "nerd" => "\u{f126}",
            "paw" => "\u{f02f2}",
            "text" => "git:",
            "none" | "" => "",
            _ => key,
        }
    }
}

pub fn available_presets() -> &'static [&'static str] {
    &["omarchy", "powerline", "rainbow", "gradient", "framed", "classic", "lean", "dense", "slanted", "minimal", "pure"]
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
        ("fade", " \u{2593}\u{2592}\u{2591} "),
        ("fade_rev", " \u{2591}\u{2592}\u{2593} "),
        ("trapezoid", " \u{e0d2} "),
        ("trapezoid_rev", " \u{e0d5} "),
        ("flame", " \u{e0c0} "),
        ("dither", " \u{e0c4} "),
    ]
}

    /// Animals (Nerd Font, verified against the NF v3 cmap).
    pub fn available_animal_chars() -> &'static [(&'static str, &'static str)] {
        &[
            ("cat", "\u{f0b58}"),
            ("penguin", "\u{f0752}"),
            ("fox", "\u{f0f86}"),
            ("owl", "\u{f1041}"),
            ("duck", "\u{f095f}"),
            ("butterfly", "\u{f10a9}"),
            ("ladybug", "\u{f0828}"),
            ("bee", "\u{f0fa1}"),
            ("dog", "\u{f094c}"),
            ("rabbit", "\u{f0810}"),
            ("turtle", "\u{f0be0}"),
            ("paw", "\u{f02f2}"),
            ("fish", "\u{f0143}"),
            ("frog", "\u{ed01}"),
            ("dragon", "\u{ee01}"),
            ("panda", "\u{f02e3}"),
            ("koala", "\u{f1648}"),
            ("unicorn", "\u{f14cb}"),
            ("teddy", "\u{f1804}"),
            ("cow", "\u{f01e4}"),
            ("horse", "\u{f0f12}"),
            ("pig", "\u{f1045}"),
            ("sheep", "\u{f1077}"),
        ]
    }

    /// Kaomoji: plain-Unicode multi-char strings, width-ambiguous in some
    /// terminals — the panel labels these experimental.
    pub fn available_kaomoji_chars() -> &'static [(&'static str, &'static str)] {
        &[
            ("kaomoji_bear", "\u{295}\u{2022}\u{1f425}\u{2022}\u{295}"),
            ("kaomoji_smile", "(\u{25d5}\u{203f}\u{25d5})"),
            ("kaomoji_rage", "(\u{256f}\u{00b0}\u{25a1}\u{00b0})\u{256f}"),
            ("kaomoji_relaxed", "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}"),
            ("kaomoji_smirk", "(\u{00ac}\u{203f}\u{00ac})"),
            ("kaomoji_disapprove", "\u{ca0}_\u{ca0}"),
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
        // Built-in default is the p10k-style rainbow powerline.
        assert_eq!(style.left_separator, " \u{e0b0} ");
        assert!(style.filled);
        assert!(style.rainbow);
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
        // Layout takes precedence only when preset is explicitly "omarchy"
        // (configs written before style.preset existed). The built-in default
        // is now "rainbow", which resolves verbatim.
        config.style.preset = "omarchy".into();
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
