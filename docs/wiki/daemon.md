# Daemon Reference (`omarchy10kd`)

[← Index](INDEX.md) | [Architecture](architecture.md) | [Protocol](protocol.md)

The daemon is the computational core of Omarchy10k. It holds all state, renders all prompts, and serves multiple concurrent connections over a Unix domain socket. One daemon instance runs per Bash session.

## Binary Entry Point (`src/main.rs`)

Bootstraps everything: config loading, tracing initialization, theme palette, shared state, filesystem watchers, parent-process monitor, and the socket server.

Module tree: `config`, `git`, `layout`, `render`, `segments`, `server`, `theme`, `terminal`.

### Startup Sequence

```
1. Init tracing subscriber (env-filter from config.daemon.log_level)
2. Load Config from ~/.config/omarchy10k/config.toml (or defaults)
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

Uses an inline `extern "C" { fn kill(...) }` declaration instead of depending on the `libc` crate — keeps the dependency tree minimal.

### Filesystem Watchers

Uses `notify` crate (v8) with a raw `mpsc` channel in `spawn_blocking`:

- **Config watcher**: monitors `config.toml` → calls `state.reload_config()`
- **Theme watcher**: monitors `colors.toml` → calls `state.reload_theme()`

The `notify-debouncer-full` crate is declared as a dependency but not imported or used. Debouncing happens implicitly through the watcher's event batching.

## Server (`src/server.rs`)

```rust
pub const PROTOCOL_VERSION: &str = "0.3";
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
    pub config_path: PathBuf,
}
```

`DaemonState::new` reads `git_ttl_ms` from `config.git.cache_ttl_ms` and passes it to `GitCache::new(ttl_ms)`.

`reload_theme` calls `ThemePalette::resolve_palette(&config)`, which respects `config.theme.source` and custom overrides — the same unified path used at startup.

`RwLock` allows concurrent read access from multiple connections with exclusive write access during reloads.

### Connection Handling

```
run_server(socket_path, state):
    bind UnixListener at socket_path
    loop:
        accept connection → spawn handle_connection task

handle_connection(stream, state):
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
| *(untagged)* | Backward-compatible: legacy `{"command":"..."}` or bare prompt JSON |

### Control Commands

| Command | Handler | Effect |
|---------|---------|--------|
| `reload_config` | `state.reload_config()` | Re-reads TOML from disk, updates `RwLock<Config>` |
| `reload_theme` | `state.reload_theme()` | Calls `ThemePalette::resolve_palette(&config)`, updates `RwLock<ThemePalette>` |
| `invalidate_git` | `state.git_cache.invalidate_all()` | Clears all cached git statuses |
| `shutdown` | Responds `{"status":"bye"}`, calls `exit(0)` | Immediate clean shutdown |
| `status` | Reads process info | Returns `status`, `pid`, `version`, `protocol_version`, `cwd` |
| `palette` | Reads in-memory palette | Returns theme colors as hex (`accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`) |
| `config_get` | Serializes in-memory config | Returns full config as JSON (requires `Serialize` on Config) |
| `config_set` | *(via typed `config` message)* | Accepts JSON patch in `rest.config`, merges into TOML on disk, calls `reload_config()` |

### Hello Response

```json
{"type":"hello","status":"ok","protocol_version":"0.3","server_version":"0.2.0"}
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

Default preview cwd is `~/projects/my-app`; default cols is 120.

### Palette Response

```json
{"type":"control","status":"ok","palette":{"accent":"#7aa2f7","foreground":"#c0caf5","muted":"#565f89","background":"#1a1b26","red":"#f7768e","green":"#9ece6a","yellow":"#e0af68","blue":"#7aa2f7"}}
```

## Config (`src/config.rs`)

TOML schema with layered defaults via `#[serde(default)]`. All Config structs derive `Serialize` (enables the `config_get` API). v0.3 adds ten new config structs: `ContainerConfig`, `PythonConfig`, `ToolchainConfig`, `NixConfig`, `K8sConfig`, `TimeConfig`, `BatteryConfig`, `NotificationConfig`, `TerminalConfig` (with nested `TitleConfig` and `ProgressConfig`). See [Configuration](config.md) for the full key reference.

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
| `git.enabled` | Yes | Toggles git segment |
| `git.mode` | Yes | adaptive/compact/expanded/hidden |
| `git.cache_ttl_ms` | Yes | Git cache TTL in milliseconds (default 5000) |
| `git.stale_display` | Yes | `stale` field set on stale/cold cache hits |
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
    pub fn from_hex(hex: &str) -> Option<Self>    // "#7aa2f7" → AnsiColor
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
    ttl: Duration,  // from config.git.cache_ttl_ms
}
```

```rust
pub fn new(ttl_ms: u64) -> Self
```

Cache key is the **repository root path**, not the cwd. Multiple cwds within the same repo share one cache entry.

### Three-Tier Cache Response (`get_status`)

1. **Fresh hit** (within TTL) → return cached status immediately
2. **Stale hit** (past TTL) → return cached status with `stale: true`, schedule async refresh
3. **Cold miss** (not in cache) → return minimal status (`is_repo: true`, empty branch) with `stale: true`, schedule async refresh

### schedule_refresh

Spawns a tokio task that:
1. Checks/sets the `in_flight` set for request coalescing (duplicate refreshes for the same repo are skipped)
2. Calls `fetch_git_status(repo_root).await`
3. Updates the cache with fresh status and timestamp
4. Removes the repo from `in_flight`

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
    pub name: String,
    pub content: String,
    pub compact_content: Option<String>,
    pub priority: u8,          // lower = more important
    pub min_width: u16,
    pub preferred_width: u16,
    pub hide_below_cols: u16,
    pub fg: Option<AnsiColor>,
    pub bg: Option<AnsiColor>,
    pub bold: bool,
    pub separator: Option<String>,
}
```

### LayoutEngine Resolution Algorithm

```
Input: segments[], terminal_cols
Output: ResolvedSegment[]

1. Filter: remove segments where cols < hide_below_cols
2. Calculate total preferred width (sum of preferred_width + 1-space separators)
3. If fits: return all segments at preferred width
4. Else: sort by priority (ascending = most important first)
5. Greedy fit: iterate sorted, add segment (using compact if preferred doesn't fit)
6. Re-sort by original_index to restore display order
```

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
    // collect_segments(ctx) → Vec<Segment>
    // LayoutPreset::apply_filter(segments, prompt.layout)
    // LayoutEngine::new(cols).resolve(segments) → Vec<ResolvedSegment>
    // format_line1(resolved, LayoutPreset::separator(layout)) → ANSI string
    // format_line2(prompt_char) → ANSI string (undercurl on error via TermCaps)
    // OSC 2 title when terminal.title.enabled
    // Wrap in OSC 133 markers when shell_integration is true
    // Single-line override when LayoutPreset::is_single_line(layout)
    // render_right(ctx) when prompt.right_prompt enabled
    // Build transient prompt if enabled
}
```

The `shell_integration` parameter controls OSC 133 emission. When `false`, prompt start/end markers are omitted (empty strings). Preview requests always pass `false`. Defaults to `true` when not specified in the prompt request.

### render_right

Returns an optional right prompt string when `config.prompt.right_prompt` is enabled:

- **Command duration** (muted) — shown when above threshold
- **Git branch** (accent, or muted when `git_status.stale`) — branch icon + name

Parts are space-separated. Returns `None` when both would be empty.

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
    // 5. Git (if config.git.enabled)
    // 6. Python env (if segments.python.enabled)
    // 7. Toolchain (if segments.toolchain.enabled)
    // 8. Nix (if segments.nix.enabled)
    // 9. K8s (if segments.k8s.enabled)
    // 10. Exit status (if enabled and exit_code != 0)
    // 11. Command duration (if enabled and above threshold)
    // 12. Jobs (if enabled and jobs > 0)
    // 13. Time (if segments.time.enabled)
    // 14. Battery (if segments.battery.enabled and below show_above threshold)
    segs
}
```

Note: `LayoutPreset::apply_filter()` in `render.rs` runs after collection and may remove segments not in the active preset.

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

Renders an OS icon based on `config.segments.os.icon`:

| Icon value | Glyph |
|------------|-------|
| `"arch"` | U+F303 |
| `"linux"` | U+F17C |
| `"omarchy"` | U+F312 |
| `"none"` | hidden |
| custom | literal string |

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

Not a layout `Segment` — rendered directly by `render.rs`:

- Success: `config.segments.character.success` (default `❯`) in accent color
- Error: `config.segments.character.error` (default `❯`) in red; undercurl when `TermCaps.has_undercurl`
- Transient: hardcoded muted `❯` (ignores character config)
