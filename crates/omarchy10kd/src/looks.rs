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
    /// `config_set`-shaped patch: top-level config keys, glyph shortcuts
    /// already expanded, palette resolved into a `theme` sub-patch.
    pub patch: serde_json::Value,
}

/// Curated palettes (moved from quattro/Model.js so Looks resolve
/// identically from CLI, gallery, and panel). Values are `theme` patches.
pub fn curated_palette(key: &str) -> Option<serde_json::Value> {
    let custom = |accent: &str, foreground: &str, muted: &str, background: &str,
                  red: &str, green: &str, yellow: &str, blue: &str,
                  magenta: &str, cyan: &str, orange: &str| {
        serde_json::json!({
            "theme": { "source": "hybrid", "custom": {
                "accent": accent, "foreground": foreground, "muted": muted,
                "background": background, "red": red, "green": green,
                "yellow": yellow, "blue": blue, "magenta": magenta,
                "cyan": cyan, "orange": orange,
            }}
        })
    };
    match key {
        "tokyo-night" => Some(custom("#7aa2f7", "#c0caf5", "#565f89", "#1a1b26", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#ff9e64")),
        "catppuccin" => Some(custom("#89b4fa", "#cdd6f4", "#6c7086", "#1e1e2e", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#cba6f7", "#94e2d5", "#fab387")),
        "gruvbox" => Some(custom("#83a598", "#ebdbb2", "#928374", "#282828", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#fe8019")),
        "nord" => Some(custom("#88c0d0", "#eceff4", "#4c566a", "#2e3440", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#8fbcbb", "#d08770")),
        "dracula" => Some(custom("#bd93f9", "#f8f8f2", "#6272a4", "#282a36", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#ffb86c")),
        "rose-pine" => Some(custom("#c4a7e7", "#e0def4", "#6e6a86", "#191724", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#9ccfd8", "#e69875")),
        "everforest" => Some(custom("#a7c080", "#d3c6aa", "#6e6a86", "#2d353b", "#e67e80", "#a7c080", "#dbbc7f", "#7fbbb3", "#d699b6", "#83c092", "#e69875")),
        "kanagawa" => Some(custom("#7e9cd8", "#dcd7ba", "#727169", "#1f1f28", "#ff5d62", "#98bb6c", "#ffa066", "#7e9cd8", "#957fb8", "#6a9589", "#ffa066")),
        _ => None,
    }
}

fn chars_patch(char_key: &str) -> serde_json::Value {
    serde_json::json!({
        "segments": { "character": { "success": char_key, "error": char_key, "transient": char_key } }
    })
}

fn look(name: &str, label: &str, patch: serde_json::Value) -> LookDef {
    LookDef { name: name.into(), label: label.into(), patch }
}

/// Compiled-in Looks. Patches are `config_set`-shaped (top-level keys,
/// glyph shortcuts already expanded, palette merged into a `theme` patch).
pub fn curated() -> Vec<LookDef> {
    vec![
        look("omnarchy", "Omnarchy", serde_json::json!({
            "style": { "preset": "omarchy", "separators": { "shape": "auto" } },
            "segments": { "os": { "icon": "arch", "enabled": true },
                          "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
            "git": { "branch_icon": "powerline" },
            "frame": { "enabled": false, "gap_char": "", "gap_gradient": "off" },
            "theme": { "source": "omarchy" },
            "directory": { "unique": false },
        })),
        look("tokyo-rainbow", "Tokyo Rainbow", {
            let mut p = serde_json::json!({
                "style": { "preset": "rainbow", "separators": { "shape": "powerline" } },
                "segments": { "os": { "icon": "arch" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": false },
            });
            if let Some(theme) = curated_palette("tokyo-night") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
        look("framed-gradient", "Framed Gradient", {
            let mut p = serde_json::json!({
                "style": { "preset": "framed", "separators": { "shape": "auto" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "chevron", "error": "chevron", "transient": "chevron" } },
                "git": { "branch_icon": "powerline" },
                "frame": { "enabled": true, "gap_char": "─", "gap_gradient": "full" },
            });
            if let Some(theme) = curated_palette("tokyo-night") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
        look("lean-pure", "Lean Pure", serde_json::json!({
            "style": { "preset": "pure", "separators": { "shape": "auto" } },
            "segments": { "os": { "icon": "none" },
                          "character": { "success": "lambda", "error": "lambda", "transient": "lambda" } },
            "git": { "branch_icon": "text" },
            "frame": { "enabled": false },
        })),
        look("slanted-owl", "Slanted Owl", {
            let mut p = serde_json::json!({
                "style": { "preset": "slanted", "separators": { "shape": "slanted" } },
                "segments": { "os": { "icon": "owl" },
                              "character": { "success": "owl", "error": "dragon", "transient": "owl" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            });
            if let Some(theme) = curated_palette("everforest") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
        look("gruvbox-drift", "Gruvbox Drift", {
            let mut p = serde_json::json!({
                "style": { "preset": "gradient", "separators": { "shape": "flame" } },
                "segments": { "os": { "icon": "paw" },
                              "character": { "success": "paw", "error": "kaomoji_rage", "transient": "paw" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            });
            if let Some(theme) = curated_palette("gruvbox") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
        look("rose-classic", "Rosé Classic", {
            let mut p = serde_json::json!({
                "style": { "preset": "classic", "separators": { "shape": "vertical" } },
                "segments": { "os": { "icon": "none" },
                              "character": { "success": "kaomoji_bear", "error": "kaomoji_disapprove", "transient": "kaomoji_bear" } },
                "git": { "branch_icon": "octicon" },
                "frame": { "enabled": false },
            });
            if let Some(theme) = curated_palette("rose-pine") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
        look("polar-lean", "Polar Lean", {
            let mut p = serde_json::json!({
                "style": { "preset": "lean", "separators": { "shape": "round" } },
                "segments": { "os": { "icon": "penguin" },
                              "character": { "success": "penguin", "error": "kaomoji_disapprove", "transient": "penguin" } },
                "git": { "branch_icon": "nerd" },
                "frame": { "enabled": false },
            });
            if let Some(theme) = curated_palette("nord") {
                p["theme"] = theme["theme"].clone();
            }
            p
        }),
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
