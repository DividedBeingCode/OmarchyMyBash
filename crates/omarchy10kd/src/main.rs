mod config;
mod git;
mod layout;
mod render;
mod segments;
mod server;
mod style;
mod theme;
mod terminal;

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use config::Config;
use server::DaemonState;
use theme::ThemePalette;

fn socket_path() -> PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/tmp".into());
    let ppid = std::env::var("O10K_PARENT_PID")
        .unwrap_or_else(|_| std::process::id().to_string());
    PathBuf::from(runtime_dir).join(format!("omarchy10k-{ppid}.sock"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config
    let config_path = Config::config_path();
    let config = Config::load(&config_path).unwrap_or_else(|e| {
        eprintln!("warning: failed to load config: {e}, using defaults");
        Config::default()
    });

    // Init tracing
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.daemon.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();

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

    // Monitor parent process (clean exit when shell dies)
    let sock_for_cleanup = sock_path.clone();
    tokio::spawn(async move {
        monitor_parent().await;
        info!("parent process exited, shutting down");
        let _ = std::fs::remove_file(&sock_for_cleanup);
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
        loop {
            match rx.recv() {
                Ok(event) => {
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

                    if is_config {
                        let state = Arc::clone(&state);
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                if let Err(e) = state.reload_config().await {
                                    warn!("auto-reload config failed: {e}");
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

    unsafe extern "C" {
        pub fn kill(pid: i32, sig: i32) -> i32;
    }
}
