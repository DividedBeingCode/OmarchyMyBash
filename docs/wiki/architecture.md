# Architecture

[← Index](INDEX.md)

## Design Philosophy

Omarchy10k exists because shell prompts are a hot path that most tools treat as a cold one. Every keystroke that triggers a prompt render adds latency the user *feels*. The architecture is driven by three non-negotiable constraints:

1. **Sub-5ms prompt render** — The daemon pre-caches git status and holds config in memory. The Bash adapter communicates over a local Unix socket. No process spawn per prompt.
2. **Zero shell pollution** — All state lives in the daemon. The Bash adapter exports only the symbols it needs (`o10k_hook_add`, `__o10k_*` internals). No global variable namespace wars.
3. **Theme-native integration** — Colors come from the Omarchy desktop theme, not hardcoded palettes. When the user switches themes, every open shell updates within seconds.

## System Topology

```
┌──────────────────────────────────────────────────────────────────────┐
│  USER SHELL (Bash)                                                    │
│                                                                       │
│  .bashrc:  eval "$(omarchy10k init bash)"                            │
│       ↓                                                               │
│  shell/omarchy10k.bash (embedded, sourced)                           │
│    ├── Daemon lifecycle (start/stop omarchy10kd)                     │
│    ├── Hook broker (precmd/preexec/chpwd/shell_exit)                 │
│    ├── Command timing (EPOCHREALTIME)                                │
│    └── PROMPT_COMMAND → __o10k_render_prompt                         │
│         ├── Gather: $PWD, $?, duration, $COLUMNS, jobs               │
│         ├── JSON request → Unix socket                               │
│         ├── Parse response (omarchy10k parse-prompt or python3)      │
│         └── PS1 = response.left                                      │
├──────────────────────────────────────────────────────────────────────┤
│  UNIX SOCKET: $XDG_RUNTIME_DIR/omarchy10k-{$$}.sock                  │
│  Protocol: Newline-delimited JSON (NDJSON)                            │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10kd (Rust async daemon, one per shell session)               │
│    ├── Config (TOML, hot-reload via filesystem watcher)              │
│    ├── ThemePalette (Omarchy colors.toml, hot-reload)                │
│    ├── GitCache (porcelain v2 parse, TTL-based, per-repo)            │
│    ├── SegmentCollector → LayoutEngine → PromptRenderer              │
│    └── Server loop (accept → handle → respond, per connection)       │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10k (Rust CLI, thin client)                                   │
│    ├── prompt → socket request → print PS1                           │
│    ├── init bash → print shell/omarchy10k.bash                       │
│    ├── doctor → system diagnostics                                   │
│    ├── reload → send reload_config command                           │
│    ├── benchmark → repeated prompt requests, latency stats           │
│    ├── debug → send status command                                   │
│    └── parse-prompt (hidden) → extract left from JSON stdin          │
├──────────────────────────────────────────────────────────────────────┤
│  QUATTRO BAR PLUGIN (QML/JS, desktop integration)                     │
│    ├── BarWidget.qml → "❯" glyph in system bar                      │
│    ├── Panel.qml → 4-tab Control Center                             │
│    │   ├── Config read/write (cat/write TOML via Process)            │
│    │   ├── Socket discovery + daemon IPC                             │
│    │   └── Tool detection (command -v)                               │
│    └── Model.js → stateless TOML parser, protocol helpers            │
├──────────────────────────────────────────────────────────────────────┤
│  OMARCHY THEME SYSTEM                                                 │
│    ├── templates/omarchy10k.toml.tpl → rendered to colors.toml       │
│    └── hooks/theme-set → broadcasts reload_theme to all sockets      │
└──────────────────────────────────────────────────────────────────────┘
```

## Data Flow: Prompt Render

This is the critical path. Every operation here adds latency the user perceives.

```
1. User presses Enter in Bash
2. Bash fires PROMPT_COMMAND (or ble.sh PRECMD hook)
3. __o10k_render_prompt() captures:
   - exit_code = $?
   - cmd_duration_ms (from EPOCHREALTIME delta)
   - cwd = $PWD
   - cols = $COLUMNS
   - jobs = $(jobs -p | wc -l)
4. Bash adapter sends JSON over Unix socket:
   {"cwd":"/home/user/project","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0}
5. omarchy10kd handle_connection receives the line
6. GitCache.get_status(cwd) — returns cached if TTL valid, else shells out to git
7. PromptRenderer.render() builds SegmentContext, calls collect_segments()
8. Each segment render(ctx) returns Option<Segment> with content, colors, priority
9. LayoutEngine.resolve(segments) — priority-based width fitting:
   a. Try all segments at preferred width
   b. If exceeds terminal cols: sort by priority, greedily fit with compact forms
   c. Restore original display order via original_index
10. format_line1() — ANSI color codes, 1-space separators between segments
11. format_line2() — prompt character (❯) with success/error color
12. OSC 133 wrapping — \e]133;A\a...\e]133;B\a for shell integration
13. JSON response: {"left":"<ansi>","transient":"<ansi>","right":null,"git_stale":false}
14. Bash adapter extracts "left" via `omarchy10k parse-prompt` or python3
15. PS1 = extracted left string
16. Bash renders the prompt
```

**Latency budget breakdown:**
- Socket I/O: ~0.1ms (local Unix socket)
- Git cache hit: ~0ms (HashMap lookup + TTL check)
- Git cache miss: 5-50ms (subprocess `git status --porcelain=v2`)
- Segment collection: ~0.01ms (pure computation)
- Layout resolution: ~0.01ms (priority sort + width arithmetic)
- ANSI formatting: ~0.01ms (string concatenation)
- JSON parse in Bash: ~1-3ms (Rust parse-prompt) or ~5-10ms (python3 fallback)

## Data Flow: Config Change via Quattro

```
1. User clicks a control in Panel.qml (e.g., switches git mode)
2. setConfigValue("git.mode", "compact") updates _configFlat
3. saveTimer fires after 300ms debounce
4. _flushSave() calls Model.buildTOML(_configFlat)
5. configWriter Process writes TOML to ~/.config/omarchy10k/config.toml
6. On write completion, sendDaemonCommand("reload_config") via Socket
7. Daemon DaemonState.reload_config() re-reads config from disk
8. Next prompt render uses updated config values
```

## Data Flow: Theme Switch

```
1. User switches Omarchy desktop theme
2. Omarchy theme engine renders templates/omarchy10k.toml.tpl
3. Output written to ~/.local/state/omarchy/current/theme/colors.toml
4. hooks/theme-set fires, globs all omarchy10k-*.sock files
5. Sends {"command":"reload_theme"} to each socket (fan-out)
6. Each daemon calls ThemePalette::load_omarchy(), updates Arc<RwLock<ThemePalette>>
7. Next prompt render uses new colors
```

## Per-Shell Daemon Model

Each Bash session gets its own daemon process and socket. This is a deliberate design choice:

| Concern | Per-shell daemon | Shared daemon |
|---------|-----------------|---------------|
| Isolation | Complete. One shell's crash doesn't affect others | Shared failure domain |
| Git cache | Per-shell TTL. Relevant to current shell's cwd | Cross-session cache pollution |
| Config | Per-shell reload. Can test configs in one shell | Must coordinate config versions |
| Lifecycle | Tied to shell PID. Auto-cleanup on shell exit | Requires explicit lifecycle management |
| Resource cost | ~2-4MB RSS per daemon. Trivial on modern systems | Lower total memory |

The daemon starts with `O10K_PARENT_PID=$$` and monitors the parent process via `kill(ppid, 0)` every 2 seconds. When the shell exits, the daemon detects it, removes its socket, and exits cleanly. The Bash adapter also sends `shutdown` on `EXIT` trap as belt-and-suspenders.

Socket naming: `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock`

## Segment Plugin Architecture

Segments are the building blocks of the prompt. Each segment is a pure function:

```rust
pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment>
```

`SegmentContext` carries everything a segment needs: cwd, home dir, exit code, duration, terminal width, job count, SSH status, git status, config, and theme palette. A segment returns `None` to hide itself.

Each `Segment` declares:
- `content` / `compact_content` — full and abbreviated display forms
- `priority` — lower number = more important, kept when space is tight
- `hide_below_cols` — minimum terminal width to show at all
- `fg` / `bg` / `bold` — ANSI color attributes
- `min_width` / `preferred_width` — layout hints

The `LayoutEngine` performs a two-pass resolution:
1. **Fit check** — sum all preferred widths; if they fit terminal width, use all
2. **Priority compress** — sort by priority, greedily add segments using compact forms, stop when terminal width exhausted
3. **Order restore** — sort resolved segments back to original display order

Current segments and their priorities:

| Segment | Priority | Hide below | Color logic |
|---------|----------|------------|-------------|
| Directory | 10 | 0 cols | Accent, bold |
| Git | 20 | 30 cols | Green=clean, yellow=dirty, red=conflicted, muted=stale |
| Exit Status | 30 | 0 cols | Red |
| Command Duration | 50 | 40 cols | Yellow |

## Dependency Graph

### Rust Workspace Dependencies

| Crate | Resolved Version | Used by | Purpose |
|-------|-----------------|---------|---------|
| `tokio` | 1.53.1 | Both | Async runtime, Unix socket I/O |
| `serde` + `serde_json` | 1.0.229 / 1.0.151 | Both | JSON serialization for protocol |
| `toml` | 0.8.23 | Both | TOML config parsing |
| `clap` | 4.6.6 | CLI only | Command-line argument parsing |
| `notify` | 8.2.0 | Daemon only | Filesystem watcher for config/theme hot-reload |
| `tracing` + `tracing-subscriber` | 0.1.44 / 0.3.23 | Both | Structured logging |
| `directories` | 6.0.0 | Both | XDG path resolution |
| `unicode-width` | 0.2.2 | Daemon only | East Asian width-aware string measurement |
| `thiserror` | 2.0.20 | Both | Typed error definitions |
| `anyhow` | 1.0.104 | Both | Error propagation |

### External Runtime Dependencies

| Tool | Required | Used by | Purpose |
|------|----------|---------|---------|
| `git` | For git segments | Daemon | `git status --porcelain=v2 --branch`, `git stash list` |
| `socat` | Preferred | Bash adapter, theme hook | Unix socket I/O |
| `python3` | Fallback | Bash adapter, theme hook | Socket I/O + JSON parsing when socat unavailable |
| `bash` ≥ 4.4 | Required | Adapter | PS0 preexec, EPOCHREALTIME (5.0+), array PROMPT_COMMAND (5.1+) |
| `ble.sh` ≥ 4 | Optional | Adapter | Enhanced hooks, transient prompt, blehook integration |
| Omarchy desktop | Optional | Theme system | Theme colors, Quattro bar |

## Internal Module Graph

```
                           ┌─────────┐
                           │  main   │
                           └────┬────┘
                    ┌───────────┼───────────┐
                    ▼           ▼           ▼
              ┌──────────┐ ┌────────┐ ┌───────┐
              │  server   │ │ config │ │ theme │
              └─────┬─────┘ └────┬───┘ └───┬───┘
                    │            │          │
                    ▼            │          │
              ┌──────────┐      │          │
              │  render   │◄────┘──────────┘
              └─────┬─────┘
           ┌────────┼────────┐
           ▼        ▼        ▼
      ┌────────┐ ┌──────┐ ┌────────────┐
      │segments│ │layout│ │    git     │
      │  /mod  │ └──────┘ └────────────┘
      └────┬───┘
   ┌───┬───┼───┬───────────┐
   ▼   ▼   ▼   ▼           ▼
  dir  git exit cmd_dur  character
```

All modules are private to the daemon binary (no `lib.rs`). This is intentional — the daemon is a self-contained service, not a library.
