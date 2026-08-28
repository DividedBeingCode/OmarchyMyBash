# CLI Reference (`omarchy10k`)

[← Index](INDEX.md) | [Protocol](protocol.md) | [Bash Adapter](bash-adapter.md)

The `omarchy10k` binary is a thin async client. It does not render prompts — it routes requests to the daemon over the Unix socket and handles user-facing subcommands.

## Subcommands

### `omarchy10k init bash`

Emits the Bash adapter source to stdout. Integration point for `.bashrc`:

```bash
eval "$(omarchy10k init bash)"
```

Prints the full contents of `shell/omarchy10k.bash` via `include_str!`. Only `"bash"` is accepted — any other shell name prints an error and exits with code 1.

### `omarchy10k prompt`

Requests a prompt render from the daemon.

| Flag | Default | Description |
|------|---------|-------------|
| `--cwd` | `"."` | Working directory (resolved to absolute if `"."`) |
| `--exit-code` | `0` | Last command exit code |
| `--cmd-duration-ms` | `0` | Last command duration in milliseconds |
| `--cols` | `80` | Terminal width |
| `--jobs` | `0` | Background job count |

Sends a JSON prompt request to the daemon socket, extracts the `left` field from the response, and prints it to stdout.

**Fallback:** If the daemon is unreachable, prints a hardcoded fallback PS1:
```
\[\e[1;34m\]\w\[\e[0m\] \[\e[1;32m\]❯\[\e[0m\]
```

### `omarchy10k bridge`

Runs as a persistent bridge coprocess between Bash and the daemon. Long-running process that amortizes socket connection overhead across prompt renders.

| Flag | Default | Description |
|------|---------|-------------|
| `--socket` | auto-detected | Unix socket path (same resolution as other subcommands) |

**Behavior:**

1. Reads JSON requests from stdin line-by-line
2. Forwards each request to the daemon via Unix socket
3. Extracts the response into FOUR NUL-terminated stdout fields:
   `left\0right\0notify_threshold_ms\0transient\0`
4. Relays `semantic_prompts`/`notify_unfocused_only` via a `<socket>.flags`
   side-channel file (written only on change)

Supports both JSON input and tab-separated `key=value` format (converted to JSON internally).

3. Extracts the response into FOUR NUL-terminated stdout fields:
   `left\0right\0notify_threshold_ms\0transient\0`
4. Relays `semantic_prompts`/`notify_unfocused_only` via a `<socket>.flags`
   side-channel file (written only on change)

The Bash adapter starts this as a coprocess:

```bash
coproc __O10K_BRIDGE { "$__O10K_BIN" bridge --socket "$__O10K_SOCKET"; }
```

### `omarchy10k doctor`

System diagnostics — checks compatibility, optional tools, daemon health, hooks, and config. Read-only, no modifications.

Checks performed (in order):

| Check | Pass | Warn | Fail |
|-------|------|------|------|
| Bash version | ≥ 5 | < 5 | Not found |
| Nerd Font | — | Visual check recommended | — |
| TrueColor | `COLORTERM=truecolor\|24bit` | Other values | — |
| ble.sh | `BLE_VERSION` set | Installed but not loaded | Not installed (optional) |
| Omarchy | `OMARCHY_PATH` set, theme readable | — | Not detected (optional) |
| Mise | `mise --version` succeeds | — | Not installed (optional) |
| Atuin | `atuin --version` succeeds | — | Not installed (optional) |
| Zoxide | `zoxide --version` succeeds | — | Not installed (optional) |
| fzf | `fzf --version` succeeds | — | Not installed (optional) |
| Terminal | `TERM_PROGRAM` or `TERM` | — | — |
| Daemon | Socket exists + status ok | Socket exists but unresponsive | Not running |
| Hooks | `O10K_PARENT_PID` exported (adapter installed) | Not detected — check after init | — |
| Config | config.toml exists at XDG path | — | Using defaults |

### `omarchy10k reload`

Sends `{"command":"reload_config"}` to the daemon. Prints `"config reloaded"` on success.

### `omarchy10k benchmark`

Performance test. Sends repeated prompt requests and reports latency statistics.

| Flag | Default | Description |
|------|---------|-------------|
| `--iterations` | `100` | Number of prompt requests |

Uses current directory (or `/tmp`) as cwd. Fixed params: `exit_code=0`, `cmd_duration_ms=0`, `cols=120`, `jobs=0`.

Output: average, p50, p95, p99 in milliseconds. Pass/fail against 5ms p50 target.

### `omarchy10k benchmark-shell` (hidden)

Hidden subcommand for end-to-end shell-level latency benchmarking. Measures real wall time for prompt render including socket I/O — closer to what the user experiences than `benchmark`, which only times the CLI→daemon round trip.

| Flag | Default | Description |
|------|---------|-------------|
| `--iterations` | `100` | Number of prompt render cycles |
| `--adapter` | — | Optional path to Bash adapter script |

### `omarchy10k update`

One-command upgrade path. Pulls latest source, rebuilds, replaces binaries, refreshes the Quattro plugin and theme hook, and gracefully restarts running daemons.

| Flag | Default | Description |
|------|---------|-------------|
| `--no-pull` | false | Skip `git pull` — rebuild from current source tree |
| `--no-build` | false | Skip `cargo build` — reinstall existing binaries + plugin only |

**Source discovery** (checked in order):

1. `O10K_SOURCE_DIR` environment variable
2. Walk up from the running binary's path looking for the workspace `Cargo.toml`
3. Read the breadcrumb file at `~/.local/share/omarchy10k/source-dir` (written by `install.sh`)

**Workflow:**

1. Locate source directory
2. Print installed vs source version
3. `git pull --ff-only` (unless `--no-pull`; skips if dirty working tree)
4. `cargo build --release` (unless `--no-build`)
5. Atomic binary install to `~/.local/bin/` (write to `.tmp`, rename)
6. Copy `quattro/*` to plugin directory; patch `manifest.json` version from `Cargo.toml`
7. Trigger Quattro plugin rescan (`omarchy-shell shell rescanPlugins`, silent if unavailable)
8. Copy `hooks/theme-set` to hook directory
9. Copy `templates/omarchy10k.toml.tpl` to `~/.local/share/omarchy/templates/` (theme bridge)
10. Send `shutdown` command to all running daemon sockets (`omarchy10k-*.sock`)
11. Print version change summary

Running daemons auto-restart on the next prompt render via the Bash adapter's existing reconnection logic.

### `omarchy10k statusline`

Renders the Claude Code statusLine through the daemon with the active theme
palette (1.2). Reads the full statusLine JSON payload from stdin (the exact
documented Claude Code contract — no session-file access), wraps it as
`{"type":"statusline","id":"cli-<pid>","payload":{...}}`, and prints the
daemon's rendered `left` string raw.

```
~/.claude/settings.json:
"statusLine": {"type": "command", "command": "omarchy10k statusline"}
```

| Condition | Behavior |
|-----------|----------|
| Daemon reachable, `status:"ok"` | Print daemon `left` (ANSI, no OSC 133) |
| Daemon down / timeout (300 ms) / non-ok response | Builtin fallback render in pure Rust: bold model display name + context % colored by conventional thresholds (green < 70 %, yellow < 90 %, red ≥ 90 %) |
| stdin is not a JSON object | stderr message, exit code 2 |

The fallback tolerates Claude Code schema drift: context % is probed at
`context_window`/`context` keys (`used_percent`/`used_pct`/`percentage`),
falling back to token counts against a 200k window; absent signals are
omitted. The whole daemon round trip is wrapped in a 300 ms timeout so the
statusline can never hang Claude Code. Warm latency budget: < 50 ms.

install.sh merges (never overwrites) the snippet into
`~/.claude/settings.json` idempotently — an existing third-party `statusLine`
is left untouched; ours is detected by its exact command string. A backup is
written to `settings.json.o10k.bak` before the first merge, and a hint is
printed when `~/.claude` is absent.

### `omarchy10k intro`

One-time themed welcome (3.3). Renders a rich simulated prompt through the
daemon `preview` path, framed in a rounded box, followed by palette swatches
(hex + truecolor blocks from the `palette` control message), a terminal
capabilities line (TERM_PROGRAM/COLORTERM/BASH_VERSION), and the measured
render round-trip latency.

| Flag / Gate | Behavior |
|-------------|----------|
| `--force` | Render even if the marker file exists (still writes it) |
| marker `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy10k/intro_shown` | Suppresses render when present |
| `O10K_NO_INTRO` | Hard gate — always silent (CI) |
| daemon down | Exits silently; marker stays unwritten so the next shell retries |

The marker is written only after a successful render. The Bash adapter calls
`intro` in the background at init when the marker is absent (non-blocking).

### `omarchy10k parse-prompt` (hidden)

## Socket Path Resolution

```rust
fn socket_path() -> PathBuf {
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or("/tmp".into());
    let ppid = env::var("O10K_PARENT_PID")
        .or_else(|_| env::var("PPID"))
        .unwrap_or("0".into());
    PathBuf::from(runtime_dir).join(format!("omarchy10k-{ppid}.sock"))
}
```

Priority: `O10K_PARENT_PID` > `PPID` > `"0"`.

## Module: `prompt.rs`

### `render(socket_path, cwd, exit_code, cmd_duration_ms, cols, jobs)`

Builds prompt JSON, sends to daemon via `send_request`, extracts `left` field from response.

### `send_command(socket_path, command)`

Sends `{"command":"<name>"}\n`, prints full response. Used by `reload`, `debug`, `doctor`.

### `benchmark(socket_path, iterations)`

Timed loop of prompt requests with percentile computation.

### `send_request(socket_path, request) -> String`

Core transport: `UnixStream::connect` → write request + `\n` → read one line response.

## Module: `statusline.rs`

### `run(socket_path)`

Reads the Claude Code statusLine JSON from stdin, sends
`{"type":"statusline","id",...,"payload"}` to the daemon, prints the rendered
`left`. Falls back to `fallback_render` (pure Rust, model + context % with
threshold colors) on any daemon failure; the round trip is wrapped in a
300 ms `tokio::time::timeout`. Non-JSON stdin exits with code 2.

## Module: `intro.rs`

### `run(socket_path, force)`

First-run welcome: sends a `preview` request with rich simulated context
(`~/projects/my-app`, `cmd_duration_ms: 2345`, `jobs: 1`), a `palette`
control request, and renders a rounded frame + palette swatches + TermCaps
line + measured latency. Writes the `intro_shown` marker only after a
successful render. Respects `O10K_NO_INTRO` and the marker file; skips
silently when the daemon is down.

## Module: `doctor.rs`

Pure diagnostic module. Each check is an independent function that probes the environment via:
- Environment variables (`BASH_VERSION`, `COLORTERM`, `BLE_VERSION`, etc.)
- Binary availability (`which`, `Command::new(...).output()`)
- File existence (`~/.local/share/blesh/ble.sh`, theme files)
- Daemon socket health (`send_command("status")`)

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | Argument parsing with derive macros |
| `tokio` | Async Unix socket I/O |
| `serde_json` | JSON parsing (parse-prompt, response handling) |
| `anyhow` | Error propagation |
| `directories` | XDG config path resolution (doctor) |
| `tracing` + `tracing-subscriber` | Logging |

## Known Issues

Recorded by the [Bug Audit](bug-audit.md).

### Socket discovery fails from an interactive shell

`socket_path()` resolves `O10K_PARENT_PID`, then `PPID`, then `"0"`. The Bash
adapter sets `O10K_PARENT_PID` only as a per-command prefix when launching the
daemon — it is never exported — and `PPID` is a Bash shell variable, not an
environment variable. Both lookups fail, so the CLI computes
`$XDG_RUNTIME_DIR/omarchy10k-0.sock`, which never exists.

Affects `omarchy10k reload`, `debug`, `prompt` and `benchmark`. `prompt` degrades
silently to the hardcoded fallback string, masking the failure.

`doctor` is the exception: `check_daemon` scans the runtime directory for
`omarchy10k-*.sock` rather than calling `socket_path()`. This is why doctor can
report the daemon healthy while `reload` fails in the same shell. See
[Bug Audit #7](bug-audit.md#7-the-cli-cannot-find-the-daemon-socket).

### `doctor` reports ble.sh availability the adapter does not honour

`check_blesh` prints `✓ enhanced mode available` whenever `BLE_VERSION` is set.
The adapter gates on `(( ${BLE_VERSION%%.*} >= 4 ))`, which never passes. Doctor
and the adapter test different conditions, so doctor advertises a mode that is
never installed. See
[Bug Audit #8](bug-audit.md#8-the-blesh-version-gate-never-passes).
