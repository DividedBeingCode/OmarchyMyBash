# Omarchy10k Project Wiki

Omarchy10k is a reactive shell UI runtime for Bash, purpose-built for the Omarchy Quattro desktop environment. It replaces Starship with a daemon-driven prompt architecture that renders in under 5ms, integrates natively with the Omarchy theme system, and provides a desktop Control Center through a Quattro bar plugin.

## Wiki Pages

| Page | Description |
|------|-------------|
| [Architecture](architecture.md) | System design, component graph, data flow diagrams, and design philosophy |
| [Daemon](daemon.md) | `omarchy10kd` module reference: server, config, theme, git, layout, segments, render |
| [CLI](cli.md) | `omarchy10k` binary: subcommands, prompt client, doctor diagnostics |
| [Bash Adapter](bash-adapter.md) | Shell integration: hook broker, daemon lifecycle, timing, ble.sh mode |
| [Quattro Plugin](quattro.md) | Desktop Control Center: manifest, QML components, daemon IPC, config UI |
| [Configuration](config.md) | Full config key reference with types, defaults, and valid values |
| [Protocol](protocol.md) | Daemon IPC specification: NDJSON over Unix socket, commands, responses |
| [Theme Integration](theme.md) | Omarchy theme bridge: template, hook, palette loading, color roles |
| [Glossary](glossary.md) | Terms, concepts, environment variables, file paths |

## Project Coordinates

| Property | Value |
|----------|-------|
| Language | Rust (daemon + CLI), Bash (shell adapter), QML/JS (Quattro plugin) |
| License | MIT |
| Version | 0.1.0 |
| Author | Ian Johnston |
| Repository | `github.com/DividedBeingCode/OmarchyMyBash` |
| Plugin ID | `community.omarchy10k` |

## Repository Layout

```
omarchy10k/
├── Cargo.toml                    # Workspace manifest
├── config/default.toml           # Default configuration (embedded in daemon)
├── crates/
│   ├── omarchy10k/               # CLI client binary
│   │   └── src/{main,prompt,doctor}.rs
│   └── omarchy10kd/              # Persistent daemon binary
│       └── src/{main,server,config,git,layout,theme,render}.rs
│       └── src/segments/{mod,directory,git,exit_status,command_duration,character}.rs
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
