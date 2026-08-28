use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub prompt: PromptConfig,
    #[serde(default)]
    pub env: EnvConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub directory: DirectoryConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub segments: SegmentsConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub statusline: StatuslineConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub looks: std::collections::BTreeMap<String, LookEntry>,
}

/// A user-defined Look: a named patch bundle plus a palette directive
/// ("theme" | "keep" | curated palette key). See `crate::looks`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LookEntry {
    pub label: String,
    pub palette: Option<String>,
    /// Raw patch tables (style / glyphs / frame / prompt) — expanded and
    /// merged onto the config tree at apply time.
    pub patch: toml::Table,
}

impl Default for LookEntry {
    fn default() -> Self {
        Self {
            label: String::new(),
            palette: Some("keep".into()),
            patch: toml::Table::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct StyleConfig {
    pub preset: String,
    #[serde(default)]
    pub separators: SeparatorConfig,
    #[serde(default)]
    pub frame: FrameConfig,
    #[serde(default)]
    pub caps: CapsConfig,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            // p10k-style rainbow powerline is the signature look — colored
            // segment fills, arrows, two-line prompt. Plain "omarchy" stays
            // available as a gallery choice.
            preset: "rainbow".into(),
            separators: SeparatorConfig::default(),
            frame: FrameConfig::default(),
            caps: CapsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SeparatorConfig {
    pub left: Option<String>,
    pub right: Option<String>,
    /// Geometry family override: "auto" (preset default) or a GlyphCatalog
    /// separator key. A set shape drives both directions together.
    pub shape: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct FrameConfig {
    pub enabled: Option<bool>,
    pub left: Option<bool>,
    pub right: Option<bool>,
    pub gap_char: Option<String>,
    /// Gap fill interpolation: off | subtle | full.
    pub gap_gradient: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CapsConfig {
    pub left_start: Option<String>,
    pub left_end: Option<String>,
    pub right_start: Option<String>,
    pub right_end: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PromptConfig {
    pub layout: String,
    pub transient: bool,
    pub newline: bool,
    /// p10k PROMPT_ADD_NEWLINE — one blank line before each prompt.
    pub blank_line: bool,
    pub right_prompt: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            layout: "omarchy".into(),
            transient: true,
            newline: true,
            blank_line: true,
            right_prompt: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub source: String,
    pub custom: Option<CustomPalette>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            source: "omarchy".into(),
            custom: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomPalette {
    pub accent: Option<String>,
    pub foreground: Option<String>,
    pub muted: Option<String>,
    pub background: Option<String>,
    pub red: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
    pub blue: Option<String>,
    pub magenta: Option<String>,
    pub cyan: Option<String>,
    pub orange: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DirectoryConfig {
    pub strategy: String,
    pub max_length: usize,
    pub repo_root_style: String,
    /// truncate_to_unique: shorten each component to the fewest characters
    /// that stay unambiguous among its siblings. Anchor-file directories
    /// are never shortened.
    pub unique: bool,
    pub anchors: Vec<String>,
}

impl Default for DirectoryConfig {
    fn default() -> Self {
        Self {
            strategy: "smart".into(),
            max_length: 40,
            repo_root_style: "bold".into(),
            unique: false,
            anchors: [
                ".git", "Cargo.toml", "package.json", "pyproject.toml",
                "go.mod", "Gemfile", "flake.nix", "README.md",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GitConfig {
    pub enabled: bool,
    pub mode: String,
    pub stale_display: bool,
    pub stale_icon: String,
    pub max_threads: usize,
    pub cache_ttl_ms: u64,
    pub branch_icon: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "adaptive".into(),
            stale_display: true,
            stale_icon: "\u{27f3}".into(),
            max_threads: 4,
            cache_ttl_ms: 5000,
            branch_icon: "powerline".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SegmentsConfig {
    pub os: OsSegmentConfig,
    pub exit_status: ExitStatusConfig,
    pub command_duration: CommandDurationConfig,
    pub jobs: JobsConfig,
    pub ssh: SshConfig,
    pub character: CharacterConfig,
    pub container: ContainerConfig,
    pub python: PythonConfig,
    pub toolchain: ToolchainConfig,
    pub nix: NixConfig,
    pub k8s: K8sConfig,
    pub time: TimeConfig,
    pub battery: BatteryConfig,
    pub ai: AiSegmentConfig,
    pub notification: NotificationConfig,
    pub load: LoadConfig,
}

impl Default for SegmentsConfig {
    fn default() -> Self {
        Self {
            os: OsSegmentConfig::default(),
            exit_status: ExitStatusConfig::default(),
            command_duration: CommandDurationConfig::default(),
            jobs: JobsConfig::default(),
            ssh: SshConfig::default(),
            character: CharacterConfig::default(),
            container: ContainerConfig::default(),
            python: PythonConfig::default(),
            toolchain: ToolchainConfig::default(),
            nix: NixConfig::default(),
            k8s: K8sConfig::default(),
            time: TimeConfig::default(),
            battery: BatteryConfig::default(),
            ai: AiSegmentConfig::default(),
            notification: NotificationConfig::default(),
            load: LoadConfig::default(),
        }
    }
}

/// Load-average sparkline segment (Wave 1).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LoadConfig {
    pub enabled: bool,
    pub width: usize,
}

impl Default for LoadConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            width: 16,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct OsSegmentConfig {
    pub enabled: bool,
    pub icon: String,
}

impl Default for OsSegmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            icon: "arch".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ExitStatusConfig {
    pub enabled: bool,
    pub show_signal_name: bool,
}

impl Default for ExitStatusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_signal_name: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CommandDurationConfig {
    pub enabled: bool,
    pub show_above_ms: u64,
}

impl Default for CommandDurationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_above_ms: 1500,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct JobsConfig {
    pub enabled: bool,
}

impl Default for JobsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SshConfig {
    pub enabled: bool,
    pub show: String,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CharacterConfig {
    pub success: String,
    pub error: String,
    pub transient: String,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            success: "\u{276f}".into(),
            error: "\u{276f}".into(),
            transient: "\u{276f}".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ContainerConfig {
    pub enabled: bool,
    pub icon: String,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            icon: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PythonConfig {
    pub enabled: bool,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ToolchainConfig {
    pub enabled: bool,
}

impl Default for ToolchainConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NixConfig {
    pub enabled: bool,
}

impl Default for NixConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct K8sConfig {
    pub enabled: bool,
    pub show_namespace: bool,
}

impl Default for K8sConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            show_namespace: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TimeConfig {
    pub enabled: bool,
    pub format: String,
}

impl Default for TimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: "%H:%M".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct BatteryConfig {
    pub enabled: bool,
    pub show_above: u32,
    pub threshold_warning: u32,
    pub threshold_critical: u32,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            show_above: 100,
            threshold_warning: 30,
            threshold_critical: 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub enabled: bool,
    pub threshold_ms: u64,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_ms: 10000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub title: TitleConfig,
    pub progress: ProgressConfig,
    pub semantic_prompts: SemanticPromptsConfig,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            title: TitleConfig::default(),
            progress: ProgressConfig::default(),
            semantic_prompts: SemanticPromptsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TitleConfig {
    pub enabled: bool,
    pub format: String,
}

impl Default for TitleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            format: "{dir}".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProgressConfig {
    pub enabled: bool,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub socket: String,
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket: "auto".into(),
            log_level: "warn".into(),
        }
    }
}

/// Frozen env allowlist (protocol 0.4). The bash adapter sends exactly these
/// keys in the prompt request's `env` object; `[env.watch]` must match.
pub const ENV_WATCH_DEFAULT_KEYS: &[&str] = &[
    "VIRTUAL_ENV",
    "CONDA_DEFAULT_ENV",
    "MISE_NODE_VERSION",
    "MISE_PYTHON_VERSION",
    "MISE_RUBY_VERSION",
    "MISE_GO_VERSION",
    "MISE_RUST_VERSION",
    "IN_NIX_SHELL",
    "DISTROBOX_ENTER_PATH",
    "container",
    "KUBECONFIG",
    "DIRENV_DIR",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_SANDBOX",
    "CODEX_HOME",
    // 15 keys total — must match the adapter's expansion list in
    // shell/omarchy10k.bash __o10k_env_json and [env.watch] in default.toml.
];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct EnvConfig {
    pub watch: EnvWatchConfig,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self { watch: EnvWatchConfig::default() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct EnvWatchConfig {
    pub keys: Vec<String>,
}

impl Default for EnvWatchConfig {
    fn default() -> Self {
        Self {
            keys: ENV_WATCH_DEFAULT_KEYS
                .iter()
                .map(|k| (*k).to_string())
                .collect(),
        }
    }
}

/// Long-command notification config (protocol 0.4). `[segments.notification]`
/// remains parsed as a deprecated alias; see `Config::effective_notifications`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub enabled: bool,
    pub threshold_ms: u64,
    pub unfocused_only: bool,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_ms: 10000,
            unfocused_only: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct StatuslineConfig {
    pub context_warning_pct: u8,
    pub context_critical_pct: u8,
}

impl Default for StatuslineConfig {
    fn default() -> Self {
        Self {
            context_warning_pct: 60,
            context_critical_pct: 85,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AiSegmentConfig {
    pub enabled: bool,
    pub hide_below_cols: u16,
}

impl Default for AiSegmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hide_below_cols: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SemanticPromptsConfig {
    pub enabled: bool,
}

impl Default for SemanticPromptsConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}


impl Default for Config {
    fn default() -> Self {
        Self {
            prompt: PromptConfig::default(),
            env: EnvConfig::default(),
            style: StyleConfig::default(),
            theme: ThemeConfig::default(),
            directory: DirectoryConfig::default(),
            git: GitConfig::default(),
            segments: SegmentsConfig::default(),
            notifications: NotificationsConfig::default(),
            statusline: StatuslineConfig::default(),
            terminal: TerminalConfig::default(),
            daemon: DaemonConfig::default(),
            looks: Default::default(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn config_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|d| d.config_dir().join("omarchy10k"))
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                    .join(".config/omarchy10k")
            })
    }


    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn default_config_str() -> &'static str {
        include_str!("../../../config/default.toml")
    }
}
impl Config {
    /// Resolve the effective notification settings. `[segments.notification]`
    /// is a deprecated alias for `[notifications]`: when the new table is left
    /// entirely at its defaults, alias values (possibly user-set) win; any
    /// explicit `[notifications]` key takes precedence.
    pub fn effective_notifications(&self) -> NotificationsConfig {
        let mut n = self.notifications.clone();
        if n == NotificationsConfig::default() {
            n.enabled = self.segments.notification.enabled;
            n.threshold_ms = self.segments.notification.threshold_ms;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_notifications_defaults() {
        let config = Config::default();
        let n = config.effective_notifications();
        assert!(n.enabled);
        assert_eq!(n.threshold_ms, 10000);
        assert!(!n.unfocused_only);
    }

    #[test]
    fn test_effective_notifications_alias_mapping() {
        // Deprecated [segments.notification] alias feeds [notifications] while
        // the new table is left at defaults.
        let mut config = Config::default();
        config.segments.notification.enabled = false;
        config.segments.notification.threshold_ms = 2500;
        let n = config.effective_notifications();
        assert!(!n.enabled);
        assert_eq!(n.threshold_ms, 2500);
        assert!(!n.unfocused_only);
    }

    #[test]
    fn test_effective_notifications_new_table_wins() {
        let mut config = Config::default();
        config.segments.notification.enabled = false;
        config.notifications.enabled = true;
        config.notifications.threshold_ms = 999;
        let n = config.effective_notifications();
        assert!(n.enabled);
        assert_eq!(n.threshold_ms, 999);
    }

    #[test]
    fn test_env_watch_defaults_are_frozen_allowlist() {
        let config = Config::default();
        assert_eq!(config.env.watch.keys.len(), 15);
        assert!(config.env.watch.keys.contains(&"CLAUDE_CODE_ENTRYPOINT".to_string()));
        assert!(config.env.watch.keys.contains(&"VIRTUAL_ENV".to_string()));
        assert!(config.env.watch.keys.contains(&"KUBECONFIG".to_string()));
    }
}
