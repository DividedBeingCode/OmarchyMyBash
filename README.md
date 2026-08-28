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

### Desktop Service (v0.4)

Omarchy10k is scriptable from the desktop. The Quattro plugin registers an IPC
target, so any keybind, script, or tool can drive it:

```bash
omarchy-shell call community.omarchy10k status
omarchy-shell call community.omarchy10k setLayout powerline
omarchy-shell call community.omarchy10k toggleTransient
omarchy-shell call community.omarchy10k picker      # session picker overlay
```

A headless **service** component holds persistent connections to every running
daemon (no polling), a keybind-summoned **session picker overlay** lists all
live shells with CWD/branch/duration and focuses or reopens them, and the bar
widget + panel render **live ANSI-colored prompt previews** via the daemon.

### Agent Statusline (v0.4)

`omarchy10k statusline` is a Claude Code statusline renderer: it reads Claude
Code's documented statusLine JSON on stdin and renders it through the daemon
with your active Omarchy theme — model, context-window % with green/yellow/red
thresholds, cost, and worktree — in under 5ms warm, with a pure-Rust fallback
when the daemon is down. The installer wires it into `~/.claude/settings.json`.
An `✦ claude` / `✳ codex` prompt segment signals when an agent session is
active in the shell.

### Live Context Segments (v0.4)

Environment-derived segments now update live. `source .venv/bin/activate`,
`mise use`, `nix develop`, and `direnv allow` all appear on the next prompt —
the adapter streams a fixed allowlist of environment variables with every
prompt request (zero forks), fixing the frozen-env limitation. True powerline
and rainbow presets render filled background segments with fg-flipped
separators, and long-command notifications route through Omarchy's
`omarchy-notification-send`.

## Installation

### Omarchy Quick Install (Recommended)

On an Omarchy Quattro machine:

```bash
git clone https://github.com/DividedBeingCode/OmarchyMyBash.git
cd OmarchyMyBash/omarchy10k
./install.sh
```

The installer is Omarchy-aware. It:

1. Checks dependencies — using `omarchy-pkg-add` guidance if Rust or git are missing
2. Builds from source and installs binaries to `~/.local/bin/` (atomic tmp+mv)
3. Adds one line to `~/.bashrc`: `eval "$(omarchy10k init bash)"`
4. Installs the Quattro plugin to `~/.config/omarchy/plugins/community.omarchy10k/`, syncs `manifest.json` version with the crate version, and triggers `omarchy-shell shell rescanPlugins`
5. Installs the `theme-set` hook to `~/.config/omarchy/hooks/theme-set.d/omarchy10k`
6. Deploys the theme bridge template to `~/.local/share/omarchy/templates/` so Omarchy's theme engine renders prompt colors on every theme switch
7. Merges the Claude Code statusline into `~/.claude/settings.json` (merge-only, never overwrites an existing statusLine, backup kept)

The plugin lands **disabled** — Omarchy's plugin security model lets you review code before enabling. Enable it with:

```bash
omarchy plugin enable community.omarchy10k
```

Placement follows the manifest's `defaultSection: right`. Then open a new terminal — done.

### Manual Install

```bash
# Clone and build
git clone https://github.com/DividedBeingCode/OmarchyMyBash.git
cd OmarchyMyBash/omarchy10k
cargo build --release

# Install binaries
mkdir -p ~/.local/bin
cp target/release/omarchy10k target/release/omarchy10kd ~/.local/bin/

# Shell init (the only required step)
echo 'eval "$(omarchy10k init bash)"' >> ~/.bashrc

# Optional: Quattro plugin (Omarchy plugin ecosystem)
mkdir -p ~/.config/omarchy/plugins/community.omarchy10k
cp quattro/* ~/.config/omarchy/plugins/community.omarchy10k/
omarchy-shell shell rescanPlugins          # if omarchy-shell is installed
omarchy plugin enable community.omarchy10k # places the widget (defaultSection: right)

# Optional: theme-set hook
mkdir -p ~/.config/omarchy/hooks/theme-set.d/
cp hooks/theme-set ~/.config/omarchy/hooks/theme-set.d/omarchy10k
chmod +x ~/.config/omarchy/hooks/theme-set.d/omarchy10k

# Optional: theme bridge template (auto-rendered on every Omarchy theme switch)
mkdir -p ~/.local/share/omarchy/templates/
cp templates/omarchy10k.toml.tpl ~/.local/share/omarchy/templates/
```

Note: the plugin lives in the `omarchy10k/` subdirectory of a workspace repo, so
one-command `omarchy plugin add <git-url>` (which expects `manifest.json` at the
repo root) is not available yet — see the roadmap.

### Updating

```bash
omarchy10k update        # pulls, rebuilds, reinstalls binaries + plugin + hook + template, restarts daemons
./install.sh --update    # same, from the source tree
```

| Flag | Effect |
|------|--------|
| `--no-pull` | Skip `git pull` (rebuild from current source tree) |
| `--no-build` | Skip `cargo build` (reinstall existing binaries + plugin only) |

The update path also re-triggers `omarchy-shell shell rescanPlugins`, so new QML
surfaces (service hub, session picker) are hot-reloaded. New terminals pick up
the update automatically; running terminals restart their daemon on the next
command.

### Uninstall

```bash
./install.sh --uninstall
```

Removes binaries, the plugin directory (with a `rescanPlugins` so the bar updates), the theme hook, the theme template, the source breadcrumb, and the `.bashrc` line. If you enabled the widget, also remove its entry under Omarchy's **Setup > Plugins** (or from `~/.config/omarchy/shell.json`).

### Verify

```bash
omarchy10k doctor
omarchy10k intro --force   # one-time welcome render: palette, capabilities, latency
```

Doctor checks Bash version, TrueColor, Nerd Font availability, ble.sh, Omarchy, Mise, Atuin, Zoxide, fzf, daemon health, hook conflicts, and config status.

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

[notifications]
enabled = true
threshold_ms = 10000        # desktop notification for long commands
unfocused_only = false      # only notify when the terminal is unfocused
                            # delivered via omarchy-notification-send, OSC 777 fallback
                            # (the old [segments.notification] table still parses, deprecated)

# ── Terminal Features ────────────────────────────────

[terminal.title]
enabled = true
format = "{dir}"            # {dir}, {user}, {host}, {branch}

[terminal.semantic_prompts]
enabled = false             # OSC 133;C/D emission — enable after verifying no
                            # conflict with Ghostty's own shell integration

[terminal.progress]
enabled = true              # OSC 9;4 progress bar

# ── v0.4 Env / Agent / Statusline ─────────────────────

[env.watch]
keys = ["VIRTUAL_ENV", "CONDA_DEFAULT_ENV", "MISE_NODE_VERSION", "MISE_PYTHON_VERSION", "MISE_RUBY_VERSION", "MISE_GO_VERSION", "MISE_RUST_VERSION", "IN_NIX_SHELL", "DISTROBOX_ENTER_PATH", "container", "KUBECONFIG", "DIRENV_DIR", "CLAUDE_CODE_ENTRYPOINT", "CODEX_SANDBOX", "CODEX_HOME"]
                            # env vars the adapter streams with every prompt
                            # request — keep in sync with the adapter's list

[segments.ai]
enabled = true              # ✦ claude / ✳ codex signal segment (env-gated)

[git]
stale_icon = "⟳"            # marker shown on stale (large-repo) git data

[statusline]
context_warn = 70           # Claude Code context % → yellow
context_crit = 90           # Claude Code context % → red
```

## CLI Reference

```
omarchy10k <command>

COMMANDS:
  init bash       Emit the Bash adapter for sourcing in .bashrc
  prompt          Render the prompt (used internally by the adapter)
  statusline      Claude Code statusline renderer: reads statusLine JSON on
                  stdin, renders through the daemon with the active Omarchy
                  theme (pure-Rust fallback when the daemon is down)
  intro           One-time welcome render: palette swatches, detected
                  capabilities, measured latency (--force re-shows)
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
5. Bash adapter sends context — including a live env snapshot — via Unix socket:
   {"cwd":"/home/ian/Code/omarchy10k","exit_code":0,"cmd_duration_ms":1523,"cols":120,"jobs":0,"env":{"VIRTUAL_ENV":"/home/ian/.venvs/api","MISE_NODE_VERSION":"22"}}
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
{"cwd":"/home/ian/Code/omarchy10k","exit_code":0,"cmd_duration_ms":0,"cols":120,"jobs":0,"env":{"VIRTUAL_ENV":"/home/ian/.venvs/api"}}
```

**Prompt response:**
```json
{"left":"...rendered ANSI...","right":null,"transient":"...muted ❯...","git_stale":false,"notify_threshold_ms":10000,"notify_unfocused_only":false}
```

**Control commands:**
```json
{"type":"control","command":"status"}   # enriched: git, last_cmd_duration_ms, session_age_secs, battery
{"type":"control","command":"reload_config"}
{"type":"control","command":"reload_theme"}
{"type":"control","command":"invalidate_git"}
{"type":"control","command":"shutdown"}
{"type":"statusline","payload":{...}}   # Claude Code statusline render
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
│   │       └── render.rs         # ANSI output, OSC 133 markers, statusline render
│   └── omarchy10k/               # CLI client
│       └── src/
│           ├── main.rs           # Clap-based CLI with subcommands
│           ├── prompt.rs         # Socket client, benchmark runner
│           ├── statusline.rs     # Claude Code statusline client + fallback render
│           ├── intro.rs          # First-run welcome render
│           └── doctor.rs         # System diagnostics
├── shell/
│   └── omarchy10k.bash           # Bash adapter + hook broker (env channel, bridge, notifications)
├── quattro/
│   ├── manifest.json             # Omarchy Quattro plugin manifest (bar-widget, service, overlay)
│   ├── BarWidget.qml             # Bar glyph + panel toggle
│   ├── Panel.qml                 # 4-tab Control Center with live ANSI preview
│   ├── Service.qml               # Headless connection hub (persistent daemon sockets, IPC target)
│   ├── SessionPicker.qml         # Keybind-summoned session picker overlay
│   └── Model.js                  # TOML/protocol helpers, ANSI-to-rich-text
├── templates/
│   └── omarchy10k.toml.tpl       # Theme bridge template
├── hooks/
│   └── theme-set                 # Omarchy theme-switch hook
├── config/
│   └── default.toml              # Default configuration
├── tests/
│   └── integration_test.sh       # 59-test integration suite
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

### v0.3 -- Control Center & Terminal

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
- [x] Protocol v0.3 with typed messages and version negotiation

### v0.4 -- Desktop Service (current)

- [x] Env channel: live env-derived segments (python, nix, mise, k8s respond to `activate`, `mise use`, `nix develop`)
- [x] True powerline/rainbow rendering: background fills, fg-flipped separators, caps
- [x] Real notifications: `[notifications]` table, `omarchy-notification-send` routing, unfocused gating
- [x] Enriched `status` ambient snapshot (git, last command, session age, battery)
- [x] Transient prompt wired end-to-end (bridge 4-field framing, `bleopt prompt_ps1_final`)
- [x] Stale-aware git placeholder (`⟳`)
- [x] `omarchy10k statusline` — daemon-rendered Claude Code statusline with theme palette
- [x] Agent signal segment (`✦ claude` / `✳ codex` via env channel)
- [x] Optional OSC 133;C/D semantic prompt emission with Ghostty coexistence gate
- [x] Quattro plugin IPC: `omarchy-shell call community.omarchy10k <method>`
- [x] Service-kind connection hub + keybind-summoned session picker overlay
- [x] ANSI-colored live panel preview + live preset cards
- [x] `omarchy10k intro` first-run render

### v0.5 -- Agents & Desktop (planned)

- [ ] Agent event registry (Claude Code hooks → daemon → prompt/bar/statusline)
- [ ] Worktree-first agent workflow surface (worktree lanes across prompt + bar)
- [ ] GitHub PR/CI segments (cached, 30-60s TTL)
- [ ] Hook-family integration: battery-low, post-update self-sync, font-set fallback

Ratified out (see `docs/wiki/v04-feature-intel.md` Kill List): WASM plugin runtime, prompt-inline images, prompt animations, tmux bridge, own history stats (Atuin does it), OSC 52.

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
| Desktop integration | None | None | None | **Quattro Control Center + plugin IPC** |
| Agent statusline | Process-per-update | N/A (Zsh) | N/A (Zsh) | **Daemon-rendered, theme-native (<5ms)** |
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
- **Omarchy Quattro** (optional, for Control Center, theme sync, plugin IPC — `omarchy-shell` enables `omarchy-shell call community.omarchy10k <method>`)
- **ble.sh** (optional, for transient/right prompts and syntax highlighting)

## License

MIT

---

<sub>Built for Omarchy Quattro by Ian Johnston.</sub>
