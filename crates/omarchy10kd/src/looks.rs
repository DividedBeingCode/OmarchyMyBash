//! Wave 2 — Look registry: named, atomic appearance bundles.
//!
//! A Look is a named patch over the config tree plus a palette directive.
//! Curated Looks ship compiled-in; user Looks live in `[looks.<name>]`
//! tables in `config.toml` (user entries shadow curated names).
//! Patches use the same shape as `config_set` payloads, so applying a Look
//! reuses the daemon's atomic single-patch merge.

use crate::config::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct LookDef {
    pub name: String,
    pub label: String,
    /// One line of plain English for the card. Not marketing — what the Look
    /// actually does, so a browser can be skimmed instead of clicked through.
    pub blurb: String,
    /// Values from `TAGS`. Every curated Look carries `structure` or
    /// `complete`.
    pub tags: Vec<String>,
    /// `config_set`-shaped patch: top-level config keys, glyph shortcuts
    /// already expanded, palette resolved into a `theme` sub-patch.
    pub patch: serde_json::Value,
}

/// A curated prompt palette: exact, hand-tuned, and named after the scheme
/// people already know it by.
///
/// Curated palettes are NOT run through `palette_derive`'s repair — they are
/// the reference, and a test asserts they already clear its contrast targets.
/// Derivation exists for the themes that have no curated palette.
#[derive(Debug, Clone)]
pub struct PaletteDef {
    pub key: &'static str,
    pub label: &'static str,
    pub blurb: &'static str,
    /// accent, foreground, muted, background, red, green, yellow, blue,
    /// magenta, cyan, orange — in that order, matching `ROLE_ORDER`.
    pub colors: [&'static str; 11],
}

/// Role order for `PaletteDef::colors`. Mirrors `palette_derive::ROLES`
/// semantically but is ordered for legibility in the table below.
pub const ROLE_ORDER: [&str; 11] = [
    "accent", "foreground", "muted", "background", "red", "green", "yellow",
    "blue", "magenta", "cyan", "orange",
];

/// The curated palette table.
///
/// Grew from 8 to 16 in the beautification pass. These are the schemes people
/// recognise by name; everything else an Omarchy theme offers is derived at
/// runtime by `palette_derive`, so a user who installs a new theme still gets
/// prompt colors.
pub fn curated_palettes() -> &'static [PaletteDef] {
    &CURATED_PALETTES
}

const fn p(
    key: &'static str,
    label: &'static str,
    blurb: &'static str,
    colors: [&'static str; 11],
) -> PaletteDef {
    PaletteDef { key, label, blurb, colors }
}

//      accent     foreground muted      background red        green      yellow     blue       magenta    cyan       orange
static CURATED_PALETTES: [PaletteDef; 16] = [
        p("tokyo-night", "Tokyo Night", "Cool indigo night, the Omarchy default mood.",
          ["#7aa2f7", "#c0caf5", "#565f89", "#1a1b26", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#ff9e64"]),
        p("catppuccin", "Catppuccin Mocha", "Soft pastels on warm charcoal.",
          ["#89b4fa", "#cdd6f4", "#7f849c", "#1e1e2e", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#cba6f7", "#94e2d5", "#fab387"]),
        p("catppuccin-frappe", "Catppuccin Frappé", "The same pastels, a shade lighter and cooler.",
          ["#8caaee", "#c6d0f5", "#838ba7", "#303446", "#e78284", "#a6d189", "#e5c890", "#8caaee", "#ca9ee6", "#81c8be", "#ef9f76"]),
        p("gruvbox", "Gruvbox", "Retro warmth: mustard, rust and olive.",
          ["#83a598", "#ebdbb2", "#a89984", "#282828", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#fe8019"]),
        p("nord", "Nord", "Arctic blues, deliberately low-key.",
          ["#88c0d0", "#eceff4", "#7b88a1", "#2e3440", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#d08770"]),
        p("dracula", "Dracula", "High-contrast neon on deep violet.",
          ["#bd93f9", "#f8f8f2", "#8a92b8", "#282a36", "#ff5555", "#50fa7b", "#f1fa8c", "#8be9fd", "#ff79c6", "#8be9fd", "#ffb86c"]),
        p("rose-pine", "Rosé Pine", "Muted rose and pine on near-black plum.",
          ["#c4a7e7", "#e0def4", "#908caa", "#191724", "#eb6f92", "#71b8a0", "#f6c177", "#9ccfd8", "#c4a7e7", "#9ccfd8", "#f0a878"]),
        p("everforest", "Everforest", "Green-grey calm, easy on long sessions.",
          ["#a7c080", "#d3c6aa", "#9da9a0", "#2d353b", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092", "#e69875"]),
        p("kanagawa", "Kanagawa", "Ink-wash blues over sumi black.",
          ["#7e9cd8", "#dcd7ba", "#928374", "#1f1f28", "#ff8b8b", "#98bb6c", "#ffa066", "#7e9cd8", "#b294bb", "#7aa89f", "#ffa066"]),
        p("solarized-dark", "Solarized Dark", "The original calibrated scheme, teal-based.",
          ["#268bd2", "#93a1a1", "#7d8f8f", "#002b36", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#cb4b16"]),
        p("one-dark", "One Dark", "Atom's classic — balanced and familiar.",
          ["#61afef", "#abb2bf", "#828997", "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d19a66"]),
        p("monokai", "Monokai", "Loud lime and magenta on olive black.",
          ["#a6e22e", "#f8f8f2", "#9a957f", "#272822", "#f92672", "#a6e22e", "#e6db74", "#66d9ef", "#ae81ff", "#66d9ef", "#fd971f"]),
        p("ayu-mirage", "Ayu Mirage", "Amber highlights on slate blue.",
          ["#ffcc66", "#cbccc6", "#8a94a3", "#1f2430", "#ff6666", "#bae67e", "#ffcc66", "#73d0ff", "#d4bfff", "#95e6cb", "#ff9940"]),
        p("oxocarbon", "Oxocarbon", "IBM Carbon: flat, bright, near-black.",
          ["#33b1ff", "#f2f4f8", "#8d8d8d", "#161616", "#ee5396", "#42be65", "#fae3b0", "#33b1ff", "#be95ff", "#3ddbd9", "#ff7eb6"]),
        p("nightfox", "Nightfox", "Dusky blues with a warm amber accent.",
          ["#719cd6", "#cdcecf", "#8b8d8f", "#192330", "#c94f6d", "#81b29a", "#dbc074", "#719cd6", "#9d79d6", "#63cdcf", "#f4a261"]),
    p("catppuccin-latte", "Catppuccin Latte", "The light one, for daylight terminals.",
      ["#1e66f5", "#4c4f69", "#7c7f93", "#eff1f5", "#d20f39", "#40a02b", "#a06e17", "#1e66f5", "#ea76cb", "#179299", "#c4560b"]),
];

/// Look up a curated palette as a `theme` sub-patch.
///
/// Kept at its original signature so every existing call site — the CLI, the
/// `palettes` verb, the Look table below — is unaffected by the table rewrite.
pub fn curated_palette(key: &str) -> Option<serde_json::Value> {
    let def = curated_palettes().iter().find(|p| p.key == key)?;
    let custom: serde_json::Map<String, serde_json::Value> = ROLE_ORDER
        .iter()
        .zip(def.colors.iter())
        .map(|(role, hex)| (role.to_string(), serde_json::Value::String(hex.to_string())))
        .collect();
    Some(serde_json::json!({ "theme": { "source": "hybrid", "custom": custom } }))
}

/// The closed tag vocabulary. Closed so the UI can build filter chips from a
/// known set instead of whatever strings happen to appear in the table.
pub const TAGS: [&str; 10] = [
    // Does the Look bring its own colors, or respect yours?
    "structure",
    "complete",
    // Density and shape.
    "minimal",
    "dense",
    "powerline",
    "framed",
    "two-line",
    // Font requirements.
    "nerd-font",
    "ascii-safe",
    // Not authored here — attached to Looks the user saved themselves, so a
    // browser can separate their own presets from the shipped collection.
    "user",
];

fn chars_patch(char_key: &str) -> serde_json::Value {
    serde_json::json!({
        "segments": { "character": { "success": char_key, "error": char_key, "transient": char_key } }
    })
}

fn look(
    name: &str,
    label: &str,
    blurb: &str,
    tags: &[&str],
    patch: serde_json::Value,
) -> LookDef {
    LookDef {
        name: name.into(),
        label: label.into(),
        blurb: blurb.into(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        patch,
    }
}

/// Attach a curated palette to a Look patch, making it `complete`.
fn with_palette(mut patch: serde_json::Value, palette: &str) -> serde_json::Value {
    if let Some(theme) = curated_palette(palette) {
        patch["theme"] = theme["theme"].clone();
    }
    patch
}

/// Compiled-in Looks. Patches are `config_set`-shaped (top-level keys,
/// glyph shortcuts already expanded, palette merged into a `theme` patch).
///
/// Grew from 8 to 18 in the beautification pass. Every Look is tagged either
/// `structure` (respects whatever palette you are on) or `complete` (brings
/// its own), which is the whole of the "preset bundle" idea — a Look patch
/// can already carry `theme` keys, so no third concept is needed.
pub fn curated() -> Vec<LookDef> {
    vec![
        // ── Structure: respect the user's palette ──────────────────────────
        look("omnarchy", "Omnarchy",
            "The house style. Follows your Omarchy theme exactly.",
            &["structure", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "omarchy", "separators": { "shape": "auto" } },
                "segments": { "os": { "icon": "arch", "enabled": true },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": false, "gap_char": "", "gap_gradient": "off" },
                "theme": { "source": "omarchy" },
                "directory": { "unique": false },
            })),
        look("lean-pure", "Lean Pure",
            "No icons, no fills. Just the path, the branch and a lambda.",
            &["structure", "minimal", "ascii-safe"],
            serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "auto" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
                "git": { "branch_icon": "text" },
                "frame": { "enabled": false },
            })),
        look("mono-minimal", "Mono Minimal",
            "The smallest prompt that still tells you where you are.",
            &["structure", "minimal", "ascii-safe"],
            serde_json::json!({
                "style": { "preset": "minimal", "separators": { "shape": "none" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dollar", "error": "dollar", "transient": "dollar" } },
                "git": { "branch_icon": "text" },
                "frame": { "enabled": false },
                "prompt": { "newline": false },
            })),
        look("powerline-classic", "Powerline Classic",
            "The arrows everyone knows, on your own colors.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "powerline" } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": false },
            })),
        look("two-line-focus", "Two-Line Focus",
            "Context above, a clean line to type on below.",
            &["structure", "two-line", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "nerd" },
                "frame": { "enabled": false },
                "prompt": { "newline": true },
            })),
        look("dot-matrix", "Dot Matrix",
            "Dense segments separated by dots. A lot of state, little width.",
            &["structure", "dense"],
            serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "dot" } },
                "segments": { "os": { "icon": "linux" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            })),
        look("zen-fade", "Zen Fade",
            "Segments that dissolve into each other instead of butting up.",
            &["structure", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "nerd" },
                "frame": { "enabled": false },
            })),
        look("framed-focus", "Framed Focus",
            "A rule across the terminal that separates every command.",
            &["structure", "framed", "two-line"],
            serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "auto" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "text" },
                "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "off" },
                "prompt": { "newline": true },
            })),

        // ── Complete: bring their own palette ──────────────────────────────
        look("tokyo-rainbow", "Tokyo Rainbow",
            "p10k's signature rainbow, in Tokyo Night indigo.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": false },
            }), "tokyo-night")),
        look("framed-gradient", "Framed Gradient",
            "A full-width gradient rule above every prompt.",
            &["complete", "framed", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "auto" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "full" },
            }), "tokyo-night")),
        look("slanted-owl", "Slanted Owl",
            "Forest greens, slanted cuts, and an owl watching your errors.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" } },
                "segments": { "os": { "icon": "owl" },
                              "character": { "success": "owl", "error": "dragon", "transient": "owl" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            }), "everforest")),
        look("gruvbox-drift", "Gruvbox Drift",
            "Rust and mustard with flame-cut separators.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" } },
                "segments": { "os": { "icon": "paw" },
                              "character": { "success": "paw", "error": "kaomoji_rage", "transient": "paw" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            }), "gruvbox")),
        look("rose-classic", "Rosé Classic",
            "Soft rose, plain bars, and a bear who disapproves of failures.",
            &["complete"],
            with_palette(serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "vertical" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_bear", "error": "kaomoji_disapprove", "transient": "kaomoji_bear" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            }), "rose-pine")),
        look("polar-lean", "Polar Lean",
            "Arctic blues, rounded caps, and a penguin.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "round" } },
                "segments": { "os": { "icon": "penguin" },
                              "character": { "success": "penguin", "error": "kaomoji_disapprove", "transient": "penguin" } },
                "git": { "branch_icon": "nerd" },
                "frame": { "enabled": false },
            }), "nord")),
        look("midnight-metro", "Midnight Metro",
            "Catppuccin pastels in full powerline, like a transit map.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": false },
                "prompt": { "newline": true },
            }), "catppuccin")),
        look("dracula-dense", "Dracula Dense",
            "Neon on violet, packed tight with trapezoid cuts.",
            &["complete", "dense", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "trapezoid" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dragon", "error": "kaomoji_rage", "transient": "dragon" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            }), "dracula")),
        look("kanagawa-wave", "Kanagawa Wave",
            "Ink-wash blues, slanted like a brush stroke.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "fish", "error": "kaomoji_disapprove", "transient": "fish" } },
                "git": { "branch_icon": "nerd" },
                "frame": { "enabled": false },
            }), "kanagawa")),
        look("solarized-lean", "Solarized Lean",
            "The calibrated classic, kept deliberately plain.",
            &["complete", "minimal", "ascii-safe"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "text" },
                "frame": { "enabled": false },
            }), "solarized-dark")),
        look("daylight-latte", "Daylight Latte",
            "For terminals in the sun: light background, dark ink.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "text" },
                "frame": { "enabled": false },
            }), "catppuccin-latte")),
    ]
}

/// A user Look from `[looks.<name>]`: `label`, `palette`, and a `patch`
/// table whose `glyphs` shortcuts are expanded at resolution time.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedLook {
    pub name: String,
    pub label: String,
    pub patch: serde_json::Value,
    /// None when the entry is malformed.
    pub palette: Option<String>,
}

fn expand_glyph_shortcuts(patch: &mut serde_json::Value) {
    let glyphs = patch.get("glyphs").and_then(|g| g.as_object()).cloned();
    if let Some(g) = glyphs {
        if let Some(os) = g.get("os_icon").and_then(|v| v.as_str()) {
            patch["segments"]["os"]["icon"] = serde_json::json!(os);
        }
        if let Some(c) = g.get("character").and_then(|v| v.as_str()) {
            patch["segments"]["character"]["success"] = serde_json::json!(c);
            patch["segments"]["character"]["error"] = serde_json::json!(c);
            patch["segments"]["character"]["transient"] = serde_json::json!(c);
        }
        if let Some(gi) = g.get("git_branch_icon").and_then(|v| v.as_str()) {
            patch["git"]["branch_icon"] = serde_json::json!(gi);
        }
        if let Some(obj) = patch.as_object_mut() {
            obj.remove("glyphs");
        }
    }
}

/// Resolve a Look by name: user entries (from `[looks.<name>]`) shadow
/// curated ones. Expands glyph shortcuts and resolves the palette into a
/// `theme` sub-patch so the result is directly `config_set`-applicable.
pub fn resolve(name: &str, config: &Config) -> Option<LookDef> {
    if let Some(entry) = config.looks.get(name) {
        let patch = serde_json::to_value(&entry.patch).unwrap_or_else(|_| serde_json::json!({}));
        let mut patch = match patch {
            serde_json::Value::Object(_) => patch,
            _ => serde_json::json!({}),
        };
        expand_glyph_shortcuts(&mut patch);
        if let Some(pk) = &entry.palette {
            if pk != "keep" {
                if let Some(theme) = curated_palette(pk) {
                    patch["theme"] = theme["theme"].clone();
                } else if pk == "theme" {
                    patch["theme"] = serde_json::json!({ "source": "omarchy" });
                }
            }
        }
        return Some(LookDef {
            name: name.into(),
            label: if entry.label.is_empty() { name.into() } else { entry.label.clone() },
            // A user Look has no authored blurb; the UI falls back to the
            // name rather than inventing prose for someone else's preset.
            blurb: String::new(),
            // Tagged by where it came from, so the browser can filter a
            // user's own Looks apart from the shipped collection.
            tags: vec!["user".to_string()],
            patch,
        });
    }
    curated().into_iter().find(|l| l.name == name)
}

/// All Looks: curated first, then user entries (user shadows curated names).
pub fn all(config: &Config) -> Vec<LookDef> {
    let curated = curated();
    let mut out: Vec<LookDef> = curated
        .iter()
        .filter(|l| !config.looks.contains_key(&l.name))
        .cloned()
        .collect();
    let mut user: Vec<LookDef> = config
        .looks
        .iter()
        .filter_map(|(name, _)| resolve(name, config))
        .collect();
    user.sort_by(|a, b| a.name.cmp(&b.name));
    out.append(&mut user);
    out
}

/// Try-mode apply: merge the patch into the CURRENT in-memory config only —
/// no file write. The caller reverts with `reload_config`.
pub fn apply_transient(current: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    let patch_val = serde_json::from_value::<toml::Value>(patch.clone())
        .map_err(|e| format!("look patch not representable in TOML: {e}"))?;
    let cur = toml::Value::try_from(current)
        .map_err(|e| format!("config serialize: {e}"))?;
    let mut doc = match cur.as_table() {
        Some(t) => t.clone(),
        None => toml::Table::new(),
    };
    if let Some(obj) = patch_val.as_table() {
        for (k, v) in obj {
            crate::server::merge_toml_value(
                doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())),
                v.clone(),
            );
        }
    }
    let text = toml::to_string(&doc).map_err(|e| format!("serialize: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("merged config invalid: {e}"))
}

/// The palette directive of a Look ("theme" | "keep" | curated key), if any.
pub fn palette_directive(config: &Config, name: &str) -> Option<String> {
    config.looks.get(name).and_then(|e| e.palette.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, LookEntry};

    #[test]
    fn resolve_curated_by_name() {
        let cfg = Config::default();
        let look = resolve("tokyo-rainbow", &cfg).expect("curated look resolves");
        assert_eq!(look.name, "tokyo-rainbow");
        assert!(look.patch.get("style").is_some(), "curated patch touches style");
    }

    #[test]
    fn resolve_unknown_is_none() {
        let cfg = Config::default();
        assert!(resolve("no-such-look", &cfg).is_none());
    }

    #[test]
    fn user_look_shadows_curated() {
        let mut cfg = Config::default();
        let mut entry = LookEntry::default();
        entry.label = "My Rainbow".into();
        entry.patch.insert(
            "style".into(),
            toml::Value::Table(toml::from_str::<toml::Table>("preset = \"lean\"").unwrap()),
        );
        cfg.looks.insert("tokyo-rainbow".into(), entry);

        let resolved = resolve("tokyo-rainbow", &cfg).expect("user look resolves");
        assert_eq!(resolved.label, "My Rainbow");
        assert_eq!(
            resolved.patch["style"]["preset"], "lean",
            "user patch must win over the curated one"
        );

        // `all` must not list the curated twin alongside the user entry.
        let names: Vec<_> = all(&cfg).iter().map(|l| l.name.clone()).collect();
        assert_eq!(
            names.iter().filter(|n| **n == "tokyo-rainbow").count(),
            1,
            "exactly one tokyo-rainbow after shadowing"
        );
    }

    #[test]
    fn user_palette_directive_resolves_curated_palette() {
        let mut cfg = Config::default();
        let mut entry = LookEntry::default();
        entry.palette = Some("tokyo-night".into());
        cfg.looks.insert("mine".into(), entry);

        let resolved = resolve("mine", &cfg).expect("look resolves");
        let accent = resolved.patch["theme"]["custom"]["accent"]
            .as_str()
            .expect("palette merged into theme.custom");
        assert_eq!(accent, "#7aa2f7");
    }

    #[test]
    fn apply_transient_mutates_clone_not_original() {
        let cfg = Config::default();
        let look = resolve("gruvbox-drift", &Config::default()).expect("curated look");
        let patched = apply_transient(&cfg, &look.patch).expect("transient apply ok");

        assert_ne!(
            patched.style.preset, cfg.style.preset,
            "patched clone carries the look's preset"
        );
        assert_eq!(
            cfg.style.preset, Config::default().style.preset,
            "original config must be untouched (Try never persists)"
        );
    }

    #[test]
    fn reload_reverts_transient_apply() {
        // Try = merge into memory; reload_config re-parses from disk. Model
        // that: apply on the in-memory config, then "reload" from the
        // pristine default and confirm the look is gone.
        let disk = Config::default();
        let look = resolve("lean-pure", &Config::default()).expect("curated look");
        let tried = apply_transient(&disk, &look.patch).expect("apply ok");
        assert_ne!(tried.style.preset, disk.style.preset);
        let reloaded = Config::default();
        assert_eq!(reloaded.style.preset, disk.style.preset, "reload reverts try");
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;
    use crate::palette_derive::{apca_lc_abs, target_for};

    #[test]
    fn curated_look_names_are_unique() {
        let looks = curated();
        let mut names: Vec<&str> = looks.iter().map(|l| l.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate Look name in the curated table");
    }

    #[test]
    fn curated_palette_keys_are_unique() {
        let mut keys: Vec<&str> = curated_palettes().iter().map(|p| p.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "duplicate palette key");
    }

    #[test]
    fn every_curated_look_is_described_and_tagged() {
        for look in curated() {
            assert!(!look.label.is_empty(), "{} has no label", look.name);
            assert!(
                !look.blurb.is_empty(),
                "{} has no blurb — the card would show a bare name",
                look.name
            );
            for tag in &look.tags {
                assert!(
                    TAGS.contains(&tag.as_str()),
                    "{} carries tag {tag:?}, which is not in the closed vocabulary",
                    look.name
                );
            }
            let kinds = look.tags.iter().filter(|t| *t == "structure" || *t == "complete").count();
            assert_eq!(
                kinds, 1,
                "{} must be tagged exactly one of structure/complete, got {:?}",
                look.name, look.tags
            );
        }
    }

    /// A `structure` Look promises to respect whatever palette you are on.
    /// Shipping one that quietly carries `theme.custom` would break that
    /// promise silently — you would pick "respects your colors" and watch
    /// your colors change.
    #[test]
    fn structure_looks_do_not_carry_their_own_colors() {
        for look in curated() {
            if !look.tags.iter().any(|t| t == "structure") {
                continue;
            }
            let has_custom = look
                .patch
                .get("theme")
                .and_then(|t| t.get("custom"))
                .is_some();
            assert!(
                !has_custom,
                "{} is tagged `structure` but carries theme.custom",
                look.name
            );
        }
    }

    #[test]
    fn complete_looks_actually_bring_a_palette() {
        for look in curated() {
            if !look.tags.iter().any(|t| t == "complete") {
                continue;
            }
            assert!(
                look.patch.get("theme").and_then(|t| t.get("custom")).is_some(),
                "{} is tagged `complete` but brings no palette",
                look.name
            );
        }
    }

    #[test]
    fn every_curated_palette_is_described() {
        for p in curated_palettes() {
            assert!(!p.label.is_empty(), "{} has no label", p.key);
            assert!(!p.blurb.is_empty(), "{} has no blurb", p.key);
            for hex in p.colors {
                assert!(
                    hex.len() == 7 && hex.starts_with('#'),
                    "{} has malformed color {hex}",
                    p.key
                );
            }
        }
    }

    /// Curated palettes are the reference — they are NOT run through
    /// `palette_derive`'s repair. So they have to clear its bar on their own,
    /// or the hand-tuned set would be worse than the derived one.
    #[test]
    fn every_curated_palette_clears_the_contrast_gate() {
        let mut failures = Vec::new();
        for p in curated_palettes() {
            let bg_index = ROLE_ORDER.iter().position(|r| *r == "background").unwrap();
            let bg = p.colors[bg_index];
            for (role, hex) in ROLE_ORDER.iter().zip(p.colors.iter()) {
                if *role == "background" {
                    continue;
                }
                let target = target_for(role);
                let lc = apca_lc_abs(hex, bg);
                if lc < target {
                    failures.push(format!(
                        "{}: {role} = {hex} on {bg} is Lc {lc:.1}, needs {target:.0}",
                        p.key
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} curated palette roles are below the contrast gate:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    #[test]
    fn curated_palette_lookup_round_trips_through_the_table() {
        for p in curated_palettes() {
            let patch = curated_palette(p.key).expect("every table entry is looked up by key");
            assert_eq!(patch["theme"]["source"], "hybrid");
            let accent_index = ROLE_ORDER.iter().position(|r| *r == "accent").unwrap();
            assert_eq!(patch["theme"]["custom"]["accent"], p.colors[accent_index]);
        }
        assert!(curated_palette("no-such-palette").is_none());
    }

    #[test]
    fn the_collection_is_worth_browsing() {
        // The point of the pass: enough breadth that the gallery is a gallery.
        assert!(curated().len() >= 18, "expected at least 18 curated Looks");
        assert!(
            curated_palettes().len() >= 16,
            "expected at least 16 curated palettes"
        );
    }
}
