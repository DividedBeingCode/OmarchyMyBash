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
    "package_version", "dir_writable", "aws_profile", "docker_context",
    "kubectl_context", "terraform_workspace", "vpn", "gcloud_project",
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
            "cat" => "\u{f011b}",
            "penguin" => "\u{f0ec0}",
            "fox" => "\u{f0239}",
            "owl" => "\u{f03d2}",
            "duck" => "\u{f01e5}",
            "butterfly" => "\u{f1589}",
            "ladybug" => "\u{f082d}",
            "bee" => "\u{f0fa1}",
            "cow" => "\u{f019a}",
            "horse" => "\u{f15bf}",
            "pig" => "\u{f0401}",
            "sheep" => "\u{f0cc6}",
            "dog" => "\u{f0a43}",
            "rabbit" => "\u{f0907}",
            "turtle" => "\u{f0cd7}",
            "paw" => "\u{f03e9}",
            "fish" => "\u{f023a}",
            "frog" => "\u{edf8}",
            "dragon" => "\u{eef8}",
            "panda" => "\u{f03da}",
            "koala" => "\u{f173f}",
            "unicorn" => "\u{f15c2}",
            "teddy" => "\u{f18fb}",
            // Kaomoji (plain Unicode; width-ambiguous in some terminals —
            // the panel marks these experimental).
            "kaomoji_bear" => "\u{295}\u{2022}\u{1d25}\u{2022}\u{294}",
            "kaomoji_smile" => "(\u{25d5}\u{203f}\u{25d5})",
            "kaomoji_rage" => "(\u{256f}\u{00b0}\u{25a1}\u{00b0})\u{256f}",
            "kaomoji_relaxed" => "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}",
            "kaomoji_smirk" => "(\u{00ac}\u{203f}\u{00ac})",
            "kaomoji_disapprove" => "\u{ca0}_\u{ca0}",
            "kaomoji_happy" => "(\u{2022}\u{203f}\u{2022})",
            "kaomoji_soft" => "(\u{b4}\u{2022}\u{1d17}\u{2022}`)",
            "kaomoji_sleepy" => "( \u{2d8}\u{3c9}\u{2d8} )",
            "kaomoji_cheer" => "\u{30fd}(\u{2022}\u{203f}\u{2022})\u{30ce}",
            "kaomoji_shrug" => "\u{af}\\_(\u{30c4})_/\u{af}",
            // Wider bestiary and the Japan/Geek set. These were offered by the
            // Studio glyph browser long before the daemon knew them, so picking
            // one wrote a key that fell through `_ => key` and rendered as its
            // own name -- a prompt reading "snail" instead of a snail.
            "snail" => "\u{f1677}",
            "spider" => "\u{f11ea}",
            "snake" => "\u{f150e}",
            "bird" => "\u{f15c6}",
            "dolphin" => "\u{f18b4}",
            "shark" => "\u{f18ba}",
            "jellyfish" => "\u{f0f01}",
            "elephant" => "\u{f07c6}",
            "kangaroo" => "\u{f1558}",
            "donkey" => "\u{f07c2}",
            "rodent" => "\u{f1327}",
            "bat" => "\u{f0b5f}",
            "bone" => "\u{f00b9}",
            "egg" => "\u{f0aaf}",
            "feather" => "\u{f06d3}",
            "bug" => "\u{f00e4}",
            "squirrel" => "\u{eb58}",
            "ninja" => "\u{f0774}",
            "torii" => "\u{eee6}",
            "sushi" => "\u{e21a}",
            "noodles" => "\u{f117e}",
            "rice" => "\u{f07ea}",
            "tea" => "\u{f0d9e}",
            "fan" => "\u{f0210}",
            "mask" => "\u{f1023}",
            "drama" => "\u{f0d02}",
            "katana" => "\u{f18be}",
            "alien" => "\u{f089a}",
            "robot" => "\u{f1719}",
            "ghost" => "\u{f02a0}",
            "sakura" => "\u{f09f1}",
            "crown" => "\u{edeb}",
            "sword" => "\u{f04e5}",
            "emoticon" => "\u{f0c68}",
            "cool" => "\u{f0c6b}",
            "wink" => "\u{f0c78}",
            "heart" => "\u{f02d1}",
            "star" => "\u{f04ce}",
            _ => key,
        }
    }

    /// Vi NORMAL-mode prompt glyph: the left-pointing angle bracket, so the
    /// mode is visible at a glance. INSERT keeps `prompt_char`; success/error
    /// coloring is unchanged.
    pub fn prompt_char_normal() -> &'static str {
        "\u{276e}"
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
            ("cat", "\u{f011b}"),
            ("penguin", "\u{f0ec0}"),
            ("fox", "\u{f0239}"),
            ("owl", "\u{f03d2}"),
            ("duck", "\u{f01e5}"),
            ("butterfly", "\u{f1589}"),
            ("ladybug", "\u{f082d}"),
            ("bee", "\u{f0fa1}"),
            ("dog", "\u{f0a43}"),
            ("rabbit", "\u{f0907}"),
            ("turtle", "\u{f0cd7}"),
            ("paw", "\u{f03e9}"),
            ("fish", "\u{f023a}"),
            ("frog", "\u{edf8}"),
            ("dragon", "\u{eef8}"),
            ("panda", "\u{f03da}"),
            ("koala", "\u{f173f}"),
            ("unicorn", "\u{f15c2}"),
            ("teddy", "\u{f18fb}"),
            ("cow", "\u{f019a}"),
            ("horse", "\u{f15bf}"),
            ("pig", "\u{f0401}"),
            ("sheep", "\u{f0cc6}"),
            ("snail", "\u{f1677}"),
            ("spider", "\u{f11ea}"),
            ("snake", "\u{f150e}"),
            ("bird", "\u{f15c6}"),
            ("dolphin", "\u{f18b4}"),
            ("shark", "\u{f18ba}"),
            ("jellyfish", "\u{f0f01}"),
            ("elephant", "\u{f07c6}"),
            ("kangaroo", "\u{f1558}"),
            ("donkey", "\u{f07c2}"),
            ("rodent", "\u{f1327}"),
            ("bat", "\u{f0b5f}"),
            ("bone", "\u{f00b9}"),
            ("egg", "\u{f0aaf}"),
            ("feather", "\u{f06d3}"),
            ("bug", "\u{f00e4}"),
            ("squirrel", "\u{eb58}"),
        ]
    }

    /// Kaomoji: plain-Unicode multi-char strings, width-ambiguous in some
    /// terminals — the panel labels these experimental.
    /// Japan/Geek symbols offered by the Studio's glyph browser.
    pub fn available_symbol_chars() -> &'static [(&'static str, &'static str)] {
        &[
            ("ninja", "\u{f0774}"),
            ("torii", "\u{eee6}"),
            ("sushi", "\u{e21a}"),
            ("noodles", "\u{f117e}"),
            ("rice", "\u{f07ea}"),
            ("tea", "\u{f0d9e}"),
            ("fan", "\u{f0210}"),
            ("mask", "\u{f1023}"),
            ("drama", "\u{f0d02}"),
            ("katana", "\u{f18be}"),
            ("alien", "\u{f089a}"),
            ("robot", "\u{f1719}"),
            ("ghost", "\u{f02a0}"),
            ("sakura", "\u{f09f1}"),
            ("crown", "\u{edeb}"),
            ("sword", "\u{f04e5}"),
            ("emoticon", "\u{f0c68}"),
            ("cool", "\u{f0c6b}"),
            ("wink", "\u{f0c78}"),
            ("heart", "\u{f02d1}"),
            ("star", "\u{f04ce}"),
        ]
    }

    pub fn available_kaomoji_chars() -> &'static [(&'static str, &'static str)] {
        &[
            ("kaomoji_bear", "\u{295}\u{2022}\u{1d25}\u{2022}\u{294}"),
            ("kaomoji_smile", "(\u{25d5}\u{203f}\u{25d5})"),
            ("kaomoji_rage", "(\u{256f}\u{00b0}\u{25a1}\u{00b0})\u{256f}"),
            ("kaomoji_relaxed", "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}"),
            ("kaomoji_smirk", "(\u{00ac}\u{203f}\u{00ac})"),
            ("kaomoji_disapprove", "\u{ca0}_\u{ca0}"),
            ("kaomoji_happy", "(\u{2022}\u{203f}\u{2022})"),
            ("kaomoji_soft", "(\u{b4}\u{2022}\u{1d17}\u{2022}`)"),
            ("kaomoji_sleepy", "( \u{2d8}\u{3c9}\u{2d8} )"),
            ("kaomoji_cheer", "\u{30fd}(\u{2022}\u{203f}\u{2022})\u{30ce}"),
            ("kaomoji_shrug", "\u{af}\\_(\u{30c4})_/\u{af}"),
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

#[cfg(test)]
mod catalog_parity_tests {
    use super::*;

    /// The Studio hardcodes its own copies of these catalogs in QML, because
    /// a picker needs the list before it has a daemon connection. That is a
    /// reasonable trade, but it means the two can drift -- and drift here is
    /// invisible: `GlyphCatalog::separator` maps an unknown key to a space,
    /// which looks exactly like `none`, and `prompt_char` passes an unknown
    /// key straight through, which looks exactly like a literal glyph.
    ///
    /// The bug that motivated this: the prompt-character row wrote the GLYPH
    /// ("❯") where every other writer -- the glyph browser, every Look, the
    /// CLI -- writes the catalog KEY ("chevron"). The daemon tolerated it via
    /// `_ => key`, so the prompt still rendered, but applying a Look left the
    /// row with nothing selected.
    fn qml(name: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../quattro")
            .join(name);
        std::fs::read_to_string(p).unwrap_or_default()
    }

    /// Pull `key: "value"` pairs out of a named QML property block.
    fn keys_in_property(src: &str, property: &str) -> Vec<String> {
        let Some(start) = src.find(&format!("property var {property}:")) else {
            return Vec::new();
        };
        let tail = &src[start..];
        let Some(end) = tail.find("\n    ]") else {
            return Vec::new();
        };
        let block = &tail[..end];
        let mut out = Vec::new();
        for part in block.split("key:").skip(1) {
            let part = part.trim_start();
            if let Some(rest) = part.strip_prefix('"') {
                if let Some(close) = rest.find('"') {
                    out.push(rest[..close].to_string());
                }
            }
        }
        out
    }

    #[test]
    fn studio_prompt_chars_are_real_catalog_keys() {
        let src = qml("StudioPrompt.qml");
        if src.is_empty() {
            eprintln!("skipping: StudioPrompt.qml not found");
            return;
        }
        let keys = keys_in_property(&src, "promptChars");
        assert!(!keys.is_empty(), "found no prompt-char keys to check");

        let known: Vec<&str> = available_prompt_chars()
            .iter()
            .map(|(k, _)| *k)
            .collect();
        for k in &keys {
            assert!(
                known.contains(&k.as_str()),
                "StudioPrompt offers prompt char {k:?}, which is not in the \
                 daemon catalog. A glyph here instead of a key is the exact \
                 bug this test exists for."
            );
        }
    }

    /// Every key the glyph browser offers must resolve in the daemon.
    ///
    /// `prompt_char` passes an unknown key through as its own literal, so a
    /// browser entry keyed on the GLYPH still renders -- it just writes a
    /// value no Look, no CLI and no other picker recognises. Sixteen entries
    /// (the whole Prompt and Kaomoji categories) were keyed that way, so
    /// picking Chevron from the browser stored "❯" while the chip row above
    /// it stored "chevron", and the two never agreed on what was selected.
    #[test]
    fn glyph_browser_keys_all_resolve() {
        let src = qml("StudioPrompt.qml");
        if src.is_empty() {
            return;
        }
        let keys = keys_in_property(&src, "glyphCatalog");
        assert!(keys.len() > 20, "expected the full glyph catalog, got {}", keys.len());

        let mut bad = Vec::new();
        for k in &keys {
            // An identifier that resolves to itself is not in the catalog.
            if GlyphCatalog::prompt_char(k) == k.as_str() {
                bad.push(k.clone());
            }
        }
        assert!(
            bad.is_empty(),
            "{} glyph-browser key(s) do not resolve in the daemon and would \
             render as their own literal text:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    #[test]
    fn studio_separators_are_real_catalog_keys() {
        let src = qml("StudioPrompt.qml");
        if src.is_empty() {
            return;
        }
        let keys = keys_in_property(&src, "separators");
        assert!(!keys.is_empty(), "found no separator keys to check");

        let known: Vec<&str> = available_separators()
            .iter()
            .map(|(k, _)| *k)
            .collect();
        for k in &keys {
            assert!(
                known.contains(&k.as_str()),
                "StudioPrompt offers separator {k:?}, which the daemon does \
                 not know. An unknown separator renders as a bare space, \
                 indistinguishable from `none`."
            );
        }
    }

    #[test]
    fn studio_presets_match_the_daemon_list() {
        let src = qml("StudioPrompt.qml");
        if src.is_empty() {
            return;
        }
        // Presets are bare strings rather than {key, glyph} pairs.
        let Some(start) = src.find("property var presets:") else {
            panic!("presets property not found");
        };
        let tail = &src[start..];
        let block = &tail[..tail.find("\n    ]").unwrap_or(tail.len())];
        let offered: Vec<String> = block
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|s| s.to_string())
            .collect();
        assert!(!offered.is_empty(), "found no presets to check");

        let known = available_presets();
        for p in &offered {
            assert!(
                known.contains(&p.as_str()),
                "StudioPrompt offers preset {p:?}, which the daemon does not \
                 implement"
            );
        }
        // And the other direction: a preset the daemon gained but the Studio
        // never surfaced is dead to every user who does not edit TOML.
        for p in known {
            assert!(
                offered.iter().any(|o| o == p),
                "the daemon implements preset {p:?} but the Studio does not \
                 offer it"
            );
        }
    }

    #[test]
    fn the_studio_catalog_has_no_duplicate_keys() {
        // `dragon` shipped twice — once category "Animals", once "Japan" — so it
        // rendered twice in the grid.
        let src = qml("StudioPrompt.qml");
        let keys = keys_in_property(&src, "glyphCatalog");
        let mut seen = std::collections::BTreeSet::new();
        let dupes: Vec<&String> = keys.iter().filter(|k| !seen.insert((*k).clone())).collect();
        assert!(dupes.is_empty(), "duplicate glyph keys in the Studio catalog: {dupes:?}");
    }

    #[test]
    fn every_glyph_the_daemon_resolves_is_browsable() {
        // kaomoji_relaxed, kaomoji_smirk and kaomoji_disapprove were resolvable
        // but not listed — and the shipped rose-classic Look uses
        // kaomoji_disapprove, so a preset depended on a glyph the picker could
        // not show.
        let src = qml("StudioPrompt.qml");
        let listed: std::collections::BTreeSet<String> =
            keys_in_property(&src, "glyphCatalog").into_iter().collect();
        let missing: Vec<&str> = available_kaomoji_chars()
            .iter()
            .chain(available_symbol_chars())
            .map(|(k, _)| *k)
            .filter(|k| !listed.contains(*k))
            .collect();
        assert!(missing.is_empty(), "daemon resolves glyphs the Studio cannot show: {missing:?}");
    }
}

#[cfg(test)]
mod catalog_self_consistency_tests {
    use super::*;

    /// The `available_*` listings and `prompt_char` are two hand-maintained
    /// copies of the same data. They HAD drifted: `kaomoji_bear` was fixed in
    /// one and not the other, so the picker and the prompt disagreed.
    #[test]
    fn listings_agree_with_the_resolver() {
        let mut bad = Vec::new();
        for (key, glyph) in available_animal_chars()
            .iter()
            .chain(available_kaomoji_chars())
            .chain(available_symbol_chars())
            .chain(available_prompt_chars())
        {
            let resolved = GlyphCatalog::prompt_char(key);
            if resolved != *glyph {
                bad.push(format!("{key}: listing {glyph:?} vs resolver {resolved:?}"));
            }
        }
        assert!(
            bad.is_empty(),
            "{} catalog entr(y/ies) disagree between the listing and \
             prompt_char:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// A listed key that `prompt_char` does not know falls through `_ => key`
    /// and renders as its own literal name in the prompt.
    #[test]
    fn every_listed_key_actually_resolves() {
        for (key, _) in available_animal_chars()
            .iter()
            .chain(available_kaomoji_chars())
            .chain(available_symbol_chars())
        {
            assert_ne!(
                GlyphCatalog::prompt_char(key),
                *key,
                "{key} is offered but not resolved -- it would render as the \
                 literal text {key:?}"
            );
        }
    }
}
