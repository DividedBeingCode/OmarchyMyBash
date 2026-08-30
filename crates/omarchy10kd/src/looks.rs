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
    /// Art-directed gradient ramp, start → end. `None` lets the palette
    /// derive one by rotating the accent's hue in OKLCH, which is right for
    /// almost every palette; this exists for the handful where the derived
    /// sweep undersells the scheme people know it by.
    pub ramp: Option<[&'static str; 2]>,
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
    PaletteDef { key, label, blurb, colors, ramp: None }
}

/// A palette with an art-directed ramp, overriding the OKLCH derivation.
const fn pr(
    key: &'static str,
    label: &'static str,
    blurb: &'static str,
    colors: [&'static str; 11],
    ramp: [&'static str; 2],
) -> PaletteDef {
    PaletteDef { key, label, blurb, colors, ramp: Some(ramp) }
}

//      accent     foreground muted      background red        green      yellow     blue       magenta    cyan       orange
static CURATED_PALETTES: [PaletteDef; 39] = [
    p("tokyo-night", "Tokyo Night", "Cool indigo night, the Omarchy default mood.",
      ["#7aa2f7", "#c0caf5", "#565f89", "#1a1b26", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#ff9e64"]),
    p("catppuccin", "Catppuccin Mocha", "Soft pastels on warm charcoal.",
      ["#89b4fa", "#cdd6f4", "#7f849c", "#1e1e2e", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#cba6f7", "#94e2d5", "#fab387"]),
    p("catppuccin-frappe", "Catppuccin Frappé", "The same pastels, a shade lighter and cooler.",
      ["#8caaee", "#c6d0f5", "#838ba7", "#303446", "#e78284", "#a6d189", "#e5c890", "#8caaee", "#ca9ee6", "#81c8be", "#ef9f76"]),
    // Gruvbox aqua is nearly grey, so a derived sweep barely moves. Aqua to
    // mustard runs straight through the scheme's own green.
    pr("gruvbox", "Gruvbox", "Retro warmth: mustard, rust and olive.",
      ["#83a598", "#ebdbb2", "#a89984", "#282828", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#fe8019"],
       ["#83a598", "#fabd2f"]),
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
    // Ayu is amber; the derived sweep pulls it toward green, away from the
    // thing the scheme is named for.
    pr("ayu-mirage", "Ayu Mirage", "Amber highlights on slate blue.",
      ["#ffcc66", "#cbccc6", "#8a94a3", "#1f2430", "#ff6666", "#bae67e", "#ffcc66", "#73d0ff", "#d4bfff", "#95e6cb", "#ff9940"],
       ["#ffcc66", "#ff9940"]),
    p("oxocarbon", "Oxocarbon", "IBM Carbon: flat, bright, near-black.",
      ["#33b1ff", "#f2f4f8", "#8d8d8d", "#161616", "#ee5396", "#42be65", "#fae3b0", "#33b1ff", "#be95ff", "#3ddbd9", "#ff7eb6"]),
    p("nightfox", "Nightfox", "Dusky blues with a warm amber accent.",
      ["#719cd6", "#cdcecf", "#8b8d8f", "#192330", "#c94f6d", "#81b29a", "#dbc074", "#719cd6", "#9d79d6", "#63cdcf", "#f4a261"]),
    p("catppuccin-latte", "Catppuccin Latte", "The light one, for daylight terminals.",
      ["#1e66f5", "#4c4f69", "#7c7f93", "#eff1f5", "#d20f39", "#40a02b", "#a06e17", "#1e66f5", "#ea76cb", "#179299", "#c4560b"]),

    // ── Neon / cyberpunk, then more community classics ────────────────────
    //
    // Adapted from the canonical iTerm2-Color-Schemes collection
    // (mbadolato/iTerm2-Color-Schemes, MIT), mapped from its Windows Terminal
    // JSON onto this project's eleven roles.
    //
    // The mapping is not a straight copy, and three things about it are worth
    // knowing:
    //
    //   * ANSI has no `orange` slot, so orange is the OKLCH midpoint of the
    //     scheme's own red and yellow -- in the scheme's own hues rather than
    //     a generic orange.
    //   * `accent` prefers the scheme's cursor color when that color actually
    //     carries chroma (plenty of schemes set it to plain white), else the
    //     first of purple/cyan/blue/yellow/green that does. RED IS NEVER the
    //     accent: in a prompt, red means error.
    //   * Roles below this module's contrast floors were repaired at
    //     authoring time by the same OKLCH walk `palette_derive` uses, so
    //     these clear `every_curated_palette_clears_the_contrast_gate`
    //     without runtime repair. Where a value differs from upstream, that
    //     is why.
    p("synthwave-alpha", "Synthwave Alpha", "Purple grid, sunset horizon, 1984 forever.",
      ["#d53bce", "#f2f2e3", "#7f7094", "#241b30", "#e60a70", "#00986c", "#adad3e", "#9151d5", "#c120ba", "#00b0b1", "#e46d00"]),
    p("outrun-electric", "Outrun Electric", "Chrome and neon, driving music at 3am.",
      ["#ff2afc", "#f2f3f7", "#546a90", "#0c0a20", "#e61f44", "#a7da1e", "#ffd400", "#1ea8fc", "#ff2afc", "#42c6ff", "#ff8000"]),
    p("neon", "Neon", "Electric cyan and magenta. Maximum voltage.",
      ["#f924e7", "#00fffc", "#686868", "#14161a", "#ff3045", "#5ffa74", "#fffc7e", "#2b5dff", "#f924e7", "#00fffc", "#ffa100"]),
    p("cyberpunk", "Cyberpunk", "Teal and hot pink over deep violet.",
      ["#21f6bc", "#e5e5e5", "#6a6a6a", "#332a57", "#ff7092", "#00fbac", "#fffa6a", "#00bfff", "#df95ff", "#86cbfe", "#ffac4c"]),
    p("scarlet-protocol", "Scarlet Protocol", "Scarlet ink on black, with acid green.",
      ["#76ff9f", "#ff587a", "#686868", "#101116", "#ff0051", "#01dc84", "#faf945", "#0271b6", "#c930c7", "#00c5c7", "#ff9300"]),
    p("laser", "Laser", "Magenta text, green cursor, no apologies.",
      ["#00ff9c", "#ff2bf1", "#8f8f8f", "#030d18", "#ff8373", "#b4fb73", "#09b4bd", "#fed300", "#ff90fe", "#d1d1fe", "#a3af3f"]),
    p("blue-matrix", "Blue Matrix", "Terminal-green rain on cold blue.",
      ["#76ff9f", "#00a2ff", "#686868", "#101116", "#ff5680", "#00ff9c", "#fffc58", "#00b0ff", "#d57bff", "#76c1ff", "#ffa219"]),
    p("vaporwave-sunset", "Vaporwave Sunset", "Hot pink and cyan on twilight purple.",
      ["#ff4fd8", "#fff7ed", "#6d5586", "#180827", "#fb7185", "#2dd4bf", "#fb923c", "#a78bfa", "#ff4fd8", "#22d3ee", "#ff7f60"]),
    p("aura", "Aura", "Violet on near-black, unapologetically saturated.",
      ["#a277ff", "#cdccce", "#5c5c5c", "#15141b", "#ff6767", "#61ffca", "#ffca85", "#a277ff", "#61ffca", "#a277ff", "#ff9b62"]),
    p("andromeda", "Andromeda", "Deep space navy with a magenta pulse.",
      ["#ca4dc9", "#e5e5e5", "#666666", "#262a33", "#d43937", "#05bc79", "#e5e512", "#2b78cf", "#bc3fbc", "#0fa8cd", "#f38900"]),
    // Hazard yellow into its own orange. Deriving sends it green, which is a
    // color Cobalt2 does not really have.
    pr("cobalt2", "Cobalt2", "Cobalt and hazard yellow. Wes Bos's.",
      ["#f0cc09", "#ffffff", "#666666", "#132738", "#ff0000", "#38de21", "#ffe50a", "#236de0", "#ff005d", "#00bbbb", "#ff8d00"],
       ["#f0cc09", "#ff8d00"]),
    p("snazzy", "Snazzy", "Hyper's bright, friendly palette.",
      ["#fc4cb4", "#ebece6", "#606060", "#1e1f29", "#fc4346", "#50fb7c", "#f0fb8c", "#49baff", "#fc4cb4", "#8be9fe", "#ffa700"]),
    p("night-owl", "Night Owl", "Built for late nights and low light.",
      ["#9069d6", "#d6deeb", "#5d5c5c", "#011627", "#ef5350", "#22da6e", "#addb67", "#82aaff", "#c792ea", "#21c7a8", "#e99a00"]),
    p("material-ocean", "Material Ocean", "Material's deepest navy.",
      ["#82aaff", "#9599a8", "#546e7a", "#0f111a", "#ff5370", "#c3e88d", "#ffcb6b", "#82aaff", "#c792ea", "#89ddff", "#ff914c"]),
    p("horizon", "Horizon", "Sunset pinks and teals on slate.",
      ["#ee64ac", "#d5d8da", "#666666", "#1c1e26", "#e95678", "#29d398", "#fab795", "#26bbd9", "#ee64ac", "#59e1e3", "#f6887a"]),
    p("monokai-pro", "Monokai Pro", "The Monokai refit: softer, still punchy.",
      ["#ab9df2", "#fcfcfa", "#727072", "#2d2a2e", "#ff6188", "#a9dc76", "#ffd866", "#fc9867", "#ab9df2", "#78dce8", "#ff9a52"]),
    p("sonokai", "Sonokai", "Monokai's high-contrast successor.",
      ["#b39df3", "#e2e2e3", "#7f8490", "#2c2e34", "#fc5d7c", "#9ed072", "#e7c664", "#76cce0", "#b39df3", "#f39660", "#ff9249"]),
    p("poimandres", "Poimandres", "Teal and mist, deliberately desaturated.",
      ["#5de4c7", "#a6accd", "#63677c", "#1a1e28", "#d0679d", "#5de4c7", "#fffac2", "#89ddff", "#fcc5e9", "#add7ff", "#fdac85"]),
    p("iceberg", "Iceberg", "Cold, restrained blues. Very quiet.",
      ["#a093c7", "#c6c8d1", "#6b7089", "#161821", "#e27878", "#b4be82", "#e2a478", "#84a0c6", "#a093c7", "#89b8c2", "#e48e72"]),
    p("everblush", "Everblush", "Soft botanical greens, low glare.",
      ["#c47fd5", "#dadada", "#565f61", "#141b1e", "#e57474", "#8ccf7e", "#e5c76b", "#67b0e8", "#c47fd5", "#6cbfbf", "#f09b5b"]),
    p("zenburn", "Zenburn", "The original low-contrast scheme, from 2003.",
      ["#dc8cc3", "#dcdccc", "#709080", "#3f3f3f", "#9b7a7a", "#60b48a", "#f0dfaf", "#748495", "#dc8cc3", "#8cd0d3", "#bc9982"]),
    p("rose-pine-moon", "Rosé Pine Moon", "Rosé Pine's moonlit variant.",
      ["#c4a7e7", "#e0def4", "#6e6a86", "#232136", "#eb6f92", "#3e8fb0", "#f6c177", "#9ccfd8", "#c4a7e7", "#ea9a97", "#fc9572"]),
    p("tokyo-night-storm", "Tokyo Night Storm", "Tokyo Night with the lights up a notch.",
      ["#bb9af7", "#c0caf5", "#5f6687", "#24283b", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#f49168"]),
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
    let mut theme = serde_json::json!({ "source": "hybrid", "custom": custom });
    if let Some(ramp) = def.ramp {
        theme["ramp"] = serde_json::json!([ramp[0], ramp[1]]);
    }
    Some(serde_json::json!({ "theme": theme }))
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
                "style": { "preset": "omarchy", "separators": { "shape": "auto" }, "frame": { "enabled": false, "gap_char": "", "gap_gradient": "off" } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "theme": { "source": "omarchy" },
            })),
        look("lean-pure", "Lean Pure",
            "No icons, no fills. Just the path, the branch and a lambda.",
            &["structure", "minimal", "ascii-safe"],
            serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "auto" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
                "git": { "branch_icon": "text" },
            })),
        look("mono-minimal", "Mono Minimal",
            "The smallest prompt that still tells you where you are.",
            &["structure", "minimal", "ascii-safe"],
            serde_json::json!({
                "style": { "preset": "minimal", "separators": { "shape": "none" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dollar", "error": "dollar", "transient": "dollar" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": false },
            })),
        look("powerline-classic", "Powerline Classic",
            "The arrows everyone knows, on your own colors.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            })),
        look("two-line-focus", "Two-Line Focus",
            "Context above, a clean line to type on below.",
            &["structure", "two-line", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "nerd" },
                "prompt": { "newline": true },
            })),
        look("dot-matrix", "Dot Matrix",
            "Dense segments separated by dots. A lot of state, little width.",
            &["structure", "dense", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "linux" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "octicon" },
            })),
        look("zen-fade", "Zen Fade",
            "Segments that dissolve into each other instead of butting up.",
            &["structure", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "nerd" },
            })),
        look("framed-focus", "Framed Focus",
            "A rule across the terminal that separates every command.",
            &["structure", "framed", "two-line"],
            serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "auto" }, "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "off" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": true },
            })),

        // ── Complete: bring their own palette ──────────────────────────────
        look("tokyo-rainbow", "Tokyo Rainbow",
            "p10k's signature rainbow, in Tokyo Night indigo.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            }), "tokyo-night")),
        look("framed-gradient", "Framed Gradient",
            "A full-width gradient rule above every prompt.",
            &["complete", "framed", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "auto" }, "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "full" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            }), "tokyo-night")),
        look("slanted-owl", "Slanted Owl",
            "Forest greens, slanted cuts, and an owl watching your errors.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "owl" },
                              "character": { "success": "owl", "error": "dragon", "transient": "owl" } },
                "git": { "branch_icon": "octicon" },
            }), "everforest")),
        look("gruvbox-drift", "Gruvbox Drift",
            "Rust and mustard with flame-cut separators.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "paw" },
                              "character": { "success": "paw", "error": "kaomoji_rage", "transient": "paw" } },
                "git": { "branch_icon": "octicon" },
            }), "gruvbox")),
        look("rose-classic", "Rosé Classic",
            "Soft rose, plain bars, and a bear who disapproves of failures.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_bear", "error": "kaomoji_disapprove", "transient": "kaomoji_bear" } },
                "git": { "branch_icon": "octicon" },
            }), "rose-pine")),
        look("polar-lean", "Polar Lean",
            "Arctic blues, rounded caps, and a penguin.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "round" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "penguin" },
                              "character": { "success": "penguin", "error": "kaomoji_disapprove", "transient": "penguin" } },
                "git": { "branch_icon": "nerd" },
            }), "nord")),
        look("midnight-metro", "Midnight Metro",
            "Catppuccin pastels in full powerline, like a transit map.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "prompt": { "newline": true },
            }), "catppuccin")),
        look("dracula-dense", "Dracula Dense",
            "Neon on violet, packed tight with trapezoid cuts.",
            &["complete", "dense", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "trapezoid" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dragon", "error": "kaomoji_rage", "transient": "dragon" } },
                "git": { "branch_icon": "octicon" },
            }), "dracula")),
        look("kanagawa-wave", "Kanagawa Wave",
            "Ink-wash blues, slanted like a brush stroke.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "fish", "error": "kaomoji_disapprove", "transient": "fish" } },
                "git": { "branch_icon": "nerd" },
            }), "kanagawa")),
        look("solarized-lean", "Solarized Lean",
            "The calibrated classic, kept deliberately plain.",
            &["complete", "minimal", "ascii-safe"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "text" },
            }), "solarized-dark")),
        // ── Neon / cyberpunk ──────────────────────────────────────────────
        look("neon-grid", "Neon Grid",
            "Full rainbow powerline in electric cyan and magenta.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "powerline" },
                "prompt": { "newline": true },
            }), "neon")),
        look("outrun", "Outrun",
            "Flame-cut segments and a chrome horizon. Drive.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "arrow", "error": "arrow", "transient": "arrow" } },
                "git": { "branch_icon": "nerd" },
            }), "outrun-electric")),
        look("synthwave", "Synthwave",
            "A gradient rule across the grid, purple all the way down.",
            &["complete", "framed", "two-line", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "slanted" }, "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "full" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "prompt": { "newline": true },
            }), "synthwave-alpha")),
        look("scarlet-protocol", "Scarlet Protocol",
            "Dense trapezoid segments, scarlet on black. Reads like a HUD.",
            &["complete", "dense", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "trapezoid" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "angle", "error": "kaomoji_rage", "transient": "angle" } },
                "git": { "branch_icon": "octicon" },
            }), "scarlet-protocol")),
        look("matrix-rain", "Matrix Rain",
            "Terminal green on cold blue, stripped to nothing but the path.",
            &["complete", "minimal", "ascii-safe"],
            with_palette(serde_json::json!({
                "style": { "preset": "minimal", "separators": { "shape": "none" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": false },
            }), "blue-matrix")),
        look("laser-focus", "Laser Focus",
            "Magenta ink, one clean line to type on.",
            &["complete", "two-line", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": true },
            }), "laser")),
        look("vapor-drift", "Vapor Drift",
            "Segments that fade into twilight. Hot pink over deep purple.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "nerd" },
            }), "vaporwave-sunset")),
        look("cobalt-hazard", "Cobalt Hazard",
            "Cobalt blue with hazard-yellow highlights.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            }), "cobalt2")),

        // ── Quieter additions ─────────────────────────────────────────────
        look("night-owl-lean", "Night Owl Lean",
            "For 2am: low glare, nothing shouting.",
            &["complete", "minimal", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "owl", "error": "owl", "transient": "owl" } },
                "git": { "branch_icon": "nerd" },
            }), "night-owl")),
        look("poimandres-zen", "Poimandres Zen",
            "Desaturated teal and mist. As calm as a prompt gets.",
            &["complete", "minimal", "ascii-safe"],
            with_palette(serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "none" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "text" },
            }), "poimandres")),
                look("daylight-latte", "Daylight Latte",
            "For terminals in the sun: light background, dark ink.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "text" },
            }), "catppuccin-latte")),

        // ── Ukiyo: the Japan glyph family, unused until now ───────────────
        look("torii-dusk", "Torii Dusk",
            "Ink-wash blues and a gate at the end of the path.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "torii", "error": "torii", "transient": "torii" } },
                "git": { "branch_icon": "octicon" },
            }), "kanagawa")),
        look("sushi-bar", "Sushi Bar",
            "Muted rose, plain bars, one piece at a time.",
            &["complete", "minimal", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sushi", "error": "kaomoji_rage", "transient": "sushi" } },
                "git": { "branch_icon": "text" },
            }), "rose-pine")),
        look("ramen-shop", "Ramen Shop",
            "Warm broth colors, dense as a full counter.",
            &["complete", "dense", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "noodles", "error": "kaomoji_rage", "transient": "noodles" } },
                "git": { "branch_icon": "octicon" },
            }), "gruvbox")),
        look("sakura-drift", "Sakura Drift",
            "Petals dissolving between segments.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sakura", "error": "sakura", "transient": "sakura" } },
                "git": { "branch_icon": "nerd" },
            }), "rose-pine-moon")),
        look("tea-house", "Tea House",
            "Green-grey calm and nothing you did not ask for.",
            &["complete", "minimal", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "tea", "error": "tea", "transient": "tea" } },
                "git": { "branch_icon": "text" },
            }), "everforest")),
        look("steel-katana", "Steel Katana",
            "Cold blue-grey with a flame-cut edge.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "katana", "error": "katana", "transient": "katana" } },
                "git": { "branch_icon": "powerline" },
            }), "iceberg")),
        look("noh-mask", "Noh Mask",
            "Muted stage colors behind a framed rule.",
            &["complete", "framed", "two-line", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "slanted" }, "frame": { "enabled": true, "gap_char": "\u{2500}", "gap_gradient": "subtle" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "mask", "error": "drama", "transient": "mask" } },
                "git": { "branch_icon": "octicon" },
                "prompt": { "newline": true },
            }), "zenburn")),

        // ── Sci-fi ────────────────────────────────────────────────────────
        look("xenomorph", "Xenomorph",
            "Acid green on black, flame-cut. Something is in the vents.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "alien", "error": "alien", "transient": "alien" } },
                "git": { "branch_icon": "nerd" },
            }), "scarlet-protocol")),
        look("bot-farm", "Bot Farm",
            "Flat IBM Carbon, arrows, and no personality whatsoever.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "robot", "error": "robot", "transient": "robot" } },
                "git": { "branch_icon": "powerline" },
            }), "oxocarbon")),
        look("ghost-shell", "Ghost Shell",
            "Segments that fade out before you finish reading them.",
            &["complete", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "fade" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "ghost", "error": "ghost", "transient": "ghost" } },
                "git": { "branch_icon": "nerd" },
            }), "poimandres")),
        look("blue-cascade", "Blue Cascade",
            "Falling green on blue-black. Dense and unblinking.",
            &["complete", "dense"],
            with_palette(serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
                "git": { "branch_icon": "text" },
            }), "blue-matrix")),
        look("deep-space", "Deep Space",
            "Navy with a magenta pulse, full rainbow segments.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "powerline" },
                "prompt": { "newline": true },
            }), "andromeda")),

        // ── Expressive: the kaomoji family ────────────────────────────────
        look("shrug-life", "Shrug Life",
            "Bright and friendly, and completely unbothered by exit 1.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_shrug", "error": "kaomoji_shrug", "transient": "kaomoji_shrug" } },
                "git": { "branch_icon": "text" },
            }), "snazzy")),
        look("sleepy-dev", "Sleepy Dev",
            "Dusky blues for the 2am session.",
            &["complete", "minimal", "two-line"],
            with_palette(serde_json::json!({
                "style": { "preset": "pure", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_sleepy", "error": "kaomoji_rage", "transient": "kaomoji_sleepy" } },
                "git": { "branch_icon": "text" },
                "prompt": { "newline": true },
            }), "nightfox")),
        look("hype-machine", "Hype Machine",
            "Maximum voltage and a prompt that is thrilled for you.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_cheer", "error": "kaomoji_rage", "transient": "kaomoji_cheer" } },
                "git": { "branch_icon": "powerline" },
            }), "neon")),
        look("zen-mode", "Zen Mode",
            "The least prompt that is still a prompt.",
            &["complete", "minimal"],
            with_palette(serde_json::json!({
                "style": { "preset": "minimal", "separators": { "shape": "none" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_relaxed", "error": "kaomoji_relaxed", "transient": "kaomoji_relaxed" } },
                "git": { "branch_icon": "none" },
            }), "iceberg")),

        // ── Regal ─────────────────────────────────────────────────────────
        look("crown-jewels", "Crown Jewels",
            "Saturated violet with rounded caps.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "round" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "crown", "error": "crown", "transient": "crown" } },
                "git": { "branch_icon": "powerline" },
            }), "aura")),
        look("swordsman", "Swordsman",
            "Hot coral, slanted cuts, one clean stroke.",
            &["complete", "powerline", "nerd-font"],
            with_palette(serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "sword", "error": "sword", "transient": "sword" } },
                "git": { "branch_icon": "octicon" },
            }), "horizon")),

        // ── Structure only: your palette, a different shape ───────────────
        look("ascii-only", "ASCII Only",
            "No Nerd Font anywhere. For a console, an SSH session, or a tmux that lies about its font.",
            &["structure", "ascii-safe", "minimal"],
            serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "vertical" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "dollar", "error": "dollar", "transient": "dollar" } },
                "git": { "branch_icon": "text" },
            })),
        look("single-line", "Single Line",
            "Everything on one row, no blank line above it.",
            &["structure", "minimal", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "dot" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "octicon" },
                "prompt": { "newline": false, "blank_line": false },
            })),
        look("wide-load", "Wide Load",
            "Every segment, packed tight, thin arrows between.",
            &["structure", "dense", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "dense", "separators": { "shape": "powerline_thin" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "linux" },
                              "character": { "success": "angle", "error": "angle", "transient": "angle" } },
                "git": { "branch_icon": "octicon" },
            })),
        look("round-trip", "Round Trip",
            "Powerline with rounded caps instead of arrows.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "round" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
            })),
        look("diamond-cut", "Diamond Cut",
            "Faceted separators. Sharper than round, softer than flame.",
            &["structure", "powerline", "nerd-font"],
            serde_json::json!({
                "style": { "preset": "powerline", "separators": { "shape": "diamond" }, "frame": { "enabled": false } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "triangle", "error": "triangle", "transient": "triangle" } },
                "git": { "branch_icon": "powerline" },
            })),
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

/// The config paths a Look owns.
///
/// Applying a Look CLEARS these before merging its patch, so a Look is the
/// atomic bundle it is presented as everywhere in the product. Without it a
/// patch is a delta: every key it omits inherits from whatever preset was
/// applied last, which is why 168 of the 812 ordered (apply A, then B) pairs
/// of the then-29-Look library rendered differently from B's own gallery
/// card. The library is 52 Looks now — 2652 pairs — and the sweep in
/// `server.rs` runs all of them.
///
/// Deliberately excludes what belongs to the user rather than the preset:
/// segment enable/disable, `git.mode`, `directory.*`, `terminal.*`, plugins,
/// rice and statusline all survive an apply untouched.
///
/// `theme` is NOT here. It stays governed by the structure/complete rule: a
/// `complete` Look's patch carries a `theme` block (and the palette-replacement
/// rule below applies), a `structure` Look carries none and leaves your
/// palette alone.
pub const LOOK_OWNED: &[&str] = &[
    "style",
    "prompt.layout",
    "prompt.newline",
    "prompt.blank_line",
    "segments.os.icon",
    "segments.character.success",
    "segments.character.error",
    "segments.character.transient",
    "git.branch_icon",
];

/// Remove the Look-owned paths from a config table.
///
/// Removal rather than writing explicit defaults: an absent key reads as
/// "default" everywhere, keeps `config.toml` readable, and leaves the bar
/// popout's modified-vs-default ink honest. Writing defaults would mark every
/// owned key as user-modified.
pub fn clear_look_owned(doc: &mut toml::Table) {
    for path in LOOK_OWNED {
        clear_path(doc, path);
    }
}

fn clear_path(table: &mut toml::Table, path: &str) {
    match path.split_once('.') {
        None => {
            table.remove(path);
        }
        Some((head, rest)) => {
            if let Some(toml::Value::Table(inner)) = table.get_mut(head) {
                clear_path(inner, rest);
            }
        }
    }
}

/// Merge a patch onto the current config as a DELTA — keys the patch omits
/// keep their current values. Used by the Look editor's working patch and by
/// project profiles, both of which genuinely want a delta.
pub fn apply_transient(current: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    merge_patch(current, patch, false)
}

/// Apply a Look ATOMICALLY: clear everything the Look owns, then merge its
/// patch. This is what makes a gallery card match what you get when you press
/// Apply — both go through here.
pub fn apply_look(current: &Config, patch: &serde_json::Value) -> Result<Config, String> {
    merge_patch(current, patch, true)
}

/// A patch that sets a palette sets the WHOLE palette.
///
/// Without this the deep merge leaves the previous palette's keys behind:
/// switching from Gruvbox (which ships an art-directed `ramp`) to a palette
/// that derives its own would keep Gruvbox's mustard ramp, and a partial user
/// palette would blend with whatever preceded it into a scheme nobody
/// designed.
///
/// One implementation, called from BOTH the in-memory merge (`merge_patch`)
/// and the on-disk merge (`server::write_patch`). They used to diverge: the
/// file kept the stale `ramp`, and because persisting reloads the config from
/// the file, the file's copy won and the in-memory clear was thrown away.
pub fn clear_replaced_palette(doc: &mut toml::Table, patch: &toml::Value) {
    let replaces_palette = patch
        .get("theme")
        .and_then(|t| t.as_table())
        .is_some_and(|t| t.contains_key("custom"));
    if !replaces_palette {
        return;
    }
    if let Some(theme) = doc.get_mut("theme").and_then(|t| t.as_table_mut()) {
        theme.remove("custom");
        theme.remove("ramp");
    }
}

fn merge_patch(
    current: &Config,
    patch: &serde_json::Value,
    atomic: bool,
) -> Result<Config, String> {
    let patch_val = serde_json::from_value::<toml::Value>(patch.clone())
        .map_err(|e| format!("look patch not representable in TOML: {e}"))?;
    let cur = toml::Value::try_from(current)
        .map_err(|e| format!("config serialize: {e}"))?;
    let mut doc = match cur.as_table() {
        Some(t) => t.clone(),
        None => toml::Table::new(),
    };
    if atomic {
        clear_look_owned(&mut doc);
    }
    clear_replaced_palette(&mut doc, &patch_val);
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

    #[test]
    fn clear_look_owned_removes_rather_than_writing_defaults() {
        // Removal, not explicit defaults: writing defaults would mark every
        // owned key as user-modified and break the panel's modified-vs-default
        // ink and its per-row reset.
        let value = toml::Value::try_from(Config::default()).expect("serialize");
        let mut doc = value.as_table().expect("table").clone();
        assert!(doc.contains_key("style"), "precondition");

        clear_look_owned(&mut doc);

        assert!(!doc.contains_key("style"), "style is Look-owned");
        let prompt = doc.get("prompt").and_then(|v| v.as_table()).expect("prompt survives");
        assert!(!prompt.contains_key("newline"), "prompt.newline is Look-owned");
        assert!(!prompt.contains_key("layout"), "prompt.layout is Look-owned");
        assert!(
            prompt.contains_key("right_prompt"),
            "prompt.right_prompt belongs to the user and must survive"
        );
        let git = doc.get("git").and_then(|v| v.as_table()).expect("git survives");
        assert!(!git.contains_key("branch_icon"), "git.branch_icon is Look-owned");
        assert!(git.contains_key("mode"), "git.mode belongs to the user");
    }

    #[test]
    fn applying_a_look_resets_what_the_previous_look_set() {
        // The headline bug: framed-focus sets gap_gradient, lean-pure does not,
        // so under a delta merge lean-pure silently kept framed-focus's rule.
        let base = Config::default();
        let framed = apply_look(&base, &resolve("framed-focus", &base).expect("curated").patch)
            .expect("apply framed-focus");
        assert_eq!(framed.style.frame.gap_gradient.as_deref(), Some("off"));

        let lean = apply_look(&framed, &resolve("lean-pure", &base).expect("curated").patch)
            .expect("apply lean-pure");
        assert_eq!(
            lean.style.frame.gap_gradient,
            Config::default().style.frame.gap_gradient,
            "lean-pure inherited framed-focus's gap_gradient"
        );
    }

    #[test]
    fn applying_a_look_leaves_your_own_settings_alone() {
        // Swept across EVERY curated Look, not one hand-picked example: the
        // contract is a property of the table, and a single Look cannot
        // notice a new entry that reaches outside what a Look owns.
        for def in curated() {
            let mut base = Config::default();
            base.segments.battery.enabled = true;
            base.git.mode = "always".into();
            base.directory.max_length = 17;
            base.directory.unique = true;
            base.segments.os.enabled = false;

            let after = apply_look(&base, &def.patch)
                .unwrap_or_else(|e| panic!("{} failed to apply: {e}", def.name));

            assert!(
                after.segments.battery.enabled,
                "{}: segment toggles are yours",
                def.name
            );
            assert!(
                !after.segments.os.enabled,
                "{}: segment toggles are yours",
                def.name
            );
            assert_eq!(after.git.mode, "always", "{}: git.mode is yours", def.name);
            assert_eq!(
                after.directory.max_length, 17,
                "{}: directory settings are yours",
                def.name
            );
            assert!(
                after.directory.unique,
                "{}: directory settings are yours",
                def.name
            );
        }
    }

    /// Every leaf path a curated patch writes, as dotted strings.
    fn leaf_paths(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    leaf_paths(v, &path, out);
                }
            }
            _ => out.push(prefix.to_string()),
        }
    }

    #[test]
    fn every_curated_patch_stays_inside_what_a_look_owns() {
        // `clear_look_owned` can only clear what LOOK_OWNED names. A patch
        // that writes anything else leaks it into the live config and then
        // into every subsequent Look -- the exact delta bug atomic apply was
        // built to end, reintroduced one table row at a time. `omnarchy` did
        // this with `segments.os.enabled` and `directory.unique`, both of
        // which the LOOK_OWNED doc comment promises survive an apply.
        //
        // `theme` is legitimately outside LOOK_OWNED: it is governed by the
        // structure/complete rule and the palette-replacement clear instead.
        let mut offenders: Vec<String> = Vec::new();
        for def in curated() {
            let mut paths = Vec::new();
            leaf_paths(&def.patch, "", &mut paths);
            for path in paths {
                let owned = LOOK_OWNED.iter().any(|owned| {
                    path == *owned || path.starts_with(&format!("{owned}."))
                });
                if !owned && path != "theme" && !path.starts_with("theme.") {
                    offenders.push(format!("{}: {path}", def.name));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "curated patches write {} path(s) no Look can clear: {:?}",
            offenders.len(),
            offenders
        );
    }

    #[test]
    fn apply_transient_still_merges_as_a_delta() {
        // The Look editor and project profiles genuinely want a delta; only
        // Apply is atomic.
        let base = Config::default();
        let framed = apply_transient(&base, &resolve("framed-focus", &base).expect("curated").patch)
            .expect("apply");
        let lean = apply_transient(&framed, &resolve("lean-pure", &base).expect("curated").patch)
            .expect("apply");
        assert_eq!(lean.style.frame.gap_gradient.as_deref(), Some("off"));
    }

    #[test]
    fn clear_look_owned_reaches_three_levels_deep_and_spares_siblings() {
        // These are the deepest paths in LOOK_OWNED. A recursion bug in
        // clear_path would either miss them entirely or remove a whole parent
        // table, neither of which the existing depth-1 and depth-2 tests would
        // catch. This test guards both the recursion working and the crucial
        // property that non-owned siblings in the same table survive the clear.
        let value = toml::Value::try_from(Config::default()).expect("serialize");
        let mut doc = value.as_table().expect("table").clone();

        clear_look_owned(&mut doc);

        // Three-level paths are cleared.
        let segments = doc.get("segments").and_then(|v| v.as_table()).expect("segments survives");
        let os = segments.get("os").and_then(|v| v.as_table()).expect("segments.os survives");
        let character = segments
            .get("character")
            .and_then(|v| v.as_table())
            .expect("segments.character survives");

        assert!(!os.contains_key("icon"), "segments.os.icon is Look-owned");
        assert!(
            !character.contains_key("success"),
            "segments.character.success is Look-owned"
        );
        assert!(
            !character.contains_key("error"),
            "segments.character.error is Look-owned"
        );
        assert!(
            !character.contains_key("transient"),
            "segments.character.transient is Look-owned"
        );

        // Non-owned siblings in the same tables survive.
        assert!(os.contains_key("enabled"), "segments.os.enabled belongs to the user");
        assert!(
            character.contains_key("vi_mode"),
            "segments.character.vi_mode belongs to the user"
        );
    }

    #[test]
    fn the_library_covers_its_own_glyph_families() {
        // 29 Looks drew on four glyph families and left Japan, sci-fi and most
        // kaomoji entirely unused.
        let all: Vec<String> = curated()
            .iter()
            .map(|l| serde_json::to_string(&l.patch).unwrap_or_default())
            .collect();
        let joined = all.join(" ");
        for glyph in ["torii", "sushi", "noodles", "sakura", "tea", "katana",
                      "alien", "robot", "ghost", "crown", "sword",
                      "kaomoji_shrug", "kaomoji_sleepy", "kaomoji_cheer"] {
            assert!(joined.contains(glyph), "no Look uses the {glyph} glyph");
        }
        assert_eq!(curated().len(), 52, "expected 52 curated Looks");
    }
}

#[cfg(test)]
mod preset_tests {
    use super::*;
    use crate::palette_derive::{apca_lc_abs, target_for};
    use crate::style::GlyphCatalog;

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

    /// The `nerd-font` tag is user-facing: it is how someone on a plain
    /// console — no patched font, SSH into a stock VM, a tmux that lies
    /// about its font — knows in advance which curated Looks will render
    /// as tofu instead of a glyph. A Look that resolves to a Private Use
    /// Area code point without carrying this tag would silently break for
    /// exactly the audience the tag exists to warn.
    #[test]
    fn a_look_that_needs_a_nerd_font_says_so() {
        fn is_pua(s: &str) -> bool {
            s.chars().any(|c| {
                let cp = c as u32;
                (0xE000..=0xF8FF).contains(&cp) || (0xF0000..=0xFFFFD).contains(&cp)
            })
        }

        fn needs_nerd_font(patch: &serde_json::Value) -> bool {
            let mut glyphs: Vec<String> = Vec::new();

            if let Some(icon) = patch.pointer("/segments/os/icon").and_then(|v| v.as_str()) {
                if let Some(resolved) = GlyphCatalog::os_icon(icon) {
                    glyphs.push(resolved.to_string());
                }
            }
            for key in ["success", "error", "transient"] {
                if let Some(c) = patch
                    .pointer(&format!("/segments/character/{key}"))
                    .and_then(|v| v.as_str())
                {
                    glyphs.push(GlyphCatalog::prompt_char(c).to_string());
                }
            }
            if let Some(gi) = patch.pointer("/git/branch_icon").and_then(|v| v.as_str()) {
                glyphs.push(GlyphCatalog::branch_icon(gi).to_string());
            }
            if let Some(shape) = patch.pointer("/style/separators/shape").and_then(|v| v.as_str()) {
                glyphs.push(GlyphCatalog::separator(shape).to_string());
            }

            glyphs.iter().any(|g| is_pua(g))
        }

        let missing: Vec<String> = curated()
            .into_iter()
            .filter(|look| {
                needs_nerd_font(&look.patch) && !look.tags.iter().any(|t| t == "nerd-font")
            })
            .map(|look| look.name)
            .collect();

        assert!(
            missing.is_empty(),
            "these Looks resolve a glyph from the Private Use Area (require a \
             Nerd Font) but are not tagged `nerd-font`: {missing:?}"
        );
    }

    #[test]
    fn the_collection_is_worth_browsing() {
        // The point of the pass: enough breadth that the gallery is a gallery.
        // Pinned exactly, not floored: a floor of 28 against a library of 52
        // cannot notice a Look being dropped, which is precisely the drift
        // this branch had to repair. Adding or removing a Look is a
        // deliberate act; update the number with it.
        assert_eq!(curated().len(), 52, "expected exactly 52 curated Looks");
        assert!(
            curated_palettes().len() >= 38,
            "expected at least 38 curated palettes"
        );
    }
}
#[cfg(test)]
mod patch_schema_tests {
    use super::*;
    use crate::config::Config;

    /// Every key a Look patch writes must exist in the config schema.
    ///
    /// `Config` does not use `deny_unknown_fields`, so serde silently DROPS
    /// anything it does not recognise. A patch key with a typo, or one at the
    /// wrong nesting level, therefore produces no error and no effect -- it
    /// simply does nothing, forever.
    ///
    /// That is not hypothetical: every curated Look carried a top-level
    /// `"frame"` object, but the frame lives at `style.frame`. All of them
    /// were silently discarded, and the framed Looks only looked right
    /// because their `style.preset` happened to set the frame too. The
    /// Studio's "Frame lines" toggle and the wizard's frame step wrote the
    /// same dead path.
    fn walk(schema: &serde_json::Value, patch: &serde_json::Value, path: &str, bad: &mut Vec<String>) {
        let (Some(sobj), Some(pobj)) = (schema.as_object(), patch.as_object()) else {
            return;
        };
        for (k, v) in pobj {
            let here = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
            match sobj.get(k) {
                None => bad.push(here),
                Some(sv) => {
                    // Recurse only into real sub-objects. A free-form map
                    // (theme.custom) accepts any key by design.
                    if v.is_object() && sv.is_object() && !here.starts_with("theme.custom") {
                        walk(sv, v, &here, bad);
                    }
                }
            }
        }
    }

    #[test]
    fn every_curated_look_writes_only_real_config_paths() {
        let schema = serde_json::to_value(Config::default()).expect("config serializes");
        let mut failures = Vec::new();
        for look in curated() {
            let mut bad = Vec::new();
            walk(&schema, &look.patch, "", &mut bad);
            for b in bad {
                failures.push(format!("{}: {b}", look.name));
            }
        }
        assert!(
            failures.is_empty(),
            "{} Look patch key(s) are not in the config schema and would be \
             silently dropped:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    #[test]
    fn the_frame_lives_under_style() {
        // Pins the specific shape, so a future edit cannot quietly move it
        // back to the top level where serde would drop it.
        let schema = serde_json::to_value(Config::default()).unwrap();
        assert!(
            schema.get("frame").is_none(),
            "there is no top-level `frame`; it is `style.frame`"
        );
        assert!(schema["style"].get("frame").is_some());
    }
}
