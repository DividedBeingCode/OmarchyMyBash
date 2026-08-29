# Daemon Reference (`omarchy10kd`)

[← Index](INDEX.md) | [Architecture](architecture.md) | [Protocol](protocol.md)

The daemon is the computational core of Omarchy10k. It holds all state, renders all prompts, and serves multiple concurrent connections over a Unix domain socket. One daemon instance runs per Bash session.

## Binary Entry Point (`src/main.rs`)

Bootstraps everything: config loading, tracing initialization, theme palette, shared state, filesystem watchers, parent-process monitor, and the socket server.

Module tree (declaration order in `main.rs`): `config`, `git`, `looks`, `plugins`, `profiles`, `layout`, `render`, `segments`, `script_exec`, `server`, `style`, `theme`, `terminal`.

### Startup Sequence

```
1. Load Config from ~/.config/omarchy10k/config.toml (or defaults on failure — the error is logged via `tracing::error!` after subscriber init, so it reaches the daemon log)
2. Init tracing subscriber (env-filter from config.daemon.log_level)
3. Load ThemePalette via `ThemePalette::resolve_palette(&config)` (unified path for all source modes)
4. Build DaemonState (Arc): config, palette, git cache (TTL from `config.git.cache_ttl_ms`), config path
5. Compute socket path: $XDG_RUNTIME_DIR/omarchy10k-{O10K_PARENT_PID}.sock
6. Spawn watcher thread for config + theme file changes
7. Spawn parent-process monitor (kill(ppid, 0) every 2s)
8. run_server(socket_path, state) — blocks on accept loop
```

### Parent Process Monitor

```rust
async fn monitor_parent() {
    // Reads O10K_PARENT_PID env var
    // Every 2 seconds: kill(ppid, 0)
    // If parent is gone: remove socket, exit(0)
}
```

On Linux the daemon additionally calls `prctl(PR_SET_PDEATHSIG, SIGTERM)` at startup (`libc` shim in `main.rs`): the kernel delivers SIGTERM the instant the parent shell dies, closing the PID-recycling race that the `kill(ppid, 0)` poll cannot close (a recycled PID would keep the poll alive indefinitely). The 2-second poll remains as a fallback for non-Linux builds and for a parent that died before the prctl call ran.


### Signal Handlers

A second spawned task installs tokio handlers for SIGTERM, SIGHUP, and SIGINT.
On the first signal it removes the socket file and exits. This matters because
closing the terminal delivers SIGHUP and service stops deliver SIGTERM — both
default-terminate the daemon before the parent monitor's next 2-second tick,
which used to leave a stale socket behind that every client then listed as a
live session. SIGKILL still orphans sockets; discovery-side liveness probes
cover that case.

Uses an inline `extern "C" { fn kill(...) }` declaration instead of depending on the `libc` crate — keeps the dependency tree minimal.

### Filesystem Watchers

Uses `notify` crate (v8) with a raw `mpsc` channel in `spawn_blocking`:

- **Config watcher**: monitors `config.toml` → calls `state.reload_config()`
- **Theme watcher**: monitors `colors.toml` → calls `state.reload_theme()`

The `notify-debouncer-full` crate is declared as a dependency but not imported or used. Debouncing happens implicitly through the watcher's event batching.

## Server (`src/server.rs`)

```rust
pub const PROTOCOL_VERSION: &str = "0.5";
```

### TypedMessage

Incoming JSON is parsed into a typed envelope before dispatch:

```rust
struct TypedMessage {
    r#type: Option<String>,   // "hello", "control", "prompt", "preview", "config"
    id: Option<String>,       // client request ID for correlation
    command: Option<String>,  // control/config subcommand
    version: Option<String>,  // reserved
    #[serde(flatten)]
    rest: serde_json::Value,  // remaining fields (e.g. prompt/preview payload)
}
```

### DaemonState

Central shared state, wrapped in `Arc` for concurrent access:

```rust
pub struct DaemonState {
    pub config: RwLock<Config>,
    pub palette: RwLock<ThemePalette>,
    pub git_cache: GitCache,
    pub plugins: RwLock<Vec<Plugin>>,           // plugin registry; rebuilt on startup + every reload_config
    pub plugin_cache: PluginCache,              // TTL cache for command-tier plugin segments (never blocks render)
    pub config_path: PathBuf,
    pub socket_path: PathBuf,
    pub last_render: RwLock<Option<RenderSummary>>,  // cwd/branch/dirty/counts/duration/exit/agent of the last prompt render (status enrichment)
    pub started_at: std::time::Instant,              // feeds status.session_age_secs
}
```

`DaemonState::new` reads `git_ttl_ms` from `config.git.cache_ttl_ms` and passes it to `GitCache::new(ttl_ms)`, and loads the plugin registry from `<config dir>/plugins/` (`plugins::load_plugins`). The `socket_path` is used by the shutdown handler to clean up the socket file on exit.

`reload_theme` calls `ThemePalette::resolve_palette(&config)`, which respects `config.theme.source` and custom overrides — the same unified path used at startup.

`RwLock` allows concurrent read access from multiple connections with exclusive write access during reloads.

### Connection Handling

```
run_server(socket_path, state):
    if socket file exists → clear_stale_socket(): unlink only if it is a socket owned by the current uid AND a connect probe fails (stale); a live listener or a foreign/non-socket file aborts startup with an error instead of hijacking
    bind UnixListener at socket_path
    chmod socket file to 0o600 (PermissionsExt)
    loop:
        accept connection → spawn handle_connection task

handle_connection(stream, state):
    reader capped at MAX_FRAME_BYTES (64 KiB, AsyncReadExt::take) — OOM guard
    loop over lines:
        line ≥ 64 KiB without newline → {"type":"error","error":"frame too large"}, close connection
        invalid JSON → {"type":"error","error":<serde msg>}, continue
    parse line as TypedMessage
    match msg.type:
        "hello"     → return protocol_version + server_version
        "control"   → handle_control(msg.command)
        "prompt"    → deserialize msg.rest as PromptRequest → render
        "preview"   → deserialize msg.rest as PreviewRequest → render (no OSC)
        "config"    → config_get or config_set (typed)
        None        → backward compat: command field → control, else PromptRequest
        other       → error response
    write_response(resp, request_id)  // echoes id when present
```

`write_response` serializes a JSON value, injects the request `id` when provided, and writes the line with trailing newline. This enables request/response correlation for clients that send an `id` field.

Connections are persistent — the server reads lines in a loop until EOF. This allows the Bash adapter to reuse connections if it wants (though currently it connects per-request).

### Message Types

| Type | Purpose |
|------|---------|
| `hello` | Handshake — returns `protocol_version` and `server_version` |
| `control` | Daemon control commands (see below) |
| `prompt` | Prompt render request (fields in flattened `rest`) |
| `preview` | Simulated prompt render for Quattro live preview (no git subprocess, no OSC 133) |
| `config` | Config read/write API |
| `statusline` | Claude Code statusLine render (no OSC 133, left-only) |
| *(untagged)* | Backward-compatible: legacy `{"command":"..."}` or bare prompt JSON |

### Control Commands

| Command | Handler | Effect |
|---------|---------|--------|
| `reload_config` | `state.reload_config()` | Re-reads TOML from disk, updates `RwLock<Config>` |
| `reload_theme` | `state.reload_theme()` | Calls `ThemePalette::resolve_palette(&config)`, updates `RwLock<ThemePalette>` |
| `invalidate_git` | `state.git_cache.invalidate_all()` | Clears all cached git statuses |
| `shutdown` | Responds `{"status":"bye"}`, removes socket file via `remove_socket_file()` (only if still a socket owned by the current uid), calls `exit(0)` | Clean shutdown with ownership-checked socket cleanup |
| `status` | Reads process info + last render summary + live git cache + battery sysfs | Returns `status`, `pid`, `version`, `protocol_version`, `cwd`, plus enrichment: `git` object, `last_cmd_duration_ms`, `last_exit_code`, `session_age_secs`, `battery` |
| `palette` | Reads in-memory palette | Returns theme colors as hex (`accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`) |
| `config_get` | Serializes in-memory config | Returns full config as JSON (requires `Serialize` on Config) |
| `config_set` | *(via typed `config` message)* | Accepts JSON patch in `rest.config`, recursively merges into TOML on disk (atomic write via tmp+rename), creates file/dirs if missing, returns structured error on TOML parse failure, auto-calls `reload_theme()` when patch touches `[theme]` |
| `looks` | `looks::all(&config)` | Returns curated + user Looks as `{name, label, patch}` entries; user entries shadow curated names |
| `looks_apply` | `looks::resolve` → `apply_transient` or `write_config_patch` | Applies a named Look; `transient: true` = in-memory only (revert via `reload_config`), default = atomic disk merge |
| `looks_save` | snapshot of in-memory config → `write_config_patch` | Writes a `[looks.<name>]` entry (style/glyph/prompt.blank_line snapshot, `palette: "keep"`) |
| `looks_delete` | `delete_user_look` | Deletes a USER look by rewriting `config.toml` without the `[looks.<name>]` entry (the merge path cannot delete keys), atomically (tmp+rename), then reloads. Curated Looks are refused (`cannot delete curated look`); a user look shadowing a curated name deletes only the override. Added in v0.4.1 (Studio). |
| `palettes` | `looks::curated_palette` | Returns the 8 curated palette keys with their `[theme]` patches |
| `defaults` | `Config::default()` | Returns the full default config as JSON (modified-vs-default comparison) |
| `script_list` | `script_exec::list_scripts` | Lists executable scripts in `~/.config/omarchy10k/scripts` (traversal-guarded names) |
| `script_run` | `script_exec::resolve_script` + `run_script` | Executes a named script with a hard timeout (default 30s, `timeout_secs` override) and output capture |

### Hardening (v0.5)

- **64 KiB frame cap** — `const MAX_FRAME_BYTES: usize = 64 * 1024` in `server.rs`. The socket reader is capped via `AsyncReadExt::take(MAX_FRAME_BYTES)`; a line reaching the cap without a newline is answered with `{"type":"error","error":"frame too large"}` and the connection is closed. Prevents a client from growing daemon memory unboundedly (OOM guard).
- **GitCache size bound** — `git.rs` `schedule_refresh` bounds the repo→status map at `MAX_CACHE_ENTRIES` (256). When the map overflows: expired entries (older than the TTL) are dropped first, then least-recently-fetched entries are evicted until under the cap. Weeks-long sessions visiting hundreds of repos no longer grow the cache unbounded.
- **Sibling-directory cache size bound** — `segments/directory.rs` bounds its cwd-keyed cache at `MAX_SIBLING_ENTRIES` (512) with the same expired-then-least-recently-stamped eviction policy (see [Wave 1 Internals](#wave-1-internals)).
- **`PR_SET_PDEATHSIG`** — kernel-enforced parent death signal (see [Parent Process Monitor](#parent-process-monitor)).
- **`write_config_patch` errors name the cause** — unrepresentable patch values list each failing key with its serde error; the shared patch path serves both `config_set` and `looks_apply`.

### Hello Response

```json
{"type":"hello","status":"ok","protocol_version":"0.5","server_version":"0.4.0"}
```

### Preview Request / Response

Quattro sends a typed `preview` message with optional simulated context:

```json
{"type":"preview","id":"1","cwd":"~/projects/my-app","exit_code":0,"cols":120,"git_branch":"main","git_staged":2,"git_unstaged":0,"in_ssh":false}
```

Response omits OSC 133 markers and uses a synthetic `GitStatus` (no subprocess):

```json
{"type":"preview","status":"ok","left":"<ansi prompt>","right":null,"id":"1"}
```


v0.5 adds a `look` field to the preview request (plus `style_separators`, `style_frame`, `prompt_newline` for the configure wizard). With `look`, the daemon resolves the named Look (user entries shadow curated ones), applies its patch via the transient in-memory merge, and renders that — a dry run; nothing is persisted. An unknown look name falls back to the current config.

Default preview cwd is `~/projects/my-app`; default cols is 120.

### Palette Response

```json
{"type":"control","status":"ok","palette":{"accent":"#7aa2f7","foreground":"#c0caf5","muted":"#565f89","background":"#1a1b26","red":"#f7768e","green":"#9ece6a","yellow":"#e0af68","blue":"#7aa2f7"}}
```

## Style (`src/style.rs`)

Added in v0.3. The style module provides a curated preset system, glyph catalogs, and style resolution that replaces the simpler `LayoutPreset` approach.

### StyleResolver

```rust
pub struct StyleResolver;

impl StyleResolver {
    pub fn resolve(config: &Config) -> ResolvedStyle;
    fn effective_preset(config: &Config) -> String;
}
```

Resolution pipeline:
1. **Determine effective preset** — if `style.preset` is explicitly set (not `"omarchy"`), use it. Otherwise fall back to `prompt.layout` for backward compatibility.
2. **Load preset defaults** — each preset defines separator glyphs, frame mode, gap char, segment ordering, and single-line override.
3. **Apply config overrides** — `style.separators`, `style.frame`, and `style.caps` can override any preset aspect independently. `Option<>` wrapping means unset values inherit from the preset.

### ResolvedStyle

```rust
pub struct ResolvedStyle {
    pub left_separator: String,
    pub right_separator: String,
    pub frame: FrameStyle,
    pub gap_char: Option<char>,
    pub left_cap_start: String,
    pub left_cap_end: String,
    pub right_cap_start: String,
    pub right_cap_end: String,
    pub segment_order: &'static [&'static str],
    pub force_single_line: bool,
}
```

### GlyphCatalog

Static lookup tables for configurable glyphs. All methods accept a key string and return the corresponding glyph, or the key itself as a custom fallback:

| Method | Options |
|--------|---------|
| `os_icon(key)` | 16 distro glyphs + `none` + custom |
| `separator(key)` | powerline, powerline_thin, slanted, round, vertical, dot, diamond, none |
| `prompt_char(key)` | chevron, arrow, lambda, dollar, angle, percent, triangle, hash + custom |
| `branch_icon(key)` | powerline (U+E0A0), octicon, nerd, text (`git:`), none + custom |

### FrameStyle

Controls box-drawing ornaments around prompt lines:

```rust
pub struct FrameStyle {
    pub enabled: bool,
    pub left: bool,     // ╭ ╰ column
    pub right: bool,    // ╮ ╯ column
    pub top_left: &'static str,     // "╭─"
    pub bottom_left: &'static str,  // "╰─"
    pub top_right: &'static str,    // "─╮"
    pub bottom_right: &'static str, // "─╯"
}
```

### Migration from LayoutPreset

The `LayoutPreset` struct in `layout.rs` is retained but no longer called from the render pipeline. `StyleResolver::effective_preset()` handles backward compatibility:

- If `style.preset` is explicitly set to anything other than `"omarchy"`, it takes precedence
- If `style.preset` is at its default (`"omarchy"`) and `prompt.layout` is set to a different value, `prompt.layout` drives the style
- Existing configs with `prompt.layout = "powerline"` continue to work identically

## Looks (`src/looks.rs`)

Added in v0.5. The Look registry: named, atomic appearance bundles. A Look is a named patch over the config tree plus a palette directive. Patches use the same shape as `config_set` payloads, so applying a Look reuses the daemon's atomic single-patch merge (`write_config_patch`).

```rust
pub struct LookDef {
    pub name: String,
    pub label: String,
    pub patch: serde_json::Value,  // config_set-shaped; glyph shortcuts expanded, palette resolved into a theme sub-patch
}
```

### Curated Looks

Eight compiled-in Looks (`looks::curated()`): `omnarchy`, `tokyo-rainbow`, `framed-gradient`, `lean-pure`, `slanted-owl`, `gruvbox-drift`, `rose-classic`, `polar-lean`. Each bundles a style preset, separators, OS icon, prompt characters, git branch icon, frame/gap settings, and (for most) a curated palette patch.

`looks::curated_palette(key)` holds the 8 curated palettes (tokyo-night, catppuccin, gruvbox, nord, dracula, rose-pine, everforest, kanagawa), each a `[theme]` patch (`source: "hybrid"` + 11 custom colors). Moved daemon-side from quattro/Model.js so Looks resolve identically from CLI, gallery, and panel.

### User Looks and Shadowing

User Looks live in `[looks.<name>]` tables in `config.toml` with `label`, `palette`, and `patch` keys. `looks::resolve(name, config)` prefers the user entry over a curated Look of the same name (user shadows curated), expands `glyphs` shortcuts (`os_icon` → `segments.os.icon`, `character` → `segments.character.{success,error,transient}`, `git_branch_icon` → `git.branch_icon`), and applies the `palette` directive:

| Directive | Effect |
|-----------|--------|
| `"keep"` (or absent) | Current palette retained |
| `"theme"` | `theme.source = "omarchy"` |
| curated key (e.g. `"gruvbox"`) | The curated palette's `theme` patch merged in |

`looks::all(config)` lists curated first (minus shadowed names), then user entries sorted by name — this is the `looks` control verb.

### Transient Apply

`looks::apply_transient(current, patch)` merges the patch into the current **in-memory** config only (JSON→TOML merge + re-serialize round-trip, no file write). `looks_apply` with `transient: true` uses this, then calls `reload_theme()`. The change reverts on any `reload_config` (watcher, control command, config write, or atomic look apply) — disk state is untouched.

## Project Profiles (`src/profiles.rs`)

Added in the Tier C wave (2026-08-29). A repository can carry its own prompt appearance in a `.o10k.toml` file at its root (`PROFILE_FILE`). Detection, validation, and merging live in `profiles.rs`; both the prompt path and the preview path consult it.

### Detection (`detect_profile`)

- Walks upward from the prompt cwd and returns the first directory containing `.o10k.toml`.
- The walk **stops at project boundaries** (any directory containing a `.git` entry) and at `$HOME` (**exclusive** — `$HOME` itself is never considered), so a profile never leaks across projects or over the whole home tree.
- Results are cached per cwd in a process-local `LazyLock<Mutex<HashMap<PathBuf, CachedDetection>>>` — **30 s TTL, bounded at 512 entries** (`DETECTION_TTL`, `MAX_DETECTION_ENTRIES`), with expired-then-oldest eviction (same pattern as the sibling-table cache). Negative results (no profile) are cached too, so warm renders skip the directory walk entirely.

### Validation (`load_profile_patch`) — untrusted input

`.o10k.toml` comes from cloned repositories, so it is treated as untrusted. The loader enforces a strict **display-key allowlist**: top-level `style`, `prompt`, `segments`, `theme`, `frame` only. State keys (`daemon`, `env`, `notifications`, `git`, …) and unknown keys are rejected with an error naming the offending key. Both a bare `config_set`-shaped patch and the wrapper form `{ patch = { ... }, name = "..." }` are accepted; an empty file is a no-op.

Every profile failure is **warn-once per profile path** (`WARNED_PROFILES` set) and swallowed: a broken repo profile must never fail the prompt (the render falls back to the base config).

### Merge order (`apply_profile`)

`apply_profile(base, patch)` merges the validated patch over the base config (profile wins) and re-parses into a `Config`; it fails only if the merged document is no longer representable. The full effective-config order, shared by prompt and preview:

```
base config (incl. any in-memory transient Look) → project profile → per-request overrides (look, patch) → style knobs
```

In the preview path (`effective_preview_config` in `server.rs`), a broken profile patch falls back to the pre-profile config for that render.

## Segment Plugins (`src/plugins.rs`)

Added in the Tier D wave (2026-08-29). Declarative segment plugins — plugins declare **data, not code**. A plugin is a directory under `~/.config/omarchy10k/plugins/<name>/` containing a `plugin.toml` manifest whose `[[segments]]` entries are either:

| Tier | Behavior |
|------|----------|
| `env` | Renders the first set env key's value; zero forks |
| `command` | Runs a command in the prompt's cwd, async, TTL-cached, hard **500 ms** timeout (`COMMAND_TIMEOUT_MS`); the render path never awaits it — a slow refresh returns stale/absent |

Command output is capped to its first line, 256 bytes (`MAX_OUTPUT_BYTES`). The command string is split quote-aware into argv (`split_command`) — never a shell string: no `sh -c`, no interpolation, no injection surface.

### Registry and lifecycle

- **Presence on disk means *available*; presence in `[plugins] enabled` (`PluginsConfig.enabled`, serde-default empty) means *active*.** Dropping a directory in never activates it.
- `DaemonState::new` loads the registry at startup and **`reload_config` rebuilds it** (re-reads the plugins dir and invalidates `PluginCache`), so `plugin add/remove/update` on disk and `[plugins].enabled` changes all land on one reload path.
- Plugin segments join the built-in pipeline with the registry name **`plugin.<plugin>.<segment>`** (`PLUGIN_SEGMENT_PREFIX`), so they can never collide with or shadow a built-in segment; the render-path preset filter lets this prefix through where built-in allowlists apply. Malformed plugins are skipped with a `warn!` — a broken plugin never takes down a shell.
- `valid_plugin_name` (`[A-Za-z0-9_-]+`, short) doubles as the traversal guard; the manifest `name` must match its install directory, so a hostile manifest cannot direct reads/writes at another path.

### PluginCache

TTL cache for command-tier segments, modelled on `GitCache`: keyed by (segment registry name, cwd), in-flight deduped so a fast prompt loop cannot spawn a process storm, generation-guarded against resurrecting pre-invalidation snapshots. Invalidated on every `reload_config`.

## Segment Utilities (`src/segments/util.rs`)

Added in the Tier D wave. Shared helpers for the cloud/ops segment catalog (and the same patterns reused elsewhere):

| Helper | Behavior |
|--------|----------|
| `TtlCache<V>` | Process-local TTL cache keyed by an arbitrary string (usually the cwd). Negative results are cached alongside positives so a miss is not retried every prompt; bounded by `max_entries`, expired entries recomputed on access |
| `on_path(bin)` | True when `bin` resolves to an existing executable file on `PATH` — a missing tool costs one stat walk, not a spawn |
| `run_command(bin, args, timeout_ms)` | Synchronous trimmed-stdout run; `None` on spawn failure, non-zero exit, or timeout. The child is **killed on timeout** so a hung CLI cannot stall the synchronous render path |

## Script Execution (`src/script_exec.rs`)

Added in v0.5. Daemon-side user-script registry and runner for the quick-actions feature (`omarchy10k script`).

- Scripts live in `$XDG_CONFIG_HOME/omarchy10k/scripts` (`scripts_dir()`), must be executable regular files. Trust model: user's own config directory — same trust level as `.bashrc`, nothing network-sourced.
- `valid_name()` traversal guard: non-empty, no `/`, no `..` substring, does not start with `.`. `resolve_script()` applies it plus regular-file and executable-bit checks.
- `list_scripts(dir)` returns `{name, path}` entries sorted by name; missing directory → empty list.
- `run_script(path, timeout_secs)` spawns the script (stdin nulled, stdout/stderr piped, `kill_on_drop`), enforces a hard timeout (`DEFAULT_SCRIPT_TIMEOUT_SECS = 30`), and returns trimmed stdout; non-zero exits carry status + trimmed stderr in the error.
- `handle_script_control(command, rest)` serves the `script_list` and `script_run` control verbs, sharing the daemon's `{"type":"control", ...}` response shape.

## Config (`src/config.rs`)

TOML schema with layered defaults via `#[serde(default)]`. All Config structs derive `Serialize` (enables the `config_get` API). v0.3 adds `StyleConfig` (with nested `SeparatorConfig`, `FrameConfig`, `CapsConfig`), plus `ContainerConfig`, `PythonConfig`, `ToolchainConfig`, `NixConfig`, `K8sConfig`, `TimeConfig`, `BatteryConfig`, `NotificationConfig`, `TerminalConfig` (with nested `TitleConfig` and `ProgressConfig`). See [Configuration](config.md) for the full key reference.

### GitConfig

```rust
pub cache_ttl_ms: u64,  // default: 5000
```

Passed to `GitCache::new()` at daemon startup. Controls how long cached git status is considered fresh.

### Path Resolution

```rust
Config::config_dir()  → directories::BaseDirs::config_dir() / "omarchy10k"
                        (typically ~/.config/omarchy10k)
Config::config_path() → config_dir() / "config.toml"
```

Uses the `directories` crate for XDG compliance. Falls back to `$HOME/.config/omarchy10k`.

### Loading Behavior

- Missing config file → all defaults (not an error)
- Parse error → `ConfigError::Parse` (daemon logs error, uses previous config)
- Default TOML embedded via `include_str!("../../../config/default.toml")`

### Config vs Implementation Status

| Config Key | Implemented | Notes |
|------------|-------------|-------|
| `prompt.layout` | Yes | Filters segments via `LayoutPreset`; controls separators and single-line override |
| `prompt.transient` | Yes | Transient prompt character |
| `prompt.newline` | Yes | Two-line vs one-line (overridden by `minimal`/`dense` presets) |
| `prompt.right_prompt` | Yes | Populates `right` field when enabled |
| `theme.source` | Yes | omarchy/custom/hybrid/terminal |
| `theme.custom.*` | Yes | Applied via `resolve_palette` on startup and reload |
| `directory.strategy` | Yes | `smart` (default), `full`, or `truncate` |
| `directory.max_length` | Yes | Truncation limit |
| `directory.repo_root_style` | Yes | Bold repo root in smart truncate mode |
| `directory.enabled` | Yes | Tier C (wizard daemon gap closed, additive): `DirectoryConfig.enabled`, default `true`; gates the directory segment inside `segments/directory.rs::render` |
| `plugins.enabled` | Yes | Tier D: list of active plugin names (`PluginsConfig.enabled`, default empty); changes land on `reload_config` |
| `directory.unique` | Yes | Wave 1: `unique = true` enables the p10k-grade unique-prefix truncation (sibling tables + anchor files); `directory.anchors` lists anchor-file names (default `.git`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `Gemfile`, `flake.nix`, `README.md`). NOTE: top-level `[directory]`, not `[segments.directory]` |
| `git.enabled` | Yes | Toggles git segment |
| `git.mode` | Yes | adaptive/compact/expanded/hidden |
| `git.cache_ttl_ms` | Yes | Git cache TTL in milliseconds (default 5000) |
| `git.stale_display` | Yes | Gates stale coloring: when `true` (default), stale cache hits render muted in the left git segment; when `false`, the `stale` flag is ignored for coloring |
| `git.max_threads` | No | Single-threaded git subprocess |
| `segments.os` | Yes | OS icon segment |
| `segments.container` | Yes | Docker/Podman/Toolbox/Distrobox detection |
| `segments.python` | Yes | VIRTUAL_ENV / CONDA_DEFAULT_ENV |
| `segments.toolchain` | Yes | Mise version env vars |
| `segments.nix` | Yes | IN_NIX_SHELL pure/impure |
| `segments.k8s` | Yes | kubeconfig current-context (disabled by default) |
| `segments.time` | Yes | Clock display via libc localtime (disabled by default) |
| `segments.battery` | Yes | sysfs BAT0/BAT1 (disabled by default) |
| `segments.notification` | No | Config stub only |
| `segments.exit_status` | Yes | Full; undercurl when TermCaps allows |
| `segments.command_duration` | Yes | Full |
| `segments.jobs` | Yes | Background job count segment |
| `segments.ssh` | Yes | SSH hostname segment |
| `segments.character` | Yes | Success/error glyphs; undercurl on error |
| `terminal.title.enabled` | Yes | OSC 2 terminal title in render pipeline |
| `terminal.title.format` | Partial | Format string defined; render uses short cwd |
| `terminal.progress.enabled` | No | Config stub only |
| `daemon.socket` | No | Socket path computed in main, ignores config |
| `daemon.log_level` | Yes | Sets tracing env filter |

## Theme (`src/theme.rs`)

### ThemePalette

Holds semantic color roles as `AnsiColor` (r, g, b):

| Field | Role | Default (Tokyo Night) |
|-------|------|----------------------|
| `accent` | Primary highlight | `#7aa2f7` |
| `foreground` | Default text | `#c0caf5` |
| `dark_foreground` | Darker text variant | `#565f89` |
| `bright_foreground` | Brighter text variant | `#c0caf5` |
| `background` | Background | `#1a1b26` |
| `muted` | De-emphasized text | `#565f89` |
| `red` | Error, destructive | `#f7768e` |
| `green` | Success | `#9ece6a` |
| `yellow` | Warning | `#e0af68` |
| `blue` | Info | `#7aa2f7` |
| `is_dark` | Dark mode flag | `true` |

### AnsiColor

```rust
pub struct AnsiColor { pub r: u8, pub g: u8, pub b: u8 }

impl AnsiColor {
    pub fn from_hex(hex: &str) -> Option<Self>    // "#7aa2f7" → AnsiColor; char-boundary safe (never byte-slices); rejects invalid length/non-hex with None + tracing::warn!
    pub fn fg_escape(&self) -> String             // "\x1b[38;2;122;162;247m"
    pub fn bg_escape(&self) -> String             // "\x1b[48;2;122;162;247m"
}
```

True-color ANSI only — no 256-color fallback. Requires `COLORTERM=truecolor` or `24bit`.

### Loading

`ThemePalette::load_omarchy()` reads `~/.local/state/omarchy/current/theme/colors.toml`:

```toml
[colors]
accent = "#7aa2f7"
foreground = "#c0caf5"
mode = "dark"
# ... etc
```

Missing file or missing keys → hardcoded Tokyo Night defaults.

`apply_custom_overrides(custom: &CustomPalette)` merges `Some(...)` fields from `[theme.custom]` config. Does not touch `dark_foreground`, `bright_foreground`, or `is_dark`.

### resolve_palette

Unified palette resolution used at startup and on `reload_theme`:

```rust
pub fn resolve_palette(config: &Config) -> Self
```

| `theme.source` | Behavior |
|----------------|----------|
| `"omarchy"` | `load_omarchy()` from colors.toml |
| `"custom"` | Tokyo Night defaults + custom overrides |
| `"hybrid"` | omarchy base + custom overrides |
| `"terminal"` / other | Hardcoded Tokyo Night defaults |

This replaces the separate startup/reload code paths and fixes the previous bug where `reload_theme` always called `load_omarchy()` regardless of source mode.

### Unit Tests

| Test | Validates |
|------|-----------|
| `test_resolve_omarchy_is_default` | omarchy source loads palette with expected accent |
| `test_resolve_custom_applies_overrides` | custom source applies hex overrides |
| `test_resolve_hybrid_merges` | hybrid merges omarchy base with custom accent |
| `test_resolve_terminal_returns_default` | terminal source returns hardcoded defaults |
| `test_hex_parsing` | `#rrggbb` parsing; rejects invalid/short hex |

## Git (`src/git.rs`)

### GitCache

```rust
pub struct GitCache {
    cache: Arc<RwLock<HashMap<PathBuf, CachedStatus>>>,
    in_flight: Arc<RwLock<HashSet<PathBuf>>>,
    generation: Arc<AtomicU64>,  // bumped by invalidate()/invalidate_all(); refresh tasks drop results that raced an invalidate
    ttl_ms: AtomicU64,  // updatable via set_ttl()
}
```

```rust
pub fn new(ttl_ms: u64) -> Self
pub fn set_ttl(&self, ttl_ms: u64)  // called by reload_config()
```

Cache key is the **repository root path**, not the cwd. Multiple cwds within the same repo share one cache entry.

### Three-Tier Cache Response (`get_status`)

1. **Fresh hit** (within TTL) → return cached status immediately
2. **Stale hit** (past TTL) → return cached status with `stale: true`, schedule async refresh
3. **Cold miss** (not in cache) → return minimal status (`is_repo: true`, empty branch) with `stale: true`, schedule async refresh

### schedule_refresh

Spawns a tokio task that:
1. Checks/sets the `in_flight` set for request coalescing (duplicate refreshes for the same repo are skipped)
2. Calls `fetch_git_status(repo_root).await` — returns `None` on spawn error or non-zero git exit
3. Drops the result if the cache generation changed mid-flight (an `invalidate` raced the refresh); the next `get_status` schedules a fresh refresh
4. On success: updates the cache with fresh status and timestamp. On failure: keeps any previous cache entry in place; inserts an `is_repo: false` entry only when none exists, so the segment renders `None` instead of a fake empty repo
5. Removes the repo from `in_flight`

### Status Fetching

1. `find_repo_root(cwd)` — walks up directory tree looking for `.git` (file or directory). Handles worktrees by reading `gitdir:` pointer.
2. `fetch_git_status(repo_root)` — runs `git --no-optional-locks status --porcelain=v2 --branch` and `git stash list` via `tokio::process::Command` (async, not `std::process::Command`)
3. `parse_porcelain_v2(output)` — extracts branch, upstream, ahead/behind, staged/unstaged/untracked/conflicted counts
4. `detect_worktree(repo_root)` — when `.git` is a file (linked worktree) or contains `commondir`, sets worktree name from repo root directory name
5. `detect_git_action(git_dir)` — checks for `.git/MERGE_HEAD`, `rebase-merge/`, `CHERRY_PICK_HEAD`, `BISECT_LOG`, `REVERT_HEAD`

### GitStatus

```rust
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: String,          // "main", "HEAD" if detached
    pub commit: String,          // short SHA
    pub tag: String,             // nearest tag
    pub upstream: String,        // "origin/main"
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub stashes: u32,
    pub action: Option<GitAction>,  // Merge, Rebase, CherryPick, Bisect, Revert
    pub is_detached: bool,
    pub worktree: Option<String>,   // worktree directory name when in linked worktree
    pub repo_root: String,
    pub stale: bool,             // true on stale/cold cache hits
    pub remote: Option<String>,  // `origin` URL captured once per cache refresh (OSC 8 branch hyperlink)
}
```

### GitAction

```rust
pub enum GitAction {
    Merge,
    Rebase(String),    // "step/total" progress
    CherryPick,
    Bisect,
    Revert,
}
```

Implements `Display` for prompt formatting: `"merge"`, `"rebase 3/5"`, etc.

## Layout (`src/layout.rs`)

### Segment

```rust
pub struct Segment {
    pub name: std::sync::Arc<str>,   // built-ins: static strings; plugin segments: owned `plugin.<plugin>.<segment>` names
    pub content: String,
    pub compact_content: Option<String>,
    pub priority: u8,          // lower = more important
    pub min_width: u16,
    pub preferred_width: u16,
    pub hide_below_cols: u16,
    pub fg: String,
    pub bg: Option<String>,
    pub bold: bool,
    pub separator: Option<String>,
}
```

`name` is an `Arc<str>` (Tier D): built-in segments clone static registry names, but plugin segments need owned data, hence shared ownership. `Box::leak` for this was explicitly banned — `Arc<str>` avoids an unbounded process-lifetime leak while keeping `Segment` cheap to clone. Colors are plain strings (`fg: String`, `bg: Option<String>`), since segments may also carry embedded escape runs.

### LayoutEngine Resolution Algorithm

```
Input: segments[], terminal_cols, separator_width
Output: ResolvedSegment[]

1. Filter: remove segments where cols < hide_below_cols
2. Calculate total preferred width (sum of preferred_width + separator_width between segments)
3. If fits: return all segments at preferred width
4. Else: sort by priority (ascending = most important first)
5. Greedy fit: iterate sorted, add segment (using compact if preferred doesn't fit)
6. Re-sort by original_index to restore display order
```

`LayoutEngine::new_with_separator_width(cols, sep_width)` uses the actual separator display width from the resolved style (e.g. 3 for powerline arrows ` \u{e0b0} `), fixing the previous hardcoded `separator_width = 1` which caused width miscalculations with multi-char separators.

Uses `unicode-width` crate for accurate East Asian character width measurement.

### LayoutPreset

```rust
pub struct LayoutPreset;

impl LayoutPreset {
    pub fn separator(preset: &str) -> &'static str;
    pub fn is_single_line(preset: &str) -> bool;
    pub fn segment_order(preset: &str) -> &'static [&'static str];
    pub fn apply_filter(segments: &mut Vec<Segment>, preset: &str);
}
```

Presets control which segments appear, in what order, and how they are joined:

| Preset | Segments | Separator | Single-line |
|--------|----------|-----------|-------------|
| `minimal` | directory | space | yes |
| `powerline` | os, ssh, container, directory, git, python_env, toolchain, nix, k8s, exit_status, command_duration, jobs, time, battery | powerline arrow (U+E0B0) | no |
| `classic` | ssh, directory, git, exit_status | ` │ ` | no |
| `pure` | directory, git | space | no |
| `dense` | os, ssh, directory, git, exit_status, command_duration, jobs | space | yes |
| `omarchy` (default) | os, ssh, container, directory, git, python_env, toolchain, nix, k8s, exit_status, command_duration, jobs, time, battery | space | no |

`apply_filter()` retains only segments whose `name` appears in the preset's segment order. `is_single_line()` overrides `prompt.newline` for `minimal` and `dense`. `separator()` is passed to `format_line1()` for preset-specific join strings.

## Terminal (`src/terminal.rs`)

Terminal feature detection engine. Detects terminal kind from environment and exposes a capability matrix used by segments and render:

```rust
pub enum TerminalKind { Ghostty, Foot, Kitty, WezTerm, Alacritty, Unknown }

pub struct TermCaps {
    pub terminal: TerminalKind,
    pub has_osc7: bool, pub has_osc8: bool, pub has_osc52: bool, pub has_osc777: bool,
    pub has_kitty_graphics: bool, pub has_sixel: bool,
    pub has_undercurl: bool, pub has_sync_output: bool,
}

impl TermCaps {
    pub fn detect() -> Self;  // reads TERM_PROGRAM, KITTY_WINDOW_ID, GHOSTTY_RESOURCES_DIR
}
```

Consumers:
- `segments/directory.rs` — OSC 8 hyperlinks on directory paths when `has_osc8`
- `segments/exit_status.rs` — undercurl (`\x1b[4:3m`) on error text when `has_undercurl`
- `segments/character.rs` — undercurl on error prompt character when `has_undercurl`

## Render (`src/render.rs`)

### PromptRenderer

Orchestrates the full render pipeline:

```rust
pub fn render(&self, cwd, exit_code, cmd_duration_ms, cols, jobs, git_status, shell_integration: bool) -> PromptResponse {
    // Build SegmentContext with all inputs + config + palette
    // StyleResolver::resolve(config) → ResolvedStyle
    // collect_segments(ctx) → Vec<Segment>
    // Filter segments by resolved_style.segment_order
    // LayoutEngine::new_with_separator_width(cols, sep_width).resolve(segments)
    // format_line1(resolved, style.left_separator) → ANSI string
    // format_line2(prompt_char via GlyphCatalog) → ANSI string
    // OSC 2 title when terminal.title.enabled
    // Wrap in OSC 133 markers when shell_integration is true
    // Single-line override when resolved_style.force_single_line
    // Frame rendering when style.frame.enabled (gap filler, ornaments)
    // render_right(ctx) when prompt.right_prompt enabled (disabled in frame mode)
    // Build transient prompt with configurable glyph if enabled
}
```

### Frame Rendering

When `style.frame.enabled` is true and the prompt is two-line, `render_framed()` produces:

```
╭─ {segments} ──────── {right} ─╮    ← line 1 with gap fill
╰─ ❯                            ─╯    ← line 2 with prompt char
```

The gap filler uses `gap_char` (e.g. `─`) rendered in muted color between left content and right content, padded to terminal width. Right prompt content is inlined into line 1 when framing is active (not sent as a separate `right` field).

The `shell_integration` parameter controls OSC 133 emission. When `false`, prompt start/end markers are omitted (empty strings). Preview requests always pass `false`. Defaults to `true` when not specified in the prompt request.

### render_right

Returns an optional right prompt string when `config.prompt.right_prompt` is enabled (and frame mode is not active). Since the Wave-2 desktop integration, the rail is **configurable** via `[prompt] right_segments` (default `["command_duration", "git"]`, byte-identical to the old hardcoded pair):

```rust
resolve_right_rail(config.prompt.right_segments) -> Vec<RightSegment>
```

Recognized names: `command_duration`, `git`, `time`, `battery`, `jobs` (`layout.rs` `RightSegment`). Unknown names are skipped with a `debug!` log; order is preserved.

- **Command duration** (muted) — shown when above `show_above_ms` and not already in the left rail
- **Git branch** (accent, or muted when `git_status.stale`) — configurable branch icon via `GlyphCatalog::branch_icon(config.git.branch_icon)` + name; suppressed on stale hits unless `git.stale_display`
- **time / battery / jobs** — reuse their left-rail renderers for gating and content, styled muted; skipped when the left rail already rendered them

Parts are space-separated. Returns `None` when the rail would be empty. When frame mode is active, right content is inlined into the framed line 1 and the `right` field in `PromptResponse` is `None`.

### OSC 2 Terminal Title

When `terminal.title.enabled` is true, the render pipeline prepends `\x1b]2;{short_cwd}\x07` to the prompt. The short cwd replaces `$HOME` with `~`. The `terminal.title.format` config key is defined but render currently uses short cwd directly.

### OSC 133 Shell Integration

When `shell_integration` is `true`:

```
Prompt start: \x01\x1b]133;A\x07\x02
Prompt end:   \x01\x1b]133;B\x07\x02
```

OSC 133 marks prompt boundaries for terminal features like command click-to-select (iTerm2, WezTerm, Kitty). The `\x01`/`\x02` wrappers are Bash-specific non-printing character delimiters.

### PromptResponse

```rust
pub struct PromptResponse {
    pub left: String,              // The full prompt string with ANSI codes
    pub right: Option<String>,     // Right prompt when prompt.right_prompt enabled
    pub transient: Option<String>, // Muted "❯" when transient enabled
    pub git_stale: bool,           // Mirrors git_status.stale from cache tier
}
```

## Segments (`src/segments/`)

### Segment Collection Order

Defined in `segments/mod.rs`:

```rust
pub fn collect_segments(ctx: &SegmentContext<'_>) -> Vec<Segment> {
    let mut segs = Vec::new();
    // 1. OS (if enabled)
    // 2. SSH (if enabled and show policy matches)
    // 3. Container (if segments.container.enabled and detected)
    // 4. Directory (always attempts)
    // 5. Dir writable (if enabled; lock glyph when cwd is NOT writable)
    // 6. Git (if config.git.enabled)
    // 7. Python env (if segments.python.enabled)
    // 8. Toolchain (if segments.toolchain.enabled)
    // 9. Nix (if segments.nix.enabled)
    // 10. K8s (if segments.k8s.enabled)
    // 11. Package version (if enabled; package.json/Cargo.toml/pyproject.toml in cwd)
    // 12. Docker context (if enabled)
    // 13. Kubectl context (if enabled)
    // 14. Terraform workspace (if enabled and .terraform exists)
    // 15. GCloud project (if enabled)
    // 16. AWS profile (if enabled)
    // 17. VPN (if enabled; tun/tap/wg interfaces)
    // 18. Exit status (if enabled and exit_code != 0)
    // 19. AI agent (if segments.ai.enabled and agent env present)
    // 20. Command duration (if enabled and above threshold)
    // 21. Jobs (if enabled and jobs > 0)
    // 22. Time (if segments.time.enabled)
    // 23. Battery (if segments.battery.enabled and below show_above threshold)
    // 24. Load sparkline (if segments.load.enabled)
    segs
}
```

Note: `LayoutPreset::apply_filter()` in `render.rs` runs after collection and may remove segments not in the active preset.

The full catalog (`ALL_SEGMENTS` in `style.rs`) is **24 names** — 16 pre-Tier D (`os` … `load`, including the Wave 1 `load` and `ai` additions) plus the 8 Tier D segments below. All 8 are **default-off** and driven by `IconSegmentConfig`-style entries (`enabled` + `icon`) under `[segments.*]`:

| Segment (`segments/*.rs`) | Signal | Mechanism | TTL / timeout |
|---------------------------|--------|-----------|---------------|
| `package_version` | Version from `package.json`, `Cargo.toml`, or `pyproject.toml` in cwd (first match wins) | Manifest scan, regex-free; cached per cwd, negative results included | 10 s |
| `dir_writable` | Lock glyph when cwd is NOT writable | Real create/delete probe (catches ACLs/RO mounts); cached per cwd | 10 s |
| `aws_profile` | Active AWS identity; precedence `AWS_PROFILE` > `AWS_VAULT` > `AWS_DEFAULT_PROFILE` | Pure env-tier via the 0.4 env channel — zero forks | — |
| `docker_context` | Docker target | `DOCKER_HOST` from env channel wins; else `docker context show` | 15 s / bounded |
| `kubectl_context` | Current kubeconfig context | `kubectl config current-context`; hidden when kubectl absent/hangs | 15 s / bounded |
| `terraform_workspace` | Workspace name | Stat-gated on `.terraform` in cwd, then `terraform workspace show` | 15 s / 1 s |
| `vpn` | Active VPN | `/sys/class/net` directory listing — tun/tap/wg interface names | 15 s |
| `gcloud_project` | GCP project | `GOOGLE_CLOUD_PROJECT` env wins; else `gcloud config get-value project` (artifacts like `() (empty)` treated as unset) | 30 s / bounded |

Command-tier segments share `segments/util.rs` (`TtlCache`, `on_path`, `run_command` — see [Segment Utilities](#segment-utilities-srcsegmentsutilrs) above).

### Container Segment (`segments/container.rs`)

Detects container runtime environment:

| Signal | Type |
|--------|------|
| `DISTROBOX_ENTER_PATH` set | distrobox |
| `/.dockerenv` exists | docker |
| `/run/.containerenv` exists | podman |
| `container` env var set | toolbox |

Content: `⬡ {type}` (icon configurable via `segments.container.icon`, default `"auto"`). Compact: icon only.

Priority: 7, hide_below_cols: 50, color: accent

### Python Env Segment (`segments/python_env.rs`)

Shows active Python environment from `VIRTUAL_ENV` (basename) or `CONDA_DEFAULT_ENV`.

Content: `🐍 {name}`. Compact: `🐍`.

Priority: 35, hide_below_cols: 50, color: yellow

### Toolchain Segment (`segments/toolchain.rs`)

Reads Mise version env vars and displays active tool versions:

| Env var | Icon | Tool |
|---------|------|------|
| `MISE_NODE_VERSION` | ⬢ | node |
| `MISE_PYTHON_VERSION` | 🐍 | python |
| `MISE_RUBY_VERSION` | 💎 | ruby |
| `MISE_GO_VERSION` | 🐹 | go |
| `MISE_RUST_VERSION` | 🦀 | rust |

Multiple tools joined with spaces. Compact: icons concatenated without spaces.

Priority: 40, hide_below_cols: 60, color: foreground

### Nix Segment (`segments/nix.rs`)

Shows when `IN_NIX_SHELL` is `"pure"` or `"impure"`.

Content: `❄ {pure|impure}`. Compact: `❄`.

Priority: 36, hide_below_cols: 50, color: blue

### K8s Segment (`segments/k8s.rs`)

Parses `current-context` from kubeconfig (`KUBECONFIG` or `~/.kube/config`). When `segments.k8s.show_namespace` is true, appends namespace from the context block.

Content: `⎈ {context}` or `⎈ {context}/{namespace}`. Compact: `⎈`.

Priority: 42, hide_below_cols: 60, color: blue. Disabled by default.

### Time Segment (`segments/time.rs`)

Current local time without the `chrono` crate — uses inline `localtime_r` FFI against libc.

Supported formats: `%H:%M` (default), `%H:%M:%S`, `%I:%M %p`.

Priority: 55, hide_below_cols: 40, color: muted. Disabled by default.

### Battery Segment (`segments/battery.rs`)

Reads capacity and charging status from `/sys/class/power_supply/BAT0` or `BAT1`. Hidden when capacity ≥ `segments.battery.show_above` (default 100, effectively always hidden until threshold lowered).

Color thresholds: red ≤ `threshold_critical` (10%), yellow ≤ `threshold_warning` (30%), green otherwise. Charging shows 🔌 instead of 🔋.

Priority: 56, hide_below_cols: 40. Disabled by default.

### OS Segment (`segments/os.rs`)

Renders an OS icon based on `config.segments.os.icon`, routed through `GlyphCatalog::os_icon()`:

| Icon value | Glyph | Icon value | Glyph |
|------------|-------|------------|-------|
| `"arch"` | U+F303 | `"alpine"` | U+F300 |
| `"ubuntu"` | U+F31B | `"void"` | U+F32E |
| `"debian"` | U+F306 | `"gentoo"` | U+F30D |
| `"fedora"` | U+F30A | `"manjaro"` | U+F312 |
| `"nixos"` | U+F313 | `"opensuse"` | U+F314 |
| `"macos"` / `"apple"` | U+F179 | `"centos"` | U+F304 |
| `"windows"` | U+F17A | `"raspberry_pi"` | U+F315 |
| `"linux"` | U+F17C | `"none"` | hidden |
| `"omarchy"` | U+F312 | custom | literal string |

Expanded from 4 options to 16+ in v0.3.

Priority: 5, hide_below_cols: 40, color: accent

### SSH Segment (`segments/ssh.rs`)

Shows hostname when in an SSH session. Controlled by `config.segments.ssh.show`:

| Value | Behavior |
|-------|----------|
| `"always"` | Always show |
| `"never"` | Never show |
| `"auto"` (default) | Show only when `SSH_TTY` or `SSH_CONNECTION` is set |

Hostname via inline `gethostname()` FFI (no `libc` crate dependency). Truncated to short hostname (before first dot).

Content: terminal icon (U+F489) + short hostname. Compact: icon only.

Priority: 8, hide_below_cols: 50, color: yellow

### Jobs Segment (`segments/jobs.rs`)

Shows background job count when > 0.

Content: gear icon (U+F013) + multiplication sign (U+00D7) + count. Compact: gear icon + count.

Priority: 45, hide_below_cols: 50, color: blue

### Directory Segment

Smart path truncation with home substitution:

- `~` replaces `$HOME` prefix
- Strategy controlled by `directory.strategy`: `smart` (default), `full`, or `truncate`
- `smart_truncate(path, max_length)`: keeps first and last path components, unique prefixes for middle directories; respects `directory.repo_root_style`
- Fallback: `first/…/last`
- **OSC 8 hyperlinks**: full path wrapped in `\x1b]8;;file://{hostname}{abs_path}\x1b\\{content}\x1b]8;;\x1b\\` for click-to-open in capable terminals

### Git Segment

Four display modes controlled by `config.git.mode`:

| Mode | Logic |
|------|-------|
| `hidden` | Returns `None` |
| `compact` | Always compact format |
| `expanded` | Expanded format, compact fallback for layout |
| `adaptive` | Expanded when dirty, compact when clean |

Symbol vocabulary: `✓` clean, `+N` staged, `!N` unstaged, `?N` untracked, `×N` conflicted, `⇡N` ahead, `⇣N` behind, `≡N` stashes

When `git.worktree` is set (linked worktree detected), the worktree directory name appears after the branch name.

Branch name truncated to 20 characters with `…` suffix.

**OSC 8 clickable branch** (C1 wave): `fetch_git_status` also runs `git remote get-url origin` once per cache refresh (additive `GitStatus.remote`; porcelain parsing never sets it). When `TermCaps.has_osc8`, the branch (and its compact form) is wrapped in an OSC 8 hyperlink to the normalized URL — scp-like ssh and `ssh://` remotes become `https://host/path` (credentials/port dropped), `http(s)://` passes through, a trailing `.git` is stripped, and `git://`/`file://`/local paths render plain. The URL is percent-encoded outside printable ASCII and widths are computed from the plain text before wrapping (`segments/git.rs` `normalize_remote_url`/`hyperlink`).

### Exit Status Segment

Only shown for non-zero exit codes. When `TermCaps::detect().has_undercurl`, content is wrapped in `\x1b[4:3m` / `\x1b[4:0m` for wavy underline styling.

Expanded format maps common codes:

| Code | Signal |
|------|--------|
| 126 | Permission denied |
| 127 | Command not found |
| 130 | SIGINT (Ctrl+C) |
| 137 | SIGKILL |
| 139 | SIGSEGV |
| 141 | SIGPIPE |
| 143 | SIGTERM |

Compact: `✘ {code}`

### Command Duration Segment

Threshold: `config.segments.command_duration.show_above_ms` (default 1500ms)

Format progression: `500ms` → `1.5s` → `1m5s` → `1h1m`

### Character (Line 2 / Transient)

Not a layout `Segment` — rendered directly by `render.rs` via `GlyphCatalog::prompt_char()`:

- Success: `config.segments.character.success` (default `❯`) in accent color
- Error: `config.segments.character.error` (default `❯`) in red; undercurl when `TermCaps.has_undercurl`
- Transient: `config.segments.character.transient` (default `❯`) in muted color. Previously hardcoded; now configurable in v0.3.
- Vi NORMAL mode (Wave-1.5): when `[segments.character] vi_mode = true` and the adapter reports `KEYMAP` through the env channel as `vi_mode` (any value starting with `n`/`N` = NORMAL), the success glyph becomes `❮`; insert mode, unset `vi_mode` env, or the config off-switch keep the default char (`segments/character.rs`).

## v0.3.0 Bug Fixes

The following issues identified by the [Bug Audit](bug-audit.md) have been fixed:

| Finding | Module | Fix | Severity |
|---------|--------|-----|----------|
| [#1](bug-audit.md#1-prompt-escapes-are-not-marked-non-printing-so-bash-miscounts-prompt-width) | `render.rs` | All ANSI escapes (colors, bold, reset, OSC 2 title, OSC 8 hyperlinks, undercurl) are wrapped in `\x01`/`\x02` readline delimiters via `wrap_np()`. Preview responses strip delimiters. | Critical |
| [#2](bug-audit.md#2-struct-tm-abi-mismatch-corrupts-the-stack-when-the-time-segment-is-enabled) | `segments/time.rs` | `struct Tm` now includes `tm_gmtoff` and `tm_zone` fields matching the C ABI | Critical |
| [#3](bug-audit.md#3-two-utf-8-byte-slicing-panics-on-ordinary-non-ascii-input) | `segments/git.rs`, `directory.rs` | `truncate_branch` and `unique_prefix` use `char_indices()` for UTF-8-safe slicing | Critical |
| [#4](bug-audit.md#4-the-daemon-exits-immediately-when-o10k_parent_pid-is-unset) | `main.rs` | `monitor_parent` blocks forever via `std::future::pending()` when PID is unset | Critical |
| [#9](bug-audit.md#9-the-right-prompt-duplicates-content-already-in-the-left-prompt) | `render.rs` | `render_right` skips segments already rendered in the left prompt | High |
| [#10](bug-audit.md#10-gitcache_ttl_ms-is-frozen-at-startup) | `git.rs`, `server.rs` | `GitCache::ttl` uses `AtomicU64`; `reload_config` updates it via `set_ttl()` | Medium |
| [#12](bug-audit.md#12-a-type-less-prompt-request-carrying-command-is-misrouted-to-the-control-handler) | `server.rs` | Type-less messages check `cwd` first (prompt) before `command` (control) | Medium |
| [#13](bug-audit.md#13-home-prefix-substitution-is-not-path-aware) | `render.rs`, `directory.rs` | Home substitution uses `Path::starts_with` (component-aware) | Medium |
| [#14](bug-audit.md#14-directoryrepo_root_style-is-ignored-and-the-bold-expression-is-inverted) | `directory.rs` | Bold is now `repo_root_style == "bold"` only; inverted logic removed | Medium |
| [#16](bug-audit.md#16-kill0-treats-eperm-as-parent-is-dead) | `main.rs` | Parent monitor distinguishes `ESRCH` (dead) from `EPERM` (alive) | Medium |
| [#17](bug-audit.md#17-shutdown-leaves-the-socket-file-behind) | `server.rs` | Shutdown handler removes socket file before `exit(0)` | Low |
| [#20b](bug-audit.md#20-smaller-confirmed-defects) | `segments/git.rs` | Empty branch on cold cache displays `…` instead of blank | Low |
| [#20c](bug-audit.md#20-smaller-confirmed-defects) | `git.rs` | `detect_worktree` checks for `/worktrees/` in gitdir path to distinguish from submodules | Low |
| [#20d](bug-audit.md#20-smaller-confirmed-defects) | `segments/battery.rs` | `show_above` uses `>` instead of `>=` so 100% battery shows at threshold 100 | Low |

## Remaining Known Issues

| Module | Issue | Severity | Status |
|--------|-------|----------|--------|
| segment layer | [Environment segments frozen at daemon start](bug-audit.md#5-every-environment-derived-segment-is-frozen-at-daemon-start) — python_env, toolchain, nix, k8s read `std::env` inside the daemon and never reflect post-startup changes. | High | v0.4 design item |
| `layout.rs` | [`Segment::display_width` measures escape bytes as columns.](bug-audit.md#20-smaller-confirmed-defects) Unreachable today because every segment sets `compact_content`; a trap for the next one that does not. | Low | Open |

## Wave 1 Internals

- **Sibling cache** (`segments/directory.rs`): process-local `LazyLock<Mutex<HashMap<PathBuf, SiblingTables>>>`, keyed by cwd, 30s TTL. Each entry caches per-ancestor sibling directory lists + anchor flags, so warm renders with `unique = true` do zero filesystem reads. Size-bounded: when the map reaches `MAX_SIBLING_ENTRIES` (512), expired entries are dropped first, then the least-recently-stamped entries are evicted until under the cap.
- **Load ring** (`segments/load.rs`): process-local `LazyLock<Mutex<VecDeque<f32>>>`, 16 slots, pushed once per render. Idle shells freeze history by design.
- **Gradient math** (`theme.rs`): `AnsiColor::lerp(a, b, t)` + `ThemePalette::ramp_color(t)` (two-stage accent→magenta→cyan) + `gap_gradient_endpoints(mode)` (complement rule: blue ≥ red accent → magenta, else cyan). `AnsiColor` is now `Copy`.
