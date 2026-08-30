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
    /// Studio terminal mock: render the SAME config against several different
    /// shell states in one request.
    ///
    /// A prompt has to survive a dirty repo, a failed command, an SSH host and
    /// a deep path, and a preview that shows only the happy case is not a
    /// preview. Six separate requests would be six round-trips and six
    /// cache entries in the Quattro preview broker, so they ride together.
    ///
    /// Omitting this field preserves the single-render response shape exactly,
    /// which is what the CLI and the bar panel still use.
    #[serde(default)]
    pub scenes: Option<Vec<PreviewScene>>,
}

/// One shell state to render the previewed config against.
///
/// Every field defaults, so a scene may specify only what it varies — a
/// caller asking for a failed command writes `{"exit_code": 127}` and inherits
/// the request's `cwd` and column count.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PreviewScene {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub cmd_duration_ms: Option<u64>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub jobs: Option<u32>,
    #[serde(default)]
    pub in_ssh: Option<bool>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_staged: Option<u32>,
    #[serde(default)]
    pub git_unstaged: Option<u32>,
    /// Free-form label echoed back so the UI can caption the row without
    /// having to re-derive what it asked for.
    #[serde(default)]
    pub label: Option<String>,
}

/// Cap on scenes per request. The terminal mock uses six; this only exists so
/// a malformed client cannot ask for ten thousand prompt renders on the
/// daemon's single request path.
const MAX_PREVIEW_SCENES: usize = 12;

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
        // Resolve the palette HERE rather than at each caller.
        //
        // The persistent `looks_apply` path forgot to: it wrote the new theme
        // to config.toml, reloaded the config, and left the daemon rendering
        // the PREVIOUS palette. The prompt showed the old colors, and in the
        // Studio every card whose theme now matched the live config took that
        // stale palette through `handle_preview`'s skip-the-resolve branch —
        // so applying a Look flipped the whole grid to the colors you had
        // before. The transient path, `config_set` and the file watcher each
        // remembered to call this; one caller forgetting is what made it a
        // bug, so no caller gets the choice any more.
        self.reload_theme().await;
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
                .map(|l| serde_json::json!({
                    "name": l.name,
                    "label": l.label,
                    // Without these the browser has no blurb to show and no
                    // tags to filter by -- the metadata exists on LookDef but
                    // was being dropped on the way out.
                    "blurb": l.blurb,
                    "tags": l.tags,
                    "patch": l.patch,
                }))
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
            write_response(writer, serde_json::json!({
                "type": "control",
                "status": "ok",
                "palettes": collect_palettes(),
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
                            match crate::looks::apply_look(&current, &l.patch) {
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
                            match write_look_patch(state, &l.patch).await {
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
                // An explicit `theme` object from the caller is folded into the
                // saved patch. Without it a Look could only ever capture the
                // daemon's CURRENT colours, so the Studio's palette editor --
                // the one place you can tune eleven roles by hand -- silently
                // discarded every edit on save.
                //
                // `palette` stays "keep" so the directive does not overwrite
                // the patch's own theme at resolve time.
                let mut entry_patch = entry_patch;
                if let Some(theme) = rest.get("theme").filter(|t| t.is_object()) {
                    if let Some(obj) = entry_patch.as_object_mut() {
                        obj.insert("theme".into(), theme.clone());
                    }
                }
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
    // The shell's terminal, like the main renderer -- not the daemon's own
    // environment. Plugin segments gate OSC 8 and undercurl on this too, so
    // leaving it as detect() meant built-in segments and plugin segments in
    // the SAME prompt could disagree about what the terminal supports.
    let term_caps =
        crate::terminal::TermCaps::for_kind(crate::terminal::kind_from_channel(req.env.as_ref()));
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
        if req.look.is_some()
            || req.patch.is_some()
            || profile_patch.is_some()
        {
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
    // The preview MUST resolve the palette from the effective config, not
    // from the daemon's live one.
    //
    // It used to always use `state.palette`, so a `complete` Look — one that
    // brings its own colors — previewed in whatever palette you happened to
    // be on and only revealed its real colors after Apply. That is the whole
    // "the preset I picked isn't what I got" complaint: the preview was
    // rendering a different palette than the thing it was previewing.
    //
    // Re-resolving costs a `colors.toml` read, so it is skipped when the
    // effective config's theme matches the live one — which is every
    // structure-only preview, and the Panel's hot path.
    let live_palette = state.palette.read().await;
    let previewed_palette;
    let palette: &crate::theme::ThemePalette = if config.theme == config_guard.theme {
        &live_palette
    } else {
        previewed_palette = crate::theme::ThemePalette::resolve_palette(config);
        &previewed_palette
    };

    // One renderer, reused across every scene: building the effective config
    // is the expensive half, and the whole point of `scenes` is to pay it
    // once. A single render is the same work the shell does on every prompt,
    // against a 5ms budget, so a six-scene mock stays well inside a frame.
    let renderer = PromptRenderer::new(config, &palette);

    let render_one = |scene: &PreviewScene| -> serde_json::Value {
        let cwd = scene.cwd.clone().unwrap_or_else(|| req.cwd.clone());
        let branch = scene.git_branch.clone().unwrap_or_else(|| req.git_branch.clone());
        let git_status = crate::git::GitStatus {
            is_repo: !branch.is_empty(),
            branch: if branch.is_empty() { "main".into() } else { branch },
            staged: scene.git_staged.unwrap_or(req.git_staged),
            unstaged: scene.git_unstaged.unwrap_or(req.git_unstaged),
            ..Default::default()
        };
        let prompt = renderer.render_with_ssh(
            &cwd,
            scene.exit_code.unwrap_or(req.exit_code),
            scene.cmd_duration_ms.unwrap_or(req.cmd_duration_ms),
            scene.cols.unwrap_or(req.cols),
            scene.jobs.unwrap_or(req.jobs),
            &git_status,
            false, // no shell integration for preview
            Some(scene.in_ssh.unwrap_or(req.in_ssh)),
            None,
            Vec::new(),
        );
        serde_json::json!({
            "label": scene.label,
            "left": strip_np(&prompt.left),
            "right": prompt.right.as_deref().map(strip_np),
        })
    };

    // The request's own fields ARE a scene — the single-render path is the
    // multi-scene path with one all-defaults entry, so there is only one
    // render code path to keep correct.
    let base_scene = PreviewScene {
        cwd: None,
        exit_code: None,
        cmd_duration_ms: None,
        cols: None,
        jobs: None,
        in_ssh: None,
        git_branch: None,
        git_staged: None,
        git_unstaged: None,
        label: None,
    };
    let base = render_one(&base_scene);

    let mut resp = serde_json::json!({
        "type": "preview",
        "status": "ok",
        // Always present, so every existing client — the CLI, the bar panel,
        // the configure wizard — is untouched by this addition.
        "left": base["left"].clone(),
        "right": base["right"].clone(),
    });

    if let Some(scenes) = &req.scenes {
        let renders: Vec<serde_json::Value> = scenes
            .iter()
            .take(MAX_PREVIEW_SCENES)
            .map(&render_one)
            .collect();
        resp["renders"] = serde_json::Value::Array(renders);
    }

    write_response(writer, resp, request_id).await
}

/// Every palette a surface can offer: the curated table first, then one
/// derived from each installed Omarchy theme that has no curated entry.
///
/// This is what closes the 8-palettes-for-22-themes gap. Entries carry flat
/// `colors` alongside the `theme` patch so a UI can draw swatches without
/// reconstructing them, and a `source` so it can say which are hand-tuned.
///
/// Reading the theme directory is I/O on a control-verb path, but `palettes`
/// is fetched once per connection and refreshed only when the config changes
/// — not on the prompt path.
/// Sample a palette's gradient ramp for the UI.
///
/// Deliberately built by running the palette's own theme patch through the
/// SAME resolution the prompt uses, rather than reimplementing the OKLCH
/// sweep in QML. A second implementation is a second thing that can disagree,
/// and the entire point of this daemon is that the preview and the prompt are
/// the same code.
fn ramp_stops(theme: &serde_json::Value, stops: usize) -> Vec<String> {
    let Ok(cfg) = crate::looks::apply_transient(
        &crate::config::Config::default(),
        &serde_json::json!({ "theme": theme }),
    ) else {
        return Vec::new();
    };
    let palette = crate::theme::ThemePalette::resolve_palette(&cfg);
    let last = stops.saturating_sub(1).max(1) as f32;
    (0..stops)
        .map(|i| palette.ramp_color(i as f32 / last).to_hex())
        .collect()
}

fn collect_palettes() -> Vec<serde_json::Value> {
    use crate::palette_derive;

    let entry = |key: &str,
                 label: &str,
                 blurb: &str,
                 source: &str,
                 colors: serde_json::Value,
                 theme: serde_json::Value,
                 low_contrast: bool| {
        serde_json::json!({
            "key": key,
            "label": label,
            "blurb": blurb,
            "source": source,
            "colors": colors,
            // Eight stops of the palette's own gradient, so the Theme tab can
            // show what a preset will actually sweep through.
            "ramp": ramp_stops(&theme, 8),
            "theme": theme,
            "low_contrast": low_contrast,
        })
    };

    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for p in crate::looks::curated_palettes() {
        let colors: serde_json::Map<String, serde_json::Value> = crate::looks::ROLE_ORDER
            .iter()
            .zip(p.colors.iter())
            .map(|(r, h)| (r.to_string(), serde_json::json!(h)))
            .collect();
        let theme = crate::looks::curated_palette(p.key)
            .map(|v| v["theme"].clone())
            .unwrap_or_else(|| serde_json::json!({}));
        seen.insert(p.key.to_string());
        out.push(entry(
            p.key,
            p.label,
            p.blurb,
            "curated",
            serde_json::Value::Object(colors),
            theme,
            false,
        ));
    }

    // Derived: one per installed theme without a curated palette of the same
    // name. Missing directory is not an error — a machine without Omarchy
    // installed simply gets the curated set.
    let themes_dir = std::path::Path::new("/usr/share/omarchy/themes");
    let Ok(entries) = std::fs::read_dir(themes_dir) else {
        return out;
    };
    let mut derived: Vec<serde_json::Value> = Vec::new();
    for e in entries.flatten() {
        let key = e.file_name().to_string_lossy().to_string();
        if seen.contains(&key) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path().join("colors.toml")) else {
            continue;
        };
        let (colors, mode) = palette_derive::parse_colors_toml(&text);
        let Some(p) = palette_derive::derive(&colors, &mode) else {
            continue;
        };
        let flat: serde_json::Map<String, serde_json::Value> = p
            .colors
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect();
        let label = title_case(&key);
        let blurb = if p.repaired.is_empty() {
            format!("Derived from the {label} theme.")
        } else {
            format!(
                "Derived from the {label} theme, with {} adjusted for contrast.",
                p.repaired.join(", ")
            )
        };
        derived.push(entry(
            &key,
            &label,
            &blurb,
            "derived",
            serde_json::Value::Object(flat),
            p.to_theme_patch()["theme"].clone(),
            !p.shortfall.is_empty(),
        ));
    }
    // read_dir order is arbitrary; a picker that reshuffles between opens is
    // disorienting.
    derived.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    out.extend(derived);
    out
}

/// `rose-pine` → `Rose Pine`. Theme directory names are the only label a
/// derived palette has.
fn title_case(key: &str) -> String {
    key.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
            // apply_look, not apply_transient: a card must show what pressing
            // Apply produces, and the only way to guarantee that is for both
            // to be the same function.
            effective = crate::looks::apply_look(&effective, &l.patch)?;
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
                                    // No reload_theme here: write_config_patch
                                    // persists and reloads, and reload_config
                                    // now resolves the palette itself. This
                                    // used to be the ONLY write path that
                                    // remembered to, which is exactly why the
                                    // Look-apply path could forget.
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
/// (tmp+rename) and reload the in-memory config.
///
/// `atomic` mirrors `looks::merge_patch`: it clears the Look-owned keys
/// before merging, so applying a Look writes the bundle it is presented as
/// rather than a delta over whatever was applied last.
///
/// The FILE needs exactly the same treatment as the in-memory config —
/// including the palette-replacement rule, which is why that rule lives in
/// `looks::clear_replaced_palette` and is called from here as well as from
/// the in-memory merge. Persisting reloads the config FROM the file, so any
/// rule the file skips is a rule that does not happen at all: the in-memory
/// result is discarded moments later.
async fn write_patch(
    state: &Arc<DaemonState>,
    patch: &serde_json::Value,
    atomic: bool,
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
    let mut patch_table = toml::Table::new();
    if let Some(obj) = patch.as_object() {
        for (k, v) in obj {
            match serde_json::from_value::<toml::Value>(v.clone()) {
                Ok(toml_val) => {
                    patch_table.insert(k.clone(), toml_val);
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

    // Atomic applies clear what a Look owns and honour the desktop lock.
    // Both rules come from `prepare_atomic_apply`, the same function the
    // in-memory merge uses: they lived only in that merge once, so the lock
    // held in a preview and evaporated the moment you pressed Apply.
    let patch_val = if atomic {
        crate::looks::prepare_atomic_apply(&mut doc, toml::Value::Table(patch_table))
    } else {
        toml::Value::Table(patch_table)
    };
    crate::looks::clear_replaced_palette(&mut doc, &patch_val);
    if let Some(obj) = patch_val.as_table() {
        for (k, v) in obj {
            merge_toml_value(
                doc.entry(k.clone()).or_insert(toml::Value::Table(toml::Table::new())),
                v.clone(),
            );
        }
    }

    persist_config_doc(state, doc).await
}

/// Persist a plain `config_set` patch as a DELTA — keys the patch omits keep
/// whatever the file already had. Used by config_set and by looks_save.
async fn write_config_patch(
    state: &Arc<DaemonState>,
    patch: &serde_json::Value,
) -> Result<(), String> {
    write_patch(state, patch, false).await
}

/// Persist a Look ATOMICALLY: clear the Look-owned keys from config.toml,
/// then merge the patch.
async fn write_look_patch(
    state: &Arc<DaemonState>,
    patch: &serde_json::Value,
) -> Result<(), String> {
    write_patch(state, patch, true).await
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
/// the file without the entry (the merge in write_patch cannot
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
/// in-memory config. Shared by write_patch and delete_user_look.
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
    pub(super) fn preview_req(patch: Option<serde_json::Value>, look: Option<&str>) -> PreviewRequest {
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
            scenes: None,
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

    // The in-memory clear (inside `apply_look`) and the on-disk clear (inside
    // `write_look_patch`, via `clear_look_owned`) are two separate code
    // paths. Only the on-disk one survives a daemon restart: if
    // `write_look_patch` ever stopped clearing the file before merging, a
    // stale Look-owned value would sit in config.toml forever and come back
    // on every reload, even though the in-memory config looked clean right
    // after Apply.
    #[tokio::test]
    async fn write_look_patch_clears_the_owned_keys_on_disk() {
        let dir = temp_config_dir("write-look-patch-clear");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[style.frame]\ngap_gradient = \"full\"\n\n[git]\nmode = \"always\"\n",
        )
        .unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load config"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));

        // lean-pure's patch touches `style.frame.enabled` but never mentions
        // `gap_gradient` -- exactly the kind of patch that used to leave a
        // stale value behind.
        let look = crate::looks::resolve("lean-pure", &Config::default()).expect("curated");
        write_look_patch(&state, &look.patch).await.expect("write ok");

        let on_disk = std::fs::read_to_string(&path).unwrap();
        let doc: toml::Table = toml::from_str(&on_disk).expect("valid toml");
        let gap_gradient = doc
            .get("style")
            .and_then(|v| v.get("frame"))
            .and_then(|v| v.get("gap_gradient"));
        assert!(
            gap_gradient.is_none(),
            "gap_gradient survived on disk: {gap_gradient:?}"
        );
        assert_eq!(
            doc.get("git").and_then(|v| v.get("mode")).and_then(|v| v.as_str()),
            Some("always"),
            "an unrelated, non-Look-owned key must survive the clear"
        );
    }

    // A palette is a WHOLE palette, on disk as much as in memory. Three
    // curated palettes ship an art-directed `ramp` (gruvbox, ayu-mirage,
    // cobalt2); every other Look derives its ramp from its colors and its
    // patch therefore never mentions `ramp` at all. The persistent path used
    // to skip the palette-replacement clear that the in-memory merge did, and
    // because persisting RELOADS the config from the file, the file's stale
    // ramp won: apply `gruvbox-drift` then `synthwave` and the card showed
    // magenta while the committed prompt kept Gruvbox's mustard, forever.
    #[tokio::test]
    async fn applying_a_look_persistently_replaces_the_palette_ramp() {
        let dir = temp_config_dir("look-palette-ramp");
        let path = dir.join("config.toml");
        // The state applying `gruvbox-drift` leaves behind.
        std::fs::write(
            &path,
            "[theme]\nsource = \"hybrid\"\nramp = [\"#d79921\", \"#b8bb26\"]\n\n\
             [theme.custom]\naccent = \"#d79921\"\n",
        )
        .unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load config"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));

        // Synthwave's palette derives its ramp; its patch sets `theme.custom`
        // but never `theme.ramp`.
        let look = crate::looks::resolve("synthwave", &Config::default()).expect("curated");
        assert!(
            look.patch.pointer("/theme/ramp").is_none(),
            "fixture assumption: synthwave must not carry its own ramp"
        );
        write_look_patch(&state, &look.patch).await.expect("write ok");

        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).expect("valid toml");
        let ramp = doc.get("theme").and_then(|v| v.get("ramp"));
        assert!(
            ramp.is_none(),
            "the previous palette's ramp survived on disk: {ramp:?}"
        );
        assert_eq!(
            doc.get("theme")
                .and_then(|v| v.get("custom"))
                .and_then(|v| v.get("accent"))
                .and_then(|v| v.as_str()),
            crate::looks::curated_palette("synthwave-alpha")
                .as_ref()
                .and_then(|p| p.pointer("/theme/custom/accent"))
                .and_then(|v| v.as_str()),
            "the incoming palette must have landed"
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

    // ── Preset cards must not contaminate each other ────────────────────
    //
    // Applying a Look used to rewrite how every OTHER card rendered. A Look
    // patch is a delta, and the gallery layered each card on the LIVE config,
    // so any key a patch did not mention was inherited from whatever had been
    // applied last. Measured before the fix: applying `synthwave` changed 27
    // of the other 28 cards; `framed-focus` changed 25.

    fn card_config(look: &str, base: &Config) -> Config {
        let req = preview_req(None, Some(look));
        effective_preview_config(&req, base, None).expect("card renders")
    }

    /// Render a config the way a card and an applied prompt both render.
    fn render_card_at(cfg: &Config, cols: u16) -> String {
        let palette = crate::theme::ThemePalette::resolve_palette(cfg);
        let renderer = PromptRenderer::new(cfg, &palette);
        let git_status = crate::git::GitStatus {
            is_repo: true,
            branch: "main".into(),
            staged: 2,
            unstaged: 1,
            ..Default::default()
        };
        strip_np(&renderer.render_with_ssh(
            "~/app", 0, 0, cols, 0, &git_status, false, Some(false), None, Vec::new(),
        ).left).to_string()
    }

    /// At the gallery card's own width.
    fn render_card_sized(cfg: &Config) -> String {
        render_card_at(cfg, 38)
    }

    #[test]
    fn a_look_looks_the_same_applied_as_it_did_in_the_gallery() {
        // WHAT THIS GUARDS, exactly: that the gallery card and the Apply
        // button run the SAME function. With no patch, no profile and no
        // style knobs, `effective_preview_config` reduces to
        // `apply_look(after_a, patch)` -- literally the other side of the
        // comparison -- so this is f(x) == f(x) and cannot detect a bug
        // INSIDE `apply_look`. Stubbing `clear_look_owned` to a no-op leaves
        // it passing. What it does catch is the card path drifting back to
        // `apply_transient`, which is the regression it was written for and
        // which does fail it. The invariant that atomicity actually holds
        // end-to-end is `..._through_the_persisted_path` below.
        //
        // Measured before the fix: 36 differing pairs out of the 812 the
        // then-29-Look library produced (29 x 28). The library is 52 Looks
        // now, so the sweep below runs 2652 pairs (52 x 51). The cause was
        // that the card was built with a delta apply while the actual Apply
        // was atomic. An earlier diagnostic reported 168 by comparing against
        // a `Config::default()` baseline instead of look A on both sides,
        // which also counted incidental `theme` differences rather than
        // isolating the structural leak (`gap_char`, `gap_gradient`,
        // `prompt.newline`, ...) this test targets.
        let base = Config::default();
        let names: Vec<String> =
            crate::looks::all(&base).iter().map(|d| d.name.clone()).collect();

        let mut mismatches: Vec<String> = Vec::new();
        for a in &names {
            let after_a = crate::looks::apply_look(
                &base, &crate::looks::resolve(a, &base).expect("curated").patch,
            ).expect("apply a");
            for b in &names {
                if a == b {
                    continue;
                }
                let card = {
                    let req = preview_req(None, Some(b));
                    effective_preview_config(&req, &after_a, None).expect("card")
                };
                let applied = crate::looks::apply_look(
                    &after_a, &crate::looks::resolve(b, &after_a).expect("curated").patch,
                ).expect("apply b");
                if render_card_sized(&card) != render_card_sized(&applied) {
                    mismatches.push(format!("{a} then {b}"));
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} of {} ordered pairs differ, e.g. {:?}",
            mismatches.len(),
            names.len() * (names.len() - 1),
            &mismatches[..mismatches.len().min(5)]
        );
    }

    #[tokio::test]
    async fn a_look_looks_the_same_applied_as_it_did_in_the_gallery_through_the_persisted_path() {
        // The invariant the sweep above only looks like it checks. Here the
        // "applied" side goes through the path that actually commits:
        // write_look_patch -> config.toml -> reload. That path reloads the
        // config FROM the file, so any clear the file skips is a clear that
        // did not happen -- which is how a stale `theme.ramp` survived every
        // subsequent Look while the card showed the new palette.
        //
        // Rendered at 80 columns, not the card's 38: `segments.os` has
        // `hide_below_cols: 40`, so a 38-column render never emits
        // `segments.os.icon` -- a LOOK_OWNED key, and one of the few whose
        // leak is visible only in the prompt.
        //
        // Every Look as B, against three fixed predecessors rather than all
        // 2652 ordered pairs: each iteration is two file writes and two
        // config reloads, and the full cross product through disk is far too
        // slow for a unit test. `gruvbox-drift` is mandatory -- it is the
        // only kind of predecessor (an art-directed `ramp`) that exposed the
        // persistence bug.
        let base = Config::default();
        let names: Vec<String> =
            crate::looks::all(&base).iter().map(|d| d.name.clone()).collect();
        let predecessors = ["gruvbox-drift", "synthwave", "lean-pure"];

        let mut mismatches: Vec<String> = Vec::new();
        let mut pairs = 0usize;
        for a in predecessors {
            for b in &names {
                if a == b {
                    continue;
                }
                pairs += 1;
                let dir = temp_config_dir(&format!("persisted-{a}-{b}"));
                let path = dir.join("config.toml");
                std::fs::write(&path, "").unwrap();
                let state = std::sync::Arc::new(DaemonState::new(
                    Config::load(&path).expect("load config"),
                    ThemePalette::default(),
                    path.clone(),
                    dir.join("d.sock"),
                ));

                // Get onto Look A the same way a user would.
                let look_a = crate::looks::resolve(a, &base).expect("curated a");
                write_look_patch(&state, &look_a.patch).await.expect("apply a");

                // B's gallery card, rendered on the live config.
                let card = {
                    let cfg = state.config.read().await;
                    let req = preview_req(None, Some(b));
                    effective_preview_config(&req, &cfg, None).expect("card")
                };

                // B applied for real, then read back out of the daemon.
                let look_b = {
                    let cfg = state.config.read().await;
                    crate::looks::resolve(b, &cfg).expect("curated b")
                };
                write_look_patch(&state, &look_b.patch).await.expect("apply b");
                let applied = state.config.read().await.clone();

                if render_card_at(&card, 80) != render_card_at(&applied, 80) {
                    mismatches.push(format!("{a} then {b} (render)"));
                } else if toml::Value::try_from(&card) != toml::Value::try_from(&applied) {
                    // The renders agreeing is necessary but not sufficient:
                    // a leaked `style.frame.gap_char` is invisible while the
                    // frame is off, and would come back the moment the user
                    // turned it on. Compare the configs too, so every
                    // LOOK_OWNED key counts whether or not it happens to
                    // reach the screen in this scene.
                    mismatches.push(format!("{a} then {b} (config)"));
                }
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        assert!(
            mismatches.is_empty(),
            "{} of {} persisted pairs differ from their card, e.g. {:?}",
            mismatches.len(),
            pairs,
            &mismatches[..mismatches.len().min(5)]
        );
    }

    #[test]
    fn the_leak_is_real_without_atomic_apply() {
        // Guards the guard. If delta apply ever stops leaking, the invariant
        // above has become vacuous and this fails to say so.
        let base = Config::default();
        let framed = crate::looks::apply_transient(
            &base, &crate::looks::resolve("framed-focus", &base).expect("curated").patch,
        ).expect("apply");
        let lean = crate::looks::apply_transient(
            &framed, &crate::looks::resolve("lean-pure", &base).expect("curated").patch,
        ).expect("apply");
        assert_eq!(
            lean.style.frame.gap_gradient.as_deref(),
            Some("off"),
            "delta apply no longer leaks; the invariant test may be vacuous"
        );
    }

    #[test]
    fn a_card_shows_the_looks_own_colors_not_the_ones_you_are_on() {
        // A Look is a complete definition, so its card must show ITS palette
        // regardless of what you are currently pinned to. This used to assert
        // the opposite -- that a `structure` Look kept your colors -- back
        // when a Look was a partial definition.
        let mut base = Config::default();
        base.theme.source = "hybrid".into();
        base.theme.custom = Some(crate::config::CustomPalette {
            accent: Some("#ff0000".into()),
            foreground: None, muted: None, background: None,
            red: None, green: None, yellow: None, blue: None,
            magenta: None, cyan: None, orange: None,
        });
        let card = card_config("dot-matrix", &base);
        let accent = card.theme.custom.as_ref().and_then(|c| c.accent.clone());
        assert_ne!(accent.as_deref(), Some("#ff0000"),
                   "the card inherited the palette you were on");
    }

    #[test]
    fn a_card_honours_the_desktop_lock() {
        // With colors locked to the desktop, a card must preview the Look's
        // shape WITHOUT its palette -- otherwise the card promises a color
        // change that applying will not make.
        let mut base = Config::default();
        base.theme.follow_desktop = true;
        let card = card_config("synthwave", &base);
        assert_eq!(card.theme.source, "omarchy", "card ignored the lock");
        assert!(card.theme.custom.is_none(), "card took the Look's palette while locked");
        assert_eq!(card.style.frame.enabled, Some(true),
                   "the lock swallowed the Look's structure in the card too");
    }

    #[test]
    fn a_complete_look_card_brings_its_own_palette() {
        let mut base = Config::default();
        base.theme.source = "hybrid".into();
        base.theme.custom = Some(crate::config::CustomPalette {
            accent: Some("#ff0000".into()),
            foreground: None, muted: None, background: None,
            red: None, green: None, yellow: None, blue: None,
            magenta: None, cyan: None, orange: None,
        });
        let card = card_config("synthwave", &base);
        let accent = card.theme.custom.as_ref().and_then(|c| c.accent.clone());
        assert_ne!(accent.as_deref(), Some("#ff0000"), "complete card kept the old accent");
    }

    #[test]
    fn switching_palettes_does_not_blend_them() {
        // Gruvbox ships an art-directed `ramp`; a palette that derives its own
        // must not inherit it.
        let base = Config::default();
        let gruvbox = crate::looks::apply_transient(
            &base,
            &crate::looks::curated_palette("gruvbox").expect("curated"),
        )
        .expect("apply");
        assert!(gruvbox.theme.ramp.is_some(), "gruvbox should ship a ramp");

        let then_tokyo = crate::looks::apply_transient(
            &gruvbox,
            &crate::looks::curated_palette("tokyo-night").expect("curated"),
        )
        .expect("apply");
        assert!(
            then_tokyo.theme.ramp.is_none(),
            "tokyo-night inherited gruvbox's ramp: {:?}",
            then_tokyo.theme.ramp
        );
    }

    #[test]
    fn every_palette_publishes_a_ramp_in_its_own_hue_family() {
        for p in collect_palettes() {
            let ramp: Vec<String> = p["ramp"]
                .as_array()
                .expect("ramp present")
                .iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect();
            assert_eq!(ramp.len(), 8, "{}: wrong stop count", p["key"]);
            for hex in &ramp {
                assert!(
                    hex.len() == 7 && hex.starts_with('#'),
                    "{}: bad stop {hex}",
                    p["key"]
                );
            }
            // The published strip must agree with what the prompt renders,
            // which is the only reason it is computed daemon-side.
            let first = crate::palette_derive::srgb_to_oklch(&ramp[0]).expect("hex");
            let last = crate::palette_derive::srgb_to_oklch(&ramp[7]).expect("hex");
            // Explicit ramps are art direction and may cross families; the
            // derived ones may not.
            let curated_with_ramp = crate::looks::curated_palettes()
                .iter()
                .any(|c| c.key == p["key"].as_str().unwrap_or_default() && c.ramp.is_some());
            if curated_with_ramp || first.c < 0.02 {
                continue;
            }
            let d = {
                let raw = (first.h - last.h).abs() % 360.0;
                if raw > 180.0 { 360.0 - raw } else { raw }
            };
            assert!(d <= 45.0, "{}: published ramp swings {d:.0}deg", p["key"]);
        }
    }

    #[tokio::test]
    async fn applying_a_look_refreshes_the_palette_not_just_the_config() {
        // The Apply button's path. It wrote the new theme to config.toml and
        // left `state.palette` on the PREVIOUS palette, so the prompt kept
        // rendering the old colors and every Studio card whose theme now
        // matched the live config inherited that staleness through
        // `handle_preview`'s skip-the-resolve branch.
        let dir = temp_config_dir("apply-refreshes-palette");
        let path = dir.join("config.toml");
        // Start on Gruvbox's accent, pinned so nothing on the host machine
        // can change what this test starts from.
        std::fs::write(
            &path,
            "[theme]\nsource = \"custom\"\n\n[theme.custom]\naccent = \"#83a598\"\n",
        )
        .unwrap();
        let config = Config::load(&path).expect("load config");
        let palette = ThemePalette::resolve_palette(&config);
        assert_eq!(palette.accent, crate::theme::AnsiColor::from_hex("#83a598").unwrap());

        let state = std::sync::Arc::new(DaemonState::new(
            config,
            palette,
            path.clone(),
            dir.join("d.sock"),
        ));

        let look = crate::looks::resolve("synthwave", &Config::default()).expect("curated");
        write_look_patch(&state, &look.patch).await.expect("write ok");

        let want = state.config.read().await.theme.clone();
        let got = state.palette.read().await;
        let expected = ThemePalette::resolve_palette(&Config {
            theme: want,
            ..Config::default()
        });
        assert_eq!(
            got.accent, expected.accent,
            "the daemon kept the previous palette after applying a Look"
        );
        assert_ne!(
            got.accent,
            crate::theme::AnsiColor::from_hex("#83a598").unwrap(),
            "palette did not move off the starting accent at all"
        );
    }

    #[tokio::test]
    async fn the_desktop_lock_survives_a_persisted_apply() {
        // The lock was honoured by the in-memory merge and ignored by the
        // on-disk write, so it held in a preview and evaporated the moment
        // you pressed Apply. Both paths now share prepare_atomic_apply.
        let dir = temp_config_dir("lock-persisted");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[theme]\nsource = \"omarchy\"\nfollow_desktop = true\n",
        )
        .unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));

        // synthwave is a Look that carries a palette.
        let look = crate::looks::resolve("synthwave", &Config::default()).expect("curated");
        write_look_patch(&state, &look.patch).await.expect("write ok");

        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).expect("valid toml");
        let theme = doc.get("theme").and_then(|t| t.as_table()).expect("theme table");
        assert_eq!(
            theme.get("follow_desktop").and_then(|v| v.as_bool()),
            Some(true),
            "the Look cleared the lock on disk"
        );
        assert!(
            theme.get("custom").is_none(),
            "the Look wrote its palette to disk despite the lock: {:?}",
            theme.get("custom")
        );
        assert_eq!(
            theme.get("source").and_then(|v| v.as_str()),
            Some("omarchy"),
            "the Look unbound colors from the desktop"
        );
        // ...but its SHAPE must still have landed.
        let frame = doc
            .get("style")
            .and_then(|v| v.get("frame"))
            .and_then(|v| v.get("enabled"))
            .and_then(|v| v.as_bool());
        assert_eq!(frame, Some(true), "the lock swallowed the Look's structure");
    }

    #[tokio::test]
    async fn the_lock_holds_when_a_palette_is_already_on_disk() {
        // The live repro: the lock is switched on while a previous Look's
        // palette is still in config.toml. The earlier lock test started
        // from a file with no [theme.custom] at all, which is not the state
        // a real user is ever in.
        let dir = temp_config_dir("lock-with-existing-palette");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[theme]\nsource = \"omarchy\"\nfollow_desktop = true\n\n             [theme.custom]\naccent = \"#76ff9f\"\n",
        )
        .unwrap();
        let state = std::sync::Arc::new(DaemonState::new(
            Config::load(&path).expect("load"),
            ThemePalette::default(),
            path.clone(),
            dir.join("d.sock"),
        ));
        let look = crate::looks::resolve("synthwave", &Config::default()).expect("curated");
        write_look_patch(&state, &look.patch).await.expect("write ok");

        let doc: toml::Table =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).expect("toml");
        let accent = doc
            .get("theme")
            .and_then(|t| t.get("custom"))
            .and_then(|c| c.get("accent"))
            .and_then(|v| v.as_str());
        assert_eq!(accent, Some("#76ff9f"), "the Look overwrote the locked palette");
    }
}

#[cfg(test)]
mod preview_scene_tests {
    use super::tests::preview_req;
    use super::*;

    fn scene(label: &str) -> PreviewScene {
        PreviewScene {
            cwd: None,
            exit_code: None,
            cmd_duration_ms: None,
            cols: None,
            jobs: None,
            in_ssh: None,
            git_branch: None,
            git_staged: None,
            git_unstaged: None,
            label: Some(label.into()),
        }
    }

    #[test]
    fn a_request_without_scenes_deserializes_with_none() {
        // The compatibility guarantee, from the wire in: every existing
        // client omits this field entirely.
        let req: PreviewRequest =
            serde_json::from_value(serde_json::json!({ "cwd": "~/x" })).unwrap();
        assert!(req.scenes.is_none());
        assert_eq!(req.cwd, "~/x");
    }

    #[test]
    fn a_scene_may_specify_only_what_it_varies() {
        let req: PreviewRequest = serde_json::from_value(serde_json::json!({
            "scenes": [{ "exit_code": 127 }, { "in_ssh": true, "label": "remote" }]
        }))
        .unwrap();
        let scenes = req.scenes.expect("scenes parsed");
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].exit_code, Some(127));
        // Unspecified fields stay None so the request's values show through.
        assert!(scenes[0].cwd.is_none());
        assert!(scenes[0].label.is_none());
        assert_eq!(scenes[1].label.as_deref(), Some("remote"));
    }

    #[test]
    fn scene_fields_override_the_request_and_fall_back_to_it() {
        let mut s = scene("override");
        s.cwd = Some("~/elsewhere".into());
        s.git_staged = Some(3);

        let req = preview_req(None, None);
        // The resolution rules handle_preview applies, asserted directly so a
        // change to them cannot pass unnoticed.
        assert_eq!(
            s.cwd.clone().unwrap_or_else(|| req.cwd.clone()),
            "~/elsewhere"
        );
        assert_eq!(s.git_staged.unwrap_or(req.git_staged), 3);
        assert_eq!(s.cols.unwrap_or(req.cols), 120, "unset falls back");
        assert_eq!(s.exit_code.unwrap_or(req.exit_code), 0);
    }

    #[test]
    fn the_scene_cap_is_enforceable() {
        let scenes: Vec<PreviewScene> = (0..50).map(|i| scene(&i.to_string())).collect();
        assert_eq!(scenes.iter().take(MAX_PREVIEW_SCENES).count(), MAX_PREVIEW_SCENES);
    }
}

#[cfg(test)]
mod palette_catalog_tests {
    use super::*;

    #[test]
    fn title_cases_theme_directory_names() {
        assert_eq!(title_case("rose-pine"), "Rose Pine");
        assert_eq!(title_case("tokyo_night"), "Tokyo Night");
        assert_eq!(title_case("gruvbox"), "Gruvbox");
        assert_eq!(title_case(""), "");
    }

    #[test]
    fn the_catalog_carries_every_curated_palette_with_swatches() {
        let all = collect_palettes();
        assert!(all.len() >= 16, "expected at least the curated set");

        for p in &all {
            assert!(!p["key"].as_str().unwrap().is_empty());
            assert!(!p["label"].as_str().unwrap().is_empty());
            assert!(!p["blurb"].as_str().unwrap().is_empty());
            // Swatches must be drawable without reconstructing them from the
            // theme patch — that reconstruction is what the UI should not do.
            let colors = p["colors"].as_object().expect("flat colors present");
            for role in ["background", "foreground", "accent"] {
                assert!(
                    colors.contains_key(role),
                    "{} is missing role {role}",
                    p["key"]
                );
            }
            assert!(p["theme"]["custom"].is_object(), "{} has no theme patch", p["key"]);
        }
    }

    #[test]
    fn palette_keys_are_unique_across_curated_and_derived() {
        let all = collect_palettes();
        let mut keys: Vec<&str> = all.iter().map(|p| p["key"].as_str().unwrap()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(before, keys.len(), "a derived palette shadowed a curated key");
    }

    #[test]
    fn curated_palettes_come_first_and_are_labelled_as_such() {
        let all = collect_palettes();
        let first_derived = all.iter().position(|p| p["source"] == "derived");
        if let Some(i) = first_derived {
            assert!(
                all[..i].iter().all(|p| p["source"] == "curated"),
                "curated entries must lead the list"
            );
            assert!(
                all[i..].iter().all(|p| p["source"] == "derived"),
                "the two sources must not interleave"
            );
        }
    }

    #[test]
    fn derived_palettes_cover_the_themes_curation_missed() {
        if !std::path::Path::new("/usr/share/omarchy/themes").is_dir() {
            eprintln!("skipping: Omarchy themes not installed");
            return;
        }
        let all = collect_palettes();
        let derived: Vec<&str> = all
            .iter()
            .filter(|p| p["source"] == "derived")
            .map(|p| p["key"].as_str().unwrap())
            .collect();
        assert!(
            !derived.is_empty(),
            "22 installed themes and 16 curated palettes should leave some to derive"
        );
        // Sorted, so a picker does not reshuffle between opens.
        let mut sorted = derived.clone();
        sorted.sort_unstable();
        assert_eq!(derived, sorted, "derived palettes must be in a stable order");
    }
}

#[cfg(test)]
mod terminal_channel_tests {
    use super::*;
    use crate::terminal::TerminalKind;

    fn env(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_shells_answer_wins() {
        // The shell probed; the daemon cannot. This is the whole point of
        // sending it over the channel.
        let e = env(&[("O10K_TERM", "foot")]);
        assert_eq!(crate::terminal::kind_from_channel(Some(&e)), TerminalKind::Foot);
    }

    #[test]
    fn every_known_terminal_round_trips() {
        for (name, want) in [
            ("ghostty", TerminalKind::Ghostty),
            ("foot", TerminalKind::Foot),
            ("kitty", TerminalKind::Kitty),
            ("wezterm", TerminalKind::WezTerm),
            ("alacritty", TerminalKind::Alacritty),
        ] {
            let e = env(&[("O10K_TERM", name)]);
            assert_eq!(crate::terminal::kind_from_channel(Some(&e)), want, "for {name}");
        }
    }

    #[test]
    fn case_and_padding_do_not_matter() {
        let e = env(&[("O10K_TERM", "  Ghostty ")]);
        assert_eq!(crate::terminal::kind_from_channel(Some(&e)), TerminalKind::Ghostty);
    }

    #[test]
    fn an_honest_unknown_is_carried_through() {
        // The shell saying "I could not identify this" must NOT fall back to
        // the daemon's own guess -- that guess is exactly what is untrusted.
        let e = env(&[("O10K_TERM", "unknown")]);
        assert_eq!(crate::terminal::kind_from_channel(Some(&e)), TerminalKind::Unknown);
    }

    #[test]
    fn a_missing_channel_value_falls_back_without_panicking() {
        let e = env(&[("VIRTUAL_ENV", "/x")]);
        let _ = crate::terminal::kind_from_channel(Some(&e));
        let _ = crate::terminal::kind_from_channel(None);
    }
}
