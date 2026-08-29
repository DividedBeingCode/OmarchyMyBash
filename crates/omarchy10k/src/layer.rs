//! Shell layer: the claim inventory as a static table plus policy resolution.
//!
//! Resolution runs once, here in Rust, at `omarchy10k init bash` / `doctor`
//! time. The emitted adapter receives a flat, pre-resolved policy prelude
//! (`__O10K_LAYER_POLICY`, `__O10K_LAYER_OVERRIDES_<name>`) and performs only
//! trivial per-item platform-signature detection in bash — no dynamic claim
//! broker, no runtime config reads, no daemon involvement.
//!
//! Precedence: `[shell.layer.overrides]` per-item entry > `[shell.layer].policy`
//! global > built-in default action per claim. The user row is absolute and not
//! configurable: a user definition (one that does not match the platform
//! signature) is never touched under any policy.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Global or per-claim policy, from `[shell.layer]` in config.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Extend platform definitions where clean, define when undefined.
    Extend,
    /// Leave platform-owned items alone; define only when undefined.
    Defer,
    /// o10k redefines platform-owned items (user definitions still win).
    Own,
    /// o10k asserts nothing.
    Off,
}

impl Policy {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "extend" => Some(Self::Extend),
            "defer" => Some(Self::Defer),
            "own" => Some(Self::Own),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extend => "extend",
            Self::Defer => "defer",
            Self::Own => "own",
            Self::Off => "off",
        }
    }
}

/// Which side of the inventory the claim sits in (spec §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Both the platform and o10k define it today.
    Contested,
    /// The platform has no opinion; current o10k behavior is preserved.
    Uncontested,
    /// Research-stack additions absent from the platform; command-guarded.
    GapFill,
    /// The platform owns it; o10k stays out entirely.
    PlatformOnly,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contested => "contested",
            Self::Uncontested => "uncontested",
            Self::GapFill => "gap-fill",
            Self::PlatformOnly => "platform-only",
        }
    }
}

/// What the claim does when the effective policy is the default (`extend`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultAction {
    /// Platform definition matched: keep it and append o10k's extension.
    Extend,
    /// Platform definition matched: leave it alone.
    Defer,
    /// o10k defines it (guarded by `command -v` where the tool matters).
    Own,
}

/// One claimable shell item. `signature` holds substring groups: the item is
/// platform-owned when every substring of ANY group appears in its current
/// definition. Substring markers, never exact strings, so an Omarchy point
/// release that reorders flags does not flip the classification.
#[derive(Debug, Clone, Copy)]
pub struct Claim {
    pub name: &'static str,
    pub kind: &'static str,
    pub category: Category,
    pub default: DefaultAction,
    pub signature: &'static [&'static [&'static str]],
    pub note: &'static str,
}

impl Claim {
    /// True when the given definition matches this claim's platform signature.
    /// Substring matching over the signature groups; exercised by tests and
    /// mirrored by the adapter's per-item detection.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn signature_matches(&self, definition: &str) -> bool {
        self.signature
            .iter()
            .any(|group| group.iter().all(|s| definition.contains(s)))
    }
}

/// The claim inventory (spec §4). Data, not logic — additions are edits here.
pub const CLAIMS: &[Claim] = &[
    // ── Contested — both define it today ────────────────────────────────────
    Claim {
        name: "ls",
        kind: "alias",
        category: Category::Contested,
        default: DefaultAction::Extend,
        signature: &[&["eza", "--group-directories-first"]],
        note: "platform long-format eza: extend it with --git",
    },
    Claim {
        name: "lt",
        kind: "alias",
        category: Category::Contested,
        default: DefaultAction::Defer,
        signature: &[&["eza", "--tree"]],
        note: "platform eza tree alias is strictly richer; leave alone",
    },
    Claim {
        name: "cd",
        kind: "func",
        category: Category::Contested,
        default: DefaultAction::Defer,
        signature: &[&["zd"], &["_zoxide"]],
        note: "platform wraps cd with zd()/zoxide; never re-init --cmd cd",
    },
    Claim {
        name: "fzf_keys",
        kind: "bind",
        category: Category::Contested,
        default: DefaultAction::Defer,
        signature: &[&["__fzf_history__"]],
        note: "platform sources fzf key-bindings; skip re-binding",
    },
    Claim {
        name: "manpager",
        kind: "env",
        category: Category::Contested,
        default: DefaultAction::Defer,
        signature: &[&["bat -l man"]],
        note: "define only when MANPAGER is unset",
    },
    Claim {
        name: "bat_theme",
        kind: "env",
        category: Category::Contested,
        default: DefaultAction::Defer,
        signature: &[&["ansi"]],
        note: "platform's ansi inherits the theme-synced terminal palette",
    },
    // ── Uncontested — o10k owns, unchanged ─────────────────────────────────
    Claim {
        name: "ll",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "eza long listing",
    },
    Claim {
        name: "la",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "eza all entries",
    },
    Claim {
        name: "tree",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "eza --tree",
    },
    Claim {
        name: "cat",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "bat --paging=never",
    },
    Claim {
        name: "grep",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "rg — compatibility-sensitive: rg flags are not grep's",
    },
    Claim {
        name: "top",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "btop",
    },
    Claim {
        name: "du",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "dust",
    },
    Claim {
        name: "df",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "duf",
    },
    Claim {
        name: "ps",
        kind: "alias",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "procs",
    },
    Claim {
        name: "y",
        kind: "func",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "yazi with cwd-follow on exit",
    },
    Claim {
        name: "atuin",
        kind: "init",
        category: Category::Uncontested,
        default: DefaultAction::Own,
        signature: &[],
        note: "atuin init bash --disable-up-arrow",
    },
    // ── Gap-fill — own, command-guarded ─────────────────────────────────────
    Claim {
        name: "lg",
        kind: "alias",
        category: Category::GapFill,
        default: DefaultAction::Own,
        signature: &[],
        note: "lazygit (themed by o10k-lazygit.yml.tpl)",
    },
    Claim {
        name: "lzd",
        kind: "alias",
        category: Category::GapFill,
        default: DefaultAction::Own,
        signature: &[],
        note: "lazydocker",
    },
    Claim {
        name: "help",
        kind: "alias",
        category: Category::GapFill,
        default: DefaultAction::Own,
        signature: &[],
        note: "tldr",
    },
    Claim {
        name: "et",
        kind: "alias",
        category: Category::GapFill,
        default: DefaultAction::Own,
        signature: &[],
        note: "erdtree (alias et='erd')",
    },
    // ── Platform-only — o10k stays out ─────────────────────────────────────
    Claim {
        name: "mise",
        kind: "init",
        category: Category::PlatformOnly,
        default: DefaultAction::Own,
        signature: &[],
        note: "platform activates mise; no o10k opinion",
    },
    Claim {
        name: "inputrc",
        kind: "readline",
        category: Category::PlatformOnly,
        default: DefaultAction::Own,
        signature: &[],
        note: "platform ships a tuned inputrc; no competing completion layer",
    },
    Claim {
        name: "history",
        kind: "shell",
        category: Category::PlatformOnly,
        default: DefaultAction::Own,
        signature: &[],
        note: "platform owns histappend/HISTCONTROL/HISTSIZE",
    },
    Claim {
        name: "editor",
        kind: "env",
        category: Category::PlatformOnly,
        default: DefaultAction::Own,
        signature: &[],
        note: "platform owns EDITOR/BROWSER",
    },
];

/// Resolved `[shell.layer]` configuration.
#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub global: Policy,
    /// Per-claim override of the global policy.
    pub overrides: BTreeMap<String, Policy>,
    /// config.toml the values came from, when one exists.
    pub source: Option<PathBuf>,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            global: Policy::Extend,
            overrides: BTreeMap::new(),
            source: None,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.config_dir().join("omarchy10k/config.toml"))
}

/// Parse `[shell.layer]` out of a TOML document. Missing file/keys are not
/// errors: defaults apply. Unknown policy strings are ignored (defaults apply).
fn layer_config_from_document(text: &str) -> LayerConfig {
    let mut cfg = LayerConfig::default();
    let Ok(value) = text.parse::<toml::Value>() else {
        return cfg;
    };
    let Some(layer) = value
        .get("shell")
        .and_then(|s| s.get("layer"))
        .and_then(|l| l.as_table())
    else {
        return cfg;
    };
    if let Some(p) = layer.get("policy").and_then(|p| p.as_str()) {
        if let Some(policy) = Policy::parse(p) {
            cfg.global = policy;
        }
    }
    if let Some(table) = layer.get("overrides").and_then(|o| o.as_table()) {
        for (name, v) in table {
            if let Some(policy) = v.as_str().and_then(Policy::parse) {
                cfg.overrides.insert(name.clone(), policy);
            }
        }
    }
    cfg
}

/// Read `[shell.layer]` from the user's config.toml. The daemon's config
/// machinery is deliberately untouched — this is a direct, standalone read.
pub fn load_layer_config() -> LayerConfig {
    let Some(path) = config_path() else {
        return LayerConfig::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return LayerConfig::default();
    };
    let mut cfg = layer_config_from_document(&text);
    cfg.source = Some(path);
    cfg
}

/// The policy that governs one claim: its override, else the global.
pub fn effective_policy(claim: &Claim, cfg: &LayerConfig) -> Policy {
    cfg.overrides.get(claim.name).copied().unwrap_or(cfg.global)
}

/// The observable action for a claim under the resolved configuration.
///
/// Platform-only claims are untouched under every policy (the inventory lists
/// them so a contributor does not "helpfully" add them). `off` disables
/// everything else. `own`-class claims keep owning under `extend`; contested
/// defers keep deferring. Overrides can push any claim to `own`/`defer`.
pub fn effective_action(claim: &Claim, cfg: &LayerConfig) -> &'static str {
    if matches!(claim.category, Category::PlatformOnly) {
        return "untouched";
    }
    match effective_policy(claim, cfg) {
        Policy::Off => "off",
        Policy::Own => "own",
        Policy::Defer => "defer",
        Policy::Extend => match claim.default {
            DefaultAction::Extend => "extend",
            DefaultAction::Defer => "defer",
            DefaultAction::Own => "own",
        },
    }
}

/// The baked policy prelude, printed by `init bash` BEFORE the adapter body.
/// The adapter and tools.sh read these variables instead of re-parsing config.
pub fn prelude(cfg: &LayerConfig) -> String {
    let mut out = String::new();
    out.push_str("# Shell layer policy — resolved from [shell.layer] by `omarchy10k init bash`.\n");
    out.push_str(&format!("__O10K_LAYER_POLICY={}\n", cfg.global.as_str()));
    for (name, policy) in &cfg.overrides {
        out.push_str(&format!(
            "__O10K_LAYER_OVERRIDES_{name}={}\n",
            policy.as_str()
        ));
    }
    out
}

/// Human-readable claim map for `omarchy10k layer`.
pub fn render_map(cfg: &LayerConfig) -> String {
    let mut out = String::new();
    match (&cfg.source, cfg.overrides.is_empty()) {
        (Some(p), false) => out.push_str(&format!(
            "Shell layer claim map — policy: {} (overrides: {}), from {}\n",
            cfg.global.as_str(),
            cfg.overrides
                .iter()
                .map(|(k, v)| format!("{k}={}", v.as_str()))
                .collect::<Vec<_>>()
                .join(","),
            p.display()
        )),
        (Some(p), true) => out.push_str(&format!(
            "Shell layer claim map — policy: {}, from {}\n",
            cfg.global.as_str(),
            p.display()
        )),
        (None, _) => out.push_str(&format!(
            "Shell layer claim map — policy: {} (defaults; no config.toml)\n",
            cfg.global.as_str()
        )),
    }
    out.push_str(&format!(
        "  {:<12}{:<9}{:<14}{:<10}{}\n",
        "CLAIM", "KIND", "CATEGORY", "ACTION", "NOTES"
    ));
    for claim in CLAIMS {
        out.push_str(&format!(
            "  {:<12}{:<9}{:<14}{:<10}{}\n",
            claim.name,
            claim.kind,
            claim.category.as_str(),
            effective_action(claim, cfg),
            claim.note
        ));
    }
    out
}

/// Structured claim map for the Control Center panel (`layer --json`).
pub fn render_json(cfg: &LayerConfig) -> serde_json::Value {
    let overrides: serde_json::Map<String, serde_json::Value> = cfg
        .overrides
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::from(v.as_str())))
        .collect();
    serde_json::json!({
        "policy": cfg.global.as_str(),
        "overrides": overrides,
        "claims": CLAIMS.iter().map(|claim| serde_json::json!({
            "name": claim.name,
            "kind": claim.kind,
            "category": claim.category.as_str(),
            "signature": claim.signature.iter()
                .map(|g| g.to_vec())
                .collect::<Vec<_>>(),
            "default": match claim.default {
                DefaultAction::Extend => "extend",
                DefaultAction::Defer => "defer",
                DefaultAction::Own => "own",
            },
            "effective": effective_action(claim, cfg),
            "note": claim.note,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(toml_text: &str) -> LayerConfig {
        layer_config_from_document(toml_text)
    }

    fn claim(name: &str) -> &'static Claim {
        CLAIMS.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn missing_config_yields_default_extend() {
        let c = cfg("");
        assert_eq!(c.global, Policy::Extend);
        assert!(c.overrides.is_empty());
        assert_eq!(effective_action(claim("ls"), &c), "extend");
        assert_eq!(effective_action(claim("lt"), &c), "defer");
        assert_eq!(effective_action(claim("grep"), &c), "own");
        assert_eq!(effective_action(claim("lg"), &c), "own");
        assert_eq!(effective_action(claim("mise"), &c), "untouched");
    }

    #[test]
    fn global_policy_applies_to_all_non_platform_claims() {
        let c = cfg("[shell.layer]\npolicy = \"off\"");
        assert_eq!(c.global, Policy::Off);
        assert_eq!(effective_action(claim("ls"), &c), "off");
        assert_eq!(effective_action(claim("grep"), &c), "off");
        assert_eq!(effective_action(claim("mise"), &c), "untouched");

        let d = cfg("[shell.layer]\npolicy = \"defer\"");
        assert_eq!(effective_action(claim("ls"), &d), "defer");
        assert_eq!(effective_action(claim("grep"), &d), "defer");
    }

    #[test]
    fn override_beats_global() {
        let c = cfg(
            "[shell.layer]\npolicy = \"defer\"\n[shell.layer.overrides]\nls = \"own\"\ngrep = \"off\"",
        );
        assert_eq!(effective_action(claim("ls"), &c), "own");
        assert_eq!(effective_action(claim("grep"), &c), "off");
        // other claims still follow the global
        assert_eq!(effective_action(claim("cat"), &c), "defer");
    }

    #[test]
    fn override_can_promote_contested_defers_to_own() {
        let c = cfg("[shell.layer.overrides]\nbat_theme = \"own\"");
        assert_eq!(effective_action(claim("bat_theme"), &c), "own");
        assert_eq!(effective_action(claim("lt"), &c), "defer");
    }

    #[test]
    fn unknown_policy_strings_are_ignored() {
        // Unparseable policy values fall back to the default; unknown claim
        // names are stored but inert (no claim consults them).
        let c = cfg("[shell.layer]\npolicy = \"bogus\"\n[shell.layer.overrides]\nls = \"nonsense\"\nnope = \"own\"");
        assert_eq!(c.global, Policy::Extend);
        assert_eq!(c.overrides.get("ls"), None);
        assert_eq!(effective_action(claim("ls"), &c), "extend");
        assert_eq!(effective_action(claim("grep"), &c), "own");
    }

    #[test]
    fn own_claims_defer_only_when_asked() {
        let c = cfg("[shell.layer.overrides]\ngrep = \"defer\"");
        assert_eq!(effective_action(claim("grep"), &c), "defer");
        assert_eq!(effective_action(claim("cat"), &c), "own");
    }

    #[test]
    fn platform_only_is_untouched_under_every_policy() {
        for policy in ["extend", "defer", "own", "off"] {
            let c = cfg(&format!("[shell.layer]\npolicy = \"{policy}\""));
            assert_eq!(effective_action(claim("mise"), &c), "untouched");
            assert_eq!(effective_action(claim("editor"), &c), "untouched");
        }
    }

    #[test]
    fn platform_signature_is_substring_based() {
        let ls = claim("ls");
        assert!(ls.signature_matches("eza -lh --group-directories-first --icons=auto"));
        assert!(ls.signature_matches("command eza --group-directories-first -lh")); // reordered
        assert!(!ls.signature_matches("ls --color=auto"));
        assert!(!ls.signature_matches("eza --icons")); // eza but not the platform shape

        let cd = claim("cd");
        assert!(cd.signature_matches("zd() { zoxide z \"$@\"; }"));
        assert!(cd.signature_matches("_zoxide_cd"));
        let bat_theme = claim("bat_theme");
        assert!(bat_theme.signature_matches("ansi"));
        assert!(!bat_theme.signature_matches("Catppuccin Mocha"));
    }

    #[test]
    fn prelude_bakes_global_and_overrides() {
        let c = cfg("[shell.layer]\npolicy = \"defer\"\n[shell.layer.overrides]\nls = \"own\"");
        let p = prelude(&c);
        assert!(p.contains("__O10K_LAYER_POLICY=defer\n"), "{p}");
        assert!(p.contains("__O10K_LAYER_OVERRIDES_ls=own\n"), "{p}");
        // default prelude is minimal
        let p = prelude(&LayerConfig::default());
        assert_eq!(p, "# Shell layer policy — resolved from [shell.layer] by `omarchy10k init bash`.\n__O10K_LAYER_POLICY=extend\n");
    }

    #[test]
    fn json_map_carries_effective_actions() {
        let c = cfg("[shell.layer.overrides]\ngrep = \"defer\"");
        let v = render_json(&c);
        assert_eq!(v["policy"], "extend");
        assert_eq!(v["overrides"]["grep"], "defer");
        let claims = v["claims"].as_array().unwrap();
        assert_eq!(claims.len(), CLAIMS.len());
        let ls = claims.iter().find(|c| c["name"] == "ls").unwrap();
        assert_eq!(ls["effective"], "extend");
        assert_eq!(ls["signature"][0][0], "eza");
        let grep = claims.iter().find(|c| c["name"] == "grep").unwrap();
        assert_eq!(grep["effective"], "defer");
    }

    #[test]
    fn emitted_init_contains_policy_prelude_and_shell_layer_markers() {
        let cfg = load_layer_config();
        let init = format!(
            "{}{}",
            prelude(&cfg),
            include_str!("../../../shell/omarchy10k.bash")
        );
        // The baked prelude precedes the adapter body.
        let prelude_end = prelude(&cfg).len();
        assert!(init[..prelude_end].contains("__O10K_LAYER_POLICY="));
        // The adapter applies the baked policy.
        assert!(init.contains("# ── Shell Layer (baked policy)"));
        assert!(init.contains("__O10K_LAYER_POLICY"));
        assert!(init.contains("__O10K_LAYER_OVERRIDES_ls:-${__O10K_LAYER_POLICY"));
        assert!(init.contains("eza -lh --group-directories-first --icons=auto --git"));
        // Prompt handoff runs at init.
        assert!(init.contains("__o10k_prompt_handoff"));
        assert!(init.contains("__O10K_DISPLACED"));
        // And the Shell Layer section sources before the Modern CLI Layer.
        let shell_layer = init.find("# ── Shell Layer (baked policy)").unwrap();
        let tools = init.find("# ── Modern CLI Layer").unwrap();
        assert!(shell_layer < tools);
    }
}
