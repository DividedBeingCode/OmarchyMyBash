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

pub const PROTOCOL_VERSION: &str = "0.2";

#[derive(Debug, serde::Deserialize)]
pub struct PromptRequest {
    pub cwd: String,
    pub exit_code: i32,
    pub cmd_duration_ms: u64,
    pub cols: u16,
    pub jobs: u32,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub shell_integration: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
struct TypedMessage {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(flatten)]
    rest: serde_json::Value,
}

pub struct DaemonState {
    pub config: RwLock<Config>,
    pub palette: RwLock<ThemePalette>,
    pub git_cache: GitCache,
    pub config_path: PathBuf,
}

impl DaemonState {
    pub fn new(config: Config, palette: ThemePalette, config_path: PathBuf) -> Self {
        let git_ttl_ms = config.git.cache_ttl_ms;
        Self {
            config: RwLock::new(config),
            palette: RwLock::new(palette),
            git_cache: GitCache::new(git_ttl_ms),
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
        let config = self.config.read().await;
        let new_palette = ThemePalette::resolve_palette(&config);
        drop(config);
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

async fn write_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    mut resp: serde_json::Value,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
    if let Some(id) = request_id {
        resp.as_object_mut().unwrap().insert("id".into(), serde_json::json!(id));
    }
    writer.write_all(resp.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

async fn handle_control(
    command: &str,
    state: &Arc<DaemonState>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request_id: Option<&str>,
) -> anyhow::Result<bool> {
    match command {
        "reload_config" => {
            if let Err(e) = state.reload_config().await {
                warn!("config reload failed: {e}");
            }
            write_response(writer, serde_json::json!({"type":"control","status":"ok"}), request_id).await?;
        }
        "reload_theme" => {
            state.reload_theme().await;
            write_response(writer, serde_json::json!({"type":"control","status":"ok"}), request_id).await?;
        }
        "invalidate_git" => {
            state.git_cache.invalidate_all().await;
            write_response(writer, serde_json::json!({"type":"control","status":"ok"}), request_id).await?;
        }
        "shutdown" => {
            info!("shutdown requested");
            write_response(writer, serde_json::json!({"type":"control","status":"bye"}), request_id).await?;
            std::process::exit(0);
        }
        "status" => {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": PROTOCOL_VERSION,
                "cwd": cwd,
            }), request_id).await?;
        }
        "config_get" => {
            let config = state.config.read().await;
            let config_json = serde_json::to_value(&*config).unwrap_or_default();
            write_response(writer, serde_json::json!({
                "type": "config",
                "status": "ok",
                "config": config_json,
            }), request_id).await?;
        }
        "config_set" => {
            write_response(writer, serde_json::json!({
                "type": "config",
                "status": "error",
                "error": "config_set requires payload; use typed message with rest field",
            }), request_id).await?;
        }
        _ => {
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "error",
                "error": format!("unknown command: {command}"),
            }), request_id).await?;
        }
    }
    Ok(true)
}

async fn handle_prompt(
    req: &PromptRequest,
    state: &Arc<DaemonState>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
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
        req.shell_integration.unwrap_or(true),
    );

    debug!("prompt rendered in {:?}", start.elapsed());

    let mut resp = serde_json::to_value(&prompt)?;
    resp.as_object_mut().unwrap().insert("type".into(), serde_json::json!("prompt"));
    write_response(writer, resp, request_id).await
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

        let msg: TypedMessage = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(e) => {
                warn!("invalid JSON: {e}");
                write_response(
                    &mut writer,
                    serde_json::json!({"type":"error","error": e.to_string()}),
                    None,
                ).await?;
                line.clear();
                continue;
            }
        };

        let request_id = msg.id.as_deref();

        match msg.r#type.as_deref() {
            Some("hello") => {
                write_response(&mut writer, serde_json::json!({
                    "type": "hello",
                    "status": "ok",
                    "protocol_version": PROTOCOL_VERSION,
                    "server_version": env!("CARGO_PKG_VERSION"),
                }), request_id).await?;
            }
            Some("control") => {
                if let Some(ref cmd) = msg.command {
                    handle_control(cmd, &state, &mut writer, request_id).await?;
                } else {
                    write_response(&mut writer, serde_json::json!({
                        "type": "error",
                        "error": "control message requires 'command' field",
                    }), request_id).await?;
                }
            }
            Some("prompt") => {
                match serde_json::from_value::<PromptRequest>(msg.rest) {
                    Ok(req) => handle_prompt(&req, &state, &mut writer, request_id).await?,
                    Err(e) => {
                        write_response(&mut writer, serde_json::json!({
                            "type": "error",
                            "error": e.to_string(),
                        }), request_id).await?;
                    }
                }
            }
            Some("config") => {
                if let Some(ref cmd) = msg.command {
                    if cmd == "set" {
                        if let Some(patch) = msg.rest.get("config") {
                            let config_path = state.config_path.clone();
                            if let Ok(existing) = std::fs::read_to_string(&config_path) {
                                let mut doc: toml::Table = toml::from_str(&existing).unwrap_or_default();
                                if let Some(obj) = patch.as_object() {
                                    for (k, v) in obj {
                                        if let Ok(toml_val) = serde_json::from_value::<toml::Value>(v.clone()) {
                                            doc.insert(k.clone(), toml_val);
                                        }
                                    }
                                }
                                let new_toml = toml::to_string_pretty(&doc).unwrap_or_default();
                                std::fs::write(&config_path, new_toml)?;
                            }
                            state.reload_config().await.ok();
                            write_response(&mut writer, serde_json::json!({
                                "type": "config",
                                "status": "ok",
                            }), request_id).await?;
                        } else {
                            write_response(&mut writer, serde_json::json!({
                                "type": "error",
                                "error": "config set requires 'config' field",
                            }), request_id).await?;
                        }
                    } else {
                        handle_control(cmd, &state, &mut writer, request_id).await?;
                    }
                } else {
                    handle_control("config_get", &state, &mut writer, request_id).await?;
                }
            }
            None => {
                if let Some(ref cmd) = msg.command {
                    handle_control(cmd, &state, &mut writer, request_id).await?;
                } else {
                    match serde_json::from_str::<PromptRequest>(trimmed) {
                        Ok(req) => handle_prompt(&req, &state, &mut writer, request_id).await?,
                        Err(e) => {
                            warn!("invalid request: {e}");
                            write_response(&mut writer, serde_json::json!({
                                "type": "error",
                                "error": e.to_string(),
                            }), request_id).await?;
                        }
                    }
                }
            }
            Some(unknown) => {
                write_response(&mut writer, serde_json::json!({
                    "type": "error",
                    "error": format!("unknown message type: {unknown}"),
                }), request_id).await?;
            }
        }

        line.clear();
    }

    Ok(())
}
