//! `omarchy10k look export/install` — portable Look share bundles (brainstorm B5).
//!
//! Export turns a Look into a self-contained TOML bundle: a comment header
//! (name, label, o10k version) plus the `[looks.<name>]` table verbatim.
//! User Looks export their raw config entry (palette directive and glyph
//! shortcuts intact); curated Looks export their compiled patch, resolved
//! through the daemon (`looks` control verb).
//!
//! Install validates the bundle (exactly one `[looks.<name>]` table, entry
//! keys limited to label/palette/patch, patch must be a table) and applies
//! it safely: without `--yes` the resolved patch is only printed. With
//! `--yes` the Look is written through the daemon socket (`config set` with
//! the `looks` table); when no daemon is reachable the config file is
//! updated locally (atomic tmp+rename, exact table replacement) — a running
//! daemon's file watcher picks the change up. Overwriting an existing user
//! Look requires `--force` (or a different name via `--as`). Sources are
//! local files or https:// URLs only — fetched with curl, never http.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Round-trip budget for daemon requests.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(30);

/// Keys allowed inside a `[looks.<name>]` bundle entry.
const LOOK_KEYS: [&str; 3] = ["label", "palette", "patch"];

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A validated Look bundle, ready to install.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub name: String,
    pub label: String,
    pub palette: Option<String>,
    pub patch: toml::Table,
}

impl Bundle {
    /// The `[looks.<name>]` entry table as it will land in config.toml.
    pub fn entry_table(&self) -> toml::Table {
        let mut entry = toml::Table::new();
        entry.insert("label".into(), toml::Value::String(self.label.clone()));
        if let Some(palette) = &self.palette {
            entry.insert("palette".into(), toml::Value::String(palette.clone()));
        }
        entry.insert("patch".into(), toml::Value::Table(self.patch.clone()));
        entry
    }
}

/// Result of [`install_in`].
#[derive(Debug, PartialEq)]
pub enum InstallOutcome {
    /// Nothing written; the resolved patch was returned for review.
    DryRun { name: String, label: String, preview: String },
    /// The Look was written.
    Installed { name: String, via_daemon: bool },
}

// ── Export ──────────────────────────────────────────────────────────────────

/// Export a Look as a portable TOML bundle.
///
/// User entries (from the local config.toml) export verbatim for full
/// fidelity; curated Looks resolve through the daemon and export their
/// compiled patch. `run_export` handles output sinks; this returns the text.
pub async fn export_in(socket_path: &Path, config_path: &Path, name: &str) -> Result<String> {
    if let Some(doc) = load_config_doc(config_path)? {
        if let Some(entry) = doc
            .get("looks")
            .and_then(|v| v.as_table())
            .and_then(|looks| looks.get(name))
            .and_then(|v| v.as_table())
        {
            let label = entry.get("label").and_then(|v| v.as_str()).unwrap_or(name);
            let palette = entry.get("palette").and_then(|v| v.as_str());
            let patch = match entry.get("patch") {
                Some(toml::Value::Table(t)) => t.clone(),
                Some(_) => bail!("user look '{name}' has a non-table patch; cannot export verbatim"),
                // The daemon deserializes a missing patch as an empty table.
                None => toml::Table::new(),
            };
            return build_bundle(name, label, palette, &patch, VERSION);
        }
    }
    export_curated(socket_path, name).await
}

/// Export a curated Look via the daemon's compiled patch.
async fn export_curated(socket_path: &Path, name: &str) -> Result<String> {
    let response = daemon_request(socket_path, r#"{"command":"looks"}"#)
        .await
        .context(
            "curated Looks are compiled into the daemon; export needs a running daemon \
             (or a user entry with that name in config.toml)",
        )?;
    let value: serde_json::Value =
        serde_json::from_str(&response).context("daemon returned invalid JSON")?;
    let looks = value
        .get("looks")
        .and_then(|v| v.as_array())
        .context("daemon response is missing 'looks'")?;
    let look = looks
        .iter()
        .find(|l| l.get("name").and_then(|v| v.as_str()) == Some(name))
        .with_context(|| format!("unknown Look: {name}"))?;
    let label = look.get("label").and_then(|v| v.as_str()).unwrap_or(name);
    let patch_json = look
        .get("patch")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let patch: toml::Table = serde_json::from_value(patch_json)
        .map_err(|e| anyhow::anyhow!("compiled patch of '{name}' is not TOML-representable: {e}"))?;
    // The compiled patch already has the palette resolved into `theme`, so
    // the bundle re-exports with the `keep` directive.
    build_bundle(name, label, Some("keep"), &patch, VERSION)
}

/// Export entry point: emit to `--out` FILE, `--clipboard`, or stdout.
pub async fn run_export(socket_path: &Path, name: &str, out: Option<&str>, clipboard: bool) -> Result<()> {
    let config_path = user_config_path()?;
    let bundle = export_in(socket_path, &config_path, name).await?;
    if let Some(path) = out {
        std::fs::write(path, &bundle)
            .with_context(|| format!("failed to write bundle to '{path}'"))?;
        println!("look bundle written to {path}");
    }
    if clipboard {
        copy_to_clipboard(&bundle)?;
        eprintln!("look bundle copied to clipboard");
    }
    if out.is_none() && !clipboard {
        print!("{bundle}");
    }
    Ok(())
}

// ── Install ─────────────────────────────────────────────────────────────────

/// Install entry point: resolve the source, validate, then print or write.
pub async fn run_install(
    socket_path: &Path,
    source: &str,
    yes: bool,
    force: bool,
    as_name: Option<&str>,
) -> Result<()> {
    let config_path = user_config_path()?;
    match install_in(socket_path, &config_path, source, yes, force, as_name).await? {
        InstallOutcome::DryRun { name, label, preview } => {
            println!("Look '{name}' ({label}) — resolved patch from {source}:");
            println!();
            println!("{preview}");
            println!(
                "dry run only — re-run with --yes to write [looks.{name}] into {}",
                config_path.display()
            );
        }
        InstallOutcome::Installed { name, via_daemon } => {
            if via_daemon {
                println!("installed look '{name}' via daemon");
            } else {
                println!("installed look '{name}' into {}", config_path.display());
            }
        }
    }
    Ok(())
}

/// Validate and (maybe) apply a Look bundle from a file path or https URL.
///
/// Never asks, never writes without `yes`. Overwriting an existing user Look
/// requires `force` (or `as_name` to pick a different name).
pub async fn install_in(
    socket_path: &Path,
    config_path: &Path,
    source: &str,
    yes: bool,
    force: bool,
    as_name: Option<&str>,
) -> Result<InstallOutcome> {
    let text = fetch_bundle(source).await?;
    let bundle = parse_bundle(&text)?;
    let name = as_name.unwrap_or(&bundle.name).to_string();
    if !valid_look_name(&name) {
        bail!(
            "invalid Look name '{name}': use only ASCII letters, digits, '-' and '_' \
             (choose another with --as NAME)"
        );
    }

    let doc = load_config_doc(config_path)?;
    let exists = doc
        .as_ref()
        .and_then(|d| d.get("looks"))
        .and_then(|v| v.as_table())
        .is_some_and(|looks| looks.contains_key(&name));
    if exists && !force {
        bail!(
            "a user Look named '{name}' already exists; pass --force to overwrite it, \
             or --as NAME to install under a different name"
        );
    }

    let entry = bundle.entry_table();
    if !yes {
        let preview = render_look_toml(&name, &entry)?;
        return Ok(InstallOutcome::DryRun { name, label: bundle.label, preview });
    }

    // Preferred write path: the daemon's atomic config patch (merges the
    // `looks` table, reloads in-memory state, refreshes the theme).
    let request = serde_json::json!({
        "type": "config",
        "command": "set",
        "config": { "looks": { name.clone(): serde_json::to_value(&entry)? } },
    });
    let via_daemon = match daemon_request(socket_path, &request.to_string()).await {
        Ok(response) => {
            let value: serde_json::Value = serde_json::from_str(&response)
                .context("daemon returned invalid JSON")?;
            if value.get("status").and_then(|s| s.as_str()) == Some("ok") {
                true
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
            false
        }
    };
    if !via_daemon {
        write_look_local(config_path, &name, &entry)?;
    }
    Ok(InstallOutcome::Installed { name, via_daemon })
}

// ── Bundle format ───────────────────────────────────────────────────────────

/// Serialize a Look into the portable bundle format: comment header plus the
/// `[looks.<name>]` table.
pub fn build_bundle(
    name: &str,
    label: &str,
    palette: Option<&str>,
    patch: &toml::Table,
    version: &str,
) -> Result<String> {
    if name.is_empty() || name.contains('\n') || name.contains('/') {
        bail!("invalid Look name: {name:?}");
    }
    let mut entry = toml::Table::new();
    entry.insert("label".into(), toml::Value::String(label.to_string()));
    if let Some(palette) = palette {
        entry.insert("palette".into(), toml::Value::String(palette.to_string()));
    }
    entry.insert("patch".into(), toml::Value::Table(patch.clone()));
    let mut looks = toml::Table::new();
    looks.insert(name.to_string(), toml::Value::Table(entry));
    let mut doc = toml::Table::new();
    doc.insert("looks".into(), toml::Value::Table(looks));
    let body = toml::to_string_pretty(&toml::Value::Table(doc))
        .context("failed to serialize Look bundle")?;
    Ok(format!(
        "# omarchy10k Look bundle\n\
         # name: {}\n\
         # label: {}\n\
         # version: {version}\n\
         # install: omarchy10k look install <file-or-url> [--as NAME] [--yes]\n\
         \n\
         {body}",
        one_line(name),
        one_line(label),
    ))
}

/// Parse and validate a Look bundle.
///
/// Rejections: invalid TOML, anything but exactly one top-level
/// `[looks.<name>]` table, entry keys outside label/palette/patch, a
/// missing or non-table patch.
pub fn parse_bundle(text: &str) -> Result<Bundle> {
    let doc: toml::Table = toml::from_str(text).context("bundle is not valid TOML")?;
    if doc.len() != 1 || !doc.contains_key("looks") {
        bail!(
            "bundle must contain exactly one top-level [looks.<name>] table \
             (found {} top-level table{})",
            doc.len(),
            if doc.len() == 1 { "" } else { "s" }
        );
    }
    let looks = doc["looks"].as_table().context("[looks] is not a table")?;
    if looks.len() != 1 {
        bail!(
            "bundle must contain exactly one [looks.<name>] table (found {})",
            looks.len()
        );
    }
    let (name, value) = looks
        .iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("[looks] table is empty"))?;
    let entry = value
        .as_table()
        .with_context(|| format!("[looks.{name}] is not a table"))?;
    for key in entry.keys() {
        if !LOOK_KEYS.contains(&key.as_str()) {
            bail!("[looks.{name}] has unexpected key '{key}' (allowed: label, palette, patch)");
        }
    }
    let label = entry
        .get("label")
        .and_then(|v| v.as_str())
        .unwrap_or(name)
        .to_string();
    let palette = entry.get("palette").and_then(|v| v.as_str()).map(String::from);
    let patch = match entry.get("patch") {
        Some(toml::Value::Table(t)) => t.clone(),
        Some(_) => bail!("[looks.{name}].patch must be a table"),
        None => bail!("[looks.{name}] is missing a 'patch' table"),
    };
    Ok(Bundle { name: name.clone(), label, palette, patch })
}

/// Names that install accept: bare TOML-key safe, short, no traversal.
pub fn valid_look_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn one_line(s: &str) -> String {
    s.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect()
}

/// Render the `[looks.<name>]` table exactly as it lands in config.toml.
fn render_look_toml(name: &str, entry: &toml::Table) -> Result<String> {
    let mut looks = toml::Table::new();
    looks.insert(name.to_string(), toml::Value::Table(entry.clone()));
    let mut doc = toml::Table::new();
    doc.insert("looks".into(), toml::Value::Table(looks));
    toml::to_string_pretty(&toml::Value::Table(doc)).context("failed to serialize Look")
}

// ── Sources & storage ───────────────────────────────────────────────────────

/// Read a bundle from a local file, or fetch an https:// URL with curl.
/// Non-https URLs are refused before any network activity.
async fn fetch_bundle(source: &str) -> Result<String> {
    match source.split_once("://") {
        Some(("https", _)) => {
            let output = tokio::process::Command::new("curl")
                .args(["-fsSL", "--max-time", "30", "--", source])
                .output()
                .await
                .context("failed to run curl (is it installed?)")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                bail!("curl failed for '{source}': {stderr}");
            }
            String::from_utf8(output.stdout).context("bundle is not valid UTF-8")
        }
        Some((scheme, _)) => bail!(
            "refusing to fetch '{source}': only https:// URLs are supported (got '{scheme}://')"
        ),
        None => std::fs::read_to_string(source)
            .with_context(|| format!("failed to read bundle file '{source}'")),
    }
}

/// Replace `[looks.<name>]` in the config file (exact table replacement,
/// atomic tmp+rename). Local fallback when the daemon is unreachable.
pub(crate) fn write_look_local(config_path: &Path, name: &str, entry: &toml::Table) -> Result<()> {
    let mut doc = match std::fs::read_to_string(config_path) {
        Ok(text) => toml::from_str::<toml::Table>(&text)
            .with_context(|| format!("config.toml has syntax errors: {}", config_path.display()))?,
        Err(_) => toml::Table::new(),
    };
    let looks = doc
        .entry("looks")
        .or_insert(toml::Value::Table(toml::Table::new()));
    let looks_table = match looks {
        toml::Value::Table(t) => t,
        _ => bail!("[looks] in config.toml is not a table"),
    };
    looks_table.insert(name.to_string(), toml::Value::Table(entry.clone()));

    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let body = toml::to_string_pretty(&toml::Value::Table(doc))
        .context("failed to serialize config")?;
    let tmp_path = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, body)
        .and_then(|_| std::fs::rename(&tmp_path, config_path))
        .context("failed to write config.toml")?;
    Ok(())
}

/// Parse the user's config.toml; `Ok(None)` when it does not exist yet.
fn load_config_doc(config_path: &Path) -> Result<Option<toml::Table>> {
    match std::fs::read_to_string(config_path) {
        Ok(text) => {
            let doc: toml::Table = toml::from_str(&text).with_context(|| {
                format!("config.toml has syntax errors: {}", config_path.display())
            })?;
            Ok(Some(doc))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", config_path.display())),
    }
}

/// `$XDG_CONFIG_HOME/omarchy10k/config.toml` (same resolution as the daemon).
pub(crate) fn user_config_path() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("XDG_CONFIG_HOME or HOME must be set")?;
    Ok(base.join("omarchy10k").join("config.toml"))
}

// ── Daemon & clipboard plumbing ─────────────────────────────────────────────

/// One newline-framed JSON request to the daemon socket; response trimmed.
pub(crate) async fn daemon_request(socket_path: &Path, request: &str) -> Result<String> {
    let fut = async {
        let stream =
            UnixStream::connect(socket_path).await.context("connect to daemon socket")?;
        let (reader, mut writer) = stream.into_split();
        writer.write_all(request.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        let mut reader = tokio::io::BufReader::new(reader);
        let mut response = String::new();
        reader.read_line(&mut response).await?;
        Ok(response.trim().to_string())
    };
    tokio::time::timeout(DAEMON_TIMEOUT, fut).await?
}

/// Copy text via wl-copy, falling back to xclip. Errors when neither works.
fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let tools: [(&str, Vec<&str>); 2] = [
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
    ];
    for (bin, args) in tools {
        let Ok(mut child) = Command::new(bin)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue; // not installed
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let status = child.wait().context("failed to wait for clipboard tool")?;
        if status.success() {
            return Ok(());
        }
    }
    bail!("no clipboard tool available (install wl-clipboard or xclip), or use --out FILE")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "o10k-cli-share-{}-{label}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_patch() -> toml::Table {
        toml::from_str(
            r#"
            [style]
            left_sep = "powerline"

            [glyphs]
            left_sep = ""

            [prompt]
            char = "❯"
            "#,
        )
        .unwrap()
    }

    fn assert_toml_eq(a: &toml::Table, b: &toml::Table) {
        assert_eq!(toml::Value::Table(a.clone()), toml::Value::Table(b.clone()));
    }

    #[test]
    fn bundle_round_trip_preserves_entry() {
        let patch = sample_patch();
        let bundle = build_bundle("nightfall", "Nightfall", Some("tokyo-night"), &patch, "0.4.0")
            .unwrap();
        assert!(bundle.starts_with("# omarchy10k Look bundle\n# name: nightfall\n"));
        assert!(bundle.contains("# version: 0.4.0\n"));
        assert!(bundle.contains("[looks.nightfall]"));

        let parsed = parse_bundle(&bundle).unwrap();
        assert_eq!(parsed.name, "nightfall");
        assert_eq!(parsed.label, "Nightfall");
        assert_eq!(parsed.palette.as_deref(), Some("tokyo-night"));
        assert_toml_eq(&parsed.patch, &patch);
    }

    #[test]
    fn parse_rejects_multi_table_bundle() {
        let text = r#"
            [looks.one]
            label = "One"
            [looks.one.patch]
            [looks.two]
            label = "Two"
            [looks.two.patch]
        "#;
        let err = parse_bundle(text).unwrap_err().to_string();
        assert!(err.contains("exactly one"), "got: {err}");
    }

    #[test]
    fn parse_rejects_extra_top_level_table() {
        let text = r#"
            [looks.solo]
            label = "Solo"
            [looks.solo.patch]
            [metadata]
            author = "someone"
        "#;
        let err = parse_bundle(text).unwrap_err().to_string();
        assert!(err.contains("exactly one top-level"), "got: {err}");
    }

    #[test]
    fn parse_rejects_bad_patch_shape() {
        let text = r#"
            [looks.flat]
            label = "Flat"
            patch = "not-a-table"
        "#;
        let err = parse_bundle(text).unwrap_err().to_string();
        assert!(err.contains("patch must be a table"), "got: {err}");
    }

    #[test]
    fn parse_rejects_missing_patch() {
        let text = r#"
            [looks.empty]
            label = "Empty"
        "#;
        let err = parse_bundle(text).unwrap_err().to_string();
        assert!(err.contains("missing a 'patch' table"), "got: {err}");
    }

    #[test]
    fn parse_rejects_unexpected_entry_key() {
        let text = r#"
            [looks.sneaky]
            label = "Sneaky"
            theme = "dracula"
            [looks.sneaky.patch]
        "#;
        let err = parse_bundle(text).unwrap_err().to_string();
        assert!(err.contains("unexpected key 'theme'"), "got: {err}");
    }

    #[tokio::test]
    async fn install_round_trips_via_local_fallback() {
        let dir = temp_dir("roundtrip");
        let config_path = dir.join("config.toml");
        let patch = sample_patch();
        let bundle = build_bundle("nightfall", "Nightfall", Some("tokyo-night"), &patch, "0.4.0")
            .unwrap();
        let bundle_path = dir.join("nightfall.toml");
        std::fs::write(&bundle_path, &bundle).unwrap();

        // Socket does not exist → local config write fallback.
        let outcome = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            bundle_path.to_str().unwrap(),
            true,
            false,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            InstallOutcome::Installed { name: "nightfall".into(), via_daemon: false }
        );

        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let entry = doc["looks"]["nightfall"].as_table().unwrap();
        assert_eq!(entry["label"].as_str().unwrap(), "Nightfall");
        assert_eq!(entry["palette"].as_str().unwrap(), "tokyo-night");
        assert_toml_eq(entry["patch"].as_table().unwrap(), &patch);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn install_refuses_http_url() {
        let dir = temp_dir("http");
        let err = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &dir.join("config.toml"),
            "http://example.com/look.toml",
            true,
            false,
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("https"), "got: {err}");
        assert!(!dir.join("config.toml").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn install_overwrite_requires_force() {
        let dir = temp_dir("overwrite");
        let config_path = dir.join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [looks.dupe]
            label = "Old"
            [looks.dupe.patch]
            frame = "round"
            style.left_sep = "old"
            "#,
        )
        .unwrap();

        let bundle = parse_bundle(
            r#"
            [looks.dupe]
            label = "New"
            [looks.dupe.patch]
            style.left_sep = "powerline"
            "#,
        )
        .unwrap();


        let bundle_path = dir.join("dupe.toml");
        std::fs::write(&bundle_path, toml_string(&bundle)).unwrap();
        let err = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            bundle_path.to_str().unwrap(),
            true,
            false,
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("--force"), "got: {err}");

        // With --force the local fallback REPLACES the entry exactly: the
        // stale `frame` key from the old look must not survive.
        let outcome = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            bundle_path.to_str().unwrap(),
            true,
            true,
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            InstallOutcome::Installed { name: "dupe".into(), via_daemon: false }
        );
        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let entry = doc["looks"]["dupe"].as_table().unwrap();
        assert_eq!(entry["label"].as_str().unwrap(), "New");
        assert!(entry["patch"].get("frame").is_none(), "stale key survived overwrite");
        assert_eq!(entry["patch"]["style"]["left_sep"].as_str().unwrap(), "powerline");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn install_dry_run_writes_nothing() {
        let dir = temp_dir("dryrun");
        let config_path = dir.join("config.toml");
        let bundle_path = dir.join("solo.toml");
        std::fs::write(
            &bundle_path,
            r#"
            [looks.solo]
            label = "Solo"
            [looks.solo.patch]
            style.left_sep = "powerline"
            "#,
        )
        .unwrap();

        let outcome = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            bundle_path.to_str().unwrap(),
            false,
            false,
            None,
        )
        .await
        .unwrap();
        let InstallOutcome::DryRun { name, label, preview } = outcome else {
            panic!("expected DryRun, got {outcome:?}");
        };
        assert_eq!(name, "solo");
        assert_eq!(label, "Solo");
        assert!(preview.contains("[looks.solo]"));
        assert!(preview.contains("left_sep"));
        assert!(!config_path.exists(), "dry run must not write config.toml");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn install_as_renames_look() {
        let dir = temp_dir("as");
        let config_path = dir.join("config.toml");
        let bundle_path = dir.join("original.toml");
        std::fs::write(
            &bundle_path,
            r#"
            [looks.original]
            label = "Original"
            [looks.original.patch]
            style.left_sep = "powerline"
            "#,
        )
        .unwrap();

        let outcome = install_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            bundle_path.to_str().unwrap(),
            true,
            false,
            Some("renamed"),
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            InstallOutcome::Installed { name: "renamed".into(), via_daemon: false }
        );
        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(doc["looks"].get("original").is_none());
        assert_eq!(doc["looks"]["renamed"]["label"].as_str().unwrap(), "Original");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn export_user_entry_is_verbatim() {
        let dir = temp_dir("export");
        let config_path = dir.join("config.toml");
        std::fs::write(
            &config_path,
            r#"
            [looks.mine]
            label = "My Look"
            palette = "gruvbox"
            [looks.mine.patch]
            glyphs.left_sep = ""
            style.left_sep = "powerline"
            "#,
        )
        .unwrap();

        let bundle = export_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &config_path,
            "mine",
        )
        .await
        .unwrap();
        assert!(bundle.contains("# name: mine\n"));
        assert!(bundle.contains("# label: My Look\n"));
        // Verbatim: the raw glyph shortcut survives, palette directive intact.
        assert!(bundle.contains("[looks.mine.patch.glyphs]"));
        assert!(bundle.contains("palette = \"gruvbox\""));

        // And the exported bundle installs back cleanly.
        let parsed = parse_bundle(&bundle).unwrap();
        assert_eq!(parsed.name, "mine");
        assert_eq!(parsed.palette.as_deref(), Some("gruvbox"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn export_unknown_look_needs_daemon() {
        let dir = temp_dir("export-missing");
        let err = export_in(
            Path::new("/nonexistent-o10k-share.sock"),
            &dir.join("config.toml"),
            "ghost",
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(err.contains("daemon"), "got: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn valid_names_are_bare_toml_safe() {
        assert!(valid_look_name("nightfall"));
        assert!(valid_look_name("tokyo-night"));
        assert!(!valid_look_name(""));
        assert!(!valid_look_name("../evil"));
        assert!(!valid_look_name("a/b"));
        assert!(!valid_look_name(".hidden"));
        assert!(!valid_look_name("has space"));
    }

    /// Helper: re-serialize a parsed bundle entry so tests can write it to a
    /// file (keeps the fixture text honest through the same code path).
    fn toml_string(bundle: &Bundle) -> String {
        let mut looks = toml::Table::new();
        looks.insert(bundle.name.clone(), toml::Value::Table(bundle.entry_table()));
        let mut doc = toml::Table::new();
        doc.insert("looks".into(), toml::Value::Table(looks));
        toml::to_string_pretty(&toml::Value::Table(doc)).unwrap()
    }
}
