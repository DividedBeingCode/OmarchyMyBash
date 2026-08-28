use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[derive(Debug, Clone)]
pub struct ThemePalette {
    pub accent: AnsiColor,
    pub foreground: AnsiColor,
    pub dark_foreground: AnsiColor,
    pub bright_foreground: AnsiColor,
    pub background: AnsiColor,
    pub muted: AnsiColor,
    pub red: AnsiColor,
    pub green: AnsiColor,
    pub yellow: AnsiColor,
    pub blue: AnsiColor,
    /// Extended roles for semantic segment fills. Derived from the primaries
    /// unless overridden via [theme.custom].
    pub magenta: AnsiColor,
    pub cyan: AnsiColor,
    pub orange: AnsiColor,
    pub is_dark: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AnsiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl AnsiColor {
    pub fn from_hex(hex: &str) -> Option<Self> {
        // Walk chars, not bytes, so multibyte input never panics on slicing.
        let hex = hex.trim_start_matches('#');
        let chars: Vec<char> = hex.chars().collect();
        if chars.len() != 6 {
            warn!("rejected invalid hex color '{hex}': expected 6 hex digits");
            return None;
        }
        let parse_pair = |pair: [char; 2]| -> Option<u8> {
            let s: String = pair.iter().collect();
            u8::from_str_radix(&s, 16).ok()
        };
        match (
            parse_pair([chars[0], chars[1]]),
            parse_pair([chars[2], chars[3]]),
            parse_pair([chars[4], chars[5]]),
        ) {
            (Some(r), Some(g), Some(b)) => Some(Self { r, g, b }),
            _ => {
                warn!("rejected invalid hex color '{hex}': expected hex digits");
                None
            }
        }
    }

    pub fn fg_escape(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// Linear interpolation a→b at t (clamped 0..=1). Wave 1 gradients.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (a.r as f32 + (b.r as f32 - a.r as f32) * t).round() as u8,
            g: (a.g as f32 + (b.g as f32 - a.g as f32) * t).round() as u8,
            b: (a.b as f32 + (b.b as f32 - a.b as f32) * t).round() as u8,
        }
    }

    /// Per-channel average — used to derive harmonized extended roles.
    pub fn blend(a: &Self, b: &Self) -> Self {
        Self {
            r: ((a.r as u16 + b.r as u16) / 2) as u8,
            g: ((a.g as u16 + b.g as u16) / 2) as u8,
            b: ((a.b as u16 + b.b as u16) / 2) as u8,
        }
    }

    pub fn bg_escape(&self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        Self {
            accent: AnsiColor { r: 122, g: 162, b: 247 },
            foreground: AnsiColor { r: 169, g: 177, b: 214 },
            dark_foreground: AnsiColor { r: 86, g: 95, b: 137 },
            bright_foreground: AnsiColor { r: 192, g: 202, b: 245 },
            background: AnsiColor { r: 26, g: 27, b: 38 },
            muted: AnsiColor { r: 65, g: 72, b: 104 },
            red: AnsiColor { r: 247, g: 118, b: 142 },
            green: AnsiColor { r: 158, g: 206, b: 106 },
            yellow: AnsiColor { r: 224, g: 175, b: 104 },
            blue: AnsiColor { r: 122, g: 162, b: 247 },
            magenta: AnsiColor { r: 187, g: 154, b: 247 },
            cyan: AnsiColor { r: 125, g: 207, b: 255 },
            orange: AnsiColor { r: 255, g: 158, b: 100 },
            is_dark: true,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OmarchyColors {
    #[serde(default = "default_mode")]
    mode: String,
    accent: Option<String>,
    foreground: Option<String>,
    dark_foreground: Option<String>,
    bright_foreground: Option<String>,
    background: Option<String>,
    muted: Option<String>,
    red: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    blue: Option<String>,
}

fn default_mode() -> String {
    "dark".into()
}

impl ThemePalette {
    /// Complement rule (Wave 1): blue ≥ red accents get magenta, warm
    /// accents get cyan. Deterministic from any palette.
    pub fn complement(&self) -> AnsiColor {
        if self.accent.b >= self.accent.r {
            self.magenta
        } else {
            self.cyan
        }
    }

    /// Wave 1 gap-gradient endpoints by mode.
    pub fn gap_gradient_endpoints(
        &self,
        mode: crate::style::GapGradient,
    ) -> (AnsiColor, AnsiColor) {
        use crate::style::GapGradient;
        match mode {
            GapGradient::Off => (self.accent, self.accent),
            GapGradient::Subtle => {
                (self.accent, AnsiColor::lerp(&self.accent, &self.background, 0.6))
            }
            GapGradient::Full => (self.accent, self.complement()),
        }
    }

    /// Two-stage stepped ramp accent → magenta → cyan, sampled at t ∈ [0,1].
    /// Wave 1 gradient preset.
    pub fn ramp_color(&self, t: f32) -> AnsiColor {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.5 {
            AnsiColor::lerp(&self.accent, &self.magenta, t * 2.0)
        } else {
            AnsiColor::lerp(&self.magenta, &self.cyan, (t - 0.5) * 2.0)
        }
    }

    pub fn omarchy_theme_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/state/omarchy/current/theme")
    }

    pub fn colors_toml_path() -> PathBuf {
        Self::omarchy_theme_dir().join("colors.toml")
    }

    pub fn load_omarchy() -> Self {
        let path = Self::colors_toml_path();
        match Self::load_from_file(&path) {
            Ok(palette) => {
                debug!("loaded omarchy theme from {}", path.display());
                palette
            }
            Err(e) => {
                warn!("failed to load omarchy theme: {e}, using defaults");
                Self::default()
            }
        }
    }

    fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let colors: OmarchyColors = if contents.contains("[colors]") {
            #[derive(Deserialize)]
            struct ColorsFile { colors: OmarchyColors }
            let file: ColorsFile = toml::from_str(&contents)?;
            file.colors
        } else {
            toml::from_str(&contents)?
        };
        let defaults = Self::default();

        let parse_or = |opt: Option<String>, fallback: &AnsiColor| -> AnsiColor {
            opt.and_then(|h| AnsiColor::from_hex(&h)).unwrap_or_else(|| fallback.clone())
        };

        let mut palette = Self {
            accent: parse_or(colors.accent, &defaults.accent),
            foreground: parse_or(colors.foreground, &defaults.foreground),
            dark_foreground: parse_or(colors.dark_foreground, &defaults.dark_foreground),
            bright_foreground: parse_or(colors.bright_foreground, &defaults.bright_foreground),
            background: parse_or(colors.background, &defaults.background),
            muted: parse_or(colors.muted, &defaults.muted),
            red: parse_or(colors.red, &defaults.red),
            green: parse_or(colors.green, &defaults.green),
            yellow: parse_or(colors.yellow, &defaults.yellow),
            blue: parse_or(colors.blue, &defaults.blue),
            magenta: defaults.magenta.clone(),
            cyan: defaults.cyan.clone(),
            orange: defaults.orange.clone(),
            is_dark: colors.mode != "light",
        };
        palette.derive_extended();
        Ok(palette)
    }

    pub fn resolve_palette(config: &crate::config::Config) -> Self {
        let source = config.theme.source.as_str();
        let mut palette = match source {
            "omarchy" => Self::load_omarchy(),
            "custom" => Self::default(),
            "hybrid" => Self::load_omarchy(),
            _ => Self::default(),
        };
        let mut explicit = [false; 3];
        if source != "omarchy" {
            if let Some(custom) = &config.theme.custom {
                explicit = palette.apply_custom_overrides(custom);
            }
        }
        palette.derive_extended_except(explicit);
        palette
    }

    /// Blends the primaries into the extended roles so magenta/cyan/orange
    /// always harmonize with the active theme.
    fn derive_extended(&mut self) {
        self.derive_extended_except([false; 3]);
    }

    fn derive_extended_except(&mut self, keep: [bool; 3]) {
        if !keep[0] {
            self.magenta = AnsiColor::blend(&self.red, &self.blue);
        }
        if !keep[1] {
            self.cyan = AnsiColor::blend(&self.blue, &self.green);
        }
        if !keep[2] {
            self.orange = AnsiColor::blend(&self.red, &self.yellow);
        }
    }

    /// Applies hex overrides; returns which extended roles were explicitly
    /// set ([magenta, cyan, orange]) so derivation can skip them.
    pub fn apply_custom_overrides(&mut self, custom: &crate::config::CustomPalette) -> [bool; 3] {
        let mut explicit = [false; 3];
        if let Some(h) = &custom.accent {
            if let Some(c) = AnsiColor::from_hex(h) { self.accent = c; }
        }
        if let Some(h) = &custom.foreground {
            if let Some(c) = AnsiColor::from_hex(h) { self.foreground = c; }
        }
        if let Some(h) = &custom.muted {
            if let Some(c) = AnsiColor::from_hex(h) { self.muted = c; }
        }
        if let Some(h) = &custom.background {
            if let Some(c) = AnsiColor::from_hex(h) { self.background = c; }
        }
        if let Some(h) = &custom.red {
            if let Some(c) = AnsiColor::from_hex(h) { self.red = c; }
        }
        if let Some(h) = &custom.green {
            if let Some(c) = AnsiColor::from_hex(h) { self.green = c; }
        }
        if let Some(h) = &custom.yellow {
            if let Some(c) = AnsiColor::from_hex(h) { self.yellow = c; }
        }
        if let Some(h) = &custom.blue {
            if let Some(c) = AnsiColor::from_hex(h) { self.blue = c; }
        }
        if let Some(h) = &custom.magenta {
            if let Some(c) = AnsiColor::from_hex(h) { self.magenta = c; explicit[0] = true; }
        }
        if let Some(h) = &custom.cyan {
            if let Some(c) = AnsiColor::from_hex(h) { self.cyan = c; explicit[1] = true; }
        }
        if let Some(h) = &custom.orange {
            if let Some(c) = AnsiColor::from_hex(h) { self.orange = c; explicit[2] = true; }
        }
        explicit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ThemeConfig, CustomPalette};

    #[test]
    fn test_resolve_omarchy_is_default() {
        let config = Config {
            theme: ThemeConfig { source: "omarchy".into(), custom: None },
            ..Config::default()
        };
        let palette = ThemePalette::resolve_palette(&config);
        assert!(palette.is_dark);
        assert_eq!(palette.accent.r, 122);
    }

    #[test]
    fn test_resolve_custom_applies_overrides() {
        let config = Config {
            theme: ThemeConfig {
                source: "custom".into(),
                custom: Some(CustomPalette {
                    accent: Some("#ff0000".into()),
                    foreground: None, muted: None, background: None,
                    red: None, green: None, yellow: None, blue: None,
                    magenta: None, cyan: None, orange: None,
                }),
            },
            ..Config::default()
        };
        let palette = ThemePalette::resolve_palette(&config);
        assert_eq!(palette.accent.r, 255);
        assert_eq!(palette.accent.g, 0);
        assert_eq!(palette.accent.b, 0);
    }

    #[test]
    fn test_resolve_hybrid_merges() {
        let config = Config {
            theme: ThemeConfig {
                source: "hybrid".into(),
                custom: Some(CustomPalette {
                    accent: Some("#00ff00".into()),
                    foreground: None, muted: None, background: None,
                    red: None, green: None, yellow: None, blue: None,
                    magenta: None, cyan: None, orange: None,
                }),
            },
            ..Config::default()
        };
        let palette = ThemePalette::resolve_palette(&config);
        assert_eq!(palette.accent.g, 255);
        assert!(palette.is_dark);
    }

    #[test]
    fn test_resolve_terminal_returns_default() {
        let config = Config {
            theme: ThemeConfig { source: "terminal".into(), custom: None },
            ..Config::default()
        };
        let palette = ThemePalette::resolve_palette(&config);
        let default_palette = ThemePalette::default();
        assert_eq!(palette.accent.r, default_palette.accent.r);
    }

    #[test]
    fn test_hex_parsing() {
        let c = AnsiColor::from_hex("#7aa2f7").unwrap();
        assert_eq!(c.r, 122);
        assert_eq!(c.g, 162);
        assert_eq!(c.b, 247);

        assert!(AnsiColor::from_hex("invalid").is_none());
        assert!(AnsiColor::from_hex("#fff").is_none());
    }
}
