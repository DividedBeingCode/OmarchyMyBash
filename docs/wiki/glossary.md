# Glossary

[← Index](INDEX.md)

## Terms and Concepts

| Term | Definition |
|------|------------|
| **Adapter** | `shell/omarchy10k.bash` — the Bash integration layer sourced into the user's shell. Manages daemon lifecycle, brokers hooks, and sets PS1. |
| **ble.sh** | Bash Line Editor — an optional enhanced readline replacement that provides native lifecycle hooks (PRECMD, PREEXEC, CHPWD) and transient prompt support. |
| **Bridge coprocess** | `omarchy10k bridge` — a persistent Rust process started as a Bash coproc that holds an open socket connection to the daemon. Eliminates fork/exec overhead on the prompt hot path. Communicates with Bash via stdin (JSON requests) and stdout (NUL-terminated prompt strings). |
| **Compact form** | A shorter version of a prompt segment used when terminal width is constrained. Each segment defines both `content` and `compact_content`. |
| **Config API** | Daemon protocol commands (`config_get`, `config_set`) that allow reading and writing configuration over the Unix socket, eliminating the need for direct TOML file I/O by clients. |
| **Control command** | A daemon protocol message with a `"command"` field (e.g., `status`, `reload_config`, `palette`). Distinguished from prompt requests by the presence of this field. |
| **Daemon** | `omarchy10kd` — the persistent Rust process that renders prompts. One instance per shell session. |
| **DaemonState** | The `Arc<DaemonState>` shared state containing config, palette, git cache, and config path. Protected by `RwLock` for concurrent access. |
| **DEC 2026** | Synchronized output mode (`\x1b[?2026h` / `\x1b[?2026l`). Buffers terminal output until complete to prevent screen tearing during prompt redraw. Emitted by the Bash adapter during each prompt render. `TermCaps.has_sync_output` records terminal support for this sequence. |
| **Fan-out** | The theme hook's behavior of broadcasting `reload_theme` to all running daemon sockets, not just one. |
| **Fallback PS1** | Hardcoded prompt (`\w ❯`) used when the daemon is unreachable. Ensures the shell remains usable. |
| **Git cache** | `GitCache` — in-memory HashMap of repo root → git status with a 5-second TTL. Prevents re-running `git status` on every prompt. |
| **Git Worktree** | Git feature allowing multiple working directories from one repo. Detected via `.git` file analysis (`gitdir:` pointer or `commondir` entry). When active, the worktree directory name appears in the git segment after the branch name. |
| **GitAction** | An in-progress git operation detected via `.git/` marker files: Merge, Rebase, CherryPick, Bisect, Revert. |
| **Hook broker** | The adapter's `o10k_hook_add`/`o10k_hook_remove` system that lets shell tools register callbacks for precmd/preexec/chpwd/shell_exit without fighting over PROMPT_COMMAND. |
| **First-run hint** | One-time diagnostic message (`"Daemon not running. Run 'omarchy10k doctor' for diagnostics."`) shown when the daemon fails to start. Gated by `~/.cache/omarchy10k/.first-run-hint-shown` flag file so it fires once per install. |
| **Hot reload** | The daemon's ability to update config or theme without restarting, via filesystem watchers or protocol commands. |
| **install.sh** | One-script installer at `omarchy10k/install.sh`. Runs `cargo build --release`, copies binaries to `~/.local/bin/`, configures `.bashrc`, installs the Quattro plugin and theme-set hook. Supports `--uninstall` to reverse all steps. |
| **In-flight set** | HashSet tracking which git repository roots currently have an active background refresh. Prevents duplicate concurrent git status queries for the same repo. |
| **Instant Prompt** | Cached prompt loaded from `~/.cache/omarchy10k/last_prompt` for zero-latency startup. The Bash adapter sets `PS1` from the cache before the daemon is ready; after each successful render, the bridge writes the new prompt atomically (`tmp` + `mv`) in a background subshell. |
| **Layout engine** | `LayoutEngine` — resolves which segments fit the terminal width using priority-based compression. |
| **Layout Preset** | Predefined segment ordering and separator style (`omarchy`, `minimal`, `powerline`, `classic`, `pure`, `dense`). Controls which segments appear, their order via `LayoutPreset::segment_order()`, separator character via `LayoutPreset::separator()`, and single-line vs two-line layout via `LayoutPreset::is_single_line()`. Configured by `prompt.layout`. |
| **NDJSON** | Newline-Delimited JSON — the protocol format. One JSON object per line, terminated by `\n`. |
| **OSC 7** | Operating System Command for CWD reporting (`\e]7;file://...\a`). Enables terminal CWD tracking for new tab/split behavior. Emitted by the Bash adapter after each successful prompt render. `TermCaps.has_osc7` records terminal support. |
| **OSC 8** | Hyperlink escape sequence. Wraps text in clickable `file://` URLs (used by the directory segment). Emitted when `TermCaps.has_osc8` is true. |
| **OSC 9;4** | Progress bar indicator escape sequence (`\e]9;4;N\a`). Shows indeterminate progress (`N=3`) during command execution and clears (`N=0`) on prompt render. Emitted by the Bash adapter during preexec/precmd. |
| **OSC 133** | Operating System Command sequence for FinalTerm/shell integration. Marks prompt boundaries so terminals can detect command regions. |
| **OSC 777** | Desktop notification escape sequence. Used for long-command notifications when `TermCaps.has_osc777` is true and notification config is enabled. |
| **Palette** | `ThemePalette` — the set of semantic color roles (accent, foreground, muted, etc.) used by all segments. |
| **Palette API** | Daemon control command (`command: "palette"`) returning current theme colors as hex strings. Used by Quattro for theme color swatches without reading `colors.toml` directly. |
| **Parent PID monitor** | The daemon's background task that checks `kill(ppid, 0)` every 2 seconds and exits when the parent shell is gone. |
| **Per-shell daemon** | Architecture where each Bash session gets its own `omarchy10kd` process and socket. Provides isolation and automatic cleanup. |
| **Porcelain v2** | Git's machine-readable status format (`git status --porcelain=v2 --branch`). Used by the daemon for reliable parsing. |
| **Preexec ready gate** | `__O10K_PREEXEC_READY` flag that prevents the preexec handler from firing multiple times per command line. |
| **Preview API** | Daemon protocol message (`type: "preview"`) that renders a prompt with simulated context for Quattro live preview. Accepts optional fields (`cwd`, `exit_code`, `git_branch`, `git_staged`, `cols`, etc.). Omits OSC 133 markers and skips git subprocesses. |
| **Priority** | Segment attribute (lower number = more important). The layout engine keeps high-priority segments when space is tight. |
| **Protocol version** | Version string (currently `"0.3"`) exchanged during `hello` handshake. Enables backward-compatible protocol evolution. |
| **Quattro** | The Omarchy desktop bar/panel system built on Quickshell. The Omarchy10k plugin appears as a bar widget. |
| **Quickshell** | The QML-based desktop shell framework used by Omarchy Quattro. Provides `Process`, `Socket`, bar/panel infrastructure. |
| **Segment** | A discrete piece of the prompt (directory, git, exit status, command duration). Each is a `render(ctx) -> Option<Segment>` function. |
| **SegmentContext** | Struct carrying all inputs a segment needs: cwd, exit code, duration, width, jobs, SSH status, git status, config, palette, and `term_caps` (cached `TermCaps` for the render cycle). |
| **Shell integration detection** | The adapter's `__o10k_detect_terminal_integration` function that checks for existing terminal shell integrations (Ghostty, VTE, WezTerm, Kitty) to avoid duplicate OSC 133 emission. Complemented by daemon-side `TermCaps::detect()` for per-terminal feature gating. |
| **Smart truncation** | Directory segment's algorithm: keeps first and last path components, uses unique prefixes for middle directories, preserves git repo roots. |
| **Stale-while-revalidate** | Git caching strategy where expired cache entries return immediately (marked `stale: true`) while an async background task refreshes the data. Prevents blocking on git subprocess. |
| **Socket path** | `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock` — per-shell daemon socket. |
| **TermCaps** | Terminal capability detection struct (`terminal.rs`). Identifies the terminal emulator (Ghostty, Foot, Kitty, WezTerm, Alacritty, Unknown) and its supported features: OSC 7/8/52/777, undercurl, sync output (DEC 2026), Kitty graphics, Sixel. Detected via environment variables (`TERM_PROGRAM`, `GHOSTTY_RESOURCES_DIR`, `KITTY_WINDOW_ID`). |
| **Transient prompt** | Feature where previous prompts are replaced with a minimal `❯` after command execution, reducing visual noise. |
| **True-color** | 24-bit RGB ANSI color (16 million colors). Omarchy10k uses this exclusively — no 256-color or 16-color fallback. |
| **Typed message** | Protocol message format with `type`, `id`, and `version` fields. Types: `hello`, `control`, `prompt`, `preview`, `config`, `error`. |
| **Undercurl** | Extended underline style (SGR `4:3`) with colored wavy underline. Used for error indicators on the prompt character and exit status segment when `TermCaps.has_undercurl` is true. |

## Environment Variables

| Variable | Set by | Read by | Purpose |
|----------|--------|---------|---------|
| `XDG_RUNTIME_DIR` | System | Adapter, CLI, daemon, hook | Base directory for socket files |
| `O10K_PARENT_PID` | Adapter (sets to `$$`) | Daemon, CLI | Socket naming PID |
| `O10K_BIN` | User (optional) | Adapter | Override `omarchy10k` binary path |
| `O10K_DAEMON_BIN` | User (optional) | Adapter | Override `omarchy10kd` binary path |
| `O10K_SHELL_INTEGRATION` | User (optional) | Adapter | Control OSC 133 emission: `auto`, `force`, `off` |
| `O10K_NOTIFY_THRESHOLD` | User (optional) | Adapter | Initial desktop notification threshold in ms (default `10000`). Overridden at runtime by `notify_threshold_ms` from daemon prompt response. |
| `GHOSTTY_RESOURCES_DIR` | Ghostty | Adapter | Ghostty terminal detection |
| `KITTY_SHELL_INTEGRATION` | Kitty | Adapter | Kitty terminal detection |
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
| `$XDG_CACHE_HOME/omarchy10k/last_prompt` | Instant prompt cache (default `~/.cache/omarchy10k/last_prompt`) | Bash adapter / bridge |
| `~/.config/omarchy/plugins/community.omarchy10k/` | Installed Quattro plugin | User (manual copy) |
| `~/.config/omarchy/hooks/theme-set.d/omarchy10k` | Theme switch hook | User (manual install) |
| `$XDG_CACHE_HOME/omarchy10k/.first-run-hint-shown` | First-run hint gate flag | Bash adapter |
| `~/.local/share/blesh/ble.sh` | ble.sh installation | User |
