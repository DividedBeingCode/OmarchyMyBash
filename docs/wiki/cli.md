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
3. Extracts the `left` field from the JSON response
4. Writes the prompt string to stdout terminated by a NUL byte (`0x00`)

Supports both JSON input and tab-separated `key=value` format (converted to JSON internally).

**Reconnection:** Retries up to 3 times on socket errors. Falls back to the hardcoded PS1 if reconnection fails.

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
| Hooks | `PROMPT_COMMAND` contains o10k | Check after init | — |
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

### `omarchy10k debug`

Sends `{"command":"status"}` to the daemon and prints the raw JSON response. Shows daemon PID, version, and status.

### `omarchy10k parse-prompt` (hidden)

Hidden subcommand used by the Bash adapter. Reads JSON from stdin, extracts the `left` field, prints it to stdout. Avoids requiring `jq` as a runtime dependency.

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
