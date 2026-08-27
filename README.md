# Omarchy10k

A reactive shell UI runtime for Bash on Omarchy Quattro.

Omarchy10k combines the prompt intelligence of Powerlevel10k, the batteries-included
convenience of Oh My Zsh, and native Omarchy Quattro integration into a single
coherent Bash experience.

## Features

- **Sub-5ms prompt rendering** via a persistent Rust daemon (`omarchy10kd`)
- **Bash hook broker** that eliminates PROMPT_COMMAND conflicts between tools
- **Smart Git status** with inotify-based cache invalidation
- **Responsive layout engine** with priority-based segment visibility
- **Omarchy theme sync** — prompt colors follow your desktop theme automatically
- **ble.sh integration** for transient prompts, right prompts, and show-on-command
- **Quattro Control Center** panel for visual configuration

## Install

```bash
# Build from source
cargo install --path crates/omarchy10k
cargo install --path crates/omarchy10kd

# Add to ~/.bashrc
eval "$(omarchy10k init bash)"

# Run diagnostics
omarchy10k doctor
```

## Configuration

Configuration lives at `~/.config/omarchy10k/config.toml`.

```toml
[prompt]
layout = "omarchy"
transient = true

[theme]
source = "omarchy"

[git]
enabled = true
mode = "adaptive"

[segments.command_duration]
show_above_ms = 1500
```

## Architecture

```
Bash → omarchy10k.bash (hook broker) → Unix socket → omarchy10kd (Rust daemon)
                                                           ↑
                                                      inotify watches
                                                    (git, theme, config)
```

## License

MIT
