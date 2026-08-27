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
│    ├── Instant prompt cache (~/.cache/omarchy10k/last_prompt)          │
│    ├── Hook broker (precmd/preexec/chpwd/shell_exit)                 │
│    ├── Command timing (EPOCHREALTIME)                                │
│    └── PROMPT_COMMAND → __o10k_render_prompt                         │
│         ├── Gather: $PWD, $?, duration, $COLUMNS, jobs               │
│         ├── [preferred] JSON request → bridge coproc stdin           │
│         │    └── bridge forwards to daemon socket, NUL-terminated    │
│         │        prompt on coproc stdout → PS1 directly              │
│         ├── [fallback] JSON request → Unix socket (socat/python3)    │
│         └── [fallback] Parse response (parse-prompt or python3)      │
├──────────────────────────────────────────────────────────────────────┤
│  BRIDGE COPROCESS (preferred prompt path)                             │
│    omarchy10k bridge — persistent Rust process, one per shell         │
│    ├── stdin: JSON requests from Bash adapter                        │
│    ├── stdout: NUL-terminated prompt strings                         │
│    └── holds open connection to daemon Unix socket                   │
├──────────────────────────────────────────────────────────────────────┤
│  UNIX SOCKET: $XDG_RUNTIME_DIR/omarchy10k-{$$}.sock                  │
│  Protocol: Typed NDJSON with version negotiation (type, id fields)   │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10kd (Rust async daemon, one per shell session)               │
│    ├── Config (TOML, hot-reload via filesystem watcher)              │
│    ├── ThemePalette (Omarchy colors.toml, hot-reload)                │
│    ├── TermCaps (per-terminal feature matrix via env detection)      │
│    ├── GitCache (porcelain v2 parse, worktree detection, TTL)        │
│    ├── SegmentCollector → LayoutPreset filter → LayoutEngine         │
│    │    → PromptRenderer (OSC 2 title, OSC 8 links, undercurl)      │
│    └── Server loop (prompt / preview / palette / config / control)   │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10k (Rust CLI, thin client)                                   │
│    ├── bridge → coprocess mode: stdin JSON, stdout NUL-terminated    │
│    ├── prompt → socket request → print PS1                           │
│    ├── init bash → print shell/omarchy10k.bash                       │
│    ├── doctor → system diagnostics                                   │
│    ├── reload → send reload_config command                           │
│    ├── benchmark → repeated prompt requests, latency stats           │
│    ├── benchmark-shell → end-to-end shell prompt latency             │
│    ├── debug → send status command                                   │
│    └── parse-prompt (hidden) → extract left from JSON stdin          │
├──────────────────────────────────────────────────────────────────────┤
│  QUATTRO BAR PLUGIN (QML/JS, desktop integration)                     │
│    ├── BarWidget.qml → "❯" glyph in system bar                      │
│    ├── Panel.qml → 4-tab Control Center                             │
│    │   ├── Config read/write (config_get/config_set via daemon IPC)  │
│    │   ├── Live preview (preview message, simulated git context)     │
│    │   ├── Theme preview (palette control command → hex colors)      │
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
0. [Instant prompt] On shell init, if ~/.cache/omarchy10k/last_prompt exists, PS1 is set immediately
   from the cached prompt (P10k-style instant prompt). After each render, the new prompt is
   atomically written back to the cache in the background.
1. User presses Enter in Bash
2. Bash fires PROMPT_COMMAND (or ble.sh PRECMD hook)
3. __o10k_render_prompt() captures:
   - exit_code = $?
   - cmd_duration_ms (from EPOCHREALTIME delta)
   - cwd = $PWD
   - cols = $COLUMNS
   - jobs = $(jobs -p | wc -l)
4. Bash adapter sends JSON prompt request (example payload):
   {"cwd":"/home/user/project","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0}
   - **Bridge path (preferred):** request → coproc stdin → bridge forwards to daemon socket → NUL-terminated prompt on coproc stdout → PS1 set directly (no parse-prompt needed)
   - **Fallback path:** JSON over Unix socket via socat or python3
5. omarchy10kd handle_connection receives the message (typed `prompt` with correlated `id`)
6. GitCache.get_status(cwd) — returns cached if TTL valid; on miss returns stale entry immediately (`stale: true`) while background refresh runs
7. PromptRenderer.render() builds SegmentContext, calls collect_segments()
8. Each segment render(ctx) returns Option<Segment> with content, colors, priority
9. LayoutPreset::apply_filter() — keeps only segments allowed by prompt.layout preset
10. LayoutEngine.resolve(segments) — priority-based width fitting:
   a. Try all segments at preferred width
   b. If exceeds terminal cols: sort by priority, greedily fit with compact forms
   c. Restore original display order via original_index
11. format_line1() — ANSI color codes, preset-specific separators (space, │, powerline arrow)
12. format_line2() — prompt character (❯) with success/error color; undercurl on error when TermCaps allows
13. OSC 2 terminal title — \e]2;{short_cwd}\a when terminal.title.enabled
14. OSC 133 wrapping — \e]133;A\a...\e]133;B\a for shell integration (skipped in preview mode)
15. JSON response: {"left":"<ansi>","transient":"<ansi>","right":null,"git_stale":false,"stale":false}
    - `stale: true` when git cache entry is expired but returned immediately (stale-while-revalidate)
16. Bash adapter extracts prompt string:
    - Bridge path: NUL-terminated stdout → PS1 directly
    - Fallback: extract "left" via `omarchy10k parse-prompt` or python3
17. PS1 = prompt string; cache written to ~/.cache/omarchy10k/last_prompt (background)
18. Bash renders the prompt
```

**Protocol:** Messages use typed NDJSON with version negotiation. Each message has `type` (`hello`, `control`, `prompt`, `preview`, `config`, `error`), `id` for request/response correlation, and `version` (currently `"0.3"`) exchanged during the `hello` handshake. The `preview` type renders a simulated prompt (no OSC markers) for Quattro live preview. The `palette` control command returns current theme colors as hex.

**Latency budget breakdown:**
- Bridge coprocess path: eliminates fork/exec overhead for socket I/O on every prompt
- Socket I/O: ~0.1ms (local Unix socket; bridge holds connection open)
- Git cache hit: ~0ms (HashMap lookup + TTL check)
- Git cache miss (stale-while-revalidate): ~0ms blocking (returns stale entry with `stale: true`; background refresh 5-50ms)
- Segment collection: ~0.01ms (pure computation)
- Layout resolution: ~0.01ms (priority sort + width arithmetic)
- ANSI formatting: ~0.01ms (string concatenation)
- JSON parse in Bash (fallback only): ~1-3ms (Rust parse-prompt) or ~5-10ms (python3 fallback)

## Data Flow: Config Change via Quattro

```
1. User clicks a control in Panel.qml (e.g., switches git mode)
2. setConfigValue("git.mode", "compact") updates _configFlat
3. saveTimer fires after 300ms debounce
4. _flushSave() sends config_set via daemon socket (JSON patch)
5. Daemon applies patch, writes TOML to ~/.config/omarchy10k/config.toml
6. Daemon reloads config in memory (no separate reload_config needed)
7. Next prompt render uses updated config values
```

Quattro no longer reads or writes the TOML file directly. All config access goes through the daemon Config API (`config_get` / `config_set`) over the Unix socket.

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

Current segments and their priorities (collection order in `segments/mod.rs`):

| Segment | Priority | Hide below | Color logic |
|---------|----------|------------|-------------|
| OS | 5 | 40 cols | Accent |
| SSH | 8 | 50 cols | Yellow |
| Container | 7 | 50 cols | Accent (Docker/Podman/Toolbox/Distrobox) |
| Directory | 10 | 0 cols | Accent, bold; OSC 8 hyperlinks when TermCaps.has_osc8 |
| Git | 20 | 30 cols | Green=clean, yellow=dirty, red=conflicted, muted=stale; worktree name |
| Python Env | 35 | 50 cols | Yellow (VIRTUAL_ENV / CONDA_DEFAULT_ENV) |
| Toolchain | 40 | 60 cols | Foreground (Mise env vars: node/python/ruby/go/rust) |
| Nix | 36 | 50 cols | Blue (IN_NIX_SHELL pure/impure) |
| K8s | 42 | 60 cols | Blue (kubeconfig current-context) |
| Exit Status | 30 | 0 cols | Red; undercurl when TermCaps.has_undercurl |
| Command Duration | 50 | 40 cols | Yellow |
| Jobs | 45 | 50 cols | Blue |
| Time | 55 | 40 cols | Muted (localtime via libc, no chrono) |
| Battery | 56 | 40 cols | Green/yellow/red by threshold (sysfs BAT0/BAT1) |

Layout presets (`prompt.layout`) filter which segments appear and control separators. The `omarchy` and `powerline` presets include all segments above; `dense` omits container/python/toolchain/nix/k8s/time/battery; `minimal` is directory-only; `pure` is directory + git. Presets `minimal` and `dense` force single-line mode regardless of `prompt.newline`.

## Terminal Capability Detection

The daemon detects the active terminal via environment variables (`TERM_PROGRAM`, `KITTY_WINDOW_ID`, `GHOSTTY_RESOURCES_DIR`) and exposes a per-terminal feature matrix through `TermCaps`:

| Terminal | OSC 7 | OSC 8 | OSC 52 | OSC 777 | Undercurl | Kitty gfx | Sixel | Sync |
|----------|-------|-------|--------|---------|-----------|-----------|-------|------|
| Ghostty | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ |
| Foot | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | ✓ |
| Kitty | ✓ | ✓ | ✓ | — | ✓ | ✓ | — | ✓ |
| WezTerm | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Alacritty | ✓ | — | ✓ | — | — | — | — | — |
| Unknown | ✓ | — | — | — | — | — | — | — |

Features gate rendering behavior: OSC 8 wraps directory paths in clickable hyperlinks; undercurl (`\x1b[4:3m`) styles exit status and error prompt characters on capable terminals.

## Preview and Palette APIs

**Preview** (`type: "preview"`) renders a prompt with simulated context — no git subprocess, no OSC 133 markers. Quattro sends optional fields (`cwd`, `exit_code`, `git_branch`, `git_staged`, `git_unstaged`, `cols`, `in_ssh`, etc.) to drive live prompt preview in the Control Center.

**Palette** (`command: "palette"`) returns the daemon's in-memory `ThemePalette` as hex strings (`accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`). Quattro uses this for theme color swatches without reading `colors.toml` directly.

## Instant Prompt Caching

The Bash adapter (not the daemon) implements instant prompt via a disk cache at `$XDG_CACHE_HOME/omarchy10k/last_prompt` (default `~/.cache/omarchy10k/last_prompt`). On shell init, if the cache file exists, `PS1` is set immediately before the daemon starts. After each successful render, the prompt is written atomically (`tmp` + `mv`) in a background subshell. This eliminates the blank-prompt flash on new shell startup while the daemon warms up.

## Git Worktree Detection

Git status fetching now calls `detect_worktree()` after porcelain v2 parse. When `.git` is a file (linked worktree) or contains a `commondir` entry, the worktree directory name is stored in `GitStatus.worktree` and displayed in the git segment after the branch name.

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
| `socat` | Fallback | Bash adapter, theme hook | Unix socket I/O when bridge unavailable |
| `python3` | Fallback | Bash adapter, theme hook | Socket I/O + JSON parsing when bridge/socat unavailable |
| `bash` ≥ 4.4 | Required | Adapter | PS0 preexec, EPOCHREALTIME (5.0+), array PROMPT_COMMAND (5.1+) |
| `ble.sh` ≥ 4 | Optional | Adapter | Enhanced hooks, transient prompt, blehook integration |
| Omarchy desktop | Optional | Theme system | Theme colors, Quattro bar |

## Internal Module Graph

```
                           ┌─────────┐
                           │  main   │
                           └────┬────┘
                    ┌───────────┼───────────┬───────────┐
                    ▼           ▼           ▼           ▼
              ┌──────────┐ ┌────────┐ ┌───────┐ ┌──────────┐
              │  server   │ │ config │ │ theme │ │ terminal │
              └─────┬─────┘ └────┬───┘ └───┬───┘ └────┬─────┘
                    │            │          │          │
                    ▼            │          │          │
              ┌──────────┐      │          │          │
              │  render   │◄────┘──────────┘          │
              └─────┬─────┘                          │
           ┌────────┼────────┐                        │
           ▼        ▼        ▼                        │
      ┌────────┐ ┌──────┐ ┌────────────┐             │
      │segments│ │layout│ │    git     │             │
      │  /mod  │ └──────┘ └────────────┘             │
      └────┬───┘                                      │
   ┌───┬───┼───┬───┬───┬───┬───┬───┬───┬───┬───┐     │
   ▼   ▼   ▼   ▼   ▼   ▼   ▼   ▼   ▼   ▼   ▼   ▼     │
  os ssh ctr dir git py tc nix k8s exit dur jobs     │
                                              time bat│
   character ◄──────────────────────────────────────┘
```

All modules are private to the daemon binary (no `lib.rs`). This is intentional — the daemon is a self-contained service, not a library.
