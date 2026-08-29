//! Daemon-side user-script registry and runner (wave-2: `omarchy10k script`).
//!
//! User scripts live in `~/.config/omarchy10k/scripts/<name>.sh` and must be
//! executable. The daemon exposes two control verbs over this module:
//! `script_list` (registry listing) and `script_run {name}` (hard-timeout
//! execution with output capture). Commands come only from the user's own
//! config directory — same trust level as `.bashrc`; nothing network-sourced.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Serialize;

/// Default execution budget for a user script.
pub const DEFAULT_SCRIPT_TIMEOUT_SECS: u64 = 30;

/// A runnable user script discovered in the scripts directory.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScriptInfo {
    pub name: String,
    pub path: PathBuf,
}

/// Default user-scripts directory: `$XDG_CONFIG_HOME/omarchy10k/scripts`.
pub fn scripts_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("omarchy10k").join("scripts"))
        .unwrap_or_else(|| PathBuf::from("/tmp/omarchy10k/scripts"))
}

/// Registry validation: regular file, executable bit, no traversal.
/// A valid name is non-empty, contains no `/`, no `..` substring, and does
/// not start with `.`.
fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains("..") && !name.starts_with('.')
}

/// Resolve a user-supplied name to a script path inside `dir`, rejecting
/// traversal and missing/non-executable/non-regular entries.
pub fn resolve_script(dir: &Path, name: &str) -> Result<PathBuf> {
    if !valid_name(name) {
        bail!("invalid script name: {name:?}");
    }
    let path = dir.join(name);
    let meta = std::fs::metadata(&path).with_context(|| format!("script not found: {name}"))?;
    if !meta.is_file() {
        bail!("not a regular file: {name}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            bail!("script is not executable: {name}");
        }
    }
    Ok(path)
}

/// List valid, executable scripts in `dir`. Missing dir → empty list.
pub fn list_scripts(dir: &Path) -> Vec<ScriptInfo> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_name(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o111 == 0 {
                continue;
            }
        }
        out.push(ScriptInfo {
            name,
            path: entry.path(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Execute a script with output capture and a hard timeout. On timeout the
/// child is killed (`kill_on_drop`); on non-zero exit the error carries
/// stderr. Returns trimmed stdout on success.
pub async fn run_script(path: &Path, timeout_secs: u64) -> Result<String> {
    if !path.exists() {
        // Distinguish from spawn failure for clearer daemon errors.
        bail!("script not found: {}", path.display());
    }
    // A script the user just dropped into the scripts directory can still be
    // held open for write by the writing process, and exec'ing it then fails
    // with ETXTBSY ("Text file busy"). That is transient by nature and is
    // exactly the run-it-right-after-saving case, so retry briefly rather
    // than reporting a spawn failure. `hook_event.rs` does the same for
    // freshly-installed hooks.
    let mut attempt = 0u32;
    let mut child = loop {
        let spawned = tokio::process::Command::new(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 5 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(10 * attempt as u64)).await;
            }
            other => {
                break other
                    .with_context(|| format!("failed to spawn {}", path.display()))?
            }
        }
    };

    let output = match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await
    {
        Ok(result) => result.context("failed to wait for script")?,
        Err(_) => bail!(
            "script timed out after {timeout_secs}s and was killed: {}",
            path.display()
        ),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output.status.code().unwrap_or(-1);
        bail!("script exited with status {code}: {stderr}");
    }
    Ok(stdout)
}

/// Handle the `script_list` / `script_run` control verbs, returning the
/// JSON response value for the server to write. Shared shape with the
/// daemon's other control responses (`{"type":"control", ...}`).
pub async fn handle_script_control(command: &str, rest: &serde_json::Value) -> serde_json::Value {
    match command {
        "script_list" => {
            let dir = scripts_dir();
            let scripts = list_scripts(&dir);
            serde_json::json!({
                "type": "control", "status": "ok",
                "dir": dir.display().to_string(),
                "scripts": scripts,
            })
        }
        "script_run" => {
            let name = rest
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timeout_secs = rest
                .get("timeout_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_SCRIPT_TIMEOUT_SECS);
            let dir = scripts_dir();
            match resolve_script(&dir, &name) {
                Ok(path) => match run_script(&path, timeout_secs).await {
                    Ok(output) => serde_json::json!({
                        "type": "control", "status": "ok",
                        "name": name, "output": output,
                    }),
                    Err(e) => serde_json::json!({
                        "type": "control", "status": "error", "error": e.to_string(),
                    }),
                },
                Err(e) => serde_json::json!({
                    "type": "control", "status": "error", "error": e.to_string(),
                }),
            }
        }
        _ => serde_json::json!({
            "type": "control", "status": "error",
            "error": format!("unknown script command: {command}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "o10k-script-{}-{}-{label}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_script(dir: &Path, name: &str, body: &str, exec: bool) -> PathBuf {
        let path = dir.join(name);
        // Write to a temp name, chmod, then rename into place.
        //
        // Exec'ing a file that was just written can fail with ETXTBSY ("Text
        // file busy") while the writing handle is still being torn down —
        // observed here as a ~1-in-3 flake ("failed to spawn ... slow.sh").
        // The rename swaps in a complete inode, so the final path is never
        // open for write when the runner spawns it. Same fix already applied
        // to the hook_event tests.
        let tmp = path.with_extension("o10k-tmp");
        std::fs::write(&tmp, body).unwrap();
        let mode = if exec { 0o755 } else { 0o644 };
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        path
    }

    #[test]
    fn rejects_traversal_and_hidden_names() {
        assert!(!valid_name("../etc/passwd"));
        assert!(!valid_name("foo/bar"));
        assert!(!valid_name("foo/../bar"));
        assert!(!valid_name(".hidden"));
        assert!(!valid_name(""));
        assert!(valid_name("update.sh"));
    }

    #[test]
    fn list_scripts_filters_and_sorts() {
        let dir = temp_dir("list");
        make_script(&dir, "b.sh", "#!/bin/sh\n", true);
        make_script(&dir, "a.sh", "#!/bin/sh\n", true);
        make_script(&dir, "noexec.sh", "#!/bin/sh\n", false);
        make_script(&dir, ".hidden.sh", "#!/bin/sh\n", true);
        std::fs::create_dir_all(dir.join("subdir")).unwrap();

        let scripts = list_scripts(&dir);
        let names: Vec<&str> = scripts.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["a.sh", "b.sh"]);

        let missing = list_scripts(&dir.join("does-not-exist"));
        assert!(missing.is_empty());

        // Nested paths never escape via name validation.
        assert!(resolve_script(&dir, "../Cargo.toml").is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_rejects_non_executable() {
        let dir = temp_dir("noexec");
        make_script(&dir, "quiet.sh", "#!/bin/sh\n", false);
        let err = resolve_script(&dir, "quiet.sh").unwrap_err().to_string();
        assert!(err.contains("not executable"), "got: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn run_captures_output() {
        let dir = temp_dir("capture");
        let path = make_script(&dir, "hello.sh", "#!/bin/sh\necho hello-o10k\n", true);
        let out = run_script(&path, 5).await.unwrap();
        assert_eq!(out, "hello-o10k");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn run_reports_nonzero_exit_with_stderr() {
        let dir = temp_dir("fail");
        let path = make_script(&dir, "fail.sh", "#!/bin/sh\necho boom >&2\nexit 3\n", true);
        let err = run_script(&path, 5).await.unwrap_err().to_string();
        assert!(err.contains("status 3") && err.contains("boom"), "got: {err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn timeout_kills_script() {
        let dir = temp_dir("timeout");
        let path = make_script(
            &dir,
            "slow.sh",
            "#!/bin/sh\nsleep 30\n",
            true,
        );
        let err = run_script(&path, 1).await.unwrap_err().to_string();
        assert!(err.contains("timed out"), "got: {err}");
        // Give the kernel a beat, then confirm no stray sleep from our child.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut stray = false;
        let mut read = std::fs::read_dir("/proc").unwrap();
        while let Some(entry) = read.next() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let ok = std::fs::read_to_string(entry.path().join("cmdline"));
            if let Ok(cmd) = ok {
                if cmd.contains("sleep 30") {
                    stray = true;
                }
            }
        }
        assert!(!stray, "timed-out script was not killed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}