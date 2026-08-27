# Omarchy10k Project Wiki

Omarchy10k is a reactive shell UI runtime for Bash, purpose-built for the Omarchy Quattro desktop environment. It replaces Starship with a daemon-driven prompt architecture that renders in under 5ms, integrates natively with the Omarchy theme system, and provides a desktop Control Center through a Quattro bar plugin.

**v0.3** adds terminal capability detection (`TermCaps`), layout presets, git worktree support, instant prompt caching, live Quattro preview via the `preview` protocol message, theme color swatches via the `palette` control command, and extended terminal integration (OSC 7/8/777, OSC 9;4 progress, DEC 2026 sync output, undercurl error styling).

## Wiki Pages

| Page | Description |
|------|-------------|
| [Architecture](architecture.md) | System design, component graph, data flow diagrams, and design philosophy |
| [Daemon](daemon.md) | `omarchy10kd` module reference: server, config, theme, git, layout, segments, render, terminal (`TermCaps`) |
| [CLI](cli.md) | `omarchy10k` binary: subcommands, prompt client, doctor diagnostics |
| [Bash Adapter](bash-adapter.md) | Shell integration: hook broker, daemon lifecycle, instant prompt cache, timing, ble.sh mode |
| [Quattro Plugin](quattro.md) | Desktop Control Center: manifest, QML components, daemon IPC, live preview, theme swatches, config UI |
| [Configuration](config.md) | Full config key reference with types, defaults, and valid values |
| [Protocol](protocol.md) | Daemon IPC specification (protocol v0.3): NDJSON over Unix socket, prompt/preview/config/control messages |
| [Theme Integration](theme.md) | Omarchy theme bridge: template, hook, palette loading, Palette API for Quattro swatches |
| [Glossary](glossary.md) | Terms, concepts, environment variables, file paths (includes v0.3 terminal and API terms) |
| [v0.3 Feature Intel](v03-feature-intel.md) | Research-backed feature catalog that informed v0.3: 30 features, compatibility matrix, priority tiers |
| [Quattro QoL Intel](quattro-qol-intel.md) | Quality-of-life improvements for Quattro integration: live preview, bar intelligence, notifications |

## Project Coordinates

| Property | Value |
|----------|-------|
| Language | Rust (daemon + CLI), Bash (shell adapter), QML/JS (Quattro plugin) |
| License | MIT |
| Version | 0.3.0 |
| Protocol version | 0.3 |
| Author | Ian Johnston |
| Repository | `github.com/DividedBeingCode/OmarchyMyBash` |
| Plugin ID | `community.omarchy10k` |

## Repository Layout

```
omarchy10k/
├── Cargo.toml                    # Workspace manifest
├── config/default.toml           # Default configuration (embedded in daemon)
├── .github/workflows/benchmark.yml  # CI benchmark workflow
├── crates/
│   ├── omarchy10k/               # CLI client binary
│   │   └── src/{main,prompt,doctor,bridge}.rs
│   └── omarchy10kd/              # Persistent daemon binary
│       └── src/{main,server,config,git,layout,theme,render,terminal}.rs
│       └── src/segments/{mod,directory,git,exit_status,command_duration,character,os,ssh,jobs,
│           container,python_env,toolchain,nix,k8s,time,battery}.rs
├── shell/omarchy10k.bash         # Bash adapter + hook broker
├── hooks/theme-set               # Omarchy theme-switch hook
├── templates/omarchy10k.toml.tpl # Theme bridge template
├── quattro/                      # Quattro bar plugin
│   ├── manifest.json
│   ├── BarWidget.qml
│   ├── Panel.qml
│   └── Model.js
├── tests/integration_test.sh     # Integration test suite
├── docs/wiki/                    # This wiki
├── README.md
└── LICENSE
```

## Quick Start

```bash
# Build
cd omarchy10k && cargo build --release

# Install binaries
cp target/release/omarchy10k target/release/omarchy10kd ~/.local/bin/

# Activate in .bashrc
echo 'eval "$(omarchy10k init bash)"' >> ~/.bashrc

# Install Quattro plugin (optional)
cp -r quattro/ ~/.config/omarchy/plugins/community.omarchy10k/

# Install theme hook (optional)
mkdir -p ~/.config/omarchy/hooks/theme-set.d
cp hooks/theme-set ~/.config/omarchy/hooks/theme-set.d/omarchy10k
chmod +x ~/.config/omarchy/hooks/theme-set.d/omarchy10k
```

## Maintenance

This wiki is maintained alongside the codebase. When making changes to the project, update the relevant wiki pages to keep documentation in sync. See the project rule `.cursor/rules/wiki-maintenance.mdc` for the update protocol.
