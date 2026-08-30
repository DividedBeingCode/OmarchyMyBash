//! Derive a prompt palette from an Omarchy theme, with a contrast guarantee.
//!
//! Omarchy ships 22 themes; Omarchy10k shipped 8 curated prompt palettes, so
//! most themes — and every theme a user installs later — had no prompt colors
//! at all. This module closes that gap.
//!
//! An Omarchy `colors.toml` already NAMES its roles (`accent`, `red`, `muted`,
//! …), so this is not a hue-bucketing problem. The work is:
//!
//!   1. MAP the named roles across, with a fallback chain for the ones a
//!      theme omits (`white` ships no `orange`).
//!   2. REPAIR contrast. Theme colors are chosen against a terminal's *own*
//!      needs, and several shipped themes put roles at contrast that is
//!      unreadable as prompt text — `hackerman` has `muted = "#2d3450"` on
//!      `background = "#0B0C16"`, which is effectively invisible.
//!   3. PRESERVE character. Hue and chroma are never invented: only lightness
//!      moves. A monochrome theme (`vantablack`, `white`) stays monochrome.
//!
//! ## Why APCA and not WCAG 2.x
//!
//! Nearly every palette here is dark, and WCAG 2.x's contrast ratio far
//! overstates contrast for dark colors — its prediction degrades once the
//! brightest color is darker than `#a0a0a0`, which is true of most terminal
//! foregrounds. Its own analysis concludes it "cannot be used for guidance
//! designing dark mode". APCA's Lc is perceptually uniform across the whole
//! range instead, so one threshold means the same thing on Vantablack as on
//! Catppuccin Latte.
//!
//! ## Why OKLCH is the repair space
//!
//! Equal lightness steps are equal *perceived* steps, so walking L toward
//! contrast does not drift one hue brighter or duller than its neighbours the
//! way an HSL walk would.
//!
//! References:
//! - <https://gist.github.com/Myndex/069a4079b0de2930e72d5401bde9af98>
//! - <https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell.html>
//! - <https://bottosson.github.io/posts/oklab/>

use std::collections::BTreeMap;

/// The eleven roles a prompt palette needs — the same set
/// `config::CustomPalette` and `looks::curated_palette` already use, so a
/// derived palette drops into the existing `theme.custom` patch unchanged.
pub const ROLES: [&str; 11] = [
    "accent",
    "foreground",
    "muted",
    "background",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "orange",
];

// ── APCA Lc targets ────────────────────────────────────────────────────────
//
// These thresholds repair what is UNREADABLE. They do not enforce what would
// be ideal, because the colors belong to the theme and the user picked it.
//
// Measured across all 22 shipped themes before choosing them (median Lc on
// each theme's own background):
//
//     foreground  80.1     accent  52.9     muted  18.7
//     red         44.3     green   56.4     blue   50.4
//
// An earlier draft used APCA's body-text tier (Lc 60) for the hue roles. That
// would have repaired 14 of 22 accents and 16 of 22 reds — systematically
// brightening every theme in the distribution. People choose Nord *because*
// it is muted; rewriting it is not a fix.
//
// So the bar is APCA's "spot readable" tier instead, which is the right one
// for prompt text: you read one short token at a time ("main", "+2", "✗1"),
// never a column of prose. `muted` sits a tier lower still because it is
// deliberately recessive — but two themes ship `muted` at Lc 0.0 against
// their own background, which is not design, it is a color meant for borders
// being used as text.

/// Text roles: foreground, accent, and the six hue roles. APCA's
/// spot-readable tier — short, isolated tokens rather than running text.
pub const LC_TEXT: f32 = 45.0;
/// Deliberately recessive, held to APCA's floor for text of any kind.
/// Below this a segment is not dim, it is missing.
pub const LC_MUTED: f32 = 30.0;

/// Fallback chain for roles a theme omits, tried in order.
fn fallbacks(role: &str) -> &'static [&'static str] {
    match role {
        // `white` ships no `orange`, so this chain is exercised by a real theme.
        "orange" => &["yellow", "red", "accent"],
        "cyan" => &["blue", "accent"],
        "magenta" => &["accent", "blue"],
        "blue" => &["accent", "cyan"],
        "green" => &["accent"],
        "yellow" => &["orange", "accent"],
        "red" => &["accent"],
        "accent" => &["blue", "foreground"],
        "muted" => &["foreground"],
        _ => &[],
    }
}

// ── sRGB ↔ OKLCH ───────────────────────────────────────────────────────────

/// A color in OKLCH: perceptual lightness 0..1, chroma 0..~0.4, hue degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

/// Parse `#rgb` or `#rrggbb` into 0..1 sRGB components.
fn parse_hex(hex: &str) -> Option<(f32, f32, f32)> {
    let s = hex.trim().trim_start_matches('#');
    let (r, g, b) = match s.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&s[i..i + 1].repeat(2), 16).ok();
            (d(0)?, d(1)?, d(2)?)
        }
        6 => {
            let d = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
            (d(0)?, d(2)?, d(4)?)
        }
        _ => return None,
    };
    Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB hex → OKLCH. `None` when the string is not a color.
pub fn srgb_to_oklch(hex: &str) -> Option<Oklch> {
    let (r, g, b) = parse_hex(hex)?;
    let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));

    // Björn Ottosson's Oklab matrices.
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;

    let (l_, m_, s_) = (l.cbrt(), m.cbrt(), s.cbrt());

    let ok_l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let ok_a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let ok_b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

    let c = (ok_a * ok_a + ok_b * ok_b).sqrt();
    let mut h = ok_b.atan2(ok_a).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    Some(Oklch { l: ok_l, c, h })
}

/// OKLCH → sRGB hex, clamped into gamut per channel.
///
/// Clamping (rather than gamut-mapping by reducing chroma) is deliberate: the
/// repair walk moves only lightness by small steps, so excursions are shallow,
/// and clamping keeps the function total — every input yields a color.
pub fn oklch_to_srgb(color: Oklch) -> String {
    let a = color.c * color.h.to_radians().cos();
    let b = color.c * color.h.to_radians().sin();

    let l_ = color.l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = color.l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = color.l - 0.0894841775 * a - 1.2914855480 * b;

    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let bl = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

    let enc = |v: f32| -> u8 {
        let v = linear_to_srgb(v);
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    format!("#{:02x}{:02x}{:02x}", enc(r), enc(g), enc(bl))
}

// ── APCA ───────────────────────────────────────────────────────────────────
//
// APCA-W3 constants (0.1.9 / 0.98G-4g). Note the luminance step uses a simple
// 2.4 power curve on each channel, NOT the sRGB piecewise transfer function —
// APCA specifies the simple curve and substituting the piecewise one silently
// shifts every result.

const APCA_TRC: f32 = 2.4;
const APCA_NORM_BG: f32 = 0.56;
const APCA_NORM_TXT: f32 = 0.57;
const APCA_REV_TXT: f32 = 0.62;
const APCA_REV_BG: f32 = 0.65;
const APCA_BLK_THRS: f32 = 0.022;
const APCA_BLK_CLMP: f32 = 1.414;
const APCA_SCALE: f32 = 1.14;
const APCA_LO_OFFSET: f32 = 0.027;
const APCA_LO_CLIP: f32 = 0.1;
const APCA_DELTA_Y_MIN: f32 = 0.0005;

fn apca_luminance(hex: &str) -> Option<f32> {
    let (r, g, b) = parse_hex(hex)?;
    Some(0.2126729 * r.powf(APCA_TRC) + 0.7151522 * g.powf(APCA_TRC) + 0.0717500 * b.powf(APCA_TRC))
}

fn apca_clamp_black(y: f32) -> f32 {
    if y > APCA_BLK_THRS {
        y
    } else {
        y + (APCA_BLK_THRS - y).powf(APCA_BLK_CLMP)
    }
}

/// Signed APCA lightness contrast (Lc) of `text` on `bg`.
///
/// Positive for dark text on a light background, negative for light text on a
/// dark background. Both directions are meaningful; callers that only care
/// about magnitude use [`apca_lc_abs`].
pub fn apca_lc(text_hex: &str, bg_hex: &str) -> f32 {
    let (Some(y_txt), Some(y_bg)) = (apca_luminance(text_hex), apca_luminance(bg_hex)) else {
        return 0.0;
    };
    let y_txt = apca_clamp_black(y_txt);
    let y_bg = apca_clamp_black(y_bg);

    if (y_bg - y_txt).abs() < APCA_DELTA_Y_MIN {
        return 0.0;
    }

    let contrast = if y_bg > y_txt {
        // Dark text on light background.
        let sapc = (y_bg.powf(APCA_NORM_BG) - y_txt.powf(APCA_NORM_TXT)) * APCA_SCALE;
        if sapc < APCA_LO_CLIP {
            0.0
        } else {
            sapc - APCA_LO_OFFSET
        }
    } else {
        // Light text on dark background.
        let sapc = (y_bg.powf(APCA_REV_BG) - y_txt.powf(APCA_REV_TXT)) * APCA_SCALE;
        if sapc > -APCA_LO_CLIP {
            0.0
        } else {
            sapc + APCA_LO_OFFSET
        }
    };
    contrast * 100.0
}

/// Magnitude of [`apca_lc`] — what the contrast targets are expressed against.
pub fn apca_lc_abs(text_hex: &str, bg_hex: &str) -> f32 {
    apca_lc(text_hex, bg_hex).abs()
}

// ── Repair ─────────────────────────────────────────────────────────────────

/// Lightness step for the repair walk. Small enough that a repaired color
/// stays recognisably itself; large enough that the walk terminates quickly.
const REPAIR_STEP: f32 = 0.02;
/// Bound on iterations so a pathological input cannot spin.
const REPAIR_MAX_STEPS: usize = 60;

/// Walk `color`'s OKLCH lightness away from `bg` until it reaches `target` Lc.
///
/// Hue and chroma are held fixed, which is what keeps a monochrome theme
/// monochrome and stops a repaired red from sliding toward orange. Returns the
/// best value found: if the target is unreachable the color saturates at pure
/// white or black, and the caller learns from a re-measure that it fell short.
fn repair(color_hex: &str, bg_hex: &str, target: f32, lighten: bool) -> String {
    if apca_lc_abs(color_hex, bg_hex) >= target {
        return color_hex.to_string();
    }
    let Some(base) = srgb_to_oklch(color_hex) else {
        return color_hex.to_string();
    };

    let mut best = color_hex.to_string();
    let mut best_lc = apca_lc_abs(color_hex, bg_hex);
    let mut candidate = base;

    for _ in 0..REPAIR_MAX_STEPS {
        candidate.l += if lighten { REPAIR_STEP } else { -REPAIR_STEP };
        candidate.l = candidate.l.clamp(0.0, 1.0);

        let hex = oklch_to_srgb(candidate);
        let lc = apca_lc_abs(&hex, bg_hex);
        if lc > best_lc {
            best_lc = lc;
            best = hex;
        }
        if best_lc >= target {
            break;
        }
        if candidate.l <= 0.0 || candidate.l >= 1.0 {
            break;
        }
    }
    best
}

// ── Derivation ─────────────────────────────────────────────────────────────

/// A prompt palette derived from a theme, plus what had to be done to it.
#[derive(Debug, Clone)]
pub struct DerivedPalette {
    /// Role → `#rrggbb`, covering exactly [`ROLES`].
    pub colors: BTreeMap<String, String>,
    /// Roles whose contrast had to be repaired. Informational — the UI can
    /// say a palette was adjusted rather than pretending it shipped this way.
    pub repaired: Vec<String>,
    /// Roles that could not reach their target even at full saturation. A
    /// palette with entries here is surfaced as `low-contrast` rather than
    /// silently shipped as if it were fine.
    pub shortfall: Vec<String>,
}

impl DerivedPalette {
    /// The `theme` sub-patch shape `config_set` and `looks` already consume.
    pub fn to_theme_patch(&self) -> serde_json::Value {
        let custom: serde_json::Map<String, serde_json::Value> = self
            .colors
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        serde_json::json!({ "theme": { "source": "hybrid", "custom": custom } })
    }
}

/// Target Lc for a role.
fn target_for(role: &str) -> f32 {
    if role == "muted" {
        LC_MUTED
    } else {
        LC_TEXT
    }
}

/// Resolve a role from the theme map, following the fallback chain.
fn resolve_role<'a>(colors: &'a BTreeMap<String, String>, role: &str) -> Option<&'a String> {
    if let Some(v) = colors.get(role) {
        return Some(v);
    }
    fallbacks(role).iter().find_map(|f| colors.get(*f))
}

/// Derive a prompt palette from a parsed `colors.toml`.
///
/// `mode` is the file's own `mode` field (`"dark"` or `"light"`) and decides
/// which way repair walks: away from the background in both cases, which means
/// lighter on a dark theme and darker on a light one.
///
/// Returns `None` only when the theme has no usable `background` or
/// `foreground` — there is nothing to anchor contrast against, and guessing
/// one would produce a palette that looks derived rather than absent. The
/// caller surfaces that theme as "no palette" instead.
pub fn derive(colors: &BTreeMap<String, String>, mode: &str) -> Option<DerivedPalette> {
    let background = colors.get("background")?.clone();
    // A theme with a background but no foreground is not worth guessing at.
    let _ = colors.get("foreground")?;

    // Walk away from the background. Trusting `mode` alone would mis-handle a
    // theme whose mode field disagrees with its own background (and one that
    // omits the field entirely), so the background's measured lightness is the
    // tiebreaker and `mode` only settles genuinely ambiguous mid-tones.
    let bg_l = srgb_to_oklch(&background).map(|c| c.l).unwrap_or(0.0);
    let lighten = if bg_l < 0.4 {
        true
    } else if bg_l > 0.6 {
        false
    } else {
        mode != "light"
    };

    let mut out = BTreeMap::new();
    let mut repaired = Vec::new();
    let mut shortfall = Vec::new();

    for role in ROLES {
        if role == "background" {
            out.insert(role.to_string(), background.clone());
            continue;
        }

        let Some(source) = resolve_role(colors, role) else {
            continue;
        };
        let target = target_for(role);
        let fixed = repair(source, &background, target, lighten);

        if &fixed != source {
            repaired.push(role.to_string());
        }
        if apca_lc_abs(&fixed, &background) < target {
            shortfall.push(role.to_string());
        }
        out.insert(role.to_string(), fixed);
    }

    Some(DerivedPalette {
        colors: out,
        repaired,
        shortfall,
    })
}

/// Parse the subset of an Omarchy `colors.toml` this module needs.
///
/// Deliberately a flat key/value scan rather than a TOML parse: the file is
/// flat `key = "#hex"` at the top level, and several themes carry trailing
/// sections this module has no business interpreting.
pub fn parse_colors_toml(text: &str) -> (BTreeMap<String, String>, String) {
    let mut colors = BTreeMap::new();
    let mut mode = String::from("dark");

    for line in text.lines() {
        let line = line.trim();
        // Stop at the first section header: everything this module wants is
        // in the top-level table.
        if line.starts_with('[') {
            break;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();

        if key == "mode" {
            mode = value.to_string();
        } else if value.starts_with('#') {
            colors.insert(key.to_string(), value.to_string());
        }
    }
    (colors, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ── OKLCH ──────────────────────────────────────────────────────────────

    #[test]
    fn oklch_round_trips_across_a_corpus() {
        // Real theme colors, plus the achromatic extremes.
        let corpus = [
            "#7aa2f7", "#f7768e", "#9ece6a", "#e0af68", "#bb9af7", "#1a1b26", "#c0caf5", "#000000",
            "#ffffff", "#808080", "#fb4934", "#83a598", "#2d3450", "#509475",
        ];
        for hex in corpus {
            let round = oklch_to_srgb(srgb_to_oklch(hex).expect("parses"));
            assert_eq!(
                round, hex,
                "{hex} did not survive an sRGB → OKLCH → sRGB round trip"
            );
        }
    }

    #[test]
    fn oklch_lightness_is_ordered() {
        let black = srgb_to_oklch("#000000").unwrap();
        let grey = srgb_to_oklch("#808080").unwrap();
        let white = srgb_to_oklch("#ffffff").unwrap();
        assert!(black.l < grey.l && grey.l < white.l);
        assert!((white.l - 1.0).abs() < 0.01, "white should be L≈1");
        assert!(black.l.abs() < 0.01, "black should be L≈0");
    }

    #[test]
    fn achromatic_colors_have_no_chroma() {
        for hex in ["#000000", "#808080", "#ffffff", "#2a2a2a"] {
            let c = srgb_to_oklch(hex).unwrap();
            assert!(c.c < 0.001, "{hex} should be achromatic, got chroma {}", c.c);
        }
    }

    #[test]
    fn short_hex_and_junk_are_handled() {
        assert_eq!(srgb_to_oklch("#fff"), srgb_to_oklch("#ffffff"));
        assert!(srgb_to_oklch("not a color").is_none());
        assert!(srgb_to_oklch("#12345").is_none());
    }

    // ── APCA ───────────────────────────────────────────────────────────────

    #[test]
    fn apca_matches_published_reference_pairs() {
        // The two canonical APCA-W3 values. Getting both to within a tenth of
        // an Lc point is a strong check on the constants and the polarity
        // split — these numbers do not come out right by accident.
        let bow = apca_lc("#000000", "#ffffff");
        let wob = apca_lc("#ffffff", "#000000");
        assert!(
            (bow - 106.04).abs() < 0.1,
            "black on white should be Lc 106.04, got {bow}"
        );
        assert!(
            (wob + 107.88).abs() < 0.1,
            "white on black should be Lc -107.88, got {wob}"
        );
    }

    #[test]
    fn apca_polarity_is_signed() {
        assert!(apca_lc("#000000", "#ffffff") > 0.0, "dark on light is positive");
        assert!(apca_lc("#ffffff", "#000000") < 0.0, "light on dark is negative");
        assert_eq!(apca_lc_abs("#ffffff", "#000000"), -apca_lc("#ffffff", "#000000"));
    }

    #[test]
    fn apca_is_zero_for_identical_colors() {
        assert_eq!(apca_lc("#7aa2f7", "#7aa2f7"), 0.0);
    }

    #[test]
    fn apca_finds_hackermans_muted_invisible() {
        // The motivating case: hackerman's muted on its own background.
        let lc = apca_lc_abs("#2d3450", "#0B0C16");
        assert!(
            lc < 20.0,
            "hackerman's muted should measure as near-invisible, got Lc {lc}"
        );
    }

    // ── Repair ─────────────────────────────────────────────────────────────

    #[test]
    fn repair_lifts_hackermans_muted_over_the_bar() {
        let fixed = repair("#2d3450", "#0B0C16", LC_MUTED, true);
        let lc = apca_lc_abs(&fixed, "#0B0C16");
        assert!(
            lc >= LC_MUTED,
            "repaired muted should clear Lc {LC_MUTED}, got {lc} ({fixed})"
        );
    }

    #[test]
    fn repair_preserves_hue_and_chroma() {
        let before = srgb_to_oklch("#2d3450").unwrap();
        let after = srgb_to_oklch(&repair("#2d3450", "#0B0C16", LC_MUTED, true)).unwrap();
        assert!(
            (before.h - after.h).abs() < 2.0,
            "hue drifted {} → {}",
            before.h,
            after.h
        );
        assert!(
            (before.c - after.c).abs() < 0.02,
            "chroma drifted {} → {}",
            before.c,
            after.c
        );
    }

    #[test]
    fn repair_leaves_a_passing_color_untouched() {
        // Tokyo Night's blue already clears body contrast on its background.
        let hex = "#7aa2f7";
        assert_eq!(repair(hex, "#1a1b26", LC_TEXT, true), hex);
    }

    #[test]
    fn repair_darkens_on_a_light_background() {
        // A pale grey on Catppuccin Latte's background: too low to read, and
        // the only way out is DOWN. Lightening it would walk it into the
        // background, which is the failure mode this direction check exists
        // to catch.
        let (color, bg) = ("#d5d8de", "#eff1f5");
        assert!(
            apca_lc_abs(color, bg) < LC_MUTED,
            "fixture must actually need repair"
        );

        let fixed = repair(color, bg, LC_MUTED, false);
        let before = srgb_to_oklch(color).unwrap();
        let after = srgb_to_oklch(&fixed).unwrap();
        assert!(after.l < before.l, "should have darkened on a light background");
        assert!(apca_lc_abs(&fixed, bg) >= LC_MUTED);
    }

    #[test]
    fn repair_terminates_on_an_impossible_target() {
        // Nothing on mid-grey reaches Lc 75; the walk must stop, not spin.
        let fixed = repair("#808080", "#7f7f7f", 75.0, true);
        assert!(srgb_to_oklch(&fixed).is_some());
    }

    // ── Derivation ─────────────────────────────────────────────────────────

    #[test]
    fn derive_covers_every_role_for_a_complete_theme() {
        let colors = map(&[
            ("background", "#1a1b26"),
            ("foreground", "#a9b1d6"),
            ("accent", "#7aa2f7"),
            ("muted", "#414868"),
            ("red", "#f7768e"),
            ("green", "#9ece6a"),
            ("yellow", "#e0af68"),
            ("blue", "#7aa2f7"),
            ("magenta", "#ad8ee6"),
            ("cyan", "#449dab"),
            ("orange", "#eb927b"),
        ]);
        let p = derive(&colors, "dark").expect("derives");
        for role in ROLES {
            assert!(p.colors.contains_key(role), "missing role {role}");
        }
    }

    #[test]
    fn derive_follows_the_fallback_chain_for_a_missing_role() {
        // `white` ships no orange — it must still get one, from yellow.
        let colors = map(&[
            ("background", "#ffffff"),
            ("foreground", "#000000"),
            ("yellow", "#4a4a4a"),
        ]);
        let p = derive(&colors, "light").expect("derives");
        assert!(
            p.colors.contains_key("orange"),
            "orange should fall back to yellow"
        );
    }

    #[test]
    fn derive_keeps_a_monochrome_theme_monochrome() {
        // Vantablack: greys on pure black. Repair must not invent hue.
        let colors = map(&[
            ("background", "#000000"),
            ("foreground", "#ffffff"),
            ("accent", "#8d8d8d"),
            ("muted", "#7a7a7a"),
            ("red", "#a4a4a4"),
            ("green", "#b6b6b6"),
            ("yellow", "#cecece"),
            ("blue", "#8d8d8d"),
            ("magenta", "#9b9b9b"),
            ("cyan", "#b0b0b0"),
            ("orange", "#b9b9b9"),
        ]);
        let p = derive(&colors, "dark").expect("derives");
        for (role, hex) in &p.colors {
            let c = srgb_to_oklch(hex).unwrap();
            assert!(
                c.c < 0.02,
                "{role} gained chroma {} ({hex}) — monochrome themes must stay monochrome",
                c.c
            );
        }
    }

    #[test]
    fn derive_repairs_hackerman_and_says_so() {
        let colors = map(&[
            ("background", "#0B0C16"),
            ("foreground", "#ddf7ff"),
            ("accent", "#82FB9C"),
            ("muted", "#2d3450"),
            ("red", "#50f872"),
            ("green", "#4fe88f"),
            ("yellow", "#50f7d4"),
            ("blue", "#829dd4"),
            ("magenta", "#86a7df"),
            ("cyan", "#7cf8f7"),
            ("orange", "#50f7a3"),
        ]);
        let p = derive(&colors, "dark").expect("derives");
        assert!(
            p.repaired.contains(&"muted".to_string()),
            "muted was invisible and must be reported as repaired"
        );
        assert!(
            apca_lc_abs(&p.colors["muted"], "#0B0C16") >= LC_MUTED,
            "muted must clear its target after derivation"
        );
    }

    #[test]
    fn derive_needs_a_background_and_foreground() {
        assert!(derive(&map(&[("foreground", "#ffffff")]), "dark").is_none());
        assert!(derive(&map(&[("background", "#000000")]), "dark").is_none());
    }

    #[test]
    fn derive_trusts_the_background_over_a_wrong_mode_field() {
        // A dark background mislabelled `light` must still lighten, or every
        // role would be walked toward the background and vanish.
        let colors = map(&[
            ("background", "#1a1b26"),
            ("foreground", "#a9b1d6"),
            ("muted", "#2a2b3a"),
        ]);
        let p = derive(&colors, "light").expect("derives");
        assert!(apca_lc_abs(&p.colors["muted"], "#1a1b26") >= LC_MUTED);
    }

    #[test]
    fn theme_patch_has_the_shape_config_set_expects() {
        let colors = map(&[("background", "#1a1b26"), ("foreground", "#a9b1d6")]);
        let patch = derive(&colors, "dark").unwrap().to_theme_patch();
        assert_eq!(patch["theme"]["source"], "hybrid");
        assert_eq!(patch["theme"]["custom"]["background"], "#1a1b26");
    }

    // ── colors.toml parsing ────────────────────────────────────────────────

    #[test]
    fn parses_an_omarchy_colors_toml() {
        let text = r##"
mode = "dark"

accent = "#7aa2f7"
muted = "#414868"
background = "#1a1b26"
foreground = "#a9b1d6"
not_a_color = "solid"

[some.section]
accent = "#ffffff"
"##;
        let (colors, mode) = parse_colors_toml(text);
        assert_eq!(mode, "dark");
        assert_eq!(colors.get("accent").unwrap(), "#7aa2f7");
        assert_eq!(
            colors.get("background").unwrap(),
            "#1a1b26",
            "the top-level table wins; sections are not read"
        );
        assert!(!colors.contains_key("not_a_color"));
    }

    #[test]
    fn parsing_defaults_to_dark_when_mode_is_absent() {
        let (_, mode) = parse_colors_toml("accent = \"#7aa2f7\"\n");
        assert_eq!(mode, "dark");
    }

    // ── The gate: every shipped Omarchy theme ──────────────────────────────

    /// Enumerates the installed Omarchy themes and asserts each derives a
    /// palette that meets its contrast targets. This is the test that makes
    /// the feature a guarantee rather than a hope: a newly shipped theme that
    /// breaks derivation fails here instead of shipping a washed-out prompt.
    ///
    /// Skips (rather than fails) when Omarchy is not installed, so the suite
    /// still runs on a machine that only has the repo.
    #[test]
    fn every_installed_omarchy_theme_derives_a_readable_palette() {
        let root = std::path::Path::new("/usr/share/omarchy/themes");
        if !root.is_dir() {
            eprintln!("skipping: {} not present", root.display());
            return;
        }

        let mut checked = 0;
        let mut failures = Vec::new();

        for entry in std::fs::read_dir(root).expect("theme dir readable").flatten() {
            let colors_path = entry.path().join("colors.toml");
            let Ok(text) = std::fs::read_to_string(&colors_path) else {
                continue;
            };
            let theme = entry.file_name().to_string_lossy().to_string();
            let (colors, mode) = parse_colors_toml(&text);

            let Some(palette) = derive(&colors, &mode) else {
                failures.push(format!("{theme}: no background/foreground to derive from"));
                continue;
            };
            checked += 1;

            let bg = &palette.colors["background"];
            for (role, hex) in &palette.colors {
                if role == "background" {
                    continue;
                }
                let target = target_for(role);
                let lc = apca_lc_abs(hex, bg);
                if lc < target {
                    failures.push(format!(
                        "{theme}: {role} = {hex} on {bg} is Lc {lc:.1}, needs {target:.0}"
                    ));
                }
            }
        }

        assert!(checked > 0, "found no themes with a colors.toml to check");
        assert!(
            failures.is_empty(),
            "{} of {checked} themes derived an unreadable palette:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
}
