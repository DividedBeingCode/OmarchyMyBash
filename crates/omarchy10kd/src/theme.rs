use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
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

    /// Perceived luminance (Rec. 709 weights, gamma skipped) in 0..=1 —
    /// accurate enough to classify a background as dark or light.
    fn perceived_luminance(&self) -> f32 {
        (0.2126 * self.r as f32 + 0.7152 * self.g as f32 + 0.0722 * self.b as f32) / 255.0
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

/// The ghostty window config Omarchy renders to
/// `~/.local/state/omarchy/current/theme/ghostty.conf` (verified format):
///
/// ```text
/// background = #1a1b26
/// foreground = #a9b1d6
/// cursor-color = #c0caf5
/// selection-background = #292e42
/// selection-foreground = #c0caf5
///
/// palette = 0=#1a1b26
/// ...
/// palette = 15=#c0caf5
/// ```
///
/// Palette entries use the repeated-key `palette = N=#hex` form (`:` is
/// accepted as the index separator too). Role mapping, sanity-checked
/// against the Tokyo Night hexes Omarchy writes to colors.toml for the
/// same theme:
///   background/foreground — the direct lines (they match
///     palette[0]/palette[7] in this theme, but the lines win)
///   accent = palette[4]   (blue — matches colors.toml `accent`)
///   muted = palette[8]    (bright black — matches colors.toml `muted`)
///   red/green/yellow/blue/magenta/cyan = palette[1..=6] (1:1 with the
///     colors.toml roles of the same names)
///   orange = palette[11]  (bright yellow — matches ThemePalette's own
///     default orange #ff9e64; colors.toml's derived `orange` tone is
///     never used by the prompt directly)
///   bright_foreground = palette[15] (matches colors.toml)
///   dark_foreground — no ANSI slot; derived as the foreground/
///     background midpoint, like the other harmonized roles
///   cursor-color — required as a well-formedness marker (Omarchy's
///     engine always writes it); ThemePalette has no cursor role
struct GhosttyPalette {
    background: AnsiColor,
    foreground: AnsiColor,
    cursor: AnsiColor,
    palette: [AnsiColor; 16],
}

impl GhosttyPalette {
    fn parse(contents: &str) -> anyhow::Result<Self> {
        let mut background = None;
        let mut foreground = None;
        let mut cursor = None;
        let mut palette: [Option<AnsiColor>; 16] = [None; 16];
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "background" => background = AnsiColor::from_hex(value),
                "foreground" => foreground = AnsiColor::from_hex(value),
                "cursor-color" => cursor = AnsiColor::from_hex(value),
                "palette" => {
                    let Some((index, hex)) = value.split_once(['=', ':']) else {
                        anyhow::bail!("malformed palette entry '{line}'");
                    };
                    let index: usize = index
                        .trim()
                        .parse()
                        .map_err(|_| anyhow::anyhow!("malformed palette index in '{line}'"))?;
                    if index >= 16 {
                        anyhow::bail!("palette index {index} out of range in '{line}'");
                    }
                    palette[index] = Some(
                        AnsiColor::from_hex(hex.trim())
                            .ok_or_else(|| anyhow::anyhow!("invalid hex in '{line}'"))?,
                    );
                }
                // selection-*, font-family, ... carry no palette data.
                _ => {}
            }
        }

        let mut entries = [AnsiColor { r: 0, g: 0, b: 0 }; 16];
        for (index, entry) in palette.into_iter().enumerate() {
            entries[index] =
                entry.ok_or_else(|| anyhow::anyhow!("missing palette entry {index}"))?;
        }
        Ok(Self {
            background: background
                .ok_or_else(|| anyhow::anyhow!("missing or invalid 'background' entry"))?,
            foreground: foreground
                .ok_or_else(|| anyhow::anyhow!("missing or invalid 'foreground' entry"))?,
            cursor: cursor
                .ok_or_else(|| anyhow::anyhow!("missing or invalid 'cursor-color' entry"))?,
            palette: entries,
        })
    }
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

    /// Resolves `theme.source = "terminal"`: read the palette of the
    /// terminal theme Omarchy currently has rendered. On any failure
    /// (missing file, missing entries, bad hex) fall back to the plain
    /// default resolution with a warning. The bool trio marks all three
    /// extended roles as already real (straight from the live palette),
    /// so derivation must not blend over them.
    fn load_terminal_from_dir(dir: &Path) -> (Self, [bool; 3]) {
        let path = dir.join("ghostty.conf");
        match Self::parse_ghostty_palette(&path) {
            Ok(palette) => {
                debug!("loaded terminal palette from {}", path.display());
                (palette, [true; 3])
            }
            Err(e) => {
                warn!(
                    "failed to load terminal palette from {}: {e}, using defaults",
                    path.display()
                );
                (Self::default(), [false; 3])
            }
        }
    }

    fn parse_ghostty_palette(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let file = GhosttyPalette::parse(&contents)?;
        Ok(Self {
            accent: file.palette[4],
            foreground: file.foreground,
            dark_foreground: AnsiColor::blend(&file.foreground, &file.background),
            bright_foreground: file.palette[15],
            background: file.background,
            muted: file.palette[8],
            red: file.palette[1],
            green: file.palette[2],
            yellow: file.palette[3],
            blue: file.palette[4],
            magenta: file.palette[5],
            cyan: file.palette[6],
            orange: file.palette[11],
            is_dark: file.background.perceived_luminance() < 0.5,
        })
    }

    pub fn resolve_palette(config: &crate::config::Config) -> Self {
        Self::resolve_palette_in(config, &Self::omarchy_theme_dir())
    }

    /// Like [`Self::resolve_palette`] but reads theme state from `dir`
    /// instead of `~/.local/state/omarchy/current/theme` (test seam).
    pub fn resolve_palette_in(config: &crate::config::Config, dir: &Path) -> Self {
        let source = config.theme.source.as_str();
        // `derived` marks extended roles that must survive blending: either
        // the source supplied real magenta/cyan/orange, or the user set
        // them explicitly via [theme.custom].
        let (mut palette, mut derived) = match source {
            "omarchy" => (Self::load_omarchy(), [false; 3]),
            "custom" => (Self::default(), [false; 3]),
            "hybrid" => (Self::load_omarchy(), [false; 3]),
            "terminal" => Self::load_terminal_from_dir(dir),
            _ => (Self::default(), [false; 3]),
        };
        if source != "omarchy" {
            if let Some(custom) = &config.theme.custom {
                let explicit = palette.apply_custom_overrides(custom);
                for i in 0..3 {
                    derived[i] = derived[i] || explicit[i];
                }
            }
        }
        palette.derive_extended_except(derived);
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

    // ── theme.source = "terminal" ──────────────────────────────────────

    /// The exact format Omarchy's theme engine writes for Tokyo Night
    /// (verified against ~/.local/state/omarchy/current/theme/ghostty.conf).
    const TOKYO_NIGHT_GHOSTTY: &str = "\
background = #1a1b26
foreground = #a9b1d6
cursor-color = #c0caf5
selection-background = #292e42
selection-foreground = #c0caf5

palette = 0=#1a1b26
palette = 1=#f7768e
palette = 2=#9ece6a
palette = 3=#e0af68
palette = 4=#7aa2f7
palette = 5=#ad8ee6
palette = 6=#449dab
palette = 7=#a9b1d6
palette = 8=#414868
palette = 9=#ff7a93
palette = 10=#b9f27c
palette = 11=#ff9e64
palette = 12=#7da6ff
palette = 13=#bb9af7
palette = 14=#0db9d7
palette = 15=#c0caf5
";

    fn fixture_dir(name: &str, ghostty: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("o10k-theme-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(contents) = ghostty {
            std::fs::write(dir.join("ghostty.conf"), contents).unwrap();
        }
        dir
    }

    fn terminal_config(custom: Option<CustomPalette>) -> Config {
        Config {
            theme: ThemeConfig { source: "terminal".into(), custom },
            ..Config::default()
        }
    }

    #[test]
    fn test_terminal_resolves_full_palette() {
        let dir = fixture_dir("full", Some(TOKYO_NIGHT_GHOSTTY));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        // Direct lines.
        assert_eq!(palette.background, AnsiColor { r: 26, g: 27, b: 38 });
        assert_eq!(palette.foreground, AnsiColor { r: 169, g: 177, b: 214 });
        // accent = palette[4], blue = palette[4].
        assert_eq!(palette.accent, AnsiColor { r: 122, g: 162, b: 247 });
        assert_eq!(palette.blue, palette.accent);
        // muted = palette[8].
        assert_eq!(palette.muted, AnsiColor { r: 65, g: 72, b: 104 });
        // red/green/yellow/magenta/cyan = palette[1..=6].
        assert_eq!(palette.red, AnsiColor { r: 247, g: 118, b: 142 });
        assert_eq!(palette.green, AnsiColor { r: 158, g: 206, b: 106 });
        assert_eq!(palette.yellow, AnsiColor { r: 224, g: 175, b: 104 });
        assert_eq!(palette.cyan, AnsiColor { r: 68, g: 157, b: 171 });
        assert_eq!(palette.magenta, AnsiColor { r: 173, g: 142, b: 230 }); // palette[5] #ad8ee6
        assert_eq!(palette.orange, AnsiColor { r: 255, g: 158, b: 100 });
        // bright_foreground = palette[15].
        assert_eq!(palette.bright_foreground, AnsiColor { r: 192, g: 202, b: 245 });
        // dark_foreground has no ANSI slot — derived midpoint.
        assert_eq!(
            palette.dark_foreground,
            AnsiColor::blend(&palette.foreground, &palette.background)
        );
        assert!(palette.is_dark);
    }

    #[test]
    fn test_terminal_extended_roles_not_blended() {
        let dir = fixture_dir("noblend", Some(TOKYO_NIGHT_GHOSTTY));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        // The live palette carries real extended roles — no harmonization.
        assert_ne!(palette.magenta, AnsiColor::blend(&palette.red, &palette.blue));
        assert_ne!(palette.cyan, AnsiColor::blend(&palette.blue, &palette.green));
        assert_ne!(palette.orange, AnsiColor::blend(&palette.red, &palette.yellow));
    }

    #[test]
    fn test_terminal_missing_entry_falls_back() {
        let broken = TOKYO_NIGHT_GHOSTTY.replace("palette = 8=#414868\n", "");
        let dir = fixture_dir("missing-entry", Some(&broken));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        let defaults = ThemePalette::default();
        assert_eq!(palette.accent, defaults.accent);
        assert_eq!(palette.background, defaults.background);
        assert_eq!(palette.magenta, AnsiColor::blend(&defaults.red, &defaults.blue));
        assert!(palette.is_dark);
    }

    #[test]
    fn test_terminal_bad_hex_falls_back() {
        let broken = TOKYO_NIGHT_GHOSTTY.replace("palette = 3=#e0af68", "palette = 3=#zzzzzz");
        let dir = fixture_dir("bad-hex", Some(&broken));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        assert_eq!(palette.yellow, ThemePalette::default().yellow);
        assert_eq!(palette.accent, ThemePalette::default().accent);
    }

    #[test]
    fn test_terminal_missing_file_falls_back() {
        let dir = fixture_dir("no-file", None);
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        let defaults = ThemePalette::default();
        assert_eq!(palette.accent, defaults.accent);
        assert_eq!(palette.background, defaults.background);
        assert!(palette.is_dark);
    }

    #[test]
    fn test_terminal_missing_cursor_falls_back() {
        // cursor-color is part of the well-formed file the engine writes.
        let broken = TOKYO_NIGHT_GHOSTTY.replace("cursor-color = #c0caf5\n", "");
        let dir = fixture_dir("no-cursor", Some(&broken));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        assert_eq!(palette.accent, ThemePalette::default().accent);
    }

    #[test]
    fn test_terminal_light_background() {
        let light = TOKYO_NIGHT_GHOSTTY.replace("background = #1a1b26", "background = #d5d6db");
        let dir = fixture_dir("light", Some(&light));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        assert!(!palette.is_dark);
    }

    #[test]
    fn test_terminal_custom_overrides_win() {
        let custom = CustomPalette {
            accent: Some("#00ff00".into()),
            foreground: None, muted: None, background: None,
            red: None, green: None, yellow: None, blue: None,
            magenta: None, cyan: None, orange: None,
        };
        let dir = fixture_dir("custom", Some(TOKYO_NIGHT_GHOSTTY));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(Some(custom)), &dir);
        assert_eq!(palette.accent, AnsiColor { r: 0, g: 255, b: 0 });
        // Non-overridden roles still come from the terminal palette.
        assert_eq!(palette.magenta, AnsiColor { r: 173, g: 142, b: 230 }); // #ad8ee6 from palette[5]
    }
    #[test]
    fn test_terminal_accepts_colon_separator() {
        // Ghostty also accepts `palette = N:#hex`; both must parse.
        let colon = TOKYO_NIGHT_GHOSTTY.replace("=#", ":#");
        let dir = fixture_dir("colon", Some(&colon));
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &dir);
        assert_eq!(palette.accent, AnsiColor { r: 122, g: 162, b: 247 });
        assert_eq!(palette.orange, AnsiColor { r: 255, g: 158, b: 100 });
    }

    #[test]
    fn test_terminal_unresolvable_dir_falls_back() {
        // No HOME fixtures involved: the state dir override is authoritative.
        let missing = std::env::temp_dir().join("o10k-theme-state-dir-does-not-exist");
        let palette = ThemePalette::resolve_palette_in(&terminal_config(None), &missing);
        assert_eq!(palette.accent, ThemePalette::default().accent);
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
