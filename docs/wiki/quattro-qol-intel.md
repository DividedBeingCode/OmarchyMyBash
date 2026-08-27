# Omarchy Quattro Integration — Quality of Life Intel

> Research-backed catalog of quality-of-life improvements for the Omarchy10k
> ↔ Quattro desktop integration. Compiled from community patterns, competitor
> analysis (Oh My Posh Studio/Configurator, Starship Builder, Tablo, Waybar
> modules, DebugDeck, agent-session-status), and first-principles analysis
> through six innovator lenses.
>
> **Bret Victor** — "Can the user see what they're doing?"
> **Torvalds** — "Where are the interface contracts broken?"
> **Collison** — "What frustrates a developer in the first 10 seconds?"
> **Ohno** — "Where is the waste? What motion adds no value?"
> **Wozniak** — "What can we build with what we already have?"
> **Jobs** — "What makes the panel, the prompt, and the terminal feel like one thing?"

---

## The Core Problem

Today, changing a config in Quattro requires:

1. Open panel
2. Change a setting
3. Close panel
4. Switch to terminal
5. Press Enter (trigger new prompt)
6. See if it looks right
7. If not, go back to step 1

That's **six context switches** to verify a single config change. Oh My Posh
Studio solved this with a live WebAssembly render in the browser. Starship
Builder solved it with a simulated environment. We have something better than
either: **a running daemon that already renders prompts on demand.** The
daemon is literally right there on a Unix socket. We should ask it to render
a preview and show it in the panel.

This document catalogs every QoL improvement worth building, organized by
the part of the integration it improves.

---

## Category 1: Live Feedback (The Victor Principle)

> "If you can't see the effect of your action, you can't learn from it."
> — Bret Victor

These features close the feedback loop between config change and visual result.

### 1.1 Live Prompt Preview in Panel

| | |
|---|---|
| **What** | Render a live prompt preview inside the Quattro panel that updates in real-time as the user changes settings |
| **Why** | The #1 missing feature. Oh My Posh Studio, Starship Builder, and the OMP Visual Configurator all revolve around this. We have a unique advantage: a running daemon that renders prompts on demand. No WebAssembly needed — just ask the daemon |
| **How** | On every config change (after the 300ms debounce save), send a prompt request to the daemon with simulated context (sample CWD, sample git status, sample exit code). Parse the response's `left` field. Strip ANSI escape codes and render in a styled `Text` component using QML `RichText`, or render with a simple ANSI-to-HTML converter in Model.js. Show as a "Preview" row above the tab bar |
| **Simulated Context** | CWD: `~/projects/my-app`, exit_code: 0 (toggle to 1 for error preview), git: branch `main`, 2 staged, 1 unstaged, cmd_duration: 0 (toggle to 5000 for duration preview), jobs: 0 |
| **Daemon change** | Add a `preview` message type that accepts full simulated context (not just CWD) and returns a rendered prompt. Or just use the existing `prompt` type with fake data |
| **Effort** | ~6 hr (ANSI rendering in QML is the hardest part) |
| **Impact** | Transformative — turns the panel from a blind config editor into a live design tool |

### 1.2 Preview Environment Toggles

| | |
|---|---|
| **What** | Small toggle buttons below the preview: "Error" (toggle exit_code 0↔1), "Git dirty" (toggle unstaged count), "SSH" (toggle SSH context), "Long command" (toggle cmd_duration), "Jobs" (toggle job count) |
| **Why** | Some segments are invisible in normal state (exit status only shows on error, SSH only shows when connected, duration only above threshold). The user needs to see all states to design their prompt. This is exactly what Starship Builder's "simulated environment" provides |
| **How** | State variables in Panel.qml that modify the simulated context sent to the daemon for preview. Toggle buttons rendered as small pill-shaped chips below the preview |
| **Effort** | ~2 hr (depends on 1.1) |
| **Impact** | High — makes invisible segments visible without opening a terminal |

### 1.3 Theme Color Preview

| | |
|---|---|
| **What** | When switching theme source (omarchy → custom → hybrid), show the resulting color palette as small color swatches in the Appearance tab |
| **Why** | "What does 'hybrid' look like?" is unanswerable without opening a terminal. The daemon already has `resolve_palette` — just expose the palette colors |
| **How** | New daemon command `palette` that returns the current palette as hex colors. On theme change, query daemon and render colored rectangles. Or, add palette to the `config_get` response alongside the config |
| **Effort** | ~2 hr |
| **Impact** | Medium — removes guesswork from theme selection |

### 1.4 Config Diff Toast

| | |
|---|---|
| **What** | When a config change is saved, show a small toast notification at the bottom of the panel: "Changed `prompt.layout` from `omarchy` to `minimal`" |
| **Why** | Confirmation that the save happened. Currently, saves are silent — the user has no way to know if the debounced save fired |
| **How** | Track previous value before `setConfigValue`. Show a `Rectangle` with fade-out animation (2s) at the panel bottom |
| **Effort** | ~1 hr |
| **Impact** | Low-medium — small polish but builds confidence |

---

## Category 2: Bar Widget Intelligence (The Ohno Principle)

> "The most dangerous kind of waste is the waste we do not recognize."
> — Taiichi Ohno

The bar widget currently shows a static `❯` glyph. That's wasted real estate.
The daemon has rich state data available via the socket — git branch, daemon
health, session count. Surface it without requiring the user to open the panel.

### 2.1 Dynamic Bar Glyph

| | |
|---|---|
| **What** | The bar widget's `❯` glyph changes color based on daemon state |
| **Why** | At a glance, the user knows whether the daemon is healthy. Currently, the only way to check is to open the panel and look at "Daemon: running" |
| **How** | On a timer (5s), send `status` command. Green = running, red = not running, yellow = stale git cache (from last prompt's `git_stale` flag). WidgetButton already supports color via `textColor` or equivalent |
| **Effort** | ~1.5 hr |
| **Impact** | Medium — immediate visual health feedback |

### 2.2 Rich Tooltip on Bar Widget

| | |
|---|---|
| **What** | Hovering over the bar widget shows a tooltip with: active session count, current CWD of active session, git branch, daemon PID, last prompt latency |
| **Why** | Quick status check without opening the panel. Waybar modules commonly provide rich tooltips with JSON data. The omarchy-systemd-widget shows this pattern is expected in the Omarchy ecosystem |
| **How** | Override `tooltipText` with a formatted multi-line string. Query daemon `status` periodically (reuse the reconnect timer). Quickshell WidgetButton supports rich tooltip text |
| **Effort** | ~1.5 hr |
| **Impact** | Medium — reduces panel-open frequency for status checks |

### 2.3 Long Command Completion Badge

| | |
|---|---|
| **What** | When a command exceeds the duration threshold and finishes, the bar widget shows a small badge or pulse animation |
| **Why** | "Is my build done?" shouldn't require switching to the terminal. Tablo (macOS) does this with a cat animation. DebugDeck does it with error count badges. agent-session-status does it with colored dots. This is the #1 community request across all desktop prompt integrations |
| **How** | The daemon could emit an event on prompt render when `cmd_duration_ms > threshold`. Or the panel could poll. On trigger, add a small colored dot overlay on the WidgetButton that clears when the panel is opened or after timeout. Desktop notification via `notify-send` as secondary channel |
| **Effort** | ~3 hr |
| **Impact** | High — transforms the bar widget from passive to active |

### 2.4 Git Status Mini-Badge

| | |
|---|---|
| **What** | Show a tiny colored dot next to the `❯` glyph: green = clean repo, yellow = dirty (unstaged changes), no dot = not in a git repo |
| **Why** | Git status is the #1 most-queried prompt segment. Surfacing it at the bar level saves a terminal round-trip. Waybar users build custom modules for exactly this |
| **How** | On daemon status poll, include git status summary. The existing bridge or a periodic status query can provide `is_repo`, `unstaged`, `staged` counts. Render as a 6px colored circle in BarWidget.qml |
| **Effort** | ~2 hr (daemon needs to include last-known git status in `status` response) |
| **Impact** | Medium — constant ambient awareness of repo state |

---

## Category 3: Panel Workflow Improvements (The Collison Principle)

> "The right thing to do is whatever removes the most friction."
> — Patrick Collison

These features reduce friction in common config workflows.

### 3.1 Segment Toggle Grid

| | |
|---|---|
| **What** | Replace the current per-segment options scattered across tabs with a unified "Segments" view showing all segments as toggle cards with enable/disable, reorder, and inline config |
| **Why** | As we add segments in v0.3 (container, python_env, toolchain, nix, k8s, time, battery), the current tab layout won't scale. OMP Configurator's drag-and-drop segment grid is the gold standard |
| **How** | New "Segments" tab (or replace Context tab). Grid of cards, one per segment. Each card: icon + name + toggle + gear icon to expand inline config. Drag handles for reorder (if LayoutPreset supports custom ordering). Config: `[segments.*.enabled]` and segment-specific options |
| **Effort** | ~6 hr |
| **Impact** | High — scales to 20+ segments without UI bloat |

### 3.2 One-Click Tool Setup

| | |
|---|---|
| **What** | In the Shell tab, for tools marked "not found," show an "Install" button that runs the appropriate install command |
| **Why** | Showing "✗ not found" without offering to fix it is an anti-pattern. The user is already in the config UI — let them act. P10k's `configure` wizard installs fonts and dependencies in-flow |
| **How** | Map each tool to its install command: `ble.sh` → `curl -L https://...`, `atuin` → `cargo install atuin`, etc. Launch via `Process.startDetached()` in a floating terminal. Re-detect tools after install. Show a spinner during installation |
| **Effort** | ~3 hr |
| **Impact** | Medium — reduces setup friction for new users |

### 3.3 Doctor Output in Panel

| | |
|---|---|
| **What** | Show `omarchy10k doctor` output in a scrollable text area within the Advanced tab instead of logging to console |
| **Why** | Currently, running Doctor sends output to `console.log` which the user never sees. The whole point of Doctor is to surface problems — hiding the output defeats the purpose |
| **How** | Add a `doctorOutput` property. In `doctorRunner`'s `StdioCollector.onStreamFinished`, set the property. Render in a fixed-height `Flickable` with `TextEdit` (read-only, monospace). Show/hide with a toggle |
| **Effort** | ~1.5 hr |
| **Impact** | Medium — makes Doctor actually useful from the panel |

### 3.4 Config Undo / History

| | |
|---|---|
| **What** | Track the last N config states. Provide "Undo" button (or Ctrl+Z in panel) to revert to previous config |
| **Why** | "Reset to Defaults" is nuclear. "Undo last change" is what users actually want 90% of the time. The current `.bak` file approach only preserves the pre-reset state, not incremental changes |
| **How** | Maintain a circular buffer of `_configFlat` snapshots in Panel.qml (last 10 states). On undo, pop the previous state and apply it via `config_set`. Store in memory only (not persisted). Show undo button in the header row, disabled when no history |
| **Effort** | ~2 hr |
| **Impact** | Medium — reduces fear of experimentation |

### 3.5 Keyboard Navigation

| | |
|---|---|
| **What** | Full keyboard navigation: Tab between controls, Enter to cycle options, arrow keys to switch tabs, Escape to close |
| **Why** | Tiling WM users (Omarchy's core audience) live on the keyboard. A mouse-only panel is friction. PanelKeyCatcher already handles Escape and Tab for panel switching — extend it to in-panel navigation |
| **How** | Add `focus` properties to ControlRow options. Tab cycles through controls top-to-bottom. Left/Right arrows cycle option values within a ControlRow. Up/Down for tab switching. Enter confirms and moves to next control |
| **Effort** | ~3 hr |
| **Impact** | High for keyboard users — turns the panel into a power-user tool |

### 3.6 Config Import / Export

| | |
|---|---|
| **What** | Buttons to export current config to clipboard or file, and import from clipboard or file |
| **Why** | Sharing configs between machines, backing up before experiments, posting configs in discussions. OMP Configurator supports JSON/YAML/TOML export. We should support at minimum clipboard copy of the TOML |
| **How** | "Copy Config" button: `collectConfig` → `buildTOML` → clipboard via `QClipboard` or `xclip` subprocess. "Paste Config" button: read clipboard → `parseTOML` → `applyConfig` → save. Show a toast confirming the action |
| **Effort** | ~2 hr |
| **Impact** | Medium — enables config sharing and portability |

---

## Category 4: Desktop Integration (The Jobs Principle)

> "Design is not just what it looks like. Design is how it works."
> — Steve Jobs

These features make the panel, the prompt, and the terminal feel like parts
of one integrated system rather than three separate tools.

### 4.1 Desktop Notifications for Long Commands

| | |
|---|---|
| **What** | When a command exceeds the duration threshold and the terminal is unfocused, send a desktop notification via `notify-send` |
| **Why** | The #1 community request across all prompt/terminal tools. DebugDeck, agent-session-status, Tablo, and Waybar modules all provide this. Users running long builds while browsing want to know when it's done without checking the terminal |
| **How** | Two paths: (A) The bash adapter emits `notify-send` after `__o10k_timer_stop` if duration exceeds threshold and `$WINDOWFOCUS` is not set. (B) The daemon emits a notification event that Quattro listens for and forwards to `notify-send`. Path A is simpler but runs per-session. Path B allows Quattro to aggregate. Config: `[notifications.long_command]` with `enabled`, `threshold_ms` (default 10000) |
| **Effort** | ~2 hr (path A), ~4 hr (path B) |
| **Impact** | Very high — the most-wanted feature in the desktop integration space |

### 4.2 AI Agent Session Monitoring

| | |
|---|---|
| **What** | Detect running Claude Code / Codex sessions and show their status in the panel or bar widget |
| **Why** | This is the hottest category in desktop tooling right now (2026). Tablo (macOS, animated cat widget), agent-session-status (Waybar/Ironbar), Séance (GTK4 multiplexer), tmux-agent-status — all ship monitoring for AI coding agents. Omarchy's audience is developers who use these tools daily |
| **How** | New panel tab "Agents" or section in Shell tab. Detect Claude Code via `$CLAUDE_CODE_ENTRYPOINT` env var in running shells. Detect Codex via `codex` process. For each agent: show project, status (working/idle/waiting), context window usage. Use `~/.claude/sessions/` directory for Claude Code session metadata. Bar widget: small robot icon with color-coded status |
| **Effort** | ~6 hr |
| **Impact** | High — differentiator; no other Linux desktop shell has native agent monitoring |

### 4.3 Session Workspace Labels

| | |
|---|---|
| **What** | In the multi-session selector, show which Hyprland/KWin workspace each terminal session is on |
| **Why** | When you have 5 sessions across 3 workspaces, "Shell 12345" is meaningless. "Workspace 2: ~/projects/api" is immediately useful. agent-session-status does this for Hyprland already |
| **How** | Query `hyprctl -j clients` (Hyprland) or KWin D-Bus (Plasma) to map PIDs to workspace names. In the session selector, show workspace label alongside PID and CWD. This is compositor-specific; detect and degrade gracefully |
| **Effort** | ~3 hr |
| **Impact** | Medium — helps power users with many terminals |

### 4.4 Theme Sync with Desktop

| | |
|---|---|
| **What** | When the Omarchy theme changes (detected via `theme-set` hook), automatically update the Quattro panel's own colors to match |
| **Why** | Currently, the panel uses hardcoded fallback colors (`#7aa2f7`, `#1a1b26`, etc.) when `qs.Commons.Color` isn't available. Even when it is, the prompt and panel should share the same palette for visual coherence. The daemon already reloads theme on `theme-set` — the panel should too |
| **How** | After daemon reconnect or `reload_theme`, query the palette via `config_get` (or new `palette` command). Apply returned colors to panel styling via dynamic properties. Fall back to current hardcoded values if unavailable |
| **Effort** | ~3 hr |
| **Impact** | Medium — visual coherence across the desktop |

### 4.5 Floating Terminal Integration

| | |
|---|---|
| **What** | "Open Terminal Here" button in the session selector that opens a floating terminal at the session's CWD |
| **Why** | The session list shows CWDs for all active shells. "I want a terminal at that path" is a one-click action instead of: open terminal, `cd /that/long/path`. Omarchy already has `omarchy-launch-floating-terminal-with-presentation` |
| **How** | Add a small terminal icon button next to each session entry. On click, `Process.startDetached()` with `omarchy-launch-floating-terminal-with-presentation bash -c "cd CWD && exec bash"` |
| **Effort** | ~1 hr |
| **Impact** | Medium — convenient shortcut for multi-project workflows |

---

## Category 5: Reliability & Polish (The Wozniak Principle)

> "The constraint is a gift. The fewer parts, the fewer things that break."
> — Steve Wozniak

These features use what's already there to fix rough edges and prevent confusion.

### 5.1 Connection Status Indicator

| | |
|---|---|
| **What** | Small colored dot in the panel header: green = daemon connected, yellow = reconnecting, red = disconnected |
| **Why** | The daemon info box in Advanced tab shows status, but you have to navigate there to see it. A persistent indicator in the header catches problems immediately |
| **How** | Bind a 6px `Rectangle` to `daemonStatus` property. Green for "running", amber for `reconnectTimer.running`, red otherwise. Place next to "Omarchy10k Control Center" title |
| **Effort** | ~30 min |
| **Impact** | Low — small polish but prevents confusion |

### 5.2 Error Feedback on Config Save

| | |
|---|---|
| **What** | If `config_set` returns an error, show it in the panel instead of silently ignoring it |
| **Why** | The current `_handleDaemonMessage` has an `error` case but doesn't surface it to the user. A bad config value could silently fail. Interface contracts (Torvalds lens) require that errors are communicated |
| **How** | On error response after a config_set, show a red toast at the panel bottom with the error message. Add a `lastError` property that clears after 5 seconds |
| **Effort** | ~1 hr |
| **Impact** | Medium — prevents silent failures |

### 5.3 Graceful Degradation Labels

| | |
|---|---|
| **What** | When the daemon doesn't support a feature (old protocol version, missing command), show "Requires daemon v0.3" instead of silently hiding the option |
| **Why** | After upgrading the panel but not the daemon binary, users will wonder why new features don't work. The `daemonProtocolVersion` is already stored — use it |
| **How** | Check `daemonProtocolVersion` against feature requirements. Disable controls with a tooltip showing the minimum version. This becomes important as protocol evolves |
| **Effort** | ~1.5 hr |
| **Impact** | Low-medium — reduces support confusion |

### 5.4 Startup Performance

| | |
|---|---|
| **What** | Reduce panel open latency by caching the last-known config and socket path |
| **Why** | Every panel open runs socket discovery (ls), tool detection (5x command -v), and config load (socket or cat). For a click-to-see interaction, this should be near-instant |
| **How** | Cache `_configFlat`, `sessionList`, and tool statuses in a `Scope`-level singleton (persists across panel open/close within the same bar session). On panel open, show cached data immediately, then refresh in background. Socket discovery runs only if cached socket is stale (doesn't exist) |
| **Effort** | ~2 hr |
| **Impact** | Medium — makes the panel feel instant |

### 5.5 Benchmark Results Display

| | |
|---|---|
| **What** | Add "Run Benchmark" to Advanced tab. Show results: p50/p95/p99 latency, iterations/sec |
| **Why** | The daemon has a benchmark command but the user has to run it from the terminal and read stdout. Surfacing it in the panel lets users measure the impact of their config changes |
| **How** | Run `omarchy10k benchmark --iterations 50` via Process. Parse stdout for timing data. Display in a formatted results card. Include "Your prompt renders in X ms (target: <5ms)" |
| **Effort** | ~2 hr |
| **Impact** | Low-medium — power user feature but builds confidence |

---

## Category 6: Future Vision

Bigger bets for later releases. Included for roadmap awareness.

### 6.1 Visual Prompt Editor (Drag-and-Drop)

| | |
|---|---|
| **What** | Full drag-and-drop segment editor in the panel. Visual representation of prompt segments as blocks. Drag to reorder, click to configure, see preview update in real-time |
| **Why** | OMP Visual Configurator and Starship Builder prove this is the most intuitive config experience. Native desktop version would be even smoother than browser-based |
| **How** | Implement when custom segment ordering is supported in the daemon. Each segment becomes a draggable `Rectangle` with icon, name, and inline settings. Drag handle on left, gear icon on right. Drop zones between segments |
| **Effort** | ~12 hr |
| **Impact** | Very high — best-in-class config experience |

### 6.2 Community Theme Gallery

| | |
|---|---|
| **What** | Browse and one-click apply community-contributed themes |
| **Why** | OMP has a massive theme gallery. Users want inspiration and one-click beauty. "I want my terminal to look like THAT" |
| **How** | Host themes as JSON/TOML files in a GitHub repo. Panel fetches the index, shows thumbnail previews. On select, download and apply. Or bundle popular themes in the package |
| **Effort** | ~8 hr |
| **Impact** | High — drives adoption and community engagement |

### 6.3 tmux / Zellij Status Bridge

| | |
|---|---|
| **What** | Expose daemon state to multiplexer status lines |
| **Why** | Users running tmux/Zellij want the same info in their status bar. Share the git cache between prompt and status line. tmux-agent-status shows the pattern |
| **How** | `omarchy10k tmux-status` subcommand that queries daemon and formats for tmux. Config: `[integration.tmux]` with format template |
| **Effort** | ~6 hr |
| **Impact** | Medium — serves tmux-heavy users |

### 6.4 Hook Broker Visibility

| | |
|---|---|
| **What** | Show registered hooks in the Shell tab: which functions are registered for precmd, preexec, chpwd, shell_exit |
| **Why** | When things go wrong (slow prompt, hooks conflicting), the user needs to see what's registered. Currently invisible without `declare -p __O10K_HOOKS_precmd` in a terminal |
| **How** | New daemon command or bridge query that lists registered hooks. Or a shell-level `omarchy10k hooks` subcommand. Display in Shell tab alongside tool detection |
| **Effort** | ~3 hr |
| **Impact** | Low — debugging feature for power users |

---

## Priority Matrix

### P0 — Ship First (highest impact, reasonable effort)

| # | Feature | Effort | Why P0 |
|---|---------|--------|--------|
| 1.1 | Live Prompt Preview | ~6 hr | Transforms the panel from blind editor to live design tool. The single biggest UX improvement possible |
| 2.3 | Long Command Badge | ~3 hr | Most-requested desktop integration feature across all communities |
| 4.1 | Desktop Notifications | ~2 hr | Same as above, different channel |
| 3.3 | Doctor Output in Panel | ~1.5 hr | Fixes a broken feature (currently logs to nowhere) |
| 5.2 | Error Feedback on Save | ~1 hr | Fixes silent failures — interface contract violation |
| 5.1 | Connection Status | ~30 min | Trivial effort, immediate clarity |

**P0 total: ~14 hr**

### P1 — Ship Soon (high impact or low effort)

| # | Feature | Effort | Why P1 |
|---|---------|--------|--------|
| 1.2 | Preview Environment Toggles | ~2 hr | Multiplies value of 1.1 |
| 2.1 | Dynamic Bar Glyph | ~1.5 hr | Passive health monitoring |
| 2.2 | Rich Tooltip | ~1.5 hr | Reduces panel-open frequency |
| 3.1 | Segment Toggle Grid | ~6 hr | Required for v0.3 segment explosion |
| 3.5 | Keyboard Navigation | ~3 hr | Core audience (tiling WM users) expects it |
| 1.4 | Config Diff Toast | ~1 hr | Small polish, big confidence |
| 4.5 | Floating Terminal | ~1 hr | Leverages existing Omarchy infrastructure |

**P1 total: ~16 hr**

### P2 — Nice to Have

| # | Feature | Effort | Why P2 |
|---|---------|--------|--------|
| 1.3 | Theme Color Preview | ~2 hr | Good polish |
| 2.4 | Git Status Mini-Badge | ~2 hr | Requires daemon protocol change |
| 3.2 | One-Click Tool Setup | ~3 hr | Setup-time improvement |
| 3.4 | Config Undo | ~2 hr | Safety net |
| 3.6 | Config Import/Export | ~2 hr | Portability |
| 4.3 | Workspace Labels | ~3 hr | Power user, compositor-specific |
| 4.4 | Theme Sync | ~3 hr | Visual coherence |
| 5.3 | Degradation Labels | ~1.5 hr | Forward-compatibility |
| 5.4 | Startup Performance | ~2 hr | Snappiness |
| 5.5 | Benchmark Display | ~2 hr | Power user |

**P2 total: ~22.5 hr**

### Backlog

| # | Feature | Effort |
|---|---------|--------|
| 4.2 | AI Agent Monitoring | ~6 hr |
| 6.1 | Visual Prompt Editor | ~12 hr |
| 6.2 | Theme Gallery | ~8 hr |
| 6.3 | tmux/Zellij Bridge | ~6 hr |
| 6.4 | Hook Broker Visibility | ~3 hr |

**Backlog total: ~35 hr**

---

## Recommended Implementation Order

```
Phase 1 (v0.3, with prompt preview as the headliner):
  5.1 Connection Status ─── 30 min warmup
  5.2 Error Feedback ────── 1 hr, fixes broken contract
  3.3 Doctor Output ─────── 1.5 hr, fixes broken feature
  1.1 Live Prompt Preview ─ 6 hr, the headliner
  1.2 Preview Toggles ───── 2 hr, multiplies 1.1's value
  2.3 Long Command Badge ── 3 hr, most-wanted feature
  4.1 Desktop Notifications  2 hr, same story different channel

Phase 2 (v0.3.1 polish):
  2.1 Dynamic Bar Glyph ─── 1.5 hr
  2.2 Rich Tooltip ──────── 1.5 hr
  1.4 Config Diff Toast ─── 1 hr
  3.5 Keyboard Navigation ─ 3 hr
  4.5 Floating Terminal ──── 1 hr

Phase 3 (v0.4, segment-driven):
  3.1 Segment Toggle Grid ─ 6 hr (needed when segments multiply)
  1.3 Theme Color Preview ── 2 hr
  3.4 Config Undo ────────── 2 hr
  3.6 Config Import/Export ─ 2 hr
```

---

## Sources

- Oh My Posh Studio: [ohmyposh.dev/docs/studio](https://ohmyposh.dev/docs/studio) — live WebAssembly prompt rendering in browser
- Oh My Posh Visual Configurator: [github.com/jamesmontemagno/ohmyposh-configurator](https://github.com/jamesmontemagno/ohmyposh-configurator) — drag-and-drop segment builder
- Starship Prompt Builder: [github.com/nicklambourne/starship-prompt-builder](https://github.com/nicklambourne/starship-prompt-builder) — simulated environment with live preview
- Starship Configurator: [github.com/bdmorin/configurator-starship](https://github.com/bdmorin/configurator-starship) — local web wizard with real-binary preview
- ShellConfigurator: [github.com/adrianjiga/ShellConfigurator](https://github.com/adrianjiga/ShellConfigurator) — terminal wizard with font install, segment picker
- Tablo: [github.com/nrkin/tablo](https://github.com/nrkin/tablo) — macOS animated cat widget for Claude/Codex session monitoring
- agent-session-status: [github.com/dcaixinha/agent-session-status](https://github.com/dcaixinha/agent-session-status) — Waybar/Ironbar module for AI agent monitoring
- Séance: [github.com/mintchaos/seance](https://github.com/mintchaos/seance) — GTK4 terminal multiplexer with agent tracking
- tmux-agent-status: [github.com/samleeney/tmux-agent-status](https://github.com/samleeney/tmux-agent-status) — sidebar agent session manager for tmux
- DebugDeck: [github.com/ryansinn/debugdeck](https://github.com/ryansinn/debugdeck) — KDE Plasma 6 systemd journal widget with notifications
- omarchy-systemd-widget: [github.com/tpatzelt/omarchy-systemd-widget](https://github.com/tpatzelt/omarchy-systemd-widget) — Omarchy bar widget for systemd failures
- Quickshell Book: [github.com/programmersd21/the_quickshell_book](https://github.com/programmersd21/the_quickshell_book) — bar widget patterns and best practices
- CShip: [cship.dev](https://cship.dev) — Starship module passthrough for status lines
- Soffit: [crates.io/crates/soffit](https://crates.io/crates/soffit) — desktop statusline editor with drag-and-drop widgets
- Waybar custom modules: [man.archlinux.org/man/waybar-custom.5](https://man.archlinux.org/man/waybar-custom.5.en.txt) — JSON-based custom module pattern
