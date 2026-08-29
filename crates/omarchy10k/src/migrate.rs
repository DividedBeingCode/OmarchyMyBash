//! `omarchy10k migrate <starship.toml>` — starship prompt migration importer.
//!
//! Parses a starship.toml and translates its segment inventory into an o10k
//! Look named `migrated-starship`. Dry-run by default: prints the mapping
//! table (starship item → o10k segment) plus an honest list of unmapped
//! items. `--yes` saves the Look through the daemon socket (`config set`
//! with the `looks` table, same as `look install`); when no daemon is
//! reachable the config file is updated locally — identical to share.rs.
//!
//! Mapping (starship → o10k, same registry the wizard/panel/catalog use):
//!   $directory            → directory
//!   $git_branch/$git_status → git
//!   $cmd_duration         → command_duration
//!   $character            → prompt char
//!   $time/$battery/$jobs  → time/battery/jobs
//!   $hostname/$username   → ssh
//!   $python/$conda        → python_env
//!   $nodejs/$rust/$golang/$ruby/$java → toolchain
//!   $aws/$gcloud/$kubernetes/$terraform → aws_profile/gcloud_project/k8s/terraform_workspace
//!   $package              → package_version
//!   $docker_context       → docker_context
//! Known gaps (reported, never silently dropped): starship's per-segment
//! color schemes, `$custom` commands, `$fill`, and any module without an
//! o10k counterpart are listed as unmapped.

use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::Path;

use crate::share::{daemon_request, user_config_path, write_look_local};

/// Name the migrated Look is saved under.
const LOOK_NAME: &str = "migrated-starship";

/// One row of the migration report: starship item(s) → o10k segment.
/// Collapsed targets list every contributing token (`git_branch, git_status`).
#[derive(Debug, Clone, PartialEq)]
pub struct Mapping {
    pub starship: String,
    pub o10k: &'static str,
}

/// Result of parsing + translating a starship config.
#[derive(Debug, PartialEq)]
pub struct Migration {
    pub mappings: Vec<Mapping>,
    pub unmapped: Vec<String>,
    pub blank_line: bool,
    pub directory_max_length: Option<usize>,
    /// starship [cmd_duration].min_time (ms) → segments.command_duration.show_above_ms
    pub cmd_duration_min_ms: Option<u64>,
    /// Starship palette table name, if the config defines `[palettes.<name>]`.
    pub palette: Option<String>,
}

/// Map a starship module name to its o10k segment, if known.
fn map_module(module: &str) -> Option<&'static str> {
    match module {
        "directory" => Some("directory"),
        "git_branch" | "git_status" => Some("git"),
        "cmd_duration" => Some("command_duration"),
        "character" => Some("character"),
        "time" => Some("time"),
        "battery" => Some("battery"),
        "jobs" => Some("jobs"),
        // starship shows hostname/username for remote sessions; o10k gates
        // its ssh segment on an actual SSH_CONNECTION, so enable it.
        "hostname" | "username" => Some("ssh"),
        "python" | "conda" => Some("python_env"),
        "nodejs" | "rust" | "golang" | "ruby" | "java" => Some("toolchain"),
        "aws" => Some("aws_profile"),
        "gcloud" => Some("gcloud_project"),
        "kubernetes" => Some("k8s"),
        "terraform" => Some("terraform_workspace"),
        "package" => Some("package_version"),
        "docker_context" => Some("docker_context"),
        _ => None,
    }
}

/// Extract the `$module` names from a starship `format` string, in order,
/// deduplicated. Handles both `$module` and `${module}` (dotted names such
/// as `${custom.foo}` collapse to their prefix `custom`).
fn modules_in_format(format: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = format;
    while let Some(idx) = rest.find('$') {
        rest = &rest[idx + 1..];
        if let Some(after) = rest.strip_prefix('{') {
            // ${name} form — name may contain dots; stop at '}'.
            let Some(close) = after.find('}') else { break };
            let name = &after[..close];
            let base = name.split('.').next().unwrap_or("");
            if !base.is_empty() && !out.iter().any(|m| m == base) {
                out.push(base.to_string());
            }
            rest = &after[close + 1..];
            continue;
        }
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if end > 0 {
            let name = &rest[..end];
            if name != "all" && !out.iter().any(|m| m == name) {
                out.push(name.to_string());
            }
        }
        rest = &rest[end..];
    }
    out
}

/// Parse a starship.toml into a migration plan.
pub fn parse_starship(text: &str) -> Result<Migration> {
    let doc: toml::Table = toml::from_str(text)
        .map_err(|e| anyhow::anyhow!("invalid starship.toml: {e}"))?;

    // Segment inventory: the format string is authoritative; configured
    // module tables add to it (a `[battery]` table is a strong signal the
    // user cares about the module even when it's absent from format).
    let mut items: Vec<String> = Vec::new();
    if let Some(f) = doc.get("format").and_then(|v| v.as_str()) {
        items.extend(modules_in_format(f));
    }
    for key in doc.keys() {
        if map_module(key).is_some() && !items.iter().any(|m| m == key) {
            items.push(key.clone());
        }
    }

    let mut mappings: Vec<Mapping> = Vec::new();
    let mut unmapped: Vec<String> = Vec::new();
    for item in &items {
        match map_module(item) {
            Some(seg) => match mappings.iter_mut().find(|m| m.o10k == seg) {
                Some(m) => m.starship.push_str(&format!(", {item}")),
                None => mappings.push(Mapping { starship: item.clone(), o10k: seg }),
            },
            None => unmapped.push(item.clone()),
        }
    }

    let blank_line = doc
        .get("add_newline")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let directory_max_length = doc
        .get("directory")
        .and_then(|d| d.get("truncation_length"))
        .and_then(|v| v.as_integer())
        .map(|v| v as usize);
    let cmd_duration_min_ms = doc
        .get("cmd_duration")
        .and_then(|d| d.get("min_time"))
        .and_then(|v| v.as_integer())
        .map(|v| v as u64);
    // Palette hint: starship `[palettes.<name>]` tables define named colors.
    // Full translation into theme.custom is a known gap — o10k palette keys
    // differ — so the run reports it and keeps the current palette.
    let palette = doc
        .get("palettes")
        .and_then(|p| p.as_table())
        .and_then(|p| p.keys().next().cloned());
    Ok(Migration {
        mappings,
        unmapped,
        blank_line,
        directory_max_length,
        cmd_duration_min_ms,
        palette,
    })
}

impl Migration {
    pub fn patch(&self) -> toml::Table {
        let mut segments = toml::Table::new();
        for m in &self.mappings {
            // `directory` is configured top-level ([directory]), not under
            // [segments.*] — skip it here; the dir table below enables it.
            if m.o10k == "directory" {
                continue;
            }
            let mut seg = toml::Table::new();
            seg.insert("enabled".into(), toml::Value::Boolean(true));
            segments.insert(m.o10k.into(), toml::Value::Table(seg));
        }
        let mut prompt = toml::Table::new();
        prompt.insert("blank_line".into(), toml::Value::Boolean(self.blank_line));

        if let Some(ms) = self.cmd_duration_min_ms {
            let seg = segments
                .get_mut("command_duration")
                .and_then(|v| v.as_table_mut())
                .expect("command_duration mapping present");
            seg.insert("show_above_ms".into(), toml::Value::Integer(ms as i64));
        }

        let mut patch = toml::Table::new();
        patch.insert("prompt".into(), toml::Value::Table(prompt));
        patch.insert("segments".into(), toml::Value::Table(segments));
        let mut dir = toml::Table::new();
        dir.insert("enabled".into(), toml::Value::Boolean(true));
        if let Some(len) = self.directory_max_length {
            dir.insert("max_length".into(), toml::Value::Integer(len as i64));
        }
        patch.insert("directory".into(), toml::Value::Table(dir));
        patch
    }
}

/// Build the `[looks.migrated-starship]` entry table from a plan.
fn look_entry(plan: &Migration) -> Result<toml::Table> {
    let mut entry = toml::Table::new();
    entry.insert("label".into(), toml::Value::String("Migrated from starship".into()));
    entry.insert("palette".into(), toml::Value::String("keep".into()));
    entry.insert("patch".into(), toml::Value::Table(plan.patch()));
    Ok(entry)
}

/// Save the Look through the daemon, falling back to a local config write.
/// Returns `Ok(via_daemon)`.
async fn save_look(socket_path: &Path, config_path: &Path, entry: &toml::Table) -> Result<bool> {
    let request = json!({
        "type": "config",
        "command": "set",
        "config": { "looks": { LOOK_NAME: serde_json::to_value(entry)? } },
    });
    match daemon_request(socket_path, &request.to_string()).await {
        Ok(response) => {
            let value: serde_json::Value =
                serde_json::from_str(&response).context("daemon returned invalid JSON")?;
            if value.get("status").and_then(|s| s.as_str()) == Some("ok") {
                Ok(true)
            } else {
                let err = value
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown daemon error");
                bail!("daemon: {err}");
            }
        }
        Err(e) => {
            eprintln!("omarchy10k: daemon unreachable ({e:#}); writing config.toml directly");
            write_look_local(config_path, LOOK_NAME, entry)?;
            Ok(false)
        }
    }
}

/// Run the migration: parse, report, and (with `yes`) save the Look.
pub async fn run(socket_path: &Path, source: &str, yes: bool) -> Result<()> {
    let config_path = user_config_path()?;
    migrate_in(socket_path, &config_path, source, yes).await
}

/// Testable core: explicit socket + config path.
pub async fn migrate_in(
    socket_path: &Path,
    config_path: &Path,
    source: &str,
    yes: bool,
) -> Result<()> {
    let text =
        std::fs::read_to_string(source).with_context(|| format!("failed to read {source}"))?;
    let plan = parse_starship(&text)?;

    // Mapping report.
    println!("starship → omarchy10k mapping ({source}):");
    for m in &plan.mappings {
        println!("  ${}  →  {}", m.starship, m.o10k);
    }
    if let Some(len) = plan.directory_max_length {
        println!("  directory.truncation_length = {len}  →  directory.max_length");
    }
    if let Some(ms) = plan.cmd_duration_min_ms {
        println!("  cmd_duration.min_time = {ms}ms  →  segments.command_duration.show_above_ms");
    }
    println!("  add_newline = {}  →  prompt.blank_line", plan.blank_line);
    if let Some(p) = &plan.palette {
        println!("  palette '{p}' → palette: keep (custom palette translation not supported)");
    }
    if plan.unmapped.is_empty() {
        println!("  (no unmapped starship modules)");
    } else {
        println!("unmapped starship modules (no o10k equivalent):");
        for u in &plan.unmapped {
            println!("  ${u}");
        }
    }

    if !yes {
        println!(
            "{} segments mapped, {} unmapped — dry run only, re-run with --yes to save Look '{LOOK_NAME}' into {}",
            plan.mappings.len(),
            plan.unmapped.len(),
            config_path.display()
        );
        return Ok(());
    }

    let entry = look_entry(&plan)?;
    let via_daemon = save_look(socket_path, config_path, &entry).await?;
    let via = if via_daemon { "via daemon" } else { "into config.toml" };
    println!(
        "{} segments mapped, {} unmapped, saved as Look {LOOK_NAME} {via}",
        plan.mappings.len(),
        plan.unmapped.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/starship.toml");

    #[test]
    fn fixture_maps_known_and_lists_unknown() {
        let plan = parse_starship(FIXTURE).unwrap();
        let pairs: Vec<(&str, &str)> =
            plan.mappings.iter().map(|m| (m.starship.as_str(), m.o10k)).collect();
        assert!(pairs.contains(&("directory", "directory")));
        assert!(pairs.contains(&("git_branch, git_status", "git")));
        assert!(pairs.contains(&("cmd_duration", "command_duration")));
        assert!(pairs.contains(&("character", "character")));
        assert!(pairs.contains(&("jobs", "jobs")));
        // git_branch and git_status collapse to one o10k git segment.
        assert_eq!(
            pairs.iter().filter(|(_, o)| *o == "git").count(),
            1,
            "git must collapse into one segment"
        );
        // The 2 unknown modules from the fixture format string.
        assert_eq!(plan.unmapped, vec!["fill", "memory_usage"]);
        assert_eq!(plan.blank_line, true);
        assert_eq!(plan.directory_max_length, Some(4));
    }

    #[test]
    fn toolchain_family_collapses() {
        let plan = parse_starship("format = \"$python$nodejs$rust$golang$conda\"\n").unwrap();
        let o10k: Vec<&str> = plan.mappings.iter().map(|m| m.o10k).collect();
        assert_eq!(o10k, vec!["python_env", "toolchain"]);
    }

    #[test]
    fn cloud_and_ssh_modules_map() {
        let plan = parse_starship(
            "format = \"$hostname$username$aws$gcloud$kubernetes$terraform\"\n",
        )
        .unwrap();
        let o10k: Vec<&str> = plan.mappings.iter().map(|m| m.o10k).collect();
        assert!(o10k.contains(&"ssh"));
        assert!(o10k.contains(&"aws_profile"));
        assert!(o10k.contains(&"gcloud_project"));
        assert!(o10k.contains(&"k8s"));
        assert!(o10k.contains(&"terraform_workspace"));
    }

    #[test]
    fn brace_form_and_dotted_names_parse() {
        let plan = parse_starship("format = \"${directory}${custom.backup}$jobs\"\n").unwrap();
        let o10k: Vec<&str> = plan.mappings.iter().map(|m| m.o10k).collect();
        assert_eq!(o10k, vec!["directory", "jobs"]);
        assert_eq!(plan.unmapped, vec!["custom"]);
    }

    #[test]
    fn patch_enables_segments_and_style_hints() {
        let plan = parse_starship(FIXTURE).unwrap();
        let patch = plan.patch();
        let segments = patch["segments"].as_table().unwrap();
        assert_eq!(segments["git"]["enabled"].as_bool(), Some(true));
        assert_eq!(segments["command_duration"]["enabled"].as_bool(), Some(true));
        assert_eq!(segments["jobs"]["enabled"].as_bool(), Some(true));
        assert_eq!(patch["prompt"]["blank_line"].as_bool(), Some(true));
        assert_eq!(segments["command_duration"]["show_above_ms"].as_integer(), Some(2000));
    }

    #[test]
    fn patch_carries_new_catalog_segments() {
        let plan =
            parse_starship("format = \"$aws$gcloud$kubernetes$terraform\"\n").unwrap();
        let patch = plan.patch();
        let segments = patch["segments"].as_table().unwrap();
        assert_eq!(segments["aws_profile"]["enabled"].as_bool(), Some(true));
        assert_eq!(segments["gcloud_project"]["enabled"].as_bool(), Some(true));
        assert_eq!(segments["k8s"]["enabled"].as_bool(), Some(true));
        assert_eq!(segments["terraform_workspace"]["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn palette_hint_is_reported_and_palette_kept() {
        let plan = parse_starship("palette = \"rose\"\n[palettes.rose]\nbase = \"#fff\"\n")
            .unwrap();
        assert_eq!(plan.palette.as_deref(), Some("rose"));
        assert_eq!(look_entry(&plan).unwrap()["palette"].as_str(), Some("keep"));
    }

    #[test]
    fn configured_table_absent_from_format_still_maps() {
        let plan = parse_starship("format = \"$directory\"\n[battery]\nfull_symbol = \"🔋\"\n")
            .unwrap();
        let o10k: Vec<&str> = plan.mappings.iter().map(|m| m.o10k).collect();
        assert!(o10k.contains(&"battery"));
    }

    #[test]
    fn malformed_file_is_a_clear_error() {
        let err = parse_starship("this is = not [valid toml").unwrap_err();
        assert!(err.to_string().contains("invalid starship.toml"), "got: {err}");
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "o10k-migrate-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn missing_file_is_a_clear_error() {
        let dir = temp_dir("missing");
        let err = migrate_in(
            Path::new("/nonexistent-o10k.sock"),
            &dir.join("config.toml"),
            dir.join("does-not-exist.toml").to_str().unwrap(),
            false,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("failed to read"), "got: {err}");
    }

    #[tokio::test]
    async fn dry_run_does_not_write() {
        let dir = temp_dir("dry");
        let config_path = dir.join("config.toml");
        let fixture_path = dir.join("starship.toml");
        std::fs::write(&fixture_path, FIXTURE).unwrap();

        migrate_in(
            Path::new("/nonexistent-o10k.sock"),
            &config_path,
            fixture_path.to_str().unwrap(),
            false,
        )
        .await
        .unwrap();

        assert!(!config_path.exists(), "dry run must not write config.toml");
    }

    #[tokio::test]
    async fn yes_saves_look_via_local_fallback() {
        let dir = temp_dir("yes");
        let config_path = dir.join("config.toml");
        let fixture_path = dir.join("starship.toml");
        std::fs::write(&fixture_path, FIXTURE).unwrap();

        // Socket does not exist → local config write fallback.
        migrate_in(
            Path::new("/nonexistent-o10k.sock"),
            &config_path,
            fixture_path.to_str().unwrap(),
            true,
        )
        .await
        .unwrap();

        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let entry = doc["looks"]["migrated-starship"].as_table().unwrap();
        assert_eq!(entry["label"].as_str(), Some("Migrated from starship"));
        assert_eq!(entry["palette"].as_str(), Some("keep"));
        let patch = entry["patch"].as_table().unwrap();
        assert_eq!(patch["segments"]["git"]["enabled"].as_bool(), Some(true));
        assert_eq!(patch["segments"]["command_duration"]["enabled"].as_bool(), Some(true));
        assert_eq!(patch["prompt"]["blank_line"].as_bool(), Some(true));
        assert_eq!(patch["directory"]["max_length"].as_integer(), Some(4));
    }
}
