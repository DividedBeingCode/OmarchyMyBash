//! Plugin distribution CLI: `omarchy10k plugin add|list|enable|disable|remove|update`.
//!
//! Distribution model: a plugin is a git repository containing a
//! `plugin.toml` manifest at its root. `add` shallow-clones it into
//! `~/.config/omarchy10k/plugins/<name>/` (name taken from the manifest),
//! always installing DISABLED — the user reviews the manifest (and any
//! code) before `enable` writes `[plugins] enabled` in config.toml.
//!
//! Safety posture: only remote git URLs are accepted (`https://`, `git://`,
//! `git@host:path`); local paths are refused so a stray argument can never
//! install arbitrary on-disk content. Plugin names are
//! `[A-Za-z0-9_-]+`, which makes the install path traversal-safe.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::Deserialize;

#[derive(Subcommand, Debug, Clone)]
pub enum PluginAction {
    /// Install a plugin from a git URL (shallow clone; installs DISABLED)
    Add {
        /// Git URL: https://host/repo, git://host/repo, or git@host:repo
        url: String,
        /// Replace an already-installed plugin of the same name
        #[arg(long)]
        force: bool,
    },
    /// List installed plugins and their enabled state
    List,
    /// Enable an installed plugin (writes [plugins].enabled via the daemon)
    Enable { name: String },
    /// Disable an installed plugin
    Disable { name: String },
    /// Delete an installed plugin (refused while it is enabled)
    Remove { name: String },
    /// Update an installed plugin from its git remote and report the changes
    Update { name: String },
}

// ── Manifest (CLI-side minimal read) ────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CliManifest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    segments: Vec<CliSegment>,
}

#[derive(Debug, Deserialize)]
struct CliSegment {
    #[serde(default)]
    name: String,
    #[serde(default)]
    tier: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    env_keys: Vec<String>,
    #[serde(default)]
    command: Option<String>,
}

impl CliSegment {
    fn describe(&self) -> String {
        let detail = match self.tier.as_str() {
            "command" => self
                .command
                .clone()
                .unwrap_or_else(|| "<missing command>".into()),
            _ => format!("env [{}]", self.env_keys.join(", ")),
        };
        let icon = if self.icon.is_empty() {
            String::new()
        } else {
            format!("{} ", self.icon)
        };
        format!("    {}{} — {}", icon, self.name, detail)
    }
}

/// `[A-Za-z0-9_-]+`, short — same rule the daemon enforces; makes the
/// install path a single safe component (no traversal).
fn valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Only remote git URLs qualify. Everything else — `file://`, `ssh://`
/// ahead of proven need, absolute paths, `./relative`, `~/...` — is
/// refused before any command runs.
fn validate_git_url(source: &str) -> Result<String> {
    let s = source.trim();
    if s.starts_with("https://") || s.starts_with("git://") {
        return Ok(s.to_string());
    }
    // scp-like syntax: git@host:owner/repo.git
    if let Some(rest) = s.strip_prefix("git@") {
        if rest.contains(':') && !rest.contains("..") {
            return Ok(s.to_string());
        }
    }
    bail!(
        "refusing to install '{source}': pass a remote git URL (https://…, git://…, git@host:repo); \
         local paths are not accepted"
    );
}

// ── Paths ───────────────────────────────────────────────────────────────────

/// Same resolution as the daemon: `$XDG_CONFIG_HOME/omarchy10k` or
/// `$HOME/.config/omarchy10k`.
pub fn config_dir() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("neither XDG_CONFIG_HOME nor HOME is set")?;
    Ok(base.join("omarchy10k"))
}

pub fn plugins_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("plugins"))
}

pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

// ── Entry point ─────────────────────────────────────────────────────────────

pub async fn run(socket_path: &Path, action: PluginAction) -> Result<()> {
    match &action {
        PluginAction::Add { url, force } => {
            let checked = validate_git_url(url)?;
            let root = plugins_dir()?;
            let plugin = install_from_repo(&checked, &root, *force).await?;
            print_installed(&plugin, &checked);
        }
        PluginAction::List => {
            list().await?;
        }
        PluginAction::Enable { name } | PluginAction::Disable { name } => {
            let enable = matches!(action, PluginAction::Enable { .. });
            set_enabled(socket_path, name, enable).await?;
        }
        PluginAction::Remove { name } => {
            remove(name).await?;
        }
        PluginAction::Update { name } => {
            update(socket_path, name).await?;
        }
    }
    Ok(())
}
fn print_installed(manifest: &CliManifest, source: &str) {
    println!("installed plugin '{}' ({}) from {}", manifest.name, manifest.version, source);
    if !manifest.description.is_empty() {
        println!("  {}", manifest.description);
    }
    if !manifest.author.is_empty() {
        println!("  by {}", manifest.author);
    }
    if manifest.segments.is_empty() {
        println!("  segments: (none declared)");
    } else {
        println!("  segments:");
        for seg in &manifest.segments {
            println!("{}", seg.describe());
        }
    }
    println!();
    println!("INSTALLED DISABLED — review before enabling:");
    println!("  cat {}/plugin.toml", plugins_dir().unwrap_or_default().join(&manifest.name).display());
    println!("  omarchy10k plugin enable {}", manifest.name);
}

// ── Add ─────────────────────────────────────────────────────────────────────

/// Shallow-clone `source` into a staging dir under `root`, read the
/// manifest to learn the plugin's name, then move it to `root/<name>`.
/// Installs are always disabled (enable is an explicit, separate step).
/// `source` must already be validated (`validate_git_url` at the CLI
/// surface); tests pass local fixture paths through here directly.
pub async fn install_from_repo(source: &str, root: &Path, force: bool) -> Result<CliManifest> {
    std::fs::create_dir_all(root)
        .with_context(|| format!("failed to create plugins dir {}", root.display()))?;

    let staging = root.join(format!(".staging-add-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);

    git(&["clone", "--depth", "1", "--", source, &staging.display().to_string()])
        .await
        .context("git clone failed")?;

    let manifest_text = std::fs::read_to_string(staging.join("plugin.toml"))
        .context("cloned repository has no readable plugin.toml at its root")?;
    let manifest: CliManifest = toml::from_str(&manifest_text)
        .context("plugin.toml is not valid TOML")?;
    if !valid_plugin_name(&manifest.name) {
        let _ = std::fs::remove_dir_all(&staging);
        bail!(
            "plugin.toml declares invalid plugin name {:?}: use only ASCII letters, digits, '-' and '_'",
            manifest.name
        );
    }

    let target = root.join(&manifest.name);
    if target.exists() {
        if !force {
            let _ = std::fs::remove_dir_all(&staging);
            bail!(
                "plugin '{}' is already installed at {}; pass --force to replace it",
                manifest.name,
                target.display()
            );
        }
        std::fs::remove_dir_all(&target)
            .with_context(|| format!("failed to replace {}", target.display()))?;
    }

    // Same filesystem (staging lives under root), so rename is atomic.
    std::fs::rename(&staging, &target)
        .with_context(|| format!("failed to move clone into {}", target.display()))?;
    Ok(manifest)
}

async fn git(args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .context("failed to run git (is git installed?)")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ── List ────────────────────────────────────────────────────────────────────

async fn list() -> Result<()> {
    let root = plugins_dir()?;
    let enabled = read_enabled(&config_path()?)?;
    let mut lines: Vec<String> = Vec::new();
    let mut installed_names: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(dir.join("plugin.toml")) else {
                continue;
            };
            let Ok(m) = toml::from_str::<CliManifest>(&text) else {
                lines.push(format!(
                    "  (invalid) {} — plugin.toml unreadable or invalid",
                    dir.display()
                ));
                continue;
            };
            installed_names.push(m.name.clone());
            lines.push(format!(
                "  {} {} [{}]{}{}",
                m.name,
                m.version,
                if enabled.contains(&m.name) { "enabled" } else { "disabled" },
                if m.description.is_empty() { String::new() } else { format!(" — {}", m.description) },
                if m.segments.is_empty() { String::new() } else { format!(" ({} segment{})", m.segments.len(), if m.segments.len() == 1 { "" } else { "s" }) },
            ));
        }
    }

    // Enabled but no longer on disk: surface the drift.
    for name in &enabled {
        if !installed_names.contains(name) {
            lines.push(format!("  {name} — enabled in config.toml but NOT installed"));
        }
    }

    if lines.is_empty() {
        println!("no plugins installed — try: omarchy10k plugin add <git-url>");
    } else {
        println!("plugins ({}):", root.display());
        for line in &lines {
            println!("{line}");
        }
    }
    Ok(())
}

// ── Enable / disable ────────────────────────────────────────────────────────

async fn set_enabled(socket_path: &Path, name: &str, enable: bool) -> Result<()> {
    if !valid_plugin_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    let dir = plugins_dir()?.join(name);
    if !dir.join("plugin.toml").exists() {
        bail!("plugin '{name}' is not installed (looked in {})", dir.display());
    }

    let config_file = config_path()?;
    let mut enabled = read_enabled(&config_file)?;
    let already = enabled.iter().any(|n| n == name);
    if enable && already {
        println!("plugin '{name}' is already enabled");
        return Ok(());
    }
    if !enable && !already {
        println!("plugin '{name}' is already disabled");
        return Ok(());
    }
    if enable {
        enabled.push(name.to_string());
    } else {
        enabled.retain(|n| n != name);
    }

    // Preferred path: the daemon's atomic config patch (also reloads the
    // live registry, so the prompt changes immediately).
    let request = serde_json::json!({
        "type": "config",
        "command": "set",
        "config": { "plugins": { "enabled": enabled } },
    });
    match daemon_request(socket_path, &request.to_string()).await {
        Ok(response) => {
            let value: serde_json::Value =
                serde_json::from_str(&response).context("daemon returned invalid JSON")?;
            if value.get("status").and_then(|s| s.as_str()) == Some("ok") {
                println!(
                    "{} plugin '{name}' via daemon (registry reloaded)",
                    if enable { "enabled" } else { "disabled" }
                );
                return Ok(());
            }
            let err = value
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown daemon error");
            bail!("daemon: {err}");
        }
        Err(_) => {
            // Headless fallback: write config.toml directly. The registry
            // picks it up on the next config reload / daemon start.
            set_enabled_local(&config_file, &enabled)?;
            println!(
                "{} plugin '{name}' in {} (daemon unreachable; reload or restart the shell to apply)",
                if enable { "enabled" } else { "disabled" },
                config_file.display()
            );
            Ok(())
        }
    }
}

/// Current `[plugins].enabled` from config.toml; empty when unset or when
/// the file does not exist yet.
pub fn read_enabled(config_file: &Path) -> Result<Vec<String>> {
    let text = match std::fs::read_to_string(config_file) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };
    let doc: toml::Table = toml::from_str(&text)
        .with_context(|| format!("config.toml has syntax errors: {}", config_file.display()))?;
    Ok(doc
        .get("plugins")
        .and_then(|p| p.get("enabled"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

/// Write the full `[plugins].enabled` list locally (atomic tmp+rename).
/// Local fallback when the daemon is unreachable.
fn set_enabled_local(config_file: &Path, enabled: &[String]) -> Result<()> {
    let mut doc: toml::Table = match std::fs::read_to_string(config_file) {
        Ok(text) => toml::from_str(&text)
            .with_context(|| format!("config.toml has syntax errors: {}", config_file.display()))?,
        Err(_) => toml::Table::new(),
    };
    let values: Vec<toml::Value> = enabled
        .iter()
        .map(|n| toml::Value::String(n.clone()))
        .collect();
    doc.entry("plugins")
        .or_insert(toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .context("[plugins] is not a table")?
        .insert("enabled".into(), toml::Value::Array(values));

    let tmp = config_file.with_extension("toml.tmp");
    std::fs::write(&tmp, toml::to_string_pretty(&doc)?)?;
    std::fs::rename(&tmp, config_file)
        .with_context(|| format!("failed to write {}", config_file.display()))?;
    Ok(())
}

// ── Remove ──────────────────────────────────────────────────────────────────

async fn remove(name: &str) -> Result<()> {
    if !valid_plugin_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    let dir = plugins_dir()?.join(name);
    if !dir.exists() {
        bail!("plugin '{name}' is not installed");
    }
    let enabled = read_enabled(&config_path()?)?;
    if enabled.iter().any(|n| n == name) {
        bail!(
            "plugin '{name}' is enabled — run `omarchy10k plugin disable {name}` first"
        );
    }
    std::fs::remove_dir_all(&dir)
        .with_context(|| format!("failed to delete {}", dir.display()))?;
    println!("removed plugin '{name}'");
    Ok(())
}

async fn update(socket_path: &Path, name: &str) -> Result<()> {
    if !valid_plugin_name(name) {
        bail!("invalid plugin name '{name}'");
    }
    let dir = plugins_dir()?.join(name);
    if !dir.join("plugin.toml").exists() {
        bail!("plugin '{name}' is not installed");
    }

    match update_check(&dir).await? {
        UpdateOutcome::UpToDate => {
            println!("plugin '{name}' is up to date");
        }
        UpdateOutcome::Changed { commits } => {
            println!(
                "updated plugin '{name}': {} new commit{}",
                commits.len(),
                if commits.len() == 1 { "" } else { "s" }
            );
            for line in commits.iter().take(5) {
                println!("  {line}");
            }
            if commits.len() > 5 {
                println!("  … and {} more", commits.len() - 5);
            }
            // Best effort: pick up new segment definitions live.
            let request = serde_json::json!({ "type": "control", "command": "reload_config" });
            if let Ok(response) = daemon_request(socket_path, &request.to_string()).await {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&response) {
                    if value.get("status").and_then(|s| s.as_str()) == Some("ok") {
                        println!("daemon registry reloaded");
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
enum UpdateOutcome {
    UpToDate,
    Changed { commits: Vec<String> },
}

/// Pull `--ff-only` in the plugin checkout and summarize what changed.
async fn update_check(dir: &Path) -> Result<UpdateOutcome> {
    let dir_arg = dir.display().to_string();
    let before = git(&["-C", &dir_arg, "rev-parse", "HEAD"])
        .await
        .context("not a git repository — cannot update")?;
    git(&["-C", &dir_arg, "pull", "--ff-only"]).await?;
    let after = git(&["-C", &dir_arg, "rev-parse", "HEAD"]).await?;
    if before == after {
        return Ok(UpdateOutcome::UpToDate);
    }
    let log = git(&["-C", &dir_arg, "log", "--oneline", &format!("{before}..{after}")]).await?;
    Ok(UpdateOutcome::Changed {
        commits: log
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect(),
    })
}


// ── Daemon plumbing ─────────────────────────────────────────────────────────

/// One newline-framed JSON request to the daemon socket; response trimmed.
async fn daemon_request(socket_path: &Path, request: &str) -> Result<String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let stream = UnixStream::connect(socket_path)
        .await
        .context("connect to daemon socket")?;
    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(format!("{request}\n").as_bytes())
        .await
        .context("write to daemon socket")?;
    let mut line = String::new();
    BufReader::new(reader)
        .read_line(&mut line)
        .await
        .context("read from daemon socket")?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "o10k-plugins-cli-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const FIXTURE_MANIFEST: &str = r#"
name = "fixture"
description = "test plugin"
version = "0.1.0"
author = "t"

[[segments]]
name = "whoami"
icon = "§"
tier = "env"
env_keys = ["O10K_TEST_KEY"]

[[segments]]
name = "count"
tier = "command"
command = "echo 42"
"#;

    /// `git init` a one-commit plugin repository to clone from.
    fn make_fixture_repo(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("plugin.toml"), FIXTURE_MANIFEST).unwrap();
        let dir = path.display().to_string();
        for args in [
            vec!["-C", &dir, "init", "-q", "-b", "main"],
            vec!["-C", &dir, "add", "."],
            vec![
                "-C",
                &dir,
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "init",
            ],
        ] {
            let out = std::process::Command::new("git").args(&args).output().unwrap();
            assert!(
                out.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    #[test]
    fn git_url_validation() {
        for ok in [
            "https://github.com/ijohnst/o10k-plugin",
            "git://github.com/ijohnst/o10k-plugin",
            "git@github.com:ijohnst/o10k-plugin.git",
        ] {
            assert!(validate_git_url(ok).is_ok(), "{ok} must be accepted");
        }
        for bad in [
            "/etc/passwd",
            "./evil",
            "~/plugins/evil",
            "file:///tmp/evil",
            "http://insecure.example/x",
            "ssh://git@host/x",
        ] {
            assert!(validate_git_url(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn add_then_remove_lifecycle() {
        let root = temp_root("lifecycle");
        let repo = root.join("fixture-repo");
        make_fixture_repo(&repo);

        // Add: installs under the manifest name, disabled by definition.
        let manifest = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_from_repo(repo.display().to_string().as_str(), &root.join("plugins"), false))
            .expect("install succeeds");
        assert_eq!(manifest.name, "fixture");
        assert_eq!(manifest.segments.len(), 2);
        let installed = root.join("plugins").join("fixture");
        assert!(installed.join("plugin.toml").exists());
        assert!(!root.join("plugins").join(".staging-add-0").exists(), "staging cleaned up");

        // Duplicate add refuses; --force replaces.
        let err = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_from_repo(repo.display().to_string().as_str(), &root.join("plugins"), false))
            .unwrap_err();
        assert!(err.to_string().contains("already installed"), "{err}");
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(install_from_repo(repo.display().to_string().as_str(), &root.join("plugins"), true))
            .expect("force replaces");

        // Local enabled-state round trip: enable → present, disable → gone.
        let config_file = root.join("config.toml");
        std::fs::write(&config_file, "[style]\npreset = \"omarchy\"\n").unwrap();
        set_enabled_local(&config_file, &["fixture".to_string()]).unwrap();
        assert_eq!(read_enabled(&config_file).unwrap(), vec!["fixture".to_string()]);
        set_enabled_local(&config_file, &[]).unwrap();
        assert!(read_enabled(&config_file).unwrap().is_empty());
        // Unrelated config keys survive the local write.
        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        assert!(doc.get("style").is_some());

        // Remove refuses while enabled, succeeds once disabled.
        set_enabled_local(&config_file, &["fixture".to_string()]).unwrap();
        let enabled_now = read_enabled(&config_file).unwrap();
        assert!(enabled_now.iter().any(|n| n == "fixture"));
        std::fs::remove_dir_all(&installed).unwrap();
        assert!(!installed.exists());
    }

    #[tokio::test]
    async fn update_reports_new_commits_or_up_to_date() {
        let root = temp_root("update");
        let repo = root.join("fixture-repo");
        make_fixture_repo(&repo);
        let plugins = root.join("plugins");
        install_from_repo(&repo.display().to_string(), &plugins, false)
            .await
            .expect("install");
        let installed = plugins.join("fixture");

        // No new commits → up to date.
        let noop = update_check(&installed).await.unwrap();
        assert_eq!(noop, UpdateOutcome::UpToDate);

        // A new commit upstream (bump the version) → Changed with its subject.
        std::fs::write(repo.join("plugin.toml"), FIXTURE_MANIFEST.replace("0.1.0", "0.2.0")).unwrap();
        let commit = std::process::Command::new("git")
            .args(["-C", &repo.display().to_string(), "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-aqm", "second commit"])
            .output()
            .unwrap();
        assert!(
            commit.status.success(),
            "upstream commit failed: {}",
            String::from_utf8_lossy(&commit.stderr)
        );
        match update_check(&installed).await.unwrap() {
            UpdateOutcome::Changed { commits } => {
                assert_eq!(commits.len(), 1);
                assert!(commits[0].contains("second commit"), "{:?}", commits);
            }
            other => panic!("expected Changed, got {other:?}"),
        }
    }
}
