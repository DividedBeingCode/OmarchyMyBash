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
    /// Art-directed gradient ramp from `[theme] ramp`, start → end. When set
    /// it wins over the derivation below.
    pub ramp: Option<(AnsiColor, AnsiColor)>,
    /// How wide a hue sweep to derive when `ramp` is unset.
    pub gradient: GradientMode,
}

/// How a palette's gradient ramp is chosen.
///
/// Gradients used to be picked from ANSI slots — `complement()` returned the
/// `magenta` role or the `cyan` role depending on `accent.b >= accent.r`. For
/// Synthwave's `#d53bce` that comparison is 213 vs 206, so seven bytes of red
/// decided whether a palette sold as "purple all the way down" ramped to
/// violet or to teal. It chose teal. The ramp is now derived from the
/// accent's own hue, so a purple palette cannot produce a green gradient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientMode {
    /// No sweep — the ramp collapses to a flat accent.
    Off,
    /// An analogous sweep. The default, and in-family for every palette.
    Auto,
    /// A wider, more theatrical sweep. Still hue-anchored to the accent.
    Full,
}

impl GradientMode {
    pub fn parse(s: Option<&str>) -> Self {
        match s.map(str::trim) {
            Some("off") => Self::Off,
            Some("full") => Self::Full,
            _ => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Auto => "auto",
            Self::Full => "full",
        }
    }

    /// Hue rotation, in OKLCH degrees, from the accent to the ramp's far end.
    ///
    /// Positive rotation walks red → orange → green → cyan → blue → purple →
    /// red. Measured across all 39 curated palettes, +38 deg keeps every accent
    /// inside its own family: blue → purple, purple → hot pink, green → cyan.
    /// Beyond roughly 45 deg the sweep starts crossing into unrelated hues,
    /// which is the failure this whole mechanism exists to prevent, so `Full`
    /// stays deliberately short of a complementary swing.
    pub fn sweep(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Auto => 38.0,
            Self::Full => 64.0,
        }
    }
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

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Rotate hue in OKLCH, holding lightness and chroma.
    ///
    /// Holding chroma is what makes this safe for monochrome themes: a grey
    /// has no chroma, so rotating its hue is a no-op and the theme stays grey.
    /// Holding lightness keeps the contrast work in `palette_derive` intact —
    /// the ramp sweeps hue only, never readability.
    pub fn hue_rotated(&self, degrees: f32) -> Self {
        match crate::palette_derive::srgb_to_oklch(&self.to_hex()) {
            Some(mut c) => {
                c.h = (c.h + degrees).rem_euclid(360.0);
                Self::from_hex(&crate::palette_derive::oklch_to_srgb(c)).unwrap_or(*self)
            }
            None => *self,
        }
    }

    /// Interpolate a→b at t through OKLCH, taking the short way around the
    /// hue circle.
    ///
    /// Straight sRGB interpolation between two saturated colors dips through
    /// a desaturated middle — a purple→pink ramp goes grey in the centre.
    /// OKLCH holds chroma up across the sweep.
    pub fn mix_oklch(a: &Self, b: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let (ca, cb) = match (
            crate::palette_derive::srgb_to_oklch(&a.to_hex()),
            crate::palette_derive::srgb_to_oklch(&b.to_hex()),
        ) {
            (Some(x), Some(y)) => (x, y),
            _ => return Self::lerp(a, b, t),
        };
        // Shortest arc: without this, 350 deg → 10 deg would sweep the long way
        // through the entire wheel.
        let mut dh = cb.h - ca.h;
        if dh > 180.0 {
            dh -= 360.0;
        } else if dh < -180.0 {
            dh += 360.0;
        }
        let mixed = crate::palette_derive::Oklch {
            l: ca.l + (cb.l - ca.l) * t,
            c: ca.c + (cb.c - ca.c) * t,
            h: (ca.h + dh * t).rem_euclid(360.0),
        };
        Self::from_hex(&crate::palette_derive::oklch_to_srgb(mixed))
            .unwrap_or_else(|| Self::lerp(a, b, t))
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
            ramp: None,
            gradient: GradientMode::Auto,
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

/// A curated palette's art-directed ramp, matched by accent.
///
/// Matching on accent rather than requiring `[theme] ramp` in the file is what
/// makes art direction reach configs that predate the key, or that were
/// written by hand in hex. Accent identity is already how the Studio decides
/// which palette chip is active, so the convention is not new here.
fn curated_ramp_for(accent: &AnsiColor) -> Option<(AnsiColor, AnsiColor)> {
    let hex = accent.to_hex();
    let def = crate::looks::curated_palettes()
        .iter()
        .find(|p| p.ramp.is_some() && p.colors[0].eq_ignore_ascii_case(&hex))?;
    let [start, end] = def.ramp?;
    Some((AnsiColor::from_hex(start)?, AnsiColor::from_hex(end)?))
}

impl ThemePalette {
    /// The palette's gradient ramp, start → end.
    ///
    /// An explicit `[theme] ramp` is art direction and wins outright.
    /// Otherwise the far end is the accent rotated through OKLCH, which keeps
    /// every ramp inside the accent's own hue family — see [`GradientMode`]
    /// for what replaced the old ANSI-slot rule.
    pub fn ramp_endpoints(&self) -> (AnsiColor, AnsiColor) {
        // "off" is the user saying they want no gradient anywhere, so it
        // outranks a palette's art direction.
        if self.gradient == GradientMode::Off {
            return (self.accent, self.accent);
        }
        if let Some((start, end)) = self.ramp {
            return (start, end);
        }
        (self.accent, self.accent.hue_rotated(self.gradient.sweep()))
    }

    /// Gap-gradient endpoints by mode.
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
            // The frame rule and the segment ramp are the same gradient seen
            // in two places, so they must not be able to disagree.
            GapGradient::Full => self.ramp_endpoints(),
        }
    }

    /// The palette's ramp sampled at t ∈ [0,1].
    ///
    /// Was a two-stage accent → magenta → cyan walk. In three curated
    /// palettes accent *is* magenta, so the whole first half sat flat and the
    /// only visible sweep ran to the cyan slot.
    pub fn ramp_color(&self, t: f32) -> AnsiColor {
        let (start, end) = self.ramp_endpoints();
        AnsiColor::mix_oklch(&start, &end, t)
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
            ramp: None,
            gradient: GradientMode::Auto,
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
            ramp: None,
            gradient: GradientMode::Auto,
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
        // The ramp is palette-level, not style-level: a gradient belongs to
        // the colors, so every surface that renders one — segment fills, the
        // frame rule, the Studio's ramp preview — reads the same two colors.
        palette.gradient = GradientMode::parse(config.theme.gradient.as_deref());
        palette.ramp = Self::configured_ramp(config).or_else(|| curated_ramp_for(&palette.accent));
        palette
    }

    /// The ramp explicitly written in `[theme] ramp`, if it is valid.
    fn configured_ramp(config: &crate::config::Config) -> Option<(AnsiColor, AnsiColor)> {
        config.theme.ramp.as_ref().and_then(|v| {
            let [start, end] = v.as_slice() else {
                warn!("[theme] ramp needs exactly two colors, got {}", v.len());
                return None;
            };
            match (AnsiColor::from_hex(start), AnsiColor::from_hex(end)) {
                (Some(a), Some(b)) => Some((a, b)),
                _ => {
                    warn!("[theme] ramp has an invalid color; deriving instead");
                    None
                }
            }
        })
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
            theme: ThemeConfig { source: "omarchy".into(), custom: None, ..Default::default() },
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
                ..Default::default()
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
                ..Default::default()
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
            theme: ThemeConfig { source: "terminal".into(), custom, ..Default::default() },
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

    // ── Gradient ramps ──────────────────────────────────────────────────
    //
    // The bug these pin: `complement()` picked the ramp's far end with
    // `if accent.b >= accent.r { magenta } else { cyan }`. For Synthwave's
    // #d53bce that is r=213 vs b=206 — seven bytes of red decided whether a
    // palette advertised as "purple all the way down" ramped to violet or to
    // teal. It picked teal, so the Look rendered purple → #00b0b1.

    /// OKLCH hue of a color, for asserting on hue families.
    fn hue_of(c: &AnsiColor) -> f32 {
        let hex = format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        crate::palette_derive::srgb_to_oklch(&hex).expect("valid hex").h
    }

    /// Shortest angular distance between two hues, in degrees.
    fn hue_delta(a: f32, b: f32) -> f32 {
        let d = (a - b).abs() % 360.0;
        if d > 180.0 { 360.0 - d } else { d }
    }

    fn palette_named(key: &str) -> ThemePalette {
        let def = crate::looks::curated_palettes()
            .iter()
            .find(|p| p.key == key)
            .unwrap_or_else(|| panic!("no curated palette {key}"));
        let mut p = ThemePalette::default();
        for (role, hex) in crate::looks::ROLE_ORDER.iter().zip(def.colors.iter()) {
            let c = AnsiColor::from_hex(hex).expect("curated hex");
            match *role {
                "accent" => p.accent = c,
                "foreground" => p.foreground = c,
                "muted" => p.muted = c,
                "background" => p.background = c,
                "red" => p.red = c,
                "green" => p.green = c,
                "yellow" => p.yellow = c,
                "blue" => p.blue = c,
                "magenta" => p.magenta = c,
                "cyan" => p.cyan = c,
                "orange" => p.orange = c,
                _ => {}
            }
        }
        p.ramp = def.ramp.and_then(|[a, b]| {
            Some((AnsiColor::from_hex(a)?, AnsiColor::from_hex(b)?))
        });
        p
    }

    #[test]
    fn a_gradient_never_leaves_the_accents_hue_family() {
        for def in crate::looks::curated_palettes() {
            let p = palette_named(def.key);
            let (start, end) = p.ramp_endpoints();
            // An explicit ramp is art direction; the derivation rule is what
            // this guards.
            if def.ramp.is_some() {
                continue;
            }
            let d = hue_delta(hue_of(&start), hue_of(&end));
            assert!(
                d <= 45.0,
                "{}: ramp #{:02x}{:02x}{:02x} -> #{:02x}{:02x}{:02x} swings {d:.0}deg out of family",
                def.key, start.r, start.g, start.b, end.r, end.g, end.b
            );
        }
    }

    #[test]
    fn synthwave_stays_purple_instead_of_going_teal() {
        let p = palette_named("synthwave-alpha");
        let (_, end) = p.ramp_endpoints();
        // The old code returned #00b0b1 here.
        assert!(
            end.r > end.g && end.b > end.g,
            "expected a magenta-family end, got #{:02x}{:02x}{:02x}",
            end.r, end.g, end.b
        );
    }

    #[test]
    fn a_grey_accent_stays_grey() {
        // Rotating hue on a zero-chroma color must be a no-op, or monochrome
        // themes would sprout a color they never asked for.
        let mut p = ThemePalette::default();
        p.accent = AnsiColor { r: 128, g: 128, b: 128 };
        let (_, end) = p.ramp_endpoints();
        let spread = end.r.max(end.g).max(end.b) - end.r.min(end.g).min(end.b);
        assert!(spread <= 2, "grey drifted to #{:02x}{:02x}{:02x}", end.r, end.g, end.b);
    }

    #[test]
    fn gradient_off_collapses_the_ramp_to_the_accent() {
        let mut p = palette_named("synthwave-alpha");
        p.gradient = GradientMode::Off;
        let (start, end) = p.ramp_endpoints();
        assert_eq!(start, end);
        assert_eq!(start, p.accent);
    }

    #[test]
    fn an_explicit_ramp_overrides_the_derivation() {
        let mut p = palette_named("synthwave-alpha");
        p.ramp = Some((
            AnsiColor::from_hex("#ff4fd8").unwrap(),
            AnsiColor::from_hex("#7c3aed").unwrap(),
        ));
        let (start, end) = p.ramp_endpoints();
        assert_eq!(start, AnsiColor::from_hex("#ff4fd8").unwrap());
        assert_eq!(end, AnsiColor::from_hex("#7c3aed").unwrap());
    }

    #[test]
    fn the_ramp_sweeps_rather_than_sitting_flat() {
        // accent == magenta in three curated palettes, which made the old
        // two-stage ramp spend its entire first half going nowhere.
        let p = palette_named("vaporwave-sunset");
        let mid = p.ramp_color(0.5);
        let (start, end) = p.ramp_endpoints();
        assert_ne!(mid, start, "ramp is flat at its midpoint");
        assert_ne!(mid, end, "ramp is flat at its midpoint");
    }

    #[test]
    fn gap_gradient_full_uses_the_palette_ramp() {
        let p = palette_named("synthwave-alpha");
        let (a, b) = p.gap_gradient_endpoints(crate::style::GapGradient::Full);
        assert_eq!((a, b), p.ramp_endpoints());
    }

    #[test]
    fn a_hand_written_gruvbox_config_still_gets_its_art_directed_ramp() {
        // A config that predates `[theme] ramp` — or that someone typed in
        // hex — must not be stuck with the derived sweep. Gruvbox's accent is
        // nearly grey, so derivation barely moves it.
        let config = Config {
            theme: ThemeConfig {
                source: "custom".into(),
                custom: Some(CustomPalette {
                    accent: Some("#83a598".into()),
                    foreground: None, muted: None, background: None,
                    red: None, green: None, yellow: None, blue: None,
                    magenta: None, cyan: None, orange: None,
                }),
                ..Default::default()
            },
            ..Config::default()
        };
        let palette = ThemePalette::resolve_palette(&config);
        let (_, end) = palette.ramp_endpoints();
        assert_eq!(end, AnsiColor::from_hex("#fabd2f").unwrap(),
                   "expected gruvbox mustard, got {}", end.to_hex());
    }

    #[test]
    fn an_explicit_ramp_still_beats_the_curated_lookup() {
        let config = Config {
            theme: ThemeConfig {
                source: "custom".into(),
                custom: Some(CustomPalette {
                    accent: Some("#83a598".into()),
                    foreground: None, muted: None, background: None,
                    red: None, green: None, yellow: None, blue: None,
                    magenta: None, cyan: None, orange: None,
                }),
                ramp: Some(vec!["#111111".into(), "#222222".into()]),
                ..Default::default()
            },
            ..Config::default()
        };
        let (start, end) = ThemePalette::resolve_palette(&config).ramp_endpoints();
        assert_eq!(start, AnsiColor::from_hex("#111111").unwrap());
        assert_eq!(end, AnsiColor::from_hex("#222222").unwrap());
    }

    #[test]
    fn a_palette_with_no_curated_match_derives() {
        let config = Config {
            theme: ThemeConfig {
                source: "custom".into(),
                custom: Some(CustomPalette {
                    accent: Some("#d53bce".into()),
                    foreground: None, muted: None, background: None,
                    red: None, green: None, yellow: None, blue: None,
                    magenta: None, cyan: None, orange: None,
                }),
                ..Default::default()
            },
            ..Config::default()
        };
        let (_, end) = ThemePalette::resolve_palette(&config).ramp_endpoints();
        // Synthwave ships no explicit ramp, so this is the OKLCH derivation —
        // and it must not be teal.
        assert!(end.r > end.g && end.b > end.g, "got {}", end.to_hex());
    }
}
