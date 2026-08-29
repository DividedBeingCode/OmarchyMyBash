# Omarchy10k Project Wiki

Omarchy10k is a reactive shell UI runtime for Bash, purpose-built for the Omarchy Quattro desktop environment. It replaces Starship with a daemon-driven prompt architecture that renders in under 5ms, integrates natively with the Omarchy theme system, and provides a desktop Control Center through a Quattro bar plugin.

**v0.3** adds terminal capability detection (`TermCaps`), layout presets, git worktree support, instant prompt caching, live Quattro preview via the `preview` protocol message, theme color swatches via the `palette` control command, extended terminal integration (OSC 7/8/777, OSC 9;4 progress, DEC 2026 sync output, undercurl error styling), a one-script installer (`install.sh`), first-run diagnostics hints, notification threshold wiring from daemon config to bash, title format placeholder expansion (`{dir}`, `{user}`, `{host}`, `{branch}`), and TermCaps-gated OSC 8 hyperlinks.

**v0.4** adds the env channel (live env-derived segments — python/nix/mise/k8s/agent now respond to `activate`, `mise use`, `nix develop`), real notifications routed through `omarchy-notification-send`, an enriched `status` ambient snapshot, transient prompt wiring via the bridge's 4-field NUL framing, stale-aware git placeholder, true powerline/rainbow background-fill rendering, the `omarchy10k statusline` subcommand for Claude Code, the agent-signal segment, optional OSC 133;C/D semantic prompt emission, Quattro plugin IPC (`omarchy-shell call community.omarchy10k <method>`), a service-kind connection hub, a session-picker overlay, ANSI-colored live panel preview, and the `omarchy10k intro` first-run render. Protocol version is now **0.4**.

**v0.5** adds Looks — atomic appearance bundles in `[looks.<name>]` with 8 curated Looks compiled in (user entries shadow curated names) — exposed through the `looks`, `looks_apply` (persistent, or transient in-memory "Try"), `looks_save`, and `palettes` control verbs plus a `look` override on `preview` for dry-run renders; the enriched `status` snapshot (live git summary, battery, last command duration, session age); a configurable right prompt rail (`[prompt].right_segments`); the vi-mode prompt character delivered over the env channel (`KEYMAP` → `vi_mode`); the quick-action scripts CLI (`omarchy10k script list|run` via the daemon's `script_exec` module) and desktop hook dispatch (`omarchy10k hook-event`); the Quattro Looks Gallery overlay and the 4-bucket Control Center rail; session workspace labels; bar badges; and daemon hardening (64 KiB socket frame cap, LRU-bounded caches, `PR_SET_PDEATHSIG`). Protocol version is now **0.5**, crate version **0.4.0**.

## Wiki Pages

| Page | Description |
|------|-------------|
| [Architecture](architecture.md) | System design, component graph, data flow diagrams, and design philosophy |
| [Daemon](daemon.md) | `omarchy10kd` module reference: server, config, theme, git, layout, segments, render, terminal (`TermCaps`) |
| [CLI](cli.md) | `omarchy10k` binary: subcommands, prompt client, doctor diagnostics |
| [Bash Adapter](bash-adapter.md) | Shell integration: hook broker, daemon lifecycle, instant prompt cache, timing, ble.sh mode |
| [Quattro Plugin](quattro.md) | Desktop Control Center: manifest, QML components, daemon IPC, live preview, theme swatches, config UI |
| [Configuration](config.md) | Full config key reference with types, defaults, and valid values (incl. Wave 1 visual-depth keys) |
| [Protocol](protocol.md) | Daemon IPC specification (protocol v0.5): NDJSON over Unix socket, prompt/preview/config/control/statusline messages, env channel, Looks verbs, enriched status |
| [Glossary](glossary.md) | Terms, concepts, environment variables, file paths (includes v0.3 terminal and API terms) |
| [v0.3 Feature Intel](v03-feature-intel.md) | Research-backed feature catalog that informed v0.3: 30 features, compatibility matrix, priority tiers |
| [Quattro QoL Intel](quattro-qol-intel.md) | Quality-of-life improvements for Quattro integration: live preview, bar intelligence, notifications |
| [Bug Audit](bug-audit.md) | Correctness audit of v0.3.0: 20 findings ranked by severity, with reproductions and fix directions |
| [v0.4 Feature Intel](v04-feature-intel.md) | Next-release feature catalog: foundations, headliners, Omarchy desktop integration, onboarding — plus a ratified kill list and watch list |

> **Current known issues:** a full correctness audit of v0.3.0 is recorded in
> [Bug Audit](bug-audit.md). Four findings are rated critical — unwrapped prompt
> escapes corrupting readline width accounting, a `struct tm` ABI mismatch in the
> time segment, two UTF-8 slicing panics, and the daemon exiting immediately
> without `O10K_PARENT_PID`. Read it before changing `render.rs`, the segment
> layer, or the Bash adapter.

## Project Coordinates

| Property | Value |
|----------|-------|
| Language | Rust (daemon + CLI), Bash (shell adapter), QML/JS (Quattro plugin) |
| License | MIT |
| Version | 0.4.0 |
| Protocol version | 0.5 |
| Repository | `github.com/DividedBeingCode/OmarchyMyBash` |
| Plugin ID | `community.omarchy10k` |

## Repository Layout

```
omarchy10k/
├── Cargo.toml                       # Workspace manifest (crate 0.4.0)
├── install.sh                       # One-script installer (builds, binaries, shell, Quattro plugin, hooks, rice templates, tools)
├── .github/workflows/benchmark.yml  # CI benchmark workflow
├── config/
│   ├── default.toml                 # Default configuration (embedded in daemon)
│   └── tools.sh                     # Modern CLI alias layer (eza/bat/rg/zoxide/fzf/...), sourced by the adapter
├── crates/
│   ├── omarchy10k/                  # CLI client binary
│   │   └── src/{main,prompt,bridge,doctor,configure,intro,script,hook_event,statusline,update}.rs
│   └── omarchy10kd/                 # Persistent daemon binary
│       └── src/{main,server,config,git,layout,looks,style,render,script_exec,terminal,theme}.rs
│       └── src/segments/{mod,ai,battery,character,command_duration,container,directory,exit_status,git,jobs,
│           k8s,load,nix,os,python_env,ssh,time,toolchain}.rs
├── shell/omarchy10k.bash            # Bash adapter + hook broker
├── hooks/                           # Omarchy hook drop-ins (installed to ~/.config/omarchy/hooks/<event>.d/omarchy10k)
│   ├── theme-set                    # Theme switch → reload_theme fan-out
│   ├── battery-low                  # Low battery → desktop notification toast
│   ├── post-update                  # After omarchy update → self-update + invalidate_git fan-out
│   └── font-set                     # Font switch → reload_theme fan-out
├── templates/
│   ├── omarchy10k.toml.tpl          # Theme bridge template
│   └── themed/                      # Rice-layer templates (o10k-env.sh, lazygit, yazi, cava) for ~/.config/omarchy/themed/
├── quattro/                         # Quattro bar plugin
│   ├── manifest.json
│   ├── BarWidget.qml                # Bar glyph + badges (daemon status, git dirty, long-cmd chip)
│   ├── Panel.qml                    # Control Center — 4-bucket rail (LOOKS · STYLE · BEHAVIOR · SYSTEM)
│   ├── Gallery.qml                  # Full-screen Looks gallery overlay (live dry-run renders)
│   ├── SessionPicker.qml            # Live session picker overlay
│   ├── Service.qml                  # Persistent connection hub (service-kind plugin)
│   └── Model.js                     # TOML parser, CONFIG_MAP, protocol helpers
├── tests/
│   ├── integration_test.sh          # Integration test suite
│   └── model_parity_test.js         # Model.js CONFIG_MAP round-trip parity harness
├── docs/wiki/                       # This wiki
├── README.md
└── LICENSE
```

## Quick Start

```bash
# One-line install (builds, installs binaries, configures shell, sets up Quattro plugin + theme hook)
cd omarchy10k && ./install.sh

# Update to latest
omarchy10k update

# Browse and apply Looks (appearance bundles; persistent or try-in-memory)
omarchy10k look list
omarchy10k look apply tokyo-rainbow
omarchy10k look apply omnarchy --transient

# Quick actions and desktop hook events
omarchy10k script list
omarchy10k hook-event battery-low 15

# To uninstall
./install.sh --uninstall
```

Or manually:

```bash
cd omarchy10k && cargo build --release
cp target/release/omarchy10k target/release/omarchy10kd ~/.local/bin/
echo 'eval "$(omarchy10k init bash)"' >> ~/.bashrc
cp -r quattro/ ~/.config/omarchy/plugins/community.omarchy10k/
for event in theme-set battery-low post-update font-set; do
  mkdir -p ~/.config/omarchy/hooks/$event.d
  cp hooks/$event ~/.config/omarchy/hooks/$event.d/omarchy10k
  chmod +x ~/.config/omarchy/hooks/$event.d/omarchy10k
done
```

## Maintenance

This wiki is maintained alongside the codebase. When making changes to the project, update the relevant wiki pages to keep documentation in sync. See the project rule `.cursor/rules/wiki-maintenance.mdc` for the update protocol.
