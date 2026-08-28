use std::os::unix::fs::PermissionsExt;
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

pub const PROTOCOL_VERSION: &str = "0.3";

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
pub struct PreviewRequest {
    #[serde(default = "default_preview_cwd")]
    pub cwd: String,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub cmd_duration_ms: u64,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default)]
    pub jobs: u32,
    #[serde(default)]
    pub in_ssh: bool,
    #[serde(default)]
    pub git_branch: String,
    #[serde(default)]
    pub git_staged: u32,
    #[serde(default)]
    pub git_unstaged: u32,
}

fn default_preview_cwd() -> String {
    "~/projects/my-app".into()
}

fn default_cols() -> u16 {
    120
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
    pub socket_path: PathBuf,
}

impl DaemonState {
    pub fn new(
        config: Config,
        palette: ThemePalette,
        config_path: PathBuf,
        socket_path: PathBuf,
    ) -> Self {
        let git_ttl_ms = config.git.cache_ttl_ms;
        Self {
            config: RwLock::new(config),
            palette: RwLock::new(palette),
            git_cache: GitCache::new(git_ttl_ms),
            config_path,
            socket_path,
        }
    }

    pub async fn reload_config(&self) -> anyhow::Result<()> {
        let new_config = Config::load(&self.config_path)?;
        self.git_cache.set_ttl(new_config.git.cache_ttl_ms);
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
    // Validate/clean up any leftover file at the socket path
    if socket_path.exists() {
        if let Err(e) = clear_stale_socket(socket_path) {
            error!("socket path check failed: {e}");
            return Err(e);
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    // Only the owner may talk to the daemon (prompt data, control commands).
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
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

mod libc {
    unsafe extern "C" {
        pub fn getuid() -> u32;
    }
}

/// Handle a leftover file at the socket path before binding: unlink it only
/// when it is a socket owned by the current user that nothing is listening
/// on. A live listener means another daemon owns the path — bail out
/// instead of hijacking it.
fn clear_stale_socket(socket_path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let meta = std::fs::metadata(socket_path)?;
    if !meta.file_type().is_socket() {
        anyhow::bail!(
            "{} exists and is not a socket; refusing to remove it",
            socket_path.display()
        );
    }
    if meta.uid() != unsafe { libc::getuid() } {
        anyhow::bail!(
            "{} is owned by another user; refusing to remove it",
            socket_path.display()
        );
    }
    match std::os::unix::net::UnixStream::connect(socket_path) {
        Ok(_) => anyhow::bail!(
            "another daemon is already listening on {}; refusing to hijack it",
            socket_path.display()
        ),
        Err(_) => {
            info!("removing stale socket {}", socket_path.display());
            std::fs::remove_file(socket_path)?;
            Ok(())
        }
    }
}

/// Remove the daemon's socket file on shutdown, but only if it is still a
/// socket owned by the current user (never delete a swapped-in file).
pub fn remove_socket_file(socket_path: &Path) {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    let Ok(meta) = std::fs::metadata(socket_path) else {
        return; // already gone
    };
    if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::getuid() } {
        warn!(
            "{} is no longer our socket; leaving it in place",
            socket_path.display()
        );
        return;
    }
    let _ = std::fs::remove_file(socket_path);
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
            match state.reload_config().await {
                Ok(()) => {
                    write_response(writer, serde_json::json!({"type":"control","status":"ok"}), request_id).await?;
                }
                Err(e) => {
                    warn!("config reload failed: {e}");
                    write_response(writer, serde_json::json!({"type":"control","status":"error","error":format!("reload failed: {e}")}), request_id).await?;
                }
            }
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
            remove_socket_file(&state.socket_path);
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
        "palette" => {
            let palette = state.palette.read().await;
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "palette": {
                    "accent": format!("#{:02x}{:02x}{:02x}", palette.accent.r, palette.accent.g, palette.accent.b),
                    "foreground": format!("#{:02x}{:02x}{:02x}", palette.foreground.r, palette.foreground.g, palette.foreground.b),
                    "muted": format!("#{:02x}{:02x}{:02x}", palette.muted.r, palette.muted.g, palette.muted.b),
                    "background": format!("#{:02x}{:02x}{:02x}", palette.background.r, palette.background.g, palette.background.b),
                    "red": format!("#{:02x}{:02x}{:02x}", palette.red.r, palette.red.g, palette.red.b),
                    "green": format!("#{:02x}{:02x}{:02x}", palette.green.r, palette.green.g, palette.green.b),
                    "yellow": format!("#{:02x}{:02x}{:02x}", palette.yellow.r, palette.yellow.g, palette.yellow.b),
                    "blue": format!("#{:02x}{:02x}{:02x}", palette.blue.r, palette.blue.g, palette.blue.b),
                }
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

async fn handle_preview(
    req: &PreviewRequest,
    state: &Arc<DaemonState>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
    let config = state.config.read().await;
    let palette = state.palette.read().await;

    let git_status = crate::git::GitStatus {
        is_repo: !req.git_branch.is_empty(),
        branch: if req.git_branch.is_empty() {
            "main".into()
        } else {
            req.git_branch.clone()
        },
        staged: req.git_staged,
        unstaged: req.git_unstaged,
        ..Default::default()
    };

    let renderer = PromptRenderer::new(&config, &palette);
    let prompt = renderer.render_with_ssh(
        &req.cwd,
        req.exit_code,
        req.cmd_duration_ms,
        req.cols,
        req.jobs,
        &git_status,
        false, // no shell integration for preview
        Some(req.in_ssh),
    );

    write_response(
        writer,
        serde_json::json!({
            "type": "preview",
            "status": "ok",
            "left": strip_np(&prompt.left),
            "right": prompt.right.as_deref().map(strip_np),
        }),
        request_id,
    )
    .await
}

fn strip_np(s: &str) -> String {
    s.replace('\x01', "").replace('\x02', "")
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
            Some("preview") => {
                match serde_json::from_value::<PreviewRequest>(msg.rest) {
                    Ok(req) => handle_preview(&req, &state, &mut writer, request_id).await?,
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

                            let mut doc: toml::Table = match std::fs::read_to_string(&config_path) {
                                Ok(existing) => match toml::from_str(&existing) {
                                    Ok(t) => t,
                                    Err(e) => {
                                        warn!("config parse error, refusing to overwrite: {e}");
                                        write_response(&mut writer, serde_json::json!({
                                            "type": "config",
                                            "status": "error",
                                            "error": format!("config.toml has syntax errors: {e}"),
                                        }), request_id).await?;
                                        line.clear();
                                        continue;
                                    }
                                },
                                Err(_) => toml::Table::new(),
                            };

                            let mut failed_keys: Vec<String> = Vec::new();
                            if let Some(obj) = patch.as_object() {
                                for (k, v) in obj {
                                    match serde_json::from_value::<toml::Value>(v.clone()) {
                                        Ok(toml_val) => {
                                            merge_toml_value(doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())), toml_val);
                                        }
                                        Err(_) => failed_keys.push(k.clone()),
                                    }
                                }
                            }

                            if !failed_keys.is_empty() {
                                // All-or-nothing: never write a partial patch.
                                warn!("config set: unconvertible values for keys: {}", failed_keys.join(", "));
                                write_response(&mut writer, serde_json::json!({
                                    "type": "config",
                                    "status": "error",
                                    "error": format!("values for keys {} are not representable in TOML; nothing was written", failed_keys.join(", ")),
                                    "failed_keys": failed_keys,
                                }), request_id).await?;
                                line.clear();
                                continue;
                            }

                            if let Some(parent) = config_path.parent() {
                                let _ = std::fs::create_dir_all(parent);
                            }

                            let new_toml = toml::to_string_pretty(&doc).unwrap_or_default();
                            let tmp_path = config_path.with_extension("toml.tmp");
                            match std::fs::write(&tmp_path, &new_toml)
                                .and_then(|_| std::fs::rename(&tmp_path, &config_path))
                            {
                                Ok(()) => {}
                                Err(e) => {
                                    warn!("config write failed: {e}");
                                    write_response(&mut writer, serde_json::json!({
                                        "type": "config",
                                        "status": "error",
                                        "error": format!("failed to write config: {e}"),
                                    }), request_id).await?;
                                    line.clear();
                                    continue;
                                }
                            }

                            let touches_theme = patch.get("theme").is_some();
                            if let Err(e) = state.reload_config().await {
                                warn!("config reload after set failed: {e}");
                            }
                            if touches_theme {
                                state.reload_theme().await;
                            }
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
                // Check for cwd first -- prompt requests always have cwd
                if msg.rest.get("cwd").is_some() {
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
                } else if let Some(ref cmd) = msg.command {
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

fn merge_toml_value(target: &mut toml::Value, patch: toml::Value) {
    match (&mut *target, patch) {
        (toml::Value::Table(target_table), toml::Value::Table(patch_table)) => {
            for (k, v) in patch_table {
                merge_toml_value(
                    target_table.entry(k).or_insert(toml::Value::Table(toml::Table::new())),
                    v,
                );
            }
        }
        (target, patch) => {
            *target = patch;
        }
    }
}
