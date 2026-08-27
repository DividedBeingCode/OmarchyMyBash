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
    pub is_dark: bool,
}

#[derive(Debug, Clone)]
pub struct AnsiColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl AnsiColor {
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    pub fn fg_escape(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
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
        let colors: OmarchyColors = toml::from_str(&contents)?;
        let defaults = Self::default();

        let parse_or = |opt: Option<String>, fallback: &AnsiColor| -> AnsiColor {
            opt.and_then(|h| AnsiColor::from_hex(&h)).unwrap_or_else(|| fallback.clone())
        };

        Ok(Self {
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
            is_dark: colors.mode != "light",
        })
    }

    pub fn apply_custom_overrides(&mut self, custom: &crate::config::CustomPalette) {
        if let Some(ref h) = custom.accent {
            if let Some(c) = AnsiColor::from_hex(h) { self.accent = c; }
        }
        if let Some(ref h) = custom.foreground {
            if let Some(c) = AnsiColor::from_hex(h) { self.foreground = c; }
        }
        if let Some(ref h) = custom.muted {
            if let Some(c) = AnsiColor::from_hex(h) { self.muted = c; }
        }
        if let Some(ref h) = custom.background {
            if let Some(c) = AnsiColor::from_hex(h) { self.background = c; }
        }
        if let Some(ref h) = custom.red {
            if let Some(c) = AnsiColor::from_hex(h) { self.red = c; }
        }
        if let Some(ref h) = custom.green {
            if let Some(c) = AnsiColor::from_hex(h) { self.green = c; }
        }
        if let Some(ref h) = custom.yellow {
            if let Some(c) = AnsiColor::from_hex(h) { self.yellow = c; }
        }
        if let Some(ref h) = custom.blue {
            if let Some(c) = AnsiColor::from_hex(h) { self.blue = c; }
        }
    }
}
