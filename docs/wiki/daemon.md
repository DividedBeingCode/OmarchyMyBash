# Daemon Reference (`omarchy10kd`)

[← Index](INDEX.md) | [Architecture](architecture.md) | [Protocol](protocol.md)

The daemon is the computational core of Omarchy10k. It holds all state, renders all prompts, and serves multiple concurrent connections over a Unix domain socket. One daemon instance runs per Bash session.

## Binary Entry Point (`src/main.rs`)

Bootstraps everything: config loading, tracing initialization, theme palette, shared state, filesystem watchers, parent-process monitor, and the socket server.

### Startup Sequence

```
1. Init tracing subscriber (env-filter from config.daemon.log_level)
2. Load Config from ~/.config/omarchy10k/config.toml (or defaults)
3. Load ThemePalette based on config.theme.source:
   - "omarchy" → load from ~/.local/state/omarchy/current/theme/colors.toml
   - "custom"  → defaults + custom overrides from config
   - "hybrid"  → omarchy base + custom overrides
   - other     → hardcoded Tokyo Night defaults
4. Build DaemonState (Arc): config, palette, git cache (5s TTL), config path
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

`RwLock` allows concurrent read access from multiple connections with exclusive write access during reloads.

### Connection Handling

```
run_server(socket_path, state):
    bind UnixListener at socket_path
    loop:
        accept connection → spawn handle_connection task

handle_connection(stream, state):
    let reader = BufReader::new(stream)
    loop:
        read line from reader (EOF → break)
        if line contains "command" field:
            dispatch control command
        else:
            parse as PromptRequest
            get_status(cwd) from GitCache
            PromptRenderer::render(...) → PromptResponse
            serialize and write response + newline
```

Connections are persistent — the server reads lines in a loop until EOF. This allows the Bash adapter to reuse connections if it wants (though currently it connects per-request).

### Control Commands

| Command | Handler | Effect |
|---------|---------|--------|
| `reload_config` | `state.reload_config()` | Re-reads TOML from disk, updates `RwLock<Config>` |
| `reload_theme` | `state.reload_theme()` | Calls `ThemePalette::load_omarchy()`, updates `RwLock<ThemePalette>` |
| `invalidate_git` | `state.git_cache.invalidate_all()` | Clears all cached git statuses |
| `shutdown` | Responds `{"status":"bye"}`, calls `exit(0)` | Immediate clean shutdown |
| `status` | Reads process info | Returns `{"status":"ok","pid":N,"version":"0.1.0"}` |

**Known issue:** `reload_theme` always calls `load_omarchy()` regardless of `config.theme.source`. Custom/hybrid overrides from config are not re-applied after a theme reload.

## Config (`src/config.rs`)

TOML schema with layered defaults via `#[serde(default)]`. See [Configuration](config.md) for the full key reference.

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
| `prompt.layout` | No | Always uses "omarchy" layout |
| `prompt.transient` | Yes | Transient prompt character |
| `prompt.newline` | Yes | Two-line vs one-line |
| `prompt.right_prompt` | No | `right` field always `None` |
| `theme.source` | Yes | omarchy/custom/hybrid/terminal |
| `theme.custom.*` | Partial | Applied on startup, not on reload_theme |
| `directory.strategy` | No | Always smart truncate |
| `directory.max_length` | Yes | Truncation limit |
| `directory.repo_root_style` | No | Always bold |
| `git.enabled` | Yes | Toggles git segment |
| `git.mode` | Yes | adaptive/compact/expanded/hidden |
| `git.stale_display` | No | `stale` field never set true |
| `git.max_threads` | No | Single-threaded git subprocess |
| `segments.os` | No | No OS segment implemented |
| `segments.exit_status` | Yes | Full |
| `segments.command_duration` | Yes | Full |
| `segments.jobs` | No | jobs passed in request but no segment |
| `segments.ssh` | No | SSH detected but no segment |
| `segments.character` | Yes | Success/error glyphs |
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

## Git (`src/git.rs`)

### GitCache

```rust
pub struct GitCache {
    cache: Arc<RwLock<HashMap<PathBuf, CachedStatus>>>,
    ttl: Duration,  // hardcoded to 5 seconds
}
```

Cache key is the **repository root path**, not the cwd. Multiple cwds within the same repo share one cache entry.

### Status Fetching

1. `find_repo_root(cwd)` — walks up directory tree looking for `.git` (file or directory). Handles worktrees by reading `gitdir:` pointer.
2. `fetch_git_status(repo_root)` — runs `git --no-optional-locks status --porcelain=v2 --branch` and `git stash list`
3. `parse_porcelain_v2(output)` — extracts branch, upstream, ahead/behind, staged/unstaged/untracked/conflicted counts
4. `detect_git_action(git_dir)` — checks for `.git/MERGE_HEAD`, `rebase-merge/`, `CHERRY_PICK_HEAD`, `BISECT_LOG`, `REVERT_HEAD`

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
    pub repo_root: String,
    pub stale: bool,             // always false in current code
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

## Render (`src/render.rs`)

### PromptRenderer

Orchestrates the full render pipeline:

```rust
pub fn render(&self, cwd, exit_code, cmd_duration_ms, cols, jobs, git_status) -> PromptResponse {
    // Build SegmentContext with all inputs + config + palette
    // collect_segments(ctx) → Vec<Segment>
    // LayoutEngine::new(cols).resolve(segments) → Vec<ResolvedSegment>
    // format_line1(resolved_segments) → ANSI string
    // If newline mode: format_line2(prompt_char) → ANSI string
    // Wrap everything in OSC 133 markers
    // Build transient prompt if enabled
}
```

### OSC 133 Shell Integration

```
Prompt start: \x01\x1b]133;A\x07\x02
Prompt end:   \x01\x1b]133;B\x07\x02
```

OSC 133 marks prompt boundaries for terminal features like command click-to-select (iTerm2, WezTerm, Kitty). The `\x01`/`\x02` wrappers are Bash-specific non-printing character delimiters.

### PromptResponse

```rust
pub struct PromptResponse {
    pub left: String,              // The full prompt string with ANSI codes
    pub right: Option<String>,     // Always None currently
    pub transient: Option<String>, // Muted "❯" when transient enabled
    pub git_stale: bool,           // Always false currently
}
```

## Segments (`src/segments/`)

### Segment Collection Order

Defined in `segments/mod.rs`:

```rust
pub fn collect_segments(ctx: &SegmentContext<'_>) -> Vec<Segment> {
    let mut segs = Vec::new();
    // 1. Directory (always attempts)
    // 2. Git (if config.git.enabled)
    // 3. Exit status (if enabled and exit_code != 0)
    // 4. Command duration (if enabled and above threshold)
    segs
}
```

### Directory Segment

Smart path truncation with home substitution:

- `~` replaces `$HOME` prefix
- `smart_truncate(path, max_length)`: keeps first and last path components, unique prefixes for middle directories
- Preserves git repo root directories (checks for `.git`)
- Fallback: `first/…/last`

### Git Segment

Four display modes controlled by `config.git.mode`:

| Mode | Logic |
|------|-------|
| `hidden` | Returns `None` |
| `compact` | Always compact format |
| `expanded` | Expanded format, compact fallback for layout |
| `adaptive` | Expanded when dirty, compact when clean |

Symbol vocabulary: `✓` clean, `+N` staged, `!N` unstaged, `?N` untracked, `×N` conflicted, `⇡N` ahead, `⇣N` behind, `≡N` stashes

Branch name truncated to 20 characters with `…` suffix.

### Exit Status Segment

Only shown for non-zero exit codes. Expanded format maps common codes:

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
- Error: `config.segments.character.error` (default `❯`) in red
- Transient: hardcoded muted `❯` (ignores character config)
