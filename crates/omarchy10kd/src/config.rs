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
    pub theme: ThemeConfig,
    #[serde(default)]
    pub directory: DirectoryConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub segments: SegmentsConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PromptConfig {
    pub layout: String,
    pub transient: bool,
    pub newline: bool,
    pub right_prompt: bool,
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            layout: "omarchy".into(),
            transient: true,
            newline: true,
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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DirectoryConfig {
    pub strategy: String,
    pub max_length: usize,
    pub repo_root_style: String,
}

impl Default for DirectoryConfig {
    fn default() -> Self {
        Self {
            strategy: "smart".into(),
            max_length: 40,
            repo_root_style: "bold".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GitConfig {
    pub enabled: bool,
    pub mode: String,
    pub stale_display: bool,
    pub max_threads: usize,
    pub cache_ttl_ms: u64,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "adaptive".into(),
            stale_display: true,
            max_threads: 4,
            cache_ttl_ms: 5000,
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
    pub notification: NotificationConfig,
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
            notification: NotificationConfig::default(),
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
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            success: "❯".into(),
            error: "❯".into(),
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
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            title: TitleConfig::default(),
            progress: ProgressConfig::default(),
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

impl Default for Config {
    fn default() -> Self {
        Self {
            prompt: PromptConfig::default(),
            theme: ThemeConfig::default(),
            directory: DirectoryConfig::default(),
            git: GitConfig::default(),
            segments: SegmentsConfig::default(),
            terminal: TerminalConfig::default(),
            daemon: DaemonConfig::default(),
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
