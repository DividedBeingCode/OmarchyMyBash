use std::path::{Path, PathBuf};
use std::process::Command;

const C_GREEN: &str = "\x1b[1;32m";
const C_RED: &str = "\x1b[1;31m";
const C_BLUE: &str = "\x1b[1;34m";
const C_YELLOW: &str = "\x1b[1;33m";
const C_BOLD: &str = "\x1b[1m";
const C_RESET: &str = "\x1b[0m";

fn info(msg: &str) {
    eprintln!("{C_BLUE}[omarchy10k]{C_RESET} {msg}");
}

fn ok(msg: &str) {
    eprintln!("{C_GREEN}      \u{2713}{C_RESET} {msg}");
}

fn warn(msg: &str) {
    eprintln!("{C_YELLOW}      \u{26a0}{C_RESET} {msg}");
}

fn fail(msg: &str) -> anyhow::Error {
    eprintln!("{C_RED}      \u{2718}{C_RESET} {msg}");
    anyhow::anyhow!("{msg}")
}

pub fn find_source_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("O10K_SOURCE_DIR") {
        let p = PathBuf::from(&dir);
        if p.join("Cargo.toml").exists() {
            return Ok(p);
        }
        return Err(fail(&format!(
            "O10K_SOURCE_DIR={dir} does not contain Cargo.toml"
        )));
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.as_path();
        for _ in 0..8 {
            if let Some(parent) = dir.parent() {
                dir = parent;
                if is_o10k_workspace(dir) {
                    return Ok(dir.to_path_buf());
                }
            }
        }
    }

    let breadcrumb = dirs_data().join("source-dir");
    if breadcrumb.exists() {
        if let Ok(contents) = std::fs::read_to_string(&breadcrumb) {
            let p = PathBuf::from(contents.trim());
            if is_o10k_workspace(&p) {
                return Ok(p);
            }
            warn("Breadcrumb source-dir points to invalid location; ignoring");
        }
    }

    Err(fail(
        "Cannot locate Omarchy10k source tree.\n\
         Set O10K_SOURCE_DIR or re-run install.sh to record the path.",
    ))
}

fn is_o10k_workspace(dir: &Path) -> bool {
    let cargo = dir.join("Cargo.toml");
    if !cargo.exists() {
        return false;
    }
    std::fs::read_to_string(cargo)
        .map(|s| s.contains("omarchy10kd"))
        .unwrap_or(false)
}

fn dirs_data() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
                .join(".local/share")
        });
    base.join("omarchy10k")
}

fn bin_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".local/bin")
}

pub fn installed_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn source_version(source_dir: &Path) -> Option<String> {
    let cargo = source_dir.join("Cargo.toml");
    let contents = std::fs::read_to_string(cargo).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version") && trimmed.contains('=') {
            let val = trimmed.split('=').nth(1)?.trim().trim_matches('"');
            return Some(val.to_string());
        }
    }
    None
}

fn git_pull(source_dir: &Path) -> anyhow::Result<()> {
    info("Pulling latest changes...");

    if !source_dir.join(".git").exists() {
        let parent = source_dir.parent().unwrap_or(source_dir);
        if !parent.join(".git").exists() {
            warn("Not a git repository; skipping pull");
            return Ok(());
        }
    }

    let git_dir = if source_dir.join(".git").exists() {
        source_dir
    } else {
        source_dir.parent().unwrap_or(source_dir)
    };

    let status = Command::new("git")
        .args(["-C", &git_dir.to_string_lossy(), "diff", "--quiet"])
        .status();

    if let Ok(s) = status {
        if !s.success() {
            warn("Working tree has local changes; skipping pull (use --no-pull to silence)");
            return Ok(());
        }
    }

    let result = Command::new("git")
        .args(["-C", &git_dir.to_string_lossy(), "pull", "--ff-only"])
        .status()?;

    if result.success() {
        ok("Pull complete");
    } else {
        warn("git pull failed (non-fast-forward?); continuing with current source");
    }

    Ok(())
}

fn cargo_build(source_dir: &Path) -> anyhow::Result<()> {
    info("Building from source...");

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(source_dir)
        .status()?;

    if !status.success() {
        return Err(fail("cargo build --release failed"));
    }

    ok("Build complete");
    Ok(())
}

fn install_binaries(source_dir: &Path) -> anyhow::Result<()> {
    info("Installing binaries...");
    let dest = bin_dir();
    std::fs::create_dir_all(&dest)?;

    for bin_name in &["omarchy10k", "omarchy10kd"] {
        let src = source_dir
            .join("target/release")
            .join(bin_name);
        if !src.exists() {
            return Err(fail(&format!("{bin_name} not found at {}", src.display())));
        }

        let dst = dest.join(bin_name);
        let tmp = dest.join(format!(".{bin_name}.tmp"));

        std::fs::copy(&src, &tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&tmp, &dst)?;
        ok(bin_name);
    }

    Ok(())
}

fn install_plugin(source_dir: &Path) -> anyhow::Result<()> {
    let plugin_src = source_dir.join("quattro");
    if !plugin_src.exists() {
        warn("Quattro plugin directory not found; skipping");
        return Ok(());
    }

    info("Refreshing Quattro plugin...");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let plugin_dir =
        PathBuf::from(&home).join(".config/omarchy/plugins/community.omarchy10k");
    std::fs::create_dir_all(&plugin_dir)?;

    copy_dir_contents(&plugin_src, &plugin_dir)?;

    if let Some(version) = source_version(source_dir) {
        let manifest = plugin_dir.join("manifest.json");
        if manifest.exists() {
            if let Ok(contents) = std::fs::read_to_string(&manifest) {
                let patched = patch_manifest_version(&contents, &version);
                let _ = std::fs::write(&manifest, patched);
            }
        }
    }

    ok("Quattro plugin updated");
    Ok(())
}

fn patch_manifest_version(manifest: &str, version: &str) -> String {
    let mut result = String::with_capacity(manifest.len());
    let mut patched = false;
    for line in manifest.lines() {
        let trimmed = line.trim_start();
        // Patch only the FIRST "version" key; nested "version" keys (which
        // appear later, deeper in the document) keep their own values.
        if !patched && trimmed.starts_with("\"version\"") {
            // Preserve the original line's trailing-comma-ness
            let comma = if trimmed.trim_end().ends_with(',') { "," } else { "" };
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            result.push_str(&format!("{indent}\"version\": \"{version}\"{comma}\n"));
            patched = true;
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn install_hook(source_dir: &Path) -> anyhow::Result<()> {
    let hook_src = source_dir.join("hooks/theme-set");
    if !hook_src.exists() {
        warn("Theme hook not found; skipping");
        return Ok(());
    }

    info("Refreshing theme hook...");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let hook_dir = PathBuf::from(&home).join(".config/omarchy/hooks/theme-set.d");
    std::fs::create_dir_all(&hook_dir)?;

    let dst = hook_dir.join("omarchy10k");
    std::fs::copy(&hook_src, &dst)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))?;
    }
    ok("Theme hook updated");
    Ok(())
}

/// Make the running Quattro shell pick up the plugin we just installed.
///
/// `rescanPlugins` re-reads the plugin LIST but does NOT invalidate QML's
/// component cache, so changed `.qml` files keep serving their previous code
/// — an update would appear to succeed and change nothing until something
/// else restarted the shell. Restart when the command exists; otherwise
/// rescan and say plainly that a restart is still required.
fn reload_shell_plugins() {
    let restarted = Command::new("omarchy")
        .args(["restart", "shell"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if restarted {
        ok("Quattro shell restarted (picks up changed QML)");
        return;
    }

    if let Ok(status) = Command::new("omarchy-shell")
        .args(["shell", "rescanPlugins"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        if status.success() {
            warn("Plugin rescanned — run `omarchy restart shell` to load changed QML");
        }
    }
}

fn install_template(source_dir: &Path) -> anyhow::Result<()> {
    let tpl_src = source_dir.join("templates/omarchy10k.toml.tpl");
    if !tpl_src.exists() {
        warn("Theme bridge template not found; skipping");
        return Ok(());
    }

    info("Installing theme bridge template...");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let tpl_dir = PathBuf::from(&home).join(".local/share/omarchy/templates");
    std::fs::create_dir_all(&tpl_dir)?;

    let dst = tpl_dir.join("omarchy10k.toml.tpl");
    std::fs::copy(&tpl_src, &dst)?;
    ok("Theme bridge template updated");
    Ok(())
}

fn copy_dir_contents(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn restart_daemons() {
    info("Restarting running daemons...");

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let runtime_path = PathBuf::from(&runtime_dir);

    let sockets: Vec<PathBuf> = std::fs::read_dir(&runtime_path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("omarchy10k-") && n.ends_with(".sock"))
        })
        .collect();

    if sockets.is_empty() {
        ok("No running daemons found");
        return;
    }

    let mut stopped = 0u32;
    for sock in &sockets {
        let shutdown_cmd = r#"{"command":"shutdown"}"#;
        if let Ok(stream) = std::os::unix::net::UnixStream::connect(sock) {
            use std::io::Write;
            let mut stream = stream;
            let msg = format!("{shutdown_cmd}\n");
            let _ = stream.write_all(msg.as_bytes());
            let _ = stream.flush();
            stopped += 1;
        } else {
            let _ = std::fs::remove_file(sock);
        }
    }

    ok(&format!(
        "{stopped} daemon(s) stopped; they will auto-restart on next prompt"
    ));
}

fn write_breadcrumb(source_dir: &Path) {
    let data_dir = dirs_data();
    if std::fs::create_dir_all(&data_dir).is_ok() {
        let _ = std::fs::write(
            data_dir.join("source-dir"),
            source_dir.to_string_lossy().as_bytes(),
        );
    }
}

pub fn run(no_pull: bool, no_build: bool) -> anyhow::Result<()> {
    eprintln!("\n{C_BOLD}  OMARCHY10K UPDATE{C_RESET}\n");

    let source_dir = find_source_dir()?;
    info(&format!("Source: {}", source_dir.display()));

    let old_version = installed_version();
    let pre_version = source_version(&source_dir).unwrap_or_else(|| "unknown".into());
    info(&format!("Installed: v{old_version}  Source: v{pre_version}"));

    if !no_pull {
        git_pull(&source_dir)?;
    } else {
        info("Skipping git pull (--no-pull)");
    }

    if !no_build {
        cargo_build(&source_dir)?;
    } else {
        info("Skipping build (--no-build)");
    }

    install_binaries(&source_dir)?;
    install_plugin(&source_dir)?;
    reload_shell_plugins();
    install_hook(&source_dir)?;
    install_template(&source_dir)?;
    write_breadcrumb(&source_dir);
    restart_daemons();

    let new_version =
        source_version(&source_dir).unwrap_or_else(|| "unknown".into());

    eprintln!();
    eprintln!(
        "{C_GREEN}{C_BOLD}  Update complete!{C_RESET}  v{old_version} \u{2192} v{new_version}"
    );
    eprintln!();
    eprintln!("  New terminals will use the updated prompt automatically.");
    eprintln!("  Running terminals will restart their daemon on next command.");
    eprintln!();

    Ok(())
}
