use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::git::GitCache;
use crate::plugins::{self, Plugin, PluginCache};
use crate::render::PromptRenderer;
use crate::theme::ThemePalette;

/// Backoff applied after a failed `accept()`, growing with consecutive
/// failures. Without it a persistent error is an unthrottled spin.
const ACCEPT_BACKOFF_BASE: std::time::Duration = std::time::Duration::from_millis(20);
const ACCEPT_BACKOFF_MAX: std::time::Duration = std::time::Duration::from_millis(1000);

pub const PROTOCOL_VERSION: &str = "0.5";

/// Hard cap on a single NDJSON frame from a client. The largest legitimate
/// message (a preview request) is a few hundred bytes; 64 KiB leaves orders
/// of magnitude of headroom while bounding attacker-controlled memory.
const MAX_FRAME_BYTES: usize = 64 * 1024;

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
    /// Wave 2: dry-run render a Look without persisting anything.
    #[serde(default)]
    pub look: Option<String>,
    #[serde(default)]
    pub style_preset: Option<String>,
    /// Catalog key applied to both separators (configure wizard live preview).
    #[serde(default)]
    pub style_separators: Option<String>,
    /// Frame mode: none | left | right | full (configure wizard live preview).
    #[serde(default)]
    pub style_frame: Option<String>,
    /// Two-line prompt toggle (configure wizard live preview).
    #[serde(default)]
    pub prompt_newline: Option<bool>,
    /// Looks Studio / ramp designer: `config_set`-shaped patch merged over
    /// the effective config (base → Look → patch; patch wins) before render.
    #[serde(default)]
    pub patch: Option<serde_json::Value>,
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
    /// AI-agent detected in the env channel of the last prompt render
    /// ("claude" | "codex"); `None` when no agent env key was present.
    pub agent: Option<String>,
}

/// Detect the AI agent present in a prompt render's env channel. Mirrors
/// segments/ai.rs detection (CLAUDE_CODE_ENTRYPOINT -> claude; CODEX_SANDBOX
/// or CODEX_HOME -> codex) so the `status` agent field always agrees with the
/// prompt's agent segment.
fn detect_agent(env: Option<&std::collections::HashMap<String, String>>) -> Option<String> {
    let env = env?;
    if env.contains_key("CLAUDE_CODE_ENTRYPOINT") {
        Some("claude".into())
    } else if env.contains_key("CODEX_SANDBOX") || env.contains_key("CODEX_HOME") {
        Some("codex".into())
    } else {
        None
    }
}

pub struct DaemonState {
    pub config: RwLock<Config>,
    pub palette: RwLock<ThemePalette>,
    pub git_cache: GitCache,
    /// Plugin registry (declarative segment plugins). Rebuilt on startup
    /// and every reload_config so add/update/remove on disk takes effect.
    pub plugins: RwLock<Vec<Plugin>>,
    /// TTL cache for command-tier plugin segments (never blocks render).
    pub plugin_cache: PluginCache,
    pub config_path: PathBuf,
    pub socket_path: PathBuf,
    pub last_render: RwLock<Option<RenderSummary>>,
    pub started_at: std::time::Instant,
    /// Bumped on every mutation of the in-memory `config`. Consumers that
    /// memoize work derived from it (the project-profile merge cache) key on
    /// this so a reload or transient Look invalidates them. Every write to
    /// `config` must go through [`Self::set_config`].
    config_generation: std::sync::atomic::AtomicU64,
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
            plugins: RwLock::new(plugins::load_plugins(&plugins::plugins_dir_for(
                config_path.parent().unwrap_or_else(|| Path::new(".")),
            ))),
            plugin_cache: PluginCache::new(),
            config_path,
            socket_path,
            last_render: RwLock::new(None),
            started_at: std::time::Instant::now(),
            config_generation: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Current in-memory config generation — see [`Self::config_generation`].
    pub fn config_generation(&self) -> u64 {
        self.config_generation
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Replace the in-memory config and bump the generation. The single
    /// choke point for config mutation: anything that writes `self.config`
    /// directly would leave derived caches serving stale values.
    pub async fn set_config(&self, new_config: Config) {
        let mut config = self.config.write().await;
        *config = new_config;
        self.config_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn reload_config(&self) -> anyhow::Result<()> {
        let new_config = Config::load(&self.config_path)?;
        self.git_cache.set_ttl(new_config.git.cache_ttl_ms);
        // Re-read the plugin registry so plugin add/remove/update on disk
        // and [plugins].enabled changes all land on one reload path.
        let new_plugins = plugins::load_plugins(&plugins::plugins_dir_for(
            self.config_path.parent().unwrap_or_else(|| Path::new(".")),
        ));
        *self.plugins.write().await = new_plugins;
        self.plugin_cache.invalidate_all().await;
        self.set_config(new_config).await;
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

    let mut accept_errors: u32 = 0;
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                accept_errors = 0;
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        debug!("connection handler error: {e}");
                    }
                });
            }
            Err(e) => {
                // Back off. A persistent accept error (EMFILE, a listener
                // whose socket was replaced underneath us) otherwise spins
                // this loop at full speed, formatting an error string per
                // iteration — a pinned core and unbounded allocation churn
                // with no external input.
                accept_errors += 1;
                if accept_errors == 1 || accept_errors % 100 == 0 {
                    error!("accept error ({accept_errors}): {e}");
                }
                let backoff = std::cmp::min(
                    ACCEPT_BACKOFF_MAX,
                    ACCEPT_BACKOFF_BASE * accept_errors.min(32),
                );
                tokio::time::sleep(backoff).await;
                continue;
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
        if let Some(obj) = resp.as_object_mut() {
            obj.insert("id".into(), serde_json::json!(id));
        }
    }
    writer.write_all(resp.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

async fn handle_control(
    command: &str,
    rest: &serde_json::Value,
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
        "looks" => {
            let config_guard = state.config.read().await;
            let looks = crate::looks::all(&config_guard);
            let looks_json: Vec<serde_json::Value> = looks
                .iter()
                .map(|l| serde_json::json!({ "name": l.name, "label": l.label, "patch": l.patch }))
                .collect();
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "looks": looks_json,
            }), request_id).await?;
        }
        "defaults" => {
            let config_json = serde_json::to_value(crate::config::Config::default())
                .unwrap_or_default();
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "config": config_json,
            }), request_id).await?;
        }
        "palettes" => {
            let palettes: Vec<serde_json::Value> = ["tokyo-night", "catppuccin", "gruvbox",
                "nord", "dracula", "rose-pine", "everforest", "kanagawa"]
                .iter()
                .filter_map(|k| crate::looks::curated_palette(k)
                    .map(|v| serde_json::json!({ "key": k, "theme": v["theme"].clone() })))
                .collect();
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "palettes": palettes,
            }), request_id).await?;
        }
        "looks_apply" => {
            let name = rest.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let transient = rest.get("transient").and_then(|v| v.as_bool()).unwrap_or(false);
            if name.is_empty() {
                write_response(writer, serde_json::json!({
                    "type": "control", "status": "error", "error": "looks_apply requires 'name'",
                }), request_id).await?;
            } else {
                // Scope the read guard: write_config_patch / the transient
                // path take the write lock below — holding the read guard
                // across them deadlocks the daemon.
                let look = {
                    let cfg_guard = state.config.read().await;
                    crate::looks::resolve(&name, &cfg_guard)
                };
                match look {
                    Some(l) => {
                        if transient {
                            // Try: in-memory only. Revert = reload_config.
                            let current = state.config.read().await.clone();
                            match crate::looks::apply_transient(&current, &l.patch) {
                                Ok(new_config) => {
                                    state.set_config(new_config).await;
                                    state.reload_theme().await;
                                    write_response(writer, serde_json::json!({
                                        "type": "control", "status": "ok", "transient": true,
                                    }), request_id).await?;
                                }
                                Err(e) => {
                                    write_response(writer, serde_json::json!({
                                        "type": "control", "status": "error", "error": e,
                                    }), request_id).await?;
                                }
                            }
                        } else {
                            match write_config_patch(state, &l.patch).await {
                                Ok(()) => {
                                    write_response(writer, serde_json::json!({
                                        "type": "control", "status": "ok",
                                    }), request_id).await?;
                                }
                                Err(e) => {
                                    write_response(writer, serde_json::json!({
                                        "type": "control", "status": "error", "error": e,
                                    }), request_id).await?;
                                }
                            }
                        }
                    }
                    None => {
                        write_response(writer, serde_json::json!({
                            "type": "control", "status": "error",
                            "error": format!("unknown look: {name}"),
                        }), request_id).await?;
                    }
                }
            }
        }
        "looks_save" => {
            let name = rest.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let label = rest.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                write_response(writer, serde_json::json!({
                    "type": "control", "status": "error", "error": "looks_save requires 'name'",
                }), request_id).await?;
            } else {
                let current = state.config.read().await.clone();
                // Options must serialize as TOML-safe values: `null` is not
                // representable and would make the whole looks patch fail to
                // write. None means "preset default" — capture it as such.
                let opt_str = |o: &Option<String>| o.clone().unwrap_or_default();
                let entry_patch = serde_json::json!({
                    "style": {
                        "preset": current.style.preset,
                        "separators": { "shape": opt_str(&current.style.separators.shape), "left": opt_str(&current.style.separators.left), "right": opt_str(&current.style.separators.right) },
                        "frame": { "enabled": current.style.frame.enabled.unwrap_or(true), "gap_char": opt_str(&current.style.frame.gap_char), "gap_gradient": opt_str(&current.style.frame.gap_gradient) },
                    },
                    "glyphs": {
                        "os_icon": current.segments.os.icon,
                        "character": current.segments.character.success,
                        "git_branch_icon": current.git.branch_icon,
                    },
                    "prompt": { "blank_line": current.prompt.blank_line },
                });
                let entry = serde_json::json!({ "label": label, "palette": "keep", "patch": entry_patch });
                let mut looks_tbl = serde_json::Map::new();
                looks_tbl.insert(name.clone(), entry);
                let patch = serde_json::json!({ "looks": looks_tbl });
                match write_config_patch(state, &patch).await {
                    Ok(()) => {
                        write_response(writer, serde_json::json!({
                            "type": "control", "status": "ok",
                        }), request_id).await?;
                    }
                    Err(e) => {
                        write_response(writer, serde_json::json!({
                            "type": "control", "status": "error", "error": e,
                        }), request_id).await?;
                    }
                }
            }
        }
        "looks_delete" => {
            let name = rest.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if name.is_empty() {
                write_response(writer, serde_json::json!({
                    "type": "control", "status": "error", "error": "looks_delete requires 'name'",
                }), request_id).await?;
            } else {
                // Scope the read guard: delete_user_look takes the write
                // lock in reload_config below — holding the read guard
                // across it deadlocks the daemon.
                let verdict = {
                    let cfg_guard = state.config.read().await;
                    validate_look_deletion(&name, &cfg_guard)
                };
                match verdict {
                    Ok(()) => match delete_user_look(state, &name).await {
                        Ok(()) => {
                            write_response(writer, serde_json::json!({
                                "type": "control", "status": "ok",
                            }), request_id).await?;
                        }
                        Err(e) => {
                            write_response(writer, serde_json::json!({
                                "type": "control", "status": "error", "error": e,
                            }), request_id).await?;
                        }
                    },
                    Err(e) => {
                        write_response(writer, serde_json::json!({
                            "type": "control", "status": "error", "error": e,
                        }), request_id).await?;
                    }
                }
            }
        }
        "script_list" | "script_run" => {
                write_response(
                    writer,
                    crate::script_exec::handle_script_control(command, rest).await,
                    request_id,
                ).await?;
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
            if let Some(obj) = resp.as_object_mut() {
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
                obj.insert(
                    "agent".into(),
                    serde_json::json!(last.as_ref().and_then(|r| r.agent.clone())),
                );
            }
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

    let base = state.config.read().await;
    let palette = state.palette.read().await;

    let cwd = PathBuf::from(&req.cwd);
    let git_status = state.git_cache.get_status(&cwd).await;

    // Merge contract (Tier C project profiles): base config (with any
    // transient Look already applied in-memory) → project profile → …
    // A profile failure is warn-once and swallowed — never fails the prompt.
    // The merge is memoized on (profile file stamp, config generation); this
    // runs on every render, and the TOML round-trip inside it is the most
    // expensive thing on the path.
    let profiled = crate::profiles::effective_config_for(
        &base,
        state.config_generation(),
        &cwd,
        &crate::profiles::home_dir(),
    );
    let config: &Config = match &profiled {
        Some(merged) => merged,
        None => &base,
    };
    let home = std::env::var("HOME").unwrap_or_default();
    let in_ssh = std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok();
    let term_caps = crate::terminal::TermCaps::detect();
    let renderer = PromptRenderer::new(config, &palette);

    // Plugin segments: rendered against the effective (profile-merged)
    // config's [plugins].enabled list; command-tier refreshes spawn in the
    // background and never block this render.
    let plugin_segments = {
        let registry = state.plugins.read().await;
        plugins::render_plugin_segments(
            &crate::segments::SegmentContext {
                cwd: &req.cwd,
                home: &home,
                exit_code: req.exit_code,
                cmd_duration_ms: req.cmd_duration_ms,
                cols: req.cols,
                jobs: req.jobs,
                in_ssh,
                git_status: &git_status,
                config,
                palette: &palette,
                term_caps: &term_caps,
                env: req.env.as_ref(),
            },
            &state.plugin_cache,
            &registry,
        )
        .await
    };

    let prompt = renderer.render(
        &req.cwd,
        req.exit_code,
        req.cmd_duration_ms,
        req.cols,
        req.jobs,
        &git_status,
        req.shell_integration.unwrap_or(true),
        req.env.as_ref(),
        plugin_segments,
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
        agent: detect_agent(req.env.as_ref()),
    });

    debug!("prompt rendered in {:?}", start.elapsed());

    let mut resp = serde_json::to_value(&prompt)?;
    if let Some(obj) = resp.as_object_mut() {
        obj.insert("type".into(), serde_json::json!("prompt"));
    }
    write_response(writer, resp, request_id).await
}

async fn handle_preview(
    req: &PreviewRequest,
    state: &Arc<DaemonState>,
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    request_id: Option<&str>,
) -> anyhow::Result<()> {
    let config_guard = state.config.read().await;
    // Tier C project profiles: the Studio/wizard preview honors the profile
    // of the previewed cwd, so what you see includes the repo's own
    // `.o10k.toml` patch. Per-request overrides (Quattro preset gallery,
    // configure wizard, Looks Studio) build the effective config as
    // base → Look → profile → patch (later wins), then style-knob overrides.
    let profile_patch = crate::profiles::profile_patch_for(
        Path::new(&req.cwd),
        &crate::profiles::home_dir(),
    );
    let preset_config;
    let look_config;
    let config: &Config =
        if req.look.is_some() || req.patch.is_some() || profile_patch.is_some() {
            // Unknown look names fall back to the base config (gallery
            // behavior); an unrepresentable patch is a preview error.
            match effective_preview_config(req, &config_guard, profile_patch.as_ref()) {
                Ok(c) => {
                    look_config = c;
                    &look_config
                }
                Err(e) => {
                    write_response(
                        writer,
                        serde_json::json!({"type": "preview", "status": "error", "error": e}),
                        request_id,
                    )
                    .await?;
                    return Ok(());
                }
            }
    } else if req.style_preset.is_some()
        || req.style_separators.is_some()
        || req.style_frame.is_some()
        || req.prompt_newline.is_some()
    {
        let mut c = config_guard.clone();
        if let Some(preset) = &req.style_preset {
            c.style.preset = preset.clone();
            c.prompt.layout = String::new();
        }
        if let Some(sep) = &req.style_separators {
            c.style.separators.left = Some(sep.clone());
            c.style.separators.right = Some(sep.clone());
        }
        if let Some(frame) = &req.style_frame {
            let (enabled, left, right) = match frame.as_str() {
                "left" => (true, Some(true), Some(false)),
                "right" => (true, Some(false), Some(true)),
                "full" => (true, Some(true), Some(true)),
                _ => (false, None, None),
            };
            c.style.frame.enabled = Some(enabled);
            c.style.frame.left = left;
            c.style.frame.right = right;
        }
        if let Some(newline) = req.prompt_newline {
            c.prompt.newline = newline;
        }
        preset_config = c;
        &preset_config
    } else {
        &config_guard
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
        Vec::new(),
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

/// Build the effective config for a preview render: base config, then the
/// requested Look (if any), then the cwd's project profile (Tier C; wins
/// over the Look), then the client's `config_set`-shaped `patch` (patch
/// wins, so Studio edits compose on top of a Look and profile). Style-knob
/// overrides (configure wizard) apply last. Reuses the transient-merge
/// machinery — no file writes, no daemon state mutation.
fn effective_preview_config(
    req: &PreviewRequest,
    current: &Config,
    profile: Option<&toml::Value>,
) -> Result<Config, String> {
    let mut effective = current.clone();
    if let Some(look_name) = &req.look {
        if let Some(l) = crate::looks::resolve(look_name, &effective) {
            effective = crate::looks::apply_transient(&effective, &l.patch)?;
        }
    }
    // Project profile (Tier C): wins over the Look, loses to the client
    // patch. A broken profile patch falls back to the pre-profile config.
    if let Some(profile) = profile {
        effective = crate::profiles::apply_profile(&effective, profile)
            .map_err(|e| format!("project profile merge failed: {e}"))?;
    }
    if let Some(patch) = &req.patch {
        effective = crate::looks::apply_transient(&effective, patch)?;
    }
    if req.style_preset.is_some()
        || req.style_separators.is_some()
        || req.style_frame.is_some()
        || req.prompt_newline.is_some()
    {
        if let Some(preset) = &req.style_preset {
            effective.style.preset = preset.clone();
            effective.prompt.layout = String::new();
        }
        if let Some(sep) = &req.style_separators {
            effective.style.separators.left = Some(sep.clone());
            effective.style.separators.right = Some(sep.clone());
        }
        if let Some(frame) = &req.style_frame {
            let (enabled, left, right) = match frame.as_str() {
                "left" => (true, Some(true), Some(false)),
                "right" => (true, Some(false), Some(true)),
                "full" => (true, Some(true), Some(true)),
                _ => (false, None, None),
            };
            effective.style.frame.enabled = Some(enabled);
            effective.style.frame.left = left;
            effective.style.frame.right = right;
        }
        if let Some(newline) = req.prompt_newline {
            effective.prompt.newline = newline;
        }
    }
    Ok(effective)
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
    // Cap the reader at MAX_FRAME_BYTES: a client streaming bytes without a
    // newline would otherwise grow `line` unboundedly and OOM the daemon.
    let mut reader = BufReader::new(reader.take(MAX_FRAME_BYTES as u64));
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        if line.len() >= MAX_FRAME_BYTES && !line.ends_with('\n') {
            warn!("oversized frame rejected (>= {MAX_FRAME_BYTES} bytes)");
            write_response(
                &mut writer,
                serde_json::json!({"type":"error","error": "frame too large"}),
                None,
            ).await?;
            break;
        }
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
                    handle_control(cmd, &msg.rest, &state, &mut writer, request_id).await?;
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

                            match write_config_patch(&state, &patch).await {
                                Ok(()) => {
                                    let touches_theme = patch.get("theme").is_some();
                                    if touches_theme {
                                        state.reload_theme().await;
                                    }
                                    write_response(&mut writer, serde_json::json!({
                                        "type": "config",
                                        "status": "ok",
                                    }), request_id).await?;
                                }
                                Err(e) => {
                                    write_response(&mut writer, serde_json::json!({
                                        "type": "config",
                                        "status": "error",
                                        "error": e,
                                    }), request_id).await?;
                                    line.clear();
                                    continue;
                                }
                            }
                        } else {
                            write_response(&mut writer, serde_json::json!({
                                "type": "error",
                                "error": "config set requires 'config' field",
                            }), request_id).await?;
                        }
                    } else {
                        handle_control(cmd, &msg.rest, &state, &mut writer, request_id).await?;
                    }
                } else {
                    handle_control("config_get", &msg.rest, &state, &mut writer, request_id).await?;
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
                    handle_control(cmd, &msg.rest, &state, &mut writer, request_id).await?;
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

/// Merge a `config_set`-shaped patch into the config file atomically
/// (tmp+rename) and reload the in-memory config. Single source of truth for
/// config_set and looks_apply.
async fn write_config_patch(
    state: &Arc<DaemonState>,
    patch: &serde_json::Value,
) -> Result<(), String> {
    let config_path = state.config_path.clone();
    let mut doc: toml::Table = match std::fs::read_to_string(&config_path) {
        Ok(existing) => match toml::from_str(&existing) {
            Ok(t) => t,
            Err(e) => return Err(format!("config.toml has syntax errors: {e}")),
        },
        Err(_) => toml::Table::new(),
    };

    let mut failed_keys: Vec<String> = Vec::new();
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            match serde_json::from_value::<toml::Value>(v.clone()) {
                Ok(toml_val) => {
                    merge_toml_value(
                        doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())),
                        toml_val,
                    );
                }
                Err(e) => failed_keys.push(format!("{k} ({e})")),
            }
        }
    }
    if !failed_keys.is_empty() {
        return Err(format!(
            "values for keys {} are not representable in TOML",
            failed_keys.join(", ")
        ));
    }

    persist_config_doc(state, doc).await
}

/// Validate a `looks_delete` target: curated Looks are compiled-in and
/// cannot be deleted, but a user entry shadowing a curated name is
/// deletable; anything else is unknown.
fn validate_look_deletion(name: &str, config: &Config) -> Result<(), String> {
    if config.looks.contains_key(name) {
        return Ok(());
    }
    if crate::looks::curated().iter().any(|l| l.name == name) {
        Err(format!("cannot delete curated look: {name}"))
    } else {
        Err(format!("unknown look: {name}"))
    }
}

/// Remove one user Look (`[looks.<name>]`) from config.toml by rewriting
/// the file without the entry (the merge in write_config_patch cannot
/// delete keys). Same atomic tmp+rename path, then reload.
async fn delete_user_look(state: &Arc<DaemonState>, name: &str) -> Result<(), String> {
    let config_path = state.config_path.clone();
    let mut doc: toml::Table = match std::fs::read_to_string(&config_path) {
        Ok(existing) => match toml::from_str(&existing) {
            Ok(t) => t,
            Err(e) => return Err(format!("config.toml has syntax errors: {e}")),
        },
        Err(_) => return Err(format!("unknown look: {name}")),
    };
    let removed = doc
        .get_mut("looks")
        .and_then(|v| v.as_table_mut())
        .map(|looks| looks.remove(name).is_some())
        .unwrap_or(false);
    if !removed {
        return Err(format!("unknown look: {name}"));
    }
    // Drop an emptied looks table so the file stays clean.
    let looks_empty = doc
        .get("looks")
        .and_then(|v| v.as_table())
        .map(|t| t.is_empty())
        .unwrap_or(false);
    if looks_empty {
        doc.remove("looks");
    }
    persist_config_doc(state, doc).await
}

/// Atomically persist a merged config doc (tmp+rename) and reload the
/// in-memory config. Shared by write_config_patch and delete_user_look.
async fn persist_config_doc(
    state: &Arc<DaemonState>,
    doc: toml::Table,
) -> Result<(), String> {
    let config_path = state.config_path.clone();
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let new_toml = toml::to_string_pretty(&doc).unwrap_or_default();
    let tmp_path = config_path.with_extension("toml.tmp");
    std::fs::write(&tmp_path, &new_toml)
        .and_then(|_| std::fs::rename(&tmp_path, &config_path))
        .map_err(|e| format!("failed to write config: {e}"))?;

    state.reload_config().await.map_err(|e| format!("reload failed: {e}"))?;
    Ok(())
}

pub(crate) fn merge_toml_value(target: &mut toml::Value, patch: toml::Value) {
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
            agent: None,
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
    fn test_detect_agent_claude_entrypoint() {
        let mut env = std::collections::HashMap::new();
        env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "cli".to_string());
        assert_eq!(detect_agent(Some(&env)).as_deref(), Some("claude"));
    }

    #[test]
    fn test_detect_agent_codex_vars() {
        let mut env = std::collections::HashMap::new();
        env.insert("CODEX_SANDBOX".to_string(), "1".to_string());
        assert_eq!(detect_agent(Some(&env)).as_deref(), Some("codex"));

        let mut env = std::collections::HashMap::new();
        env.insert("CODEX_HOME".to_string(), "/tmp/codex".to_string());
        assert_eq!(detect_agent(Some(&env)).as_deref(), Some("codex"));
    }

    #[test]
    fn test_detect_agent_none_without_env_keys() {
        let env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        assert_eq!(detect_agent(Some(&env)), None);
        assert_eq!(detect_agent(None), None);
    }

    #[test]
    fn test_detect_agent_claude_wins_over_codex() {
        let mut env = std::collections::HashMap::new();
        env.insert("CLAUDE_CODE_ENTRYPOINT".to_string(), "cli".to_string());
        env.insert("CODEX_SANDBOX".to_string(), "1".to_string());
        assert_eq!(detect_agent(Some(&env)).as_deref(), Some("claude"));
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
            agent: Some("claude".into()),
        };
        let v = serde_json::to_value(&last).unwrap();
        assert_eq!(v["cmd_duration_ms"], 42);
        assert_eq!(v["stale"], true);
        assert_eq!(v["agent"], "claude");
    }

    /// Minimal fully-specified PreviewRequest for override tests.
    fn preview_req(patch: Option<serde_json::Value>, look: Option<&str>) -> PreviewRequest {
        PreviewRequest {
            cwd: "~/projects/my-app".into(),
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_branch: String::new(),
            git_staged: 0,
            git_unstaged: 0,
            look: look.map(|s| s.to_string()),
            style_preset: None,
            style_separators: None,
            style_frame: None,
            prompt_newline: None,
            patch,
        }
    }

    /// Render the preview left line the way handle_preview does.
    fn render_preview_left(config: &Config) -> String {
        let palette = ThemePalette::default();
        let renderer = PromptRenderer::new(config, &palette);
        let git_status = crate::git::GitStatus {
            is_repo: false,
            branch: "main".into(),
            ..Default::default()
        };
        let prompt = renderer.render_with_ssh(
            "/home/u/project",
            0,
            0,
            120,
            0,
            &git_status,
            false,
            Some(false),
            None,
            Vec::new(),
        );
        strip_np(&prompt.left)
    }

    fn temp_config_dir(label: &str) -> std::path::PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "o10kd-server-test-{label}-{n}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn preview_patch_changes_render() {
        let base = render_preview_left(&Config::default());
        let req = preview_req(
            Some(serde_json::json!({"segments": {"character": {"success": "⇉"}}})),
            None,
        );
        let cfg =
            effective_preview_config(&req, &Config::default(), None).expect("patch applies");
        let patched = render_preview_left(&cfg);
        assert_ne!(base, patched, "patch must flip the rendered prompt");
        assert!(patched.contains("⇉"), "patched glyph must reach the output");
    }

    #[test]
    fn preview_patch_composes_over_look() {
        let look =
            crate::looks::resolve("gruvbox-drift", &Config::default()).expect("curated look");
        let req = preview_req(
            Some(serde_json::json!({"segments": {"character": {"success": "★"}}})),
            Some("gruvbox-drift"),
        );
        let cfg =
            effective_preview_config(&req, &Config::default(), None).expect("compose applies");
        assert_eq!(
            look.patch["style"]["preset"].as_str(),
            Some(cfg.style.preset.as_str()),
            "look must be applied under the patch"
        );
        assert_ne!(
            cfg.style.preset,
            Config::default().style.preset,
            "look actually changed the preset"
        );
        assert_eq!(cfg.segments.character.success, "★", "patch wins over look");
    }

    #[test]
    fn preview_invalid_patch_is_error() {
        // JSON null is not TOML-representable, so the merge must fail loudly.
        let req = preview_req(Some(serde_json::json!({"style": {"preset": null}})), None);
        assert!(effective_preview_config(&req, &Config::default(), None).is_err());
    }

    // ---- Tier C project profiles ----

    fn profile_patch(src: &str) -> toml::Value {
        toml::from_str(src).expect("profile patch parses")
    }

    #[test]
    fn profile_wins_over_look_loses_to_patch() {
        let look_preset = crate::looks::resolve("gruvbox-drift", &Config::default())
            .expect("curated look")
            .patch["style"]["preset"]
            .as_str()
            .expect("look sets a preset")
            .to_string();
        let profile = profile_patch(
            "[style]\npreset = \"classic\"\n\n[prompt]\nblank_line = false\n",
        );
        assert_ne!(look_preset, "classic", "sanity: look and profile differ");

        // base → look → profile: the profile wins over the Look.
        let req = preview_req(None, Some("gruvbox-drift"));
        let cfg = effective_preview_config(&req, &Config::default(), Some(&profile))
            .expect("merge ok");
        assert_eq!(cfg.style.preset, "classic", "profile beats look");
        assert!(!cfg.prompt.blank_line, "profile display keys apply");

        // base → look → profile → patch: the client patch wins over both.
        let req = preview_req(
            Some(serde_json::json!({"style": {"preset": "lean"}})),
            Some("gruvbox-drift"),
        );
        let cfg = effective_preview_config(&req, &Config::default(), Some(&profile))
            .expect("merge ok");
        assert_eq!(cfg.style.preset, "lean", "patch beats profile");
        assert!(
            !cfg.prompt.blank_line,
            "profile keys untouched by the patch survive"
        );
    }

    async fn read_response(
        reader: &mut (impl tokio::io::AsyncRead + Unpin),
    ) -> serde_json::Value {
        let mut line = String::new();
        tokio::io::BufReader::new(reader)
            .read_line(&mut line)
            .await
            .expect("response line");
        serde_json::from_str(line.trim()).expect("response JSON")
    }

    #[tokio::test]
    async fn preview_honors_profile_of_preview_cwd() {
        let dir = temp_config_dir("profile-preview");
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(".o10k.toml"), "[segments.character]\nsuccess = \"λ\"\n")
            .unwrap();

        let state = std::sync::Arc::new(DaemonState::new(
            Config::default(),
            ThemePalette::default(),
            dir.join("config.toml"),
            dir.join("d.sock"),
        ));
        let (client, server_sock) = tokio::net::UnixStream::pair().unwrap();
        let (mut reader, _client_writer) = client.into_split();
        let (_server_reader, mut writer) = server_sock.into_split();

        // No look, no patch — the profile of the previewed cwd alone must
        // drive the render.
        let mut req = preview_req(None, None);
        req.cwd = repo.display().to_string();
        handle_preview(&req, &state, &mut writer, None)
            .await
            .expect("preview ok");

        let resp = read_response(&mut reader).await;
        assert_eq!(resp["status"], "ok");
        let left = resp["left"].as_str().expect("left prompt");
        let base = render_preview_left(&Config::default());
        assert!(!base.contains('λ'), "baseline prompt has the default glyph");
        assert!(left.contains('λ'), "profile glyph must reach the preview: {left}");
    }

    #[tokio::test]
    async fn prompt_with_broken_profile_still_renders() {
        let dir = temp_config_dir("profile-broken");
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        // Malformed TOML: parse fails, warn-once, prompt renders with base.
        std::fs::write(repo.join(".o10k.toml"), "[style\npreset = \"lean\"\n").unwrap();

        let state = std::sync::Arc::new(DaemonState::new(
            Config::default(),
            ThemePalette::default(),
            dir.join("config.toml"),
            dir.join("d.sock"),
        ));
        let (client, server_sock) = tokio::net::UnixStream::pair().unwrap();
        let (mut reader, _client_writer) = client.into_split();
        let (_server_reader, mut writer) = server_sock.into_split();

        let req = PromptRequest {
            cwd: repo.display().to_string(),
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            command: None,
            shell_integration: Some(false),
            env: None,
        };
        handle_prompt(&req, &state, &mut writer, None)
            .await
            .expect("a broken repo profile must never fail the prompt");

        let resp = read_response(&mut reader).await;
        assert_eq!(resp["type"], "prompt");
        assert!(resp["left"].as_str().is_some(), "left prompt rendered");
    }

    #[tokio::test]
    async fn prompt_renders_with_project_profile() {
        let dir = temp_config_dir("profile-prompt");
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join(".o10k.toml"), "[segments.character]\nsuccess = \"λ\"\n")
            .unwrap();

        let state = std::sync::Arc::new(DaemonState::new(
            Config::default(),
            ThemePalette::default(),
            dir.join("config.toml"),
            dir.join("d.sock"),
        ));
        let (client, server_sock) = tokio::net::UnixStream::pair().unwrap();
        let (mut reader, _client_writer) = client.into_split();
        let (_server_reader, mut writer) = server_sock.into_split();

        let req = PromptRequest {
            cwd: repo.display().to_string(),
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            command: None,
            shell_integration: Some(false),
            env: None,
        };
        handle_prompt(&req, &state, &mut writer, None)
            .await
            .expect("prompt ok");

        let resp = read_response(&mut reader).await;
        assert_eq!(resp["type"], "prompt");
        let left = resp["left"].as_str().expect("left prompt");
        assert!(left.contains('λ'), "profile glyph must reach the prompt: {left}");
    }

    #[test]
    fn looks_delete_rejects_curated_and_unknown() {
        let cfg = Config::default();
        assert_eq!(
            validate_look_deletion("tokyo-rainbow", &cfg),
            Err("cannot delete curated look: tokyo-rainbow".to_string())
        );
        assert_eq!(
            validate_look_deletion("no-such-look", &cfg),
            Err("unknown look: no-such-look".to_string())
        );
    }

    #[test]
    fn looks_delete_allows_user_shadowing_curated_name() {
        let mut cfg = Config::default();
        cfg.looks
            .insert("tokyo-rainbow".into(), crate::config::LookEntry::default());
        assert_eq!(validate_look_deletion("tokyo-rainbow", &cfg), Ok(()));
    }

    #[tokio::test]
    async fn looks_delete_removes_user_entry_and_reloads() {
        let dir = temp_config_dir("delete");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[style]\npreset = \"classic\"\n\n[looks.mine]\nlabel = \"Mine\"\npalette = \"keep\"\n\n[looks.mine.patch.style]\npreset = \"lean\"\n",
        )
        .unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load config"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));

        delete_user_look(&state, "mine").await.expect("delete ok");

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("[looks.mine]"), "entry gone from file");
        assert!(on_disk.contains("[style]"), "unrelated sections survive");
        assert!(
            !state.config.read().await.looks.contains_key("mine"),
            "in-memory config reloaded without the entry"
        );
        assert_eq!(
            state.config.read().await.style.preset, "classic",
            "reload preserved the rest of the config"
        );
    }

    #[tokio::test]
    async fn looks_delete_unknown_reports_error() {
        let dir = temp_config_dir("delete-unknown");
        let path = dir.join("config.toml");
        std::fs::write(&path, "[style]\npreset = \"classic\"\n").unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load config"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));

        let err = delete_user_look(&state, "mine").await.unwrap_err();
        assert!(err.contains("unknown look"), "got: {err}");
    }
}
