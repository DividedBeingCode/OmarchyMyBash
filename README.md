<p align="center">
  <br />
  <b>OMARCHY10K</b>
  <br />
  <sub>A reactive shell UI runtime for Bash on Omarchy Quattro</sub>
  <br />
  <br />
</p>

---

**The shell should feel designed, not assembled.**

Omarchy10k makes Bash feel like a first-class part of the Omarchy desktop: beautiful and coherent out of the box, deeply customizable when you want to rice it, and fast enough that you forget it's there.

It combines the prompt intelligence of [Powerlevel10k](https://github.com/romkatv/powerlevel10k), the batteries-included convenience of [Oh My Zsh](https://ohmyz.sh/), and native [Omarchy Quattro](https://omarchy.com) integration into a single coherent Bash experience.

```
  ~/Code/omarchy10k  main +3 !1 ?2 ⇡2                    1.5s
  ❯ _
```

## Design Philosophy

Omarchy10k is built on seven principles:

1. **Beautiful by default.** The prompt, color use, spacing, and information density should look intentional with zero configuration. A fresh install should feel finished.

2. **Theme-native by default.** Colors derive from the active Omarchy theme through semantic roles (`accent`, `foreground`, `muted`, `red`, `green`, `yellow`, `blue`). Switch your desktop theme and the prompt follows instantly.

3. **Progressive disclosure.** Show the minimum useful information until context makes more detail relevant. A clean repo shows a checkmark. A dirty repo expands to show staged, unstaged, untracked, conflicts, stashes, ahead/behind, and active operations.

4. **One shell lifecycle.** Prompt rendering, environment managers, directory jumping, history, and command hooks compose through one hook broker instead of fighting over `PROMPT_COMMAND`. No more Mise clobbering Atuin clobbering Zoxide.

5. **Fast enough to disappear.** Normal prompt rendering completes in under 5ms via a persistent Rust daemon. Git status is cached and invalidated by filesystem events, not polled. The prompt never blocks on network calls.

6. **Files are the source of truth.** The Quattro Control Center edits user-owned TOML config. Headless use, dotfile workflows, and version control remain first-class.

7. **Override at any layer.** Opinionated defaults are the starting point, not a wall. Ricing is treated as a feature, not a hack.

### The Target Feeling

The closest product analogy is macOS in its confidence, not in its restrictions. The system makes strong aesthetic and behavioral choices so that a fresh install feels complete. Unlike macOS, it exposes the seams intentionally. You can inspect, override, remix, and redistribute every part of the experience.

## Architecture

Omarchy10k is not a prompt theme. It is a shell runtime with four cooperating subsystems:

```
                        OMARCHY QUATTRO DESKTOP
                    ┌──────────────────────────────┐
                    │  BarWidget.qml  Panel.qml    │
                    │  (status glyph)  (4-tab UI)  │
                    └────────────┬─────────────────┘
                                 │ socket
    TERMINAL (Bash)              │               FILESYSTEM
  ┌──────────────────┐    ┌──────┴──────────┐    ┌──────────────┐
  │ omarchy10k.bash  │◄──►│  omarchy10kd    │◄───│ .git/index   │
  │                  │    │  (Rust daemon)  │    │ .git/HEAD    │
  │ hook broker      │    │                 │◄───│ colors.toml  │
  │ PROMPT_COMMAND   │    │  git cache      │    │ config.toml  │
  │ PS0 preexec      │    │  segment engine │    └──────────────┘
  │ chpwd emulation  │    │  theme reader   │      inotify/kqueue
  │ daemon lifecycle │    │  layout engine  │      watches
  │ fallback prompt  │    │  ANSI renderer  │
  └──────────────────┘    └─────────────────┘
         │
    ┌────┴────┬────────┬────────┐
    │ Mise    │ Atuin  │ Zoxide │  (registered via o10k_hook_add)
    └─────────┴────────┴────────┘
```

### Why a Daemon?

Starship spawns a new process for every prompt. Powerlevel10k runs a persistent `gitstatusd` daemon. Omarchy10k takes P10k's approach and generalizes it: a single Rust daemon (`omarchy10kd`) per shell session handles all expensive work.

| Approach | Cold prompt | Cached prompt | Git status |
|----------|-----------|-------------|-----------|
| Starship (process-per-prompt) | ~40-80ms | ~15-30ms | ~50-200ms |
| P10k + gitstatusd (Zsh only) | ~5ms | ~1ms | ~5ms |
| **Omarchy10k** | **~10ms** | **< 5ms** | **< 10ms** |

The daemon starts when you open a shell, listens on a PID-scoped Unix socket (`$XDG_RUNTIME_DIR/omarchy10k-$$.sock`), and cleans up when the shell exits. If it crashes, the Bash adapter renders a static fallback prompt -- your shell never breaks.

## Features

### Prompt Segments

| Segment | What it shows | Adaptive behavior |
|---------|-------------|------------------|
| **Directory** | Smart-truncated path with `~` substitution | Repo roots stay bold; middle directories compress to unique prefixes |
| **Git** | Branch, ahead/behind, staged/unstaged/untracked/conflicts/stashes | **Adaptive mode:** compact checkmark when clean, full expansion when dirty. Detects merge, rebase, cherry-pick, bisect, revert operations. |
| **Exit Status** | Last command's exit code with signal names | `SIGKILL` instead of `137`, `SIGINT` instead of `130`, `command not found` instead of `127` |
| **Command Duration** | How long the last command took | Hidden below threshold (default 1.5s). Human-readable: `1.5s`, `2m30s`, `1h5m` |
| **Prompt Character** | `❯` colored by success/failure | Accent color on success, red on error. Transient form uses muted color. |

### Responsive Layout

Every segment declares a priority, preferred width, compact form, and minimum column threshold. The layout engine makes intelligent decisions instead of blindly truncating:

- **Wide terminal (120+ cols):** Full path, expanded git status, duration, right prompt
- **Medium terminal (80 cols):** Compressed directories, compact git counts
- **Narrow terminal (40 cols):** Current directory, branch, prompt character only

### Hook Broker

The hook broker solves a class of bugs where shell tools fight over `PROMPT_COMMAND`. Instead of each tool clobbering the previous one:

```bash
# The old way (broken)
eval "$(mise activate bash)"     # sets PROMPT_COMMAND
eval "$(atuin init bash)"        # overwrites PROMPT_COMMAND
eval "$(zoxide init bash)"       # overwrites PROMPT_COMMAND again

# The Omarchy10k way (composable)
o10k_hook_add precmd _mise_hook
o10k_hook_add precmd _atuin_precmd
o10k_hook_add chpwd _zoxide_hook
```

Four lifecycle events are available:

| Event | When it fires | Typical consumers |
|-------|-------------|------------------|
| `precmd` | Before every prompt render | Mise, Atuin, Zoxide, prompt |
| `preexec` | Before command execution | Atuin history, timing start |
| `chpwd` | Working directory changed | Mise, Zoxide, git cache invalidation |
| `shell_exit` | Shell is closing | Daemon cleanup |

### ble.sh Enhanced Mode

When [ble.sh](https://github.com/akinomyoga/ble.sh) is loaded before Omarchy10k, the experience upgrades automatically:

| Feature | Vanilla Bash | With ble.sh |
|---------|------------|------------|
| Prompt rendering | Full prompt | Full prompt |
| Git status | Cached, fast | Cached, fast |
| Hook broker | `PROMPT_COMMAND` array | `blehook PRECMD/PREEXEC/CHPWD` |
| Transient prompt | No | Yes -- previous prompts collapse to `❯` |
| Right prompt | No | Yes |
| Syntax highlighting | No | Yes |
| Autosuggestions | No | Yes |
| Show-on-command | No | Yes -- type `git` and the git segment expands |

No configuration needed. Omarchy10k detects `BLE_VERSION` and switches automatically.

### Theme Modes

Omarchy10k reads the Omarchy desktop palette from `~/.local/state/omarchy/current/theme/colors.toml` and watches it for changes.

| Mode | Behavior |
|------|---------|
| **Follow Omarchy** | Prompt colors update instantly when you switch desktop themes |
| **Custom** | Use a fully independent palette, ignore desktop theme changes |
| **Hybrid** | Follow the desktop theme but pin specific colors (e.g., keep your accent purple) |

### Quattro Control Center

A native Omarchy Quattro bar widget with a 4-tab configuration panel:

| Tab | Controls |
|-----|---------|
| **Appearance** | Preset, theme mode, prompt lines, transient toggle, OS icon |
| **Context** | Git detail level, duration threshold, SSH display, exit status format |
| **Shell** | Integration status for ble.sh, Atuin, Mise, Zoxide, fzf |
| **Advanced** | Open config file, run doctor, reload daemon, reset to defaults, daemon status |

The panel edits `~/.config/omarchy10k/config.toml` directly. Every change made in the UI has a file-level equivalent.

## Installation

### Quick Install (Recommended)

```bash
git clone https://github.com/DividedBeingCode/OmarchyMyBash.git
cd OmarchyMyBash/omarchy10k
./install.sh
```

This builds from source, installs binaries to `~/.local/bin/`, configures `.bashrc`, installs the Quattro Control Center plugin, and sets up the theme-set hook. Open a new terminal and you're done.

To uninstall: `./install.sh --uninstall`

### Manual Install

```bash
# Clone
git clone https://github.com/DividedBeingCode/OmarchyMyBash.git
cd OmarchyMyBash

# Build
cargo build --release

# Install binaries
cp target/release/omarchy10k ~/.local/bin/
cp target/release/omarchy10kd ~/.local/bin/

# Add to ~/.bashrc (one line, that's it)
echo 'eval "$(omarchy10k init bash)"' >> ~/.bashrc

# Optional: install the Quattro plugin
cp -r omarchy10k/quattro/ ~/.config/omarchy/plugins/community.omarchy10k/

# Optional: install the theme-set hook
mkdir -p ~/.config/omarchy/hooks/theme-set.d/
cp omarchy10k/hooks/theme-set ~/.config/omarchy/hooks/theme-set.d/omarchy10k
chmod +x ~/.config/omarchy/hooks/theme-set.d/omarchy10k
```

### Updating

Once installed, update Omarchy10k with a single command:

```bash
omarchy10k update
```

This pulls the latest source, rebuilds, replaces binaries, refreshes the Quattro plugin and theme hook, and gracefully restarts any running daemons. New terminals pick up the update automatically; running terminals restart their daemon on the next command.

**Flags:**

| Flag | Effect |
|------|--------|
| `--no-pull` | Skip `git pull` (rebuild from current source tree) |
| `--no-build` | Skip `cargo build` (reinstall existing binaries + plugin only) |

You can also update via the installer script:

```bash
./install.sh --update
```

### Verify

```bash
omarchy10k doctor
```

Doctor checks Bash version, TrueColor support, Nerd Font availability, ble.sh, Omarchy, Mise, Atuin, Zoxide, fzf, daemon health, hook conflicts, and config status.

## Configuration

Configuration lives at `~/.config/omarchy10k/config.toml`. Every setting has a sensible default -- you only need a config file to change something.

### Full Reference

```toml
# ── Prompt ────────────────────────────────────────────

[prompt]
layout = "omarchy"         # omarchy | minimal | powerline | classic | pure | dense
transient = true           # collapse previous prompts to ❯ (requires ble.sh)
newline = true             # two-line prompt (line 1: segments, line 2: ❯)
right_prompt = true        # show right-side content (requires ble.sh)

# ── Theme ─────────────────────────────────────────────

[theme]
source = "omarchy"         # omarchy | custom | hybrid | terminal

# Custom palette (only used when source = "custom" or "hybrid")
# [theme.custom]
# accent = "#bb9af7"
# foreground = "#c0caf5"
# muted = "#565f89"
# background = "#1a1b26"
# red = "#f7768e"
# green = "#9ece6a"
# yellow = "#e0af68"
# blue = "#7aa2f7"

# ── Directory ─────────────────────────────────────────

[directory]
strategy = "smart"          # smart | full | truncate
max_length = 40             # max display width before truncation
repo_root_style = "bold"    # bold | normal

# ── Git ───────────────────────────────────────────────

[git]
enabled = true
mode = "adaptive"           # hidden | compact | expanded | adaptive
stale_display = true        # show greyed-out state while refreshing
max_threads = 4

# ── Segments ──────────────────────────────────────────

[segments.os]
enabled = true
icon = "arch"               # arch | linux | omarchy | custom | none

[segments.exit_status]
enabled = true
show_signal_name = true     # SIGKILL instead of 137

[segments.command_duration]
enabled = true
show_above_ms = 1500        # only show when command took longer than this

[segments.jobs]
enabled = true

[segments.ssh]
enabled = true
show = "auto"               # auto | always | never

[segments.character]
success = "❯"
error = "❯"

# ── v0.3 Context Segments ────────────────────────────

[segments.container]
enabled = true

[segments.python]
enabled = true              # show active venv / conda env

[segments.toolchain]
enabled = true              # show Mise-managed versions

[segments.nix]
enabled = true              # show Nix shell

[segments.k8s]
enabled = false             # show Kubernetes context
show_namespace = true

[segments.time]
enabled = false             # show current time
format = "%H:%M"            # %H:%M | %H:%M:%S | %I:%M %p

[segments.battery]
enabled = false             # show battery (laptops)

[segments.notification]
enabled = true
threshold_ms = 10000        # desktop notification for long commands

# ── Terminal Features ────────────────────────────────

[terminal.title]
enabled = true
format = "{dir}"            # {dir}, {user}, {host}, {branch}

[terminal.progress]
enabled = true              # OSC 9;4 progress bar

# ── Daemon ────────────────────────────────────────────

[daemon]
socket = "auto"             # auto | /path/to/socket
log_level = "warn"          # trace | debug | info | warn | error
```

## CLI Reference

```
omarchy10k <command>

COMMANDS:
  init bash       Emit the Bash adapter for sourcing in .bashrc
  prompt          Render the prompt (used internally by the adapter)
  doctor          Run diagnostics and check system compatibility
  reload          Signal the daemon to re-read config and theme
  update          Pull, rebuild, and reinstall (--no-pull, --no-build)
  benchmark       Render prompts in a loop and report p50/p95/p99 latency
  debug           Dump daemon state (PID, version, cache status)
```

### `omarchy10k doctor`

```
Omarchy10k Doctor
══════════════════════════════════════

  Bash              5.2.37      ✓
  TrueColor                     ✓
  Nerd Font                     ? (visual check recommended)
  ble.sh            0.4.0-dev   ✓ enhanced mode available
  Omarchy           Quattro     ✓ theme: Tokyo Night
  Mise              2024.12.0   ✓
  Atuin             18.4.0      ✓
  Zoxide            0.9.6       ✓
  fzf               0.57.0      ✓
  Terminal           foot        ✓
  Daemon                        ✓ running
  Hook conflicts                ✓ none detected
  Config             ~/.config/omarchy10k/config.toml
```

### `omarchy10k benchmark`

```
Omarchy10k Benchmark (100 iterations)
────────────────────────────────────
  avg:     2.34ms
  p50:     1.89ms
  p95:     4.12ms
  p99:     6.71ms
  result: ✓ sub-5ms target met
```

## How It Works

### Prompt Lifecycle

```
1. You press Enter
2. Bash evaluates PROMPT_COMMAND
3. omarchy10k.bash captures $? and stops the duration timer
4. Hook broker dispatches precmd to Mise, Atuin, Zoxide
5. Bash adapter sends context to omarchy10kd via Unix socket:
   {"cwd":"/home/ian/Code","exit_code":0,"cmd_duration_ms":1523,"cols":120,"jobs":0}
6. Daemon checks git cache (inotify-invalidated, not polled)
7. Segment engine builds directory + git + exit + duration segments
8. Layout engine resolves segment visibility based on terminal width
9. ANSI renderer produces the prompt string with OSC 133 markers
10. Bash adapter sets PS1
11. You see your prompt in < 5ms
```

### Daemon Lifecycle

```
Shell opens  →  omarchy10k.bash starts omarchy10kd in background
                omarchy10kd creates $XDG_RUNTIME_DIR/omarchy10k-$$.sock
                omarchy10kd watches: config.toml, colors.toml, .git/*

Shell runs   →  Each prompt: adapter sends context, daemon responds
                Theme changes: daemon reloads palette automatically
                Config changes: daemon reloads configuration automatically
                Git changes: inotify invalidates cache, next prompt gets fresh status

Shell closes →  EXIT trap fires
                Adapter sends shutdown command
                Daemon cleans up socket and exits

Daemon dies  →  Adapter detects missing socket
                Renders static fallback prompt (never breaks the shell)
                Next prompt attempt restarts daemon automatically
```

### IPC Protocol

Communication uses NDJSON (newline-delimited JSON) over Unix sockets for debuggability.

**Prompt request:**
```json
{"cwd":"/home/ian/Code/omarchy10k","exit_code":0,"cmd_duration_ms":0,"cols":120,"jobs":0}
```

**Prompt response:**
```json
{"left":"...rendered ANSI...","right":null,"transient":"...muted ❯...","git_stale":false}
```

**Control commands:**
```json
{"command":"status"}
{"command":"reload_config"}
{"command":"reload_theme"}
{"command":"invalidate_git"}
{"command":"shutdown"}
```

## Project Structure

```
omarchy10k/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── omarchy10kd/              # Rust daemon
│   │   └── src/
│   │       ├── main.rs           # Tokio runtime, watchers, parent monitoring
│   │       ├── server.rs         # Unix socket accept loop, request dispatch
│   │       ├── git.rs            # Git cache, porcelain-v2 parser, action detection
│   │       ├── segments/         # Segment modules (dir, git, exit, duration, char)
│   │       ├── layout.rs         # Priority-based responsive layout engine
│   │       ├── theme.rs          # Omarchy palette reader, hex-to-RGB, hot reload
│   │       ├── config.rs         # TOML config with layered defaults
│   │       └── render.rs         # ANSI output with OSC 133 markers
│   └── omarchy10k/               # CLI client
│       └── src/
│           ├── main.rs           # Clap-based CLI with subcommands
│           ├── prompt.rs         # Socket client, benchmark runner
│           └── doctor.rs         # System diagnostics
├── shell/
│   └── omarchy10k.bash           # Bash adapter + hook broker (~280 lines)
├── quattro/
│   ├── manifest.json             # Omarchy Quattro plugin manifest
│   ├── BarWidget.qml             # Bar glyph + panel toggle
│   ├── Panel.qml                 # 4-tab Control Center
│   └── Model.js                  # State management
├── templates/
│   └── omarchy10k.toml.tpl       # Theme bridge template
├── hooks/
│   └── theme-set                 # Omarchy theme-switch hook
├── config/
│   └── default.toml              # Default configuration
├── tests/
│   └── integration_test.sh       # 39-test integration suite
├── README.md
└── LICENSE                       # MIT
```

## Roadmap

### v0.1 -- Prompt Core

The foundation: replace Starship with equal or better prompt quality.

- [x] Persistent Rust daemon with Unix socket IPC
- [x] 5 core segments (directory, git, exit status, duration, character)
- [x] Git cache with porcelain-v2 parsing and inotify invalidation
- [x] Responsive layout engine with priority-based compaction
- [x] Omarchy theme reader with filesystem watch
- [x] Bash hook broker with `o10k_hook_add` API
- [x] ble.sh enhanced mode (transient prompt, right prompt)
- [x] CLI with doctor, benchmark, reload, debug
- [x] Quattro Control Center plugin
- [x] Graceful fallback on daemon failure

### v0.2 -- Shell UX

- [x] Hook broker composable lifecycle (precmd, preexec, chpwd, shell_exit)
- [x] SSH/container context segments
- [x] Bridge coprocess for zero-fork prompt rendering
- [x] OSC 133 shell integration markers
- [x] Instant prompt cache

### v0.3 -- Control Center & Terminal (current)

- [x] Live prompt preview in Quattro panel
- [x] Preset/layout system (omarchy, minimal, powerline, classic, pure, dense)
- [x] Theme modes (Follow, Custom, Hybrid) in panel UI with palette preview
- [x] 7 new segments (container, python, toolchain, nix, k8s, time, battery)
- [x] Terminal feature detection (TermCaps) for Ghostty, Foot, Kitty, WezTerm, Alacritty
- [x] OSC 7 CWD, OSC 8 hyperlinks, OSC 777 notifications, OSC 9;4 progress, DEC 2026 sync output
- [x] Terminal title with `{dir}`, `{user}`, `{host}`, `{branch}` placeholders
- [x] Git worktree detection and display
- [x] One-script installer with `--uninstall` support
- [x] Multi-session discovery and switching in Quattro
- [x] Config undo/history, import/export, diff toast
- [x] Segment toggle grid, one-click tool setup, benchmark display
- [x] Protocol v0.3 with typed messages and version negotiation

### v0.4 -- Plugin Ecosystem

- [ ] External executable plugins (`~/.config/omarchy10k/plugins/`)
- [ ] WASM plugin runtime (wasmtime, AOT-precompiled)
- [ ] Plugin browser in Control Center
- [ ] Preset import/export

### v0.5 -- Agents

- [ ] Codex / Claude Code / OpenCode / Gemini CLI adapters
- [ ] Worktree provenance segments
- [ ] Agent-aware prompt policies
- [ ] GitHub PR/CI segments (cached, 30-60s TTL)

### 1.0 -- Daily Driver

- [ ] Stable config schema with migration guarantees
- [ ] Performance targets verified (sub-5ms cached, sub-50ms cold git)
- [x] Tested upgrade path (`omarchy10k update`)
- [ ] Full documentation
- [ ] Curated plugin catalog

## Comparison

| | Starship | Powerlevel10k | Oh My Zsh | **Omarchy10k** |
|---|---------|-------------|---------|--------------|
| Shell | Any | Zsh only | Zsh only | **Bash** (native) |
| Architecture | Process per prompt | Daemon (gitstatusd) | Sourced scripts | **Daemon (omarchy10kd)** |
| Prompt latency | 15-80ms | 1-5ms | 10-50ms | **< 5ms** |
| Hook management | None | Zsh hooks | Zsh hooks | **Hook broker** |
| Desktop integration | None | None | None | **Quattro Control Center** |
| Theme sync | Manual | Manual | Manual | **Automatic (inotify)** |
| Config format | TOML | Zsh source | Zsh source | **TOML** |
| Crash recovery | N/A | Fallback | N/A | **Static fallback prompt** |
| ble.sh integration | None | N/A (Zsh) | N/A (Zsh) | **Auto-detected upgrade** |
| License | ISC | MIT | MIT | **MIT** |

## Requirements

- **Bash** 4.4+ (5.1+ recommended for PROMPT_COMMAND array support)
- **Rust** 1.75+ (for building from source)
- A terminal with **TrueColor** support (`COLORTERM=truecolor`)
- A [**Nerd Font**](https://www.nerdfonts.com/) for glyphs
- **Omarchy Quattro** (optional, for Control Center and theme sync)
- **ble.sh** (optional, for transient/right prompts and syntax highlighting)

## License

MIT

---

<sub>Built for Omarchy Quattro by Ian Johnston.</sub>
