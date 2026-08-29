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
│  64 KiB max NDJSON frame; oversized frames rejected, socket survives │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10kd (Rust async daemon, one per shell session)               │
│    ├── Config (TOML, hot-reload via filesystem watcher)              │
│    ├── ThemePalette (Omarchy colors.toml, hot-reload)                │
│    ├── TermCaps (per-terminal feature matrix via env detection)      │
│    ├── GitCache (porcelain v2 parse, worktree detection, TTL,        │
│    │            LRU-bounded eviction)                                │
│    ├── Looks (curated + user [looks.<name>] bundles, palette merge)  │
│    ├── ScriptExec (~/.config/omarchy10k/scripts, 30s timeout,        │
│    │            traversal guard)                                     │
│    ├── PluginRegistry (~/.config/omarchy10k/plugins/<name>/          │
│    │            plugin.toml, env/command tiers,                      │
│    │            `plugin.<plugin>.<segment>` names, reload pickup)    │
│    ├── Profiles (per-repo .o10k.toml, allowlisted, 30s TTL cache)    │
│    ├── SegmentCollector → style-preset filter → LayoutEngine         │
│    │    → PromptRenderer (OSC 2 title, OSC 8 links, undercurl,       │
│    │      right rail via resolve_right_rail → render_right)          │
│    └── Server loop (prompt / preview / config / statusline /         │
│         control: looks, looks_apply, looks_save, palettes, defaults, │
│         script_list/run, status, palette, reload, invalidate_git)    │
├──────────────────────────────────────────────────────────────────────┤
│  omarchy10k (Rust CLI, thin client)                                   │
│    ├── bridge → coprocess mode: stdin JSON, stdout NUL-terminated    │
│    ├── prompt → socket request → print PS1                           │
│    ├── init bash → print shell/omarchy10k.bash                       │
│    ├── look list|apply|save → looks / looks_apply / looks_save       │
│    ├── script list|run → script_list / script_run (local fallback)   │
│    ├── hook-event <name> → omarchy-hook or hooks/<event>.d walk      │
│    ├── statusline → Claude Code statusLine render                    │
│    ├── intro → first-run themed render; configure → setup wizard     │
│    ├── doctor → system diagnostics                                   │
│    ├── reload → send reload_config command                           │
│    ├── benchmark / benchmark-shell → latency stats                   │
│    ├── debug → send status command                                   │
│    └── parse-prompt (hidden) → extract left from JSON stdin          │
├──────────────────────────────────────────────────────────────────────┤
│  QUATTRO BAR PLUGIN (QML/JS, desktop integration)                     │
│    ├── BarWidget.qml → "❯" glyph in system bar + bar badges          │
│    │   (daemon-status dot, git dirty dot, agents badge, long-cmd chip)│
│    ├── Panel.qml → Control Center, 4-bucket rail                     │
│    │   (LOOKS · STYLE · BEHAVIOR · SYSTEM)                           │
│    │   ├── Looks cards → looks_apply; Save-as-Look → looks_save      │
│    │   ├── Config read/write (config_get/config_set, delta saves)    │
│    │   ├── Live preview (preview message, look dry-run override)     │
│    │   ├── Palettes/defaults (palette + defaults control verbs)      │
│    │   ├── Socket discovery + daemon IPC (spawns headless daemon)    │
│    │   └── PanelLooks/PanelStyle/PanelBehavior/PanelSystem.qml →     │
│    │       extracted bucket panes; PanelKit.qml → shared unbound     │
│    │       components (bound inline components cannot instantiate    │
│    │       cross-file — the KEY FINDING of the C4 decomposition)     │
│    ├── Gallery.qml → full-screen Looks gallery overlay + Looks Studio│
│    │   editor (palette/cycle rows, Gradient Ramp Designer)           │
│    ├── SessionPicker.qml → live session list overlay                 │
│    ├── Service.qml → persistent connection hub for all sockets       │
│    └── Model.js → TOML parser, CONFIG_MAP, protocol helpers          │
├──────────────────────────────────────────────────────────────────────┤
│  OMARCHY THEME SYSTEM + DESKTOP HOOKS                                 │
│    ├── templates/omarchy10k.toml.tpl → rendered to colors.toml       │
│    └── hooks/<event>.d/omarchy10k drop-ins:                          │
│        theme-set / font-set → reload_theme fan-out                   │
│        post-update → omarchy10k update --no-pull + invalidate_git    │
│        battery-low → omarchy-notification-send toast                 │
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
   {"cwd":"/home/user/project","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0,
    "shell_integration":true,"env":{"VIRTUAL_ENV":"~/venv","vi_mode":"insert"}}
   - `env` is a frozen allowlist built by __o10k_env_json (zero forks); the bash KEYMAP value rides along as vi_mode when the shell reports it
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

**Protocol:** Messages use typed NDJSON with version negotiation. Each message has `type` (`hello`, `prompt`, `preview`, `config`, `statusline`, `control`, `error`), `id` for request/response correlation, and `version` (currently `"0.5"`) exchanged during the `hello` handshake. Prompt requests carry an `env` object — a shell-side allowlist (VIRTUAL_ENV, CONDA_DEFAULT_ENV, MISE_*, IN_NIX_SHELL, …) plus `vi_mode` (the bash `KEYMAP` value) — so environment-derived segments and the vi-mode prompt character track the live shell. The `preview` type renders a simulated prompt (no OSC markers) and accepts a `look` override for Look dry-runs. Control verbs include `palette`, `looks` / `looks_apply` / `looks_save`, `palettes`, `defaults`, `script_list` / `script_run`, `reload`, `invalidate_git`, and the enriched `status` snapshot. Single frames are capped at 64 KiB.

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

## Data Flow: Look Apply (protocol 0.5)

```
1. User clicks a Look card in Panel.qml or Gallery.qml, or runs `omarchy10k look apply <name>`
2. The client sends `looks_apply {name, transient}` (the `looks` verb lists curated + user Looks)
3. looks.rs resolves the Look: user `[looks.<name>]` entries shadow curated names; glyph
   shortcuts expand and the palette directive merges into a `theme` sub-patch, producing a
   config_set-shaped patch
4. Persistent apply: the patch flows through the daemon's atomic config_set path — merged,
   written to ~/.config/omarchy10k/config.toml, hot-reloaded in memory
   Transient apply (`--transient` / Try): looks::apply_transient patches the in-memory Config
   only — no file write; the next config reload reverts it
5. `looks_save {name, label}` snapshots the current config as a `[looks.<name>]` entry
   (option fields normalized to preset defaults so the TOML stays valid)
6. `palettes` returns the curated palette set (moved daemon-side from Model.js); `defaults`
   returns Config::default() for the panel's modified-vs-default ink bars and per-row reset
7. Dry-run renders: `preview` accepts a `look` override, so panel and Gallery cards render a
   Look without applying it
```

## Data Flow: Quick Actions (User Scripts)

```
1. User drops executable scripts in ~/.config/omarchy10k/scripts/*.sh
2. `omarchy10k script list` → daemon `script_list` verb → sorted registry of valid scripts
3. `omarchy10k script run <name>` → daemon `script_run {name}` → script_exec resolves the path
   (traversal-guarded: no `/`, no `..`, no leading `.`; must be a regular executable file)
4. The script runs with a hard 30s timeout by default (`timeout_secs` overridable; killed on
   expiry); trimmed stdout is returned; non-zero exit carries stderr in the error
5. No reachable daemon → the CLI falls back to running the script locally
```

## Data Flow: Desktop Hooks

```
1. An Omarchy desktop event fires (theme-set, font-set, battery-low, post-update)
2. Omarchy's hook system runs every consumer in ~/.config/omarchy/hooks/<event>.d/ —
   install.sh drops hooks/<event> there as <event>.d/omarchy10k
3. theme-set / font-set → {"command":"reload_theme"} fan-out to every omarchy10k-*.sock
4. post-update → `omarchy10k update --no-pull` (skipped on TTY or O10K_SKIP_HOOK_UPDATE),
   then {"command":"invalidate_git"} fan-out so prompts re-query updated repositories
5. battery-low → omarchy-notification-send toast, enriched with a live daemon `status`
   blob (crate version) when a daemon answers
6. Outside the desktop: `omarchy10k hook-event <name> [args]` dispatches via omarchy-hook
   when present, else walks ~/.config/omarchy/hooks/<event>.d/ directly (individual hook
   failures are logged and skipped — a desktop event is never dropped by one broken consumer)
```

## Data Flow: Right Prompt Rail

```
1. [prompt].right_segments lists rail segments left-to-right
   (default ["command_duration","git"] — byte-identical to the historical hardcoded pair)
2. layout.rs resolve_right_rail() maps names → RightSegment (command_duration, git, time,
   battery, jobs); unknown names are skipped with a debug log
3. render.rs render_right() composes the rail, skipping entries already drawn inline on the
   left, and only when prompt.right_prompt is on and the style preset is not framed
4. Framed presets keep right content on the pre-existing inline path (duration + git only);
   rail entries apply to non-framed styles
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

The daemon starts with `O10K_PARENT_PID=$$` and monitors the parent process via `kill(ppid, 0)` every 2 seconds. On Linux it also sets `PR_SET_PDEATHSIG` (SIGTERM) at startup, closing the parent-death PID-recycling race between polls. When the shell exits, the daemon detects it, removes its socket, and exits cleanly. The Bash adapter also sends `shutdown` on `EXIT` trap as belt-and-suspenders, and an idle-shell watchdog respawns a killed daemon within ~10s without any prompt activity.

Socket naming: `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock`. A daemon started with `O10K_SOCK_NAME=<name>` binds `omarchy10k-<name>.sock` instead — the headless daemon the Control Center spawns when no shell sessions exist uses this.

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
- `name` — registry name as `Arc<str>`: built-ins are static strings, plugin segments carry `plugin.<plugin>.<segment>` and need owned data (the old `Box::leak` trick is banned)

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
| AI Agent | 38 | 60 cols | Accent (env-only detection: Claude Code / Codex) |
| Exit Status | 30 | 0 cols | Red; undercurl when TermCaps.has_undercurl |
| Command Duration | 50 | 40 cols | Yellow |
| Jobs | 45 | 50 cols | Blue |
| Time | 55 | 40 cols | Muted (localtime via libc, no chrono) |
| Battery | 56 | 40 cols | Green/yellow/red by threshold (sysfs BAT0/BAT1) |
| Dir Writable | 6 | 40 cols | Red when the cwd is NOT writable (cached write probe; silent when writable) |
| Package Version | 33 | 50 cols | Accent (manifest scan of the cwd, TTL-cached) |
| VPN | 34 | 50 cols | Green (sysfs `/sys/class/net` tun*/tap*/wg* interface names) |
| AWS Profile | 35 | 50 cols | Yellow (env: AWS_PROFILE / AWS_VAULT / AWS_DEFAULT_PROFILE) |
| Docker Context | 36 | 50 cols | Blue (env DOCKER_HOST, else async `docker context show`, TTL-cached) |
| Kubectl Context | 37 | 50 cols | Cyan (async `kubectl config current-context`, `on_path`-gated, TTL-cached) |
| Terraform Workspace | 39 | 50 cols | Magenta (stat gate on a `.terraform` dir in the cwd, then async `terraform workspace show`, TTL-cached) |
| GCloud Project | 40 | 50 cols | Orange (env GOOGLE_CLOUD_PROJECT, else async `gcloud config get-value project`, TTL-cached) |
| Load | 55 | 40 cols | Muted (braille sparkline of per-render load ring, opt-in) |

*Tier D note:* the eight segments above (v0.4.0 Tier D catalog) are **all default-off** — each is gated on its own `[segments.<name>].enabled` flag and excluded from every style preset until enabled. Shared helpers (`TtlCache`, `run_command`, `on_path`) live in `segments/util.rs`. Detection is tiered to protect the sub-5ms budget: pure env read → cached filesystem stat → `on_path`-gated async subprocess with a hard timeout, TTL-cached across renders.

Style presets (`style.preset`, honoring legacy `prompt.layout` as an input) control separators, frames, and the segment allowlist. `omarchy`, `powerline`, `rainbow`, `gradient`, `framed`, `classic`, `lean`, and `slanted` resolve their segment lists from `ALL_SEGMENTS` in `style.rs` (everything in the table above); `dense` keeps os/ssh/directory/git/exit_status/command_duration/jobs; `minimal` is directory-only; `pure` is directory + git. Presets `minimal` and `dense` force single-line mode regardless of `prompt.newline`.

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

## Preview, Palette, Looks, and Status APIs

**Preview** (`type: "preview"`) renders a prompt with simulated context — no git subprocess, no OSC 133 markers. Quattro sends optional fields (`cwd`, `exit_code`, `git_branch`, `git_staged`, `git_unstaged`, `cols`, `in_ssh`, etc.) to drive live prompt preview in the Control Center, plus an optional `look` name to dry-run a Look without applying it.

**Palette** (`command: "palette"`) returns the daemon's in-memory `ThemePalette` as hex strings (`accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`). Quattro uses this for theme color swatches without reading `colors.toml` directly. **`palettes`** returns the full curated palette set (moved daemon-side so CLI, panel, and Gallery resolve identically); **`defaults`** returns `Config::default()` for the panel's modified-vs-default ink bars.

**Looks** (`command: "looks"` / `"looks_apply"` / `"looks_save"`) list, apply, and snapshot appearance bundles. `looks_apply` merges the resolved patch through the config_set path, or patches the in-memory config only when `transient` is true. See [Data Flow: Look Apply](#data-flow-look-apply-protocol-05).

**Status** (`command: "status"`) is the ambient snapshot served additively: `pid`, `version`, `protocol_version`, `cwd`, the last render summary (branch, dirty counts, duration, exit code, and `agent` — `"claude"|"codex"|null`, detected from the same env channel the AI-agent segment reads via `detect_agent`), a live git-cache summary for that cwd, battery (sysfs; `null` on desktops), `last_cmd_duration_ms`, and `session_age_secs`. The BarWidget badges (including the robot-glyph agents badge) and SessionPicker consume this stream — no new timers.

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
| `crossterm` | 0.29.0 | CLI only | Raw-mode terminal interaction for the `configure` wizard |
| `notify` | 8.2.0 | Daemon only | Filesystem watcher for config/theme hot-reload |
| `notify-debouncer-full` | 0.5.0 | Daemon only | Debounced filesystem events for config/theme hot-reload |
| `tracing` + `tracing-subscriber` | 0.1.44 / 0.3.23 | Both | Structured logging |
| `directories` | 6.0.0 | Both | XDG path resolution |
| `unicode-width` | 0.2.2 | Both | East Asian width-aware string measurement |
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
        ┌───────────┬──────────────┼──────────────┬───────────┬───────────┐
        ▼           ▼              ▼              ▼           ▼           ▼
  ┌──────────┐ ┌───────┐ ┌─────────────┐ ┌────────┐ ┌───────┐ ┌──────────┐
  │  server   │ │ looks │ │ script_exec │ │ config │ │ theme │ │ terminal │
  └─────┬────┘ └───┬───┘ └──────┬──────┘ └───┬────┘ └───┬───┘ └────┬─────┘
        │          │            │            │          │          │
        ▼          │            │            │          │          │
  ┌──────────┐     │            │            │          │          │
  │  render   │◄────┴────────────┘            │          │          │
  └─────┬────┘                               │          │          │
        │                                    │          │          │
   ┌────┼──────────┐                         │          │          │
   ▼    ▼          ▼                         │          │          │
┌────────┐ ┌──────┐ ┌──────┐                 │          │          │
│segments│ │layout│ │ git  │                 │          │          │
└───┬────┘ └──────┘ └──────┘                 │          │          │
    │                                        │          │          │
    ├── os ssh container directory git       │          │          │
    ├── package_version dir_writable aws_profile docker_context
    ├── kubectl_context terraform_workspace vpn gcloud_project
    │   (Tier D catalog; shared TtlCache/run_command/on_path in util.rs)
    ├── python_env toolchain nix k8s         │          │          │
    ├── ai exit_status command_duration jobs │          │          │
    └── time battery load                    │          │          │
                                             │          │          │
   character ◄───────────────────────────────┴──────────┴──────────┘
```

All modules are private to the daemon binary (no `lib.rs`). This is intentional — the daemon is a self-contained service, not a library.

## Project Profiles (v0.4.1)

Render merge order: base config → transient Look → **project profile** (`.o10k.toml`, display-keys allowlisted: style/prompt/segments/theme/frame, minus the exec-tier segments' `enabled` flag — an untrusted repo may not make the daemon spawn `kubectl`/`terraform`/`gcloud`/`docker` in its own directory; `.git`-boundary-stopped detection; 30s TTL / 512-entry detection cache, plus a 64-entry cache of the *merged* config keyed on the profile's mtime+length and the daemon's config generation, since the merge is a full TOML round-trip of the whole config on every render) → the Looks Studio's ephemeral `preview.patch` override (config_set-shaped; last layer wins, never persisted). `theme.source = "terminal"` resolves the palette from the rendered ghostty palette file (accent=4, muted=8, red..cyan=1–6, orange=11) with default fallback. The configure wizard gained context previews, per-segment toggles, and three finish paths (apply / Look / profile).

## Plugin Economy (v0.4.0 Tier D)

Plugins are data, not code. A plugin is a directory under
`~/.config/omarchy10k/plugins/<name>/` containing a `plugin.toml` manifest that
declares segments in two tiers:

| Tier | Mechanism | Cost profile |
|------|-----------|--------------|
| `env` | Renders the first set key from an ordered `env_keys` list, via the shell's env channel | Zero forks; cannot stall the prompt |
| `command` | Runs a fixed command line in the prompt's cwd, async with a hard timeout, TTL-cached (TtlCache modelled on GitCache) | Subprocess cost paid off the critical path |

Plugin segments render as `plugin.<plugin>.<segment>` so they can never collide
with (or shadow) a built-in segment. Presence on disk means *available*; presence
in `[plugins] enabled` means *active* — the registry rebuilds on every config
reload, so `omarchy10k plugin enable|disable` (which writes that table via the
daemon) picks up immediately.

The CLI lifecycle: `plugin add <git-url>` accepts only remote git URLs
(`https://`, `git://`, scp-like `git@host:repo`; local paths and `file://` are
refused before any command runs) and shallow-clones into place — always
installed DISABLED with a review hint, since the manifest and any code ship
unreviewed. `list` shows installed plugins with their enabled state; `remove`
is refused while the plugin is enabled.

## Starship Migration (v0.4.0 Tier D)

`omarchy10k migrate <starship.toml> [--yes]` reads a Starship config, extracts
the `$module` names from its `format` string, and maps them onto o10k segments
(e.g. `git_branch`/`git_status` → `git`, `cmd_duration` → `command_duration`,
`hostname`/`username` → `ssh`, `python`/`conda` → `python_env`, `aws` →
`aws_profile`, `kubernetes` → `k8s`, `package` → `package_version`, …).
Modules with no o10k counterpart are listed honestly as unmapped. Dry-run by
default (mapping table + unmapped list); with `--yes` it saves a
`[looks.migrated-starship]` Look through the daemon, with an atomic local
fallback when no daemon answers.

## Known Architectural Issues

Two findings from the [Bug Audit](bug-audit.md) were architectural rather than
local; one remains open, one is closed as of the env channel.

### The prompt string is not readline-safe by construction

Nothing in the pipeline owns the job of marking escapes non-printing. Segments
emit raw ANSI via `AnsiColor::fg_escape()`, the layout engine passes content
through untouched, and `render.rs` wraps only its own OSC 133 constants. The Bash
adapter then assigns the result straight to `PS1`. Because the responsibility sits
nowhere, every segment added since has inherited the defect — and the OSC 8
hyperlink added in v0.3 made it worse on exactly the terminals the project
targets.

The fix belongs at a single chokepoint (escape emission, or a final pass over
`left`/`transient`), not spread across segments. See
[Bug Audit #1](bug-audit.md#1-prompt-escapes-are-not-marked-non-printing-so-bash-miscounts-prompt-width).

### The daemon's environment is not the shell's environment

The data flow diagrams above show `cwd`, `exit_code`, `cmd_duration_ms`, `cols`
and `jobs` crossing the socket — the complete set of per-prompt state the protocol
carries. But seven segments (`python_env`, `toolchain`, `nix`, `container`, `k8s`,
`ssh`, and the title's `{user}`/`{host}`) read `std::env` inside the daemon
process instead.

The daemon is spawned once per shell and lives for the session, so those segments
see the environment exactly as it was at shell startup and never again. This is
why the "v0.3 Context Segments" do not track context: activating a venv, switching
mise tool versions, or entering a nix shell changes the *shell's* environment, not
the daemon's.

**Closed in v0.4.** The `prompt` message now carries an `env` object — a shell-side
allowlist built by the adapter's `__o10k_env_json` (zero forks) — and
`SegmentContext::env_get` reads from it, so `python_env`, `toolchain`, `nix`,
`container`, `k8s`, and the AI-agent segment track the live shell environment.
The bash `KEYMAP` value rides the same channel as `vi_mode`, driving the opt-in
vi-mode prompt character (`[segments.character] vi_mode`). Historical context:
see [Bug Audit #5](bug-audit.md#5-every-environment-derived-segment-is-frozen-at-daemon-start).
