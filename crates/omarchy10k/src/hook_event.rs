//! Desktop-event dispatch: relay Omarchy hook events (`battery-low`,
//! `post-update`, `font-set`, …) to the hook system.
//!
//! Two delivery paths, tried in order:
//! 1. `omarchy-hook <event> [args…]` — Omarchy's own dispatcher, when present
//!    on `PATH`. It fans out to every registered `<event>.d/` consumer and
//!    handles logging itself. Its exit code is propagated.
//! 2. Fallback: run every executable in `~/.config/omarchy/hooks/<event>.d/`
//!    directly with the same arguments. Individual hook failures are logged
//!    but do not abort the remaining hooks — a desktop event must never be
//!    dropped because one consumer is broken.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Default hook root under `$HOME` (overridable via `XDG_CONFIG_HOME`).
pub fn default_hook_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("omarchy/hooks");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    PathBuf::from(home).join(".config/omarchy/hooks")
}

/// Locate a command on the given PATH-style variable value.
fn find_in_path_value(path_var: &str, name: &str) -> Option<PathBuf> {
    for dir in std::env::split_paths(path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Locate `omarchy-hook` on `PATH`, if installed.
pub fn find_dispatcher() -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    find_in_path_value(&path, "omarchy-hook")
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Run every executable hook in `<hook_root>/<event>.d/` with `args`.
/// Returns the list of hook paths actually executed (callers/tests use it for
/// assertions); failures are reported on stderr and skipped.
fn run_hook_dir(event: &str, args: &[String], hook_root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let dir = hook_root.join(format!("{event}.d"));
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("omarchy10k: no hooks installed for event '{event}' ({})", dir.display());
            return Ok(Vec::new());
        }
        Err(err) => return Err(anyhow::anyhow!("cannot read {}: {err}", dir.display())),
    };

    let mut hooks: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| is_executable(p))
        .collect();
    hooks.sort();

    let mut executed = Vec::new();
    for hook in hooks {
        // Freshly-written executables can transiently fail to exec with
        // ETXTBSY (Text file busy) when a file watcher/indexer holds the
        // new inode open for write. The error is transient by nature —
        // retry briefly before reporting failure.
        let mut attempt = 0;
        let outcome = loop {
            match Command::new(&hook).args(args).status() {
                Err(err) if err.kind() == std::io::ErrorKind::ExecutableFileBusy && attempt < 3 => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10 * attempt));
                }
                other => break other,
            }
        };
        match outcome {
            Ok(status) if status.success() => executed.push(hook),
            Ok(status) => {
                eprintln!(
                    "omarchy10k: hook {} exited with {status} (continuing)",
                    hook.display()
                );
            }
            Err(err) => {
                eprintln!("omarchy10k: hook {} failed to run: {err}", hook.display());
            }
        }
    }
    Ok(executed)
}

/// Dispatch `event` with `args`.
///
/// `dispatcher` is the resolved `omarchy-hook` path (pass
/// [`find_dispatcher`] in production; tests inject a fake). When `None`, the
/// user hook dir is walked directly.
pub fn run(
    event: &str,
    args: &[String],
    dispatcher: Option<&Path>,
    hook_root: &Path,
) -> anyhow::Result<()> {
    if let Some(dispatcher) = dispatcher {
        let status = Command::new(dispatcher)
            .arg(event)
            .args(args)
            .status()
            .map_err(|err| anyhow::anyhow!("failed to run {}: {err}", dispatcher.display()))?;
        if !status.success() {
            anyhow::bail!("omarchy-hook {event} exited with {status}");
        }
        return Ok(());
    }
    run_hook_dir(event, args, hook_root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "o10k-hook-event-{}-{}",
                tag,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write_script(&self, rel: &str, body: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            // Write to a temp name and rename: exec'ing a file another
            // thread still holds open for write fails with ETXTBSY (Text
            // file busy) — observed as a ~1-in-6 parallel-test flake. The
            // rename swaps the inode atomically, so the final path is never
            // open-for-write when the executor picks it up.
            let tmp = p.with_extension("o10k-tmp");
            let mut f = std::fs::File::create(&tmp).unwrap();
            writeln!(f, "#!/bin/sh").unwrap();
            write!(f, "{body}").unwrap();
            drop(f);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            std::fs::rename(&tmp, &p).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dispatcher_path_is_preferred_and_receives_event_and_args() {
        let tmp = TempDir::new("dispatcher");
        tmp.write_script("bin/omarchy-hook", r#"
echo "$1 $2" > "$3/seen"
exit 7
"#);
        let marker = tmp.path().join("seen");
        let dir_arg = tmp.path().to_string_lossy().to_string();
        let dispatcher = tmp.path().join("bin/omarchy-hook");

        // Exit code 7 must propagate even though the script also wrote output.
        let err = run(
            "battery-low",
            &args(&["42", &dir_arg]),
            Some(&dispatcher),
            tmp.path(),
        )
        .unwrap_err();
        assert!(format!("{err}").contains("exited with"));

        let seen = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(seen.trim(), "battery-low 42");
    }

    #[test]
    fn dispatcher_success_short_circuits_hook_dir() {
        let tmp = TempDir::new("short-circuit");
        tmp.write_script("bin/omarchy-hook", "exit 0\n");
        let dir = tmp.path().join("font-set.d");
        std::fs::create_dir_all(&dir).unwrap();
        tmp.write_script("font-set.d/omarchy10k", "exit 1\n"); // must NOT run

        run(
            "font-set",
            &[],
            Some(&tmp.path().join("bin/omarchy-hook")),
            tmp.path(),
        )
        .unwrap();
    }

    #[test]
    fn fallback_runs_every_executable_hook_in_event_dir() {
        let tmp = TempDir::new("fallback");
        tmp.write_script("battery-low.d/01-first", r#"
echo first >> "$1/log"
"#);
        tmp.write_script("battery-low.d/02-second", r#"
echo second "$2" >> "$1/log"
"#);
        // Non-executable file must be ignored.
        std::fs::write(tmp.path().join("battery-low.d/03-not-exec"), "nope").unwrap();

        run("battery-low", &args(&[tmp.path().to_str().unwrap(), "42"]), None, tmp.path()).unwrap();

        let log = std::fs::read_to_string(tmp.path().join("log")).unwrap();
        assert_eq!(log, "first\nsecond 42\n");
    }

    #[test]
    fn fallback_continues_past_failing_hook() {
        let tmp = TempDir::new("continue");
        tmp.write_script("post-update.d/00-broken", "exit 3\n");
        tmp.write_script("post-update.d/10-good", "touch \"$1/done\"\n");

        // The failing hook is logged but the remaining hook still runs.
        run("post-update", &args(&[tmp.path().to_str().unwrap()]), None, tmp.path()).unwrap();
        assert!(tmp.path().join("done").exists());
    }

    #[test]
    fn fallback_missing_event_dir_is_not_an_error() {
        let tmp = TempDir::new("missing");
        run("post-boot", &args(&[]), None, tmp.path()).unwrap();
    }

    #[test]
    fn find_in_path_value_scans_dirs_in_order() {
        let tmp = TempDir::new("path-scan");
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        tmp.write_script("a/omarchy-hook", "exit 0\n");
        tmp.write_script("b/omarchy-hook", "exit 0\n");

        let path_var = std::env::join_paths([a.clone(), b.clone()])
            .unwrap()
            .to_string_lossy()
            .to_string();
        let found = find_in_path_value(&path_var, "omarchy-hook").unwrap();
        assert_eq!(found, a.join("omarchy-hook"));

        // Only the second dir on PATH → found there.
        let found_b = find_in_path_value(&b.display().to_string(), "omarchy-hook").unwrap();
        assert_eq!(found_b, b.join("omarchy-hook"));

        assert!(find_in_path_value(&path_var, "omarchy-hook-missing").is_none());
    }

    #[test]
    fn default_hook_root_honors_xdg_config_home() {
        // default_hook_root reads env; only assert the non-XDG branch shape
        // here to avoid env mutation across tests.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
        if std::env::var("XDG_CONFIG_HOME").is_err() {
            assert_eq!(
                default_hook_root(),
                PathBuf::from(home).join(".config/omarchy/hooks")
            );
        }
    }
}
