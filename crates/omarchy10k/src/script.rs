//! `omarchy10k script` — list and run user-defined quick actions (wave-2 item 4).
//!
//! `script list` reads the scripts directory directly and prints
//! machine-readable JSON. `script run <name>` sends a `script_run` control
//! verb to the daemon socket; if no daemon is reachable it falls back to
//! executing the script locally. Scripts come only from the user's own
//! config directory — same trust level as `.bashrc`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Connect/response budget for the daemon round-trip. Generous: the daemon
/// itself enforces a 30s execution timeout on the script.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(60);

/// Default local fallback execution budget.
const LOCAL_TIMEOUT: u64 = 30;

#[derive(Debug, Serialize)]
struct ScriptEntry {
    name: String,
    path: String,
}

/// `$XDG_CONFIG_HOME/omarchy10k/scripts` (matches the daemon's directory).
pub fn scripts_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("omarchy10k").join("scripts"))
        .unwrap_or_else(|| PathBuf::from("/tmp/omarchy10k/scripts"))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains("..") && !name.starts_with('.')
}

/// List executable scripts in `dir` (same validation as the daemon side).
fn local_list(dir: &Path) -> Vec<ScriptEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<ScriptEntry> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !valid_name(&name) {
                return false;
            }
            let Ok(meta) = e.metadata() else { return false };
            if !meta.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 == 0 {
                    return false;
                }
            }
            true
        })
        .map(|e| ScriptEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().display().to_string(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub async fn run(socket_path: &Path, action: &str, name: Option<&str>) -> Result<()> {
    run_in(socket_path, &scripts_dir(), action, name).await
}

pub async fn run_in(
    socket_path: &Path,
    dir: &Path,
    action: &str,
    name: Option<&str>,
) -> Result<()> {
    match action {
        "list" => {
            let scripts = local_list(dir);
            let payload = serde_json::json!({
                "dir": dir.display().to_string(),
                "scripts": scripts,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            Ok(())
        }
        "run" => {
            let Some(name) = name else {
                bail!("`omarchy10k script run` requires a script name");
            };
            if !valid_name(name) {
                bail!("invalid script name: {name:?}");
            }
            let request = serde_json::json!({ "command": "script_run", "name": name });
            match send_request(socket_path, &request.to_string()).await {
                Ok(response) => {
                    let value: serde_json::Value = serde_json::from_str(&response)
                        .context("daemon returned invalid JSON")?;
                    if value.get("status").and_then(|s| s.as_str()) == Some("ok") {
                        if let Some(output) = value.get("output").and_then(|o| o.as_str()) {
                            if !output.is_empty() {
                                println!("{output}");
                            }
                        }
                        Ok(())
                    } else {
                        let err = value
                            .get("error")
                            .and_then(|e| e.as_str())
                            .unwrap_or("unknown daemon error");
                        bail!("daemon: {err}");
                    }
                }
                Err(daemon_err) => {
                    eprintln!("omarchy10k: daemon unreachable ({daemon_err}); running locally");
                    run_local(dir, name, LOCAL_TIMEOUT).await
                }
            }
        }
        _ => bail!(
            "unknown script action '{action}' (expected `list` or `run <name>`)"
        ),
    }
}

/// Direct local execution fallback (no daemon): same hard-timeout model.
async fn run_local(dir: &Path, name: &str, timeout_secs: u64) -> Result<()> {
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

    let mut child = tokio::process::Command::new(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to spawn {}", path.display()))?;
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("script timed out after {timeout_secs}s and was killed"))?
    .context("failed to wait for script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let code = output.status.code().unwrap_or(-1);
        bail!("script exited with status {code}: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        println!("{stdout}");
    }
    Ok(())
}

async fn send_request(socket_path: &Path, request: &str) -> Result<String> {
    let fut = async {
        let stream = UnixStream::connect(socket_path)
            .await
            .context("connect to daemon socket")?;
        let (reader, mut writer) = stream.into_split();
        writer.write_all(request.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        let mut reader = BufReader::new(reader);
        let mut response = String::new();
        reader.read_line(&mut response).await?;
        Ok(response.trim().to_string())
    };
    tokio::time::timeout(DAEMON_TIMEOUT, fut).await?
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
            "o10k-cli-script-{}-{}-{label}-{n}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn local_list_validates_and_sorts() {
        let dir = temp_dir("list");
        for (name, exec) in [
            ("b.sh", true),
            ("a.sh", true),
            ("noexec.sh", false),
            (".hidden.sh", true),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(
                &path,
                std::fs::Permissions::from_mode(if exec { 0o755 } else { 0o644 }),
            )
            .unwrap();
        }
        std::fs::create_dir_all(dir.join("subdir")).unwrap();

        let scripts = local_list(&dir);
        let names: Vec<String> = scripts.iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["a.sh", "b.sh"]);
        assert_eq!(scripts[0].path, dir.join("a.sh").display().to_string());

        assert!(local_list(&dir.join("missing")).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_traversal_names() {
        assert!(!valid_name("../x"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(".x"));
        assert!(valid_name("update.sh"));
    }

    #[test]
    fn run_rejects_missing_name() {
        // Deterministic validation error before any socket work.
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(Path::new("/nonexistent.sock"), "run", None));
        assert!(err.is_err());
        let err = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(Path::new("/nonexistent.sock"), "list", None));
        assert!(err.is_ok());
    }

    #[tokio::test]
    async fn run_falls_back_to_local_exec() {
        let dir = temp_dir("fallback");
        let path = dir.join("hello.sh");
        std::fs::write(&path, "#!/bin/sh\necho local-fallback\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Daemon socket path does not exist → local exec fallback.
        run_in(
            Path::new("/nonexistent-o10k-test.sock"),
            &dir,
            "run",
            Some("hello.sh"),
        )
        .await
        .unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
    }
}