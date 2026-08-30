mod config;
mod git;
mod looks;
mod palette_derive;
mod plugins;
mod profiles;
mod layout;
mod render;
mod segments;
mod script_exec;
mod server;
mod style;
mod theme;
mod terminal;

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use server::DaemonState;
use theme::ThemePalette;

fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".into());
    // Headless instances (Control Center with no terminal open) bind a fixed
    // name: idempotent spawns (a live daemon refuses hijack, a stale socket
    // is cleared), and discovery treats the non-numeric pid as always alive.
    if let Ok(name) = std::env::var("O10K_SOCK_NAME") {
        let safe: String = name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if !safe.is_empty() {
            return PathBuf::from(runtime_dir).join(format!("omarchy10k-{safe}.sock"));
        }
    }
    let ppid = std::env::var("O10K_PARENT_PID")
        .unwrap_or_else(|_| std::process::id().to_string());
    PathBuf::from(runtime_dir).join(format!("omarchy10k-{ppid}.sock"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config
    let config_path = Config::config_path();
    // Load config first: the log level comes from it. A load failure is
    // logged after tracing init so it reaches the daemon log.
    let (config, config_error) = match Config::load(&config_path) {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(e)),
    };

    // Init tracing
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.daemon.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();

    if let Some(e) = &config_error {
        error!("failed to load config {}: {e}; using defaults", config_path.display());
    }

    info!(
        "omarchy10kd v{} starting (pid {})",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    );

    // Load theme palette (unified resolution for startup and reload)
    let palette = ThemePalette::resolve_palette(&config);

    let sock_path = socket_path();
    let state = Arc::new(DaemonState::new(
        config,
        palette,
        config_path.clone(),
        sock_path.clone(),
    ));

    // Start filesystem watchers in background
    let state_watcher = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = run_watchers(state_watcher, &config_path).await {
            warn!("watcher error: {e}");
        }
    });

    // Kernel-enforced parent death: SIGTERM the instant the parent shell
    // dies. Closes the PID-recycling race that the kill(ppid, 0) poll in
    // monitor_parent cannot close (a recycled PID keeps that poll alive
    // indefinitely). The poll remains as a fallback for non-Linux builds
    // and for a parent that died before this line ran.
    #[cfg(target_os = "linux")]
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
    }

    // Monitor parent process (clean exit when shell dies)
    let sock_for_cleanup = sock_path.clone();
    tokio::spawn(async move {
        monitor_parent().await;
        info!("parent process exited, shutting down");
        server::remove_socket_file(&sock_for_cleanup);
        std::process::exit(0);
    });

    // Signal-driven socket cleanup. Closing the terminal delivers SIGHUP and
    // service stops deliver SIGTERM — both default-terminate the daemon and
    // skip the parent-watch cleanup, leaving a stale socket behind that every
    // client then lists as a live session.
    let sock_for_signals = sock_path.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sighup = signal(SignalKind::hangup()).expect("install SIGHUP handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sighup.recv() => {},
            _ = sigint.recv() => {},
        }
        info!("signal received, removing socket and shutting down");
        server::remove_socket_file(&sock_for_signals);
        std::process::exit(0);
    });

    // Run the server
    server::run_server(&sock_path, state).await
}

async fn monitor_parent() {
    let ppid_str = std::env::var("O10K_PARENT_PID").unwrap_or_default();
    let ppid: u32 = ppid_str.parse().unwrap_or(0);
    if ppid == 0 {
        // No parent PID tracking -- sleep forever so daemon stays alive
        std::future::pending::<()>().await;
        return;
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let ret = unsafe { libc::kill(ppid as i32, 0) };
        if ret == 0 {
            continue;
        }
        // EPERM means process exists but we can't signal it
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
            return;
        }
    }
}

/// Minimum gap between filesystem-triggered reloads.
const RELOAD_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether an inotify event represents an actual content change.
///
/// Access and metadata-only events must NOT trigger a reload: `reload_config`
/// reads the very directories being watched, so treating a read as a change
/// makes the watcher feed itself forever.
fn is_content_change(kind: &notify::EventKind) -> bool {
    use notify::event::{EventKind, ModifyKind};
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        // Metadata changes (atime in particular) are exactly the feedback
        // source; only data and name changes count.
        EventKind::Modify(ModifyKind::Data(_)) | EventKind::Modify(ModifyKind::Name(_)) => true,
        EventKind::Modify(ModifyKind::Any) => true,
        _ => false,
    }
}

async fn run_watchers(
    state: Arc<DaemonState>,
    config_path: &std::path::Path,
) -> anyhow::Result<()> {
    use notify::Watcher;
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;

    // Watch config file
    if let Some(config_dir) = config_path.parent() {
        if config_dir.exists() {
            watcher.watch(config_dir, notify::RecursiveMode::NonRecursive)?;
            info!("watching config dir: {}", config_dir.display());
        }
        // Plugin registry: watch plugins/ so drop-in add/update/remove and
        // manifest edits trigger a registry reload like a config change.
        //
        // Create it if absent rather than skipping the watch. `plugin add`
        // creates the directory on first use, and a watch can only be
        // registered on a path that exists — so skipping here left a daemon
        // that started before the first install with no plugins watch for
        // the rest of its life (the non-recursive config-dir watch sees the
        // directory appear but cannot arm a watch on it). An empty
        // directory inside the daemon's own config dir is harmless.
        let plugins_dir = plugins::plugins_dir_for(config_dir);
        if let Err(e) = std::fs::create_dir_all(&plugins_dir) {
            warn!("cannot create plugins dir {}: {e}", plugins_dir.display());
        }
        if plugins_dir.exists() {
            watcher.watch(&plugins_dir, notify::RecursiveMode::Recursive)?;
            info!("watching plugins dir: {}", plugins_dir.display());
        }
    }

    // Watch Omarchy theme
    let theme_dir = ThemePalette::omarchy_theme_dir();
    if theme_dir.exists() {
        watcher.watch(&theme_dir, notify::RecursiveMode::NonRecursive)?;
        info!("watching theme dir: {}", theme_dir.display());
    }

    // Process events in a blocking thread
    let config_path = config_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let _watcher = watcher; // Keep alive
        // Coalesce bursts: an editor writing a file emits several events, and
        // reload_config re-reads config.toml AND re-scans the plugins dir —
        // work far too expensive to repeat per raw inotify event.
        let mut last_reload = std::time::Instant::now()
            .checked_sub(RELOAD_DEBOUNCE)
            .unwrap_or_else(std::time::Instant::now);
        loop {
            match rx.recv() {
                Ok(event) => {
                    // Only real content changes may trigger a reload.
                    //
                    // Without this filter EVERY inotify event counted —
                    // including IN_ACCESS/IN_OPEN. Since reload_config()
                    // itself READS the watched plugins directory, each
                    // reload generated fresh access events, which triggered
                    // another reload: a self-sustaining loop that pinned a
                    // core and allocated a TOML parse per iteration.
                    if !is_content_change(&event.kind) {
                        continue;
                    }
                    let is_config = event.paths.iter().any(|p| {
                        p.file_name()
                            .is_some_and(|n| n == "config.toml")
                            && p.parent().is_some_and(|parent| {
                                config_path.parent().is_some_and(|cp| cp == parent)
                            })
                    });
                    let is_theme = event.paths.iter().any(|p| {
                        p.file_name()
                            .is_some_and(|n| n == "colors.toml")
                    });
                    // Any event under the plugins dir rebuilds the registry.
                    let plugins_root = plugins::plugins_dir_for(
                        config_path.parent().unwrap_or_else(|| std::path::Path::new(".")),
                    );
                    let is_plugin = event
                        .paths
                        .iter()
                        .any(|p| p.starts_with(&plugins_root));

                    if (is_config || is_plugin || is_theme)
                        && last_reload.elapsed() < RELOAD_DEBOUNCE
                    {
                        continue;
                    }
                    if is_config || is_plugin || is_theme {
                        last_reload = std::time::Instant::now();
                    }

                    if is_config {
                        let state = Arc::clone(&state);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                if let Err(e) = state.reload_config().await {
                                    warn!("auto-reload config failed: {e}");
                                }
                            });
                        });
                    } else if is_plugin {
                        let state = Arc::clone(&state);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                if let Err(e) = state.reload_config().await {
                                    warn!("auto-reload plugins failed: {e}");
                                }
                            });
                        });
                    }
                    if is_theme {
                        let state = Arc::clone(&state);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                state.reload_theme().await;
                            });
                        });
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(())
}

mod libc {
    pub const ESRCH: i32 = 3;
    pub const SIGTERM: i32 = 15;
    /// Linux prctl option: set the parent-death signal.
    pub const PR_SET_PDEATHSIG: i32 = 1;

    unsafe extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
        pub fn prctl(option: i32, ...) -> i32;
    }
}

#[cfg(test)]
mod watcher_tests {
    use super::is_content_change;
    use notify::event::{
        AccessKind, AccessMode, CreateKind, DataChange, EventKind, MetadataKind, ModifyKind,
        RemoveKind, RenameMode,
    };

    #[test]
    fn access_events_never_trigger_a_reload() {
        // The feedback loop that pinned a core and leaked ~9.5 MB/s:
        // reload_config() READS the watched plugins directory, so counting a
        // read as a change made the watcher feed itself forever.
        assert!(!is_content_change(&EventKind::Access(AccessKind::Read)));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Read
        ))));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(!is_content_change(&EventKind::Access(AccessKind::Any)));
    }

    #[test]
    fn metadata_only_changes_never_trigger_a_reload() {
        // atime updates are emitted by the very reads reload_config performs.
        for kind in [
            MetadataKind::AccessTime,
            MetadataKind::Any,
            MetadataKind::Permissions,
        ] {
            assert!(
                !is_content_change(&EventKind::Modify(ModifyKind::Metadata(kind))),
                "metadata change {kind:?} must not reload"
            );
        }
    }

    #[test]
    fn real_content_changes_still_trigger_a_reload() {
        // Hot-reload is a documented feature; the filter must not break it.
        assert!(is_content_change(&EventKind::Create(CreateKind::File)));
        assert!(is_content_change(&EventKind::Remove(RemoveKind::File)));
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        // Atomic saves (tmp + rename) arrive as a rename, which is how
        // config_set and every editor write the file.
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Name(
            RenameMode::To
        ))));
        // Backends that cannot classify the change report Modify(Any); it
        // must stay reloadable or hot-reload silently dies on those.
        assert!(is_content_change(&EventKind::Modify(ModifyKind::Any)));
    }
}
