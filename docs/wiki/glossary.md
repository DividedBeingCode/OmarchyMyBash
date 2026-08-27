# Glossary

[← Index](INDEX.md)

## Terms and Concepts

| Term | Definition |
|------|------------|
| **Adapter** | `shell/omarchy10k.bash` — the Bash integration layer sourced into the user's shell. Manages daemon lifecycle, brokers hooks, and sets PS1. |
| **ble.sh** | Bash Line Editor — an optional enhanced readline replacement that provides native lifecycle hooks (PRECMD, PREEXEC, CHPWD) and transient prompt support. |
| **Compact form** | A shorter version of a prompt segment used when terminal width is constrained. Each segment defines both `content` and `compact_content`. |
| **Control command** | A daemon protocol message with a `"command"` field (e.g., `status`, `reload_config`). Distinguished from prompt requests by the presence of this field. |
| **Daemon** | `omarchy10kd` — the persistent Rust process that renders prompts. One instance per shell session. |
| **DaemonState** | The `Arc<DaemonState>` shared state containing config, palette, git cache, and config path. Protected by `RwLock` for concurrent access. |
| **Fan-out** | The theme hook's behavior of broadcasting `reload_theme` to all running daemon sockets, not just one. |
| **Fallback PS1** | Hardcoded prompt (`\w ❯`) used when the daemon is unreachable. Ensures the shell remains usable. |
| **Git cache** | `GitCache` — in-memory HashMap of repo root → git status with a 5-second TTL. Prevents re-running `git status` on every prompt. |
| **GitAction** | An in-progress git operation detected via `.git/` marker files: Merge, Rebase, CherryPick, Bisect, Revert. |
| **Hook broker** | The adapter's `o10k_hook_add`/`o10k_hook_remove` system that lets shell tools register callbacks for precmd/preexec/chpwd/shell_exit without fighting over PROMPT_COMMAND. |
| **Hot reload** | The daemon's ability to update config or theme without restarting, via filesystem watchers or protocol commands. |
| **Layout engine** | `LayoutEngine` — resolves which segments fit the terminal width using priority-based compression. |
| **NDJSON** | Newline-Delimited JSON — the protocol format. One JSON object per line, terminated by `\n`. |
| **OSC 133** | Operating System Command sequence for FinalTerm/shell integration. Marks prompt boundaries so terminals can detect command regions. |
| **Palette** | `ThemePalette` — the set of semantic color roles (accent, foreground, muted, etc.) used by all segments. |
| **Parent PID monitor** | The daemon's background task that checks `kill(ppid, 0)` every 2 seconds and exits when the parent shell is gone. |
| **Per-shell daemon** | Architecture where each Bash session gets its own `omarchy10kd` process and socket. Provides isolation and automatic cleanup. |
| **Porcelain v2** | Git's machine-readable status format (`git status --porcelain=v2 --branch`). Used by the daemon for reliable parsing. |
| **Preexec ready gate** | `__O10K_PREEXEC_READY` flag that prevents the preexec handler from firing multiple times per command line. |
| **Priority** | Segment attribute (lower number = more important). The layout engine keeps high-priority segments when space is tight. |
| **Quattro** | The Omarchy desktop bar/panel system built on Quickshell. The Omarchy10k plugin appears as a bar widget. |
| **Quickshell** | The QML-based desktop shell framework used by Omarchy Quattro. Provides `Process`, `Socket`, bar/panel infrastructure. |
| **Segment** | A discrete piece of the prompt (directory, git, exit status, command duration). Each is a `render(ctx) -> Option<Segment>` function. |
| **SegmentContext** | Struct carrying all inputs a segment needs: cwd, exit code, duration, width, jobs, SSH status, git status, config, palette. |
| **Smart truncation** | Directory segment's algorithm: keeps first and last path components, uses unique prefixes for middle directories, preserves git repo roots. |
| **Socket path** | `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock` — per-shell daemon socket. |
| **Transient prompt** | Feature where previous prompts are replaced with a minimal `❯` after command execution, reducing visual noise. |
| **True-color** | 24-bit RGB ANSI color (16 million colors). Omarchy10k uses this exclusively — no 256-color or 16-color fallback. |

## Environment Variables

| Variable | Set by | Read by | Purpose |
|----------|--------|---------|---------|
| `XDG_RUNTIME_DIR` | System | Adapter, CLI, daemon, hook | Base directory for socket files |
| `O10K_PARENT_PID` | Adapter (sets to `$$`) | Daemon, CLI | Socket naming PID |
| `O10K_BIN` | User (optional) | Adapter | Override `omarchy10k` binary path |
| `O10K_DAEMON_BIN` | User (optional) | Adapter | Override `omarchy10kd` binary path |
| `PPID` | Shell | CLI | Fallback PID for socket path |
| `HOME` | System | Config, theme, doctor | Home directory for paths |
| `BASH_VERSION` | Bash | Doctor | Version detection |
| `BASH_VERSINFO` | Bash | Adapter | Feature detection (PS0, array PROMPT_COMMAND) |
| `BASH_COMMAND` | Bash | Adapter | Command string for preexec |
| `BLE_VERSION` | ble.sh | Adapter, doctor | ble.sh detection and version |
| `COLUMNS` | Terminal | Adapter | Terminal width for layout |
| `EPOCHREALTIME` | Bash 5.0+ | Adapter | Microsecond command timing |
| `COLORTERM` | Terminal | Doctor | True-color support detection |
| `TERM` | Terminal | Doctor | Terminal type |
| `TERM_PROGRAM` | Terminal | Doctor | Terminal emulator name |
| `PROMPT_COMMAND` | Bash | Adapter, doctor | Hook chain (preserved on install) |
| `OMARCHY_PATH` | Omarchy | Doctor | Omarchy installation detection |
| `SSH_TTY` | OpenSSH | Daemon (render.rs) | SSH session detection |
| `SSH_CONNECTION` | OpenSSH | Daemon (render.rs) | SSH session detection |
| `PWD` | Shell | Adapter | Current directory, chpwd detection |
| `EDITOR` | User | Quattro panel | Config file editor |

## File Paths

| Path | Purpose | Created by |
|------|---------|------------|
| `~/.config/omarchy10k/config.toml` | User configuration | User or Quattro panel |
| `~/.local/state/omarchy/current/theme/colors.toml` | Generated theme palette | Omarchy theme engine |
| `~/.local/state/omarchy/current/theme.name` | Current theme name | Omarchy theme engine |
| `$XDG_RUNTIME_DIR/omarchy10k-{pid}.sock` | Daemon socket | `omarchy10kd` |
| `~/.config/omarchy/plugins/community.omarchy10k/` | Installed Quattro plugin | User (manual copy) |
| `~/.config/omarchy/hooks/theme-set.d/omarchy10k` | Theme switch hook | User (manual install) |
| `~/.local/share/blesh/ble.sh` | ble.sh installation | User |
