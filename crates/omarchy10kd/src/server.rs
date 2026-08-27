use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::git::GitCache;
use crate::render::PromptRenderer;
use crate::theme::ThemePalette;

#[derive(Debug, serde::Deserialize)]
pub struct PromptRequest {
    pub cwd: String,
    pub exit_code: i32,
    pub cmd_duration_ms: u64,
    pub cols: u16,
    pub jobs: u32,
    #[serde(default)]
    pub command: Option<String>,
}

pub struct DaemonState {
    pub config: RwLock<Config>,
    pub palette: RwLock<ThemePalette>,
    pub git_cache: GitCache,
    pub config_path: PathBuf,
}

impl DaemonState {
    pub fn new(config: Config, palette: ThemePalette, config_path: PathBuf) -> Self {
        let git_ttl = 5; // seconds
        Self {
            config: RwLock::new(config),
            palette: RwLock::new(palette),
            git_cache: GitCache::new(git_ttl),
            config_path,
        }
    }

    pub async fn reload_config(&self) -> anyhow::Result<()> {
        let new_config = Config::load(&self.config_path)?;
        let mut config = self.config.write().await;
        *config = new_config;
        info!("config reloaded");
        Ok(())
    }

    pub async fn reload_theme(&self) {
        let new_palette = ThemePalette::load_omarchy();
        let mut palette = self.palette.write().await;
        *palette = new_palette;
        info!("theme reloaded");
    }
}

pub async fn run_server(
    socket_path: &Path,
    state: Arc<DaemonState>,
) -> anyhow::Result<()> {
    // Clean up stale socket
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    info!("daemon listening on {}", socket_path.display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        debug!("connection handler error: {e}");
                    }
                });
            }
            Err(e) => {
                error!("accept error: {e}");
            }
        }
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: Arc<DaemonState>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();

        // Handle control commands
        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(command) = cmd.get("command").and_then(|c| c.as_str()) {
                match command {
                    "reload_config" => {
                        if let Err(e) = state.reload_config().await {
                            warn!("config reload failed: {e}");
                        }
                        let resp = serde_json::json!({"status": "ok"});
                        writer.write_all(resp.to_string().as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        line.clear();
                        continue;
                    }
                    "reload_theme" => {
                        state.reload_theme().await;
                        let resp = serde_json::json!({"status": "ok"});
                        writer.write_all(resp.to_string().as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        line.clear();
                        continue;
                    }
                    "invalidate_git" => {
                        state.git_cache.invalidate_all().await;
                        let resp = serde_json::json!({"status": "ok"});
                        writer.write_all(resp.to_string().as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        line.clear();
                        continue;
                    }
                    "shutdown" => {
                        info!("shutdown requested");
                        let resp = serde_json::json!({"status": "bye"});
                        writer.write_all(resp.to_string().as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        std::process::exit(0);
                    }
                    "status" => {
                        let resp = serde_json::json!({
                            "status": "ok",
                            "pid": std::process::id(),
                            "version": env!("CARGO_PKG_VERSION"),
                        });
                        writer.write_all(resp.to_string().as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        line.clear();
                        continue;
                    }
                    _ => {}
                }
            }
        }

        // Handle prompt request
        let response = match serde_json::from_str::<PromptRequest>(trimmed) {
            Ok(req) => {
                let start = std::time::Instant::now();

                let config = state.config.read().await;
                let palette = state.palette.read().await;

                let cwd = PathBuf::from(&req.cwd);
                let git_status = state.git_cache.get_status(&cwd).await;

                let renderer = PromptRenderer::new(&config, &palette);
                let prompt = renderer.render(
                    &req.cwd,
                    req.exit_code,
                    req.cmd_duration_ms,
                    req.cols,
                    req.jobs,
                    &git_status,
                );

                debug!("prompt rendered in {:?}", start.elapsed());
                serde_json::to_string(&prompt)?
            }
            Err(e) => {
                warn!("invalid request: {e}");
                serde_json::json!({"error": e.to_string()}).to_string()
            }
        };

        writer.write_all(response.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        line.clear();
    }

    Ok(())
}
