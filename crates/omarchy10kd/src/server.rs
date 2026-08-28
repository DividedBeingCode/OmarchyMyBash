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

pub const PROTOCOL_VERSION: &str = "0.4";

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
    /// Environment values carried from the shell (protocol 0.4 env channel).
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
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
    /// Per-request preset override (v0.4, used by the Quattro preset gallery).
    #[serde(default)]
    pub style_preset: Option<String>,
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

/// Ambient snapshot of the most recent prompt render (v0.4 0.3 status
/// enrichment). Served additively by the `status` control command.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenderSummary {
    pub cwd: String,
    pub branch: String,
    pub dirty: bool,
    pub staged: u32,
    pub unstaged: u32,
    pub conflicted: u32,
    pub ahead: u32,
    pub behind: u32,
    pub worktree: Option<String>,
    pub stale: bool,
    pub cmd_duration_ms: u64,
    pub exit_code: i32,
}

pub struct DaemonState {
    pub config: RwLock<Config>,
    pub palette: RwLock<ThemePalette>,
    pub git_cache: GitCache,
    pub config_path: PathBuf,
    pub socket_path: PathBuf,
    pub last_render: RwLock<Option<RenderSummary>>,
    pub started_at: std::time::Instant,
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
            last_render: RwLock::new(None),
            started_at: std::time::Instant::now(),
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

            // v0.4 0.3 status enrichment: merge the last render summary with
            // the live git cache entry and a cheap battery sysfs read.
            let last = state.last_render.read().await.clone();
            let live_git = match &last {
                Some(r) => Some(state.git_cache.get_status(Path::new(&r.cwd)).await),
                None => None,
            };
            let session_age_secs = state.started_at.elapsed().as_secs();

            let mut resp = serde_json::json!({
                "type": "control",
                "status": "ok",
                "pid": std::process::id(),
                "version": env!("CARGO_PKG_VERSION"),
                "protocol_version": PROTOCOL_VERSION,
                "cwd": cwd,
            });
            let obj = resp.as_object_mut().unwrap();
            obj.insert(
                "git".into(),
                git_summary_json(last.as_ref(), live_git.as_ref()),
            );
            obj.insert(
                "last_cmd_duration_ms".into(),
                serde_json::json!(last.as_ref().map(|r| r.cmd_duration_ms).unwrap_or(0)),
            );
            obj.insert(
                "last_exit_code".into(),
                serde_json::json!(last.as_ref().map(|r| r.exit_code).unwrap_or(0)),
            );
            obj.insert("session_age_secs".into(), serde_json::json!(session_age_secs));
            obj.insert("battery".into(), battery_json(battery_status()));
            write_response(writer, resp, request_id).await?;
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
        req.env.as_ref(),
    );

    // Record the ambient snapshot served by `status` (v0.4 0.3).
    *state.last_render.write().await = Some(RenderSummary {
        cwd: req.cwd.clone(),
        branch: if git_status.is_repo { git_status.branch.clone() } else { String::new() },
        dirty: git_status.is_dirty(),
        staged: git_status.staged,
        unstaged: git_status.unstaged,
        conflicted: git_status.conflicted,
        ahead: git_status.ahead,
        behind: git_status.behind,
        worktree: git_status.worktree.clone(),
        stale: git_status.stale,
        cmd_duration_ms: req.cmd_duration_ms,
        exit_code: req.exit_code,
    });

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
    let config_guard = state.config.read().await;
    // Per-request preset override (Quattro preset gallery, v0.4): clone the
    // config and force the requested preset, clearing the legacy layout
    // mapping so the override always resolves verbatim.
    let preset_config;
    let config: &Config = match &req.style_preset {
        Some(preset) => {
            let mut c = config_guard.clone();
            c.style.preset = preset.clone();
            c.prompt.layout = String::new();
            preset_config = c;
            &preset_config
        }
        None => &config_guard,
    };
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

    let renderer = PromptRenderer::new(config, &palette);
    let prompt = renderer.render_with_ssh(
        &req.cwd,
        req.exit_code,
        req.cmd_duration_ms,
        req.cols,
        req.jobs,
        &git_status,
        false, // no shell integration for preview
        Some(req.in_ssh),
        None,
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

/// Render the Claude Code statusline payload with the current config+palette
/// (v0.4 1.2). Left-only line, no OSC 133.
async fn handle_statusline(
    payload: &crate::render::StatuslinePayload,
    state: &Arc<DaemonState>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let config = state.config.read().await;
    let palette = state.palette.read().await;
    let renderer = PromptRenderer::new(&config, &palette);
    let left = renderer.render_statusline(payload);
    debug!("statusline rendered in {:?}", start.elapsed());
    write_response(
        writer,
        serde_json::json!({
            "type": "statusline",
            "status": "ok",
            "left": left,
        }),
        request_id,
    )
    .await
}

fn strip_np(s: &str) -> String {
    s.replace('\x01', "").replace('\x02', "")
}

/// Git summary object for the enriched `status` response. Prefers the live
/// git cache entry; falls back to the recorded render summary. `null` before
/// the first prompt render.
fn git_summary_json(
    last: Option<&RenderSummary>,
    live: Option<&crate::git::GitStatus>,
) -> serde_json::Value {
    let Some(l) = last else {
        return serde_json::Value::Null;
    };
    let (branch, dirty, staged, unstaged, conflicted, ahead, behind, worktree, stale) =
        match live {
            Some(g) if g.is_repo => (
                g.branch.clone(),
                g.is_dirty(),
                g.staged,
                g.unstaged,
                g.conflicted,
                g.ahead,
                g.behind,
                g.worktree.clone(),
                g.stale,
            ),
            _ => (
                l.branch.clone(),
                l.dirty,
                l.staged,
                l.unstaged,
                l.conflicted,
                l.ahead,
                l.behind,
                l.worktree.clone(),
                l.stale,
            ),
        };
    serde_json::json!({
        "branch": branch,
        "dirty": dirty,
        "staged": staged,
        "unstaged": unstaged,
        "conflicted": conflicted,
        "ahead": ahead,
        "behind": behind,
        "worktree": worktree,
        "stale": stale,
    })
}

/// Cheap battery read for the status response; reuse of the battery segment's
/// sysfs helper. `None` when no battery exists (desktops).
fn battery_status() -> Option<(u32, bool)> {
    crate::segments::battery::read_battery()
}

fn battery_json(battery: Option<(u32, bool)>) -> serde_json::Value {
    match battery {
        Some((capacity, charging)) => serde_json::json!({
            "capacity": capacity,
            "status": if charging { "Charging" } else { "Discharging" },
        }),
        None => serde_json::Value::Null,
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
            Some("statusline") => {
                // Claude Code statusLine JSON arrives verbatim under
                // `payload`; accept a flat payload too (legacy/test clients).
                let payload_value = match msg.rest.get("payload") {
                    Some(p) if p.is_object() => p.clone(),
                    _ => msg.rest.clone(),
                };
                match serde_json::from_value::<crate::render::StatuslinePayload>(payload_value) {
                    Ok(payload) => handle_statusline(&payload, &state, &mut writer, request_id).await?,
                    Err(e) => {
                        write_response(&mut writer, serde_json::json!({
                            "type": "statusline",
                            "status": "error",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_summary_null_before_first_render() {
        assert_eq!(git_summary_json(None, None), serde_json::Value::Null);
    }

    #[test]
    fn test_git_summary_serializes_contract_fields() {
        let last = RenderSummary {
            cwd: "/home/u/project".into(),
            branch: "main".into(),
            dirty: true,
            staged: 1,
            unstaged: 2,
            conflicted: 0,
            ahead: 3,
            behind: 0,
            worktree: Some("wt".into()),
            stale: false,
            cmd_duration_ms: 1500,
            exit_code: 2,
        };
        let v = git_summary_json(Some(&last), None);
        assert_eq!(v["branch"], "main");
        assert_eq!(v["dirty"], true);
        assert_eq!(v["staged"], 1);
        assert_eq!(v["unstaged"], 2);
        assert_eq!(v["conflicted"], 0);
        assert_eq!(v["ahead"], 3);
        assert_eq!(v["behind"], 0);
        assert_eq!(v["worktree"], "wt");
        assert_eq!(v["stale"], false);
    }

    #[test]
    fn test_battery_json_contract() {
        let charging = battery_json(Some((77, true)));
        assert_eq!(charging["capacity"], 77);
        assert_eq!(charging["status"], "Charging");

        let discharging = battery_json(Some((12, false)));
        assert_eq!(discharging["status"], "Discharging");

        assert_eq!(battery_json(None), serde_json::Value::Null);
    }

    #[test]
    fn test_render_summary_serializes() {
        let last = RenderSummary {
            cwd: "/tmp".into(),
            branch: String::new(),
            dirty: false,
            staged: 0,
            unstaged: 0,
            conflicted: 0,
            ahead: 0,
            behind: 0,
            worktree: None,
            stale: true,
            cmd_duration_ms: 42,
            exit_code: 0,
        };
        let v = serde_json::to_value(&last).unwrap();
        assert_eq!(v["cmd_duration_ms"], 42);
        assert_eq!(v["stale"], true);
    }
}
