# Daemon IPC Protocol

[← Index](INDEX.md) | [Daemon](daemon.md) | [Architecture](architecture.md)

Omarchy10k uses **newline-delimited JSON (NDJSON)** over **Unix domain sockets** for all inter-process communication. The protocol is deliberately simple — no framing, no handshake, no versioning — because both endpoints are controlled by the same project.

## Transport

| Property | Value |
|----------|-------|
| Socket type | Unix domain socket (`AF_UNIX`, `SOCK_STREAM`) |
| Socket path | `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock` |
| Framing | Newline-delimited (`\n`) |
| Encoding | UTF-8 JSON |
| Connection model | Persistent (server loops on read until EOF) |
| Timeout | Client-side: 2 seconds (socat `-T2`, python3 `settimeout(2)`) |

### Socket Path Convention

```
${XDG_RUNTIME_DIR:-/tmp}/omarchy10k-${PID}.sock
```

Where `PID` is the Bash shell's process ID (`$$`). The daemon receives this via `O10K_PARENT_PID` environment variable at startup. The CLI resolves it from `O10K_PARENT_PID` or `PPID`.

## Message Types

All messages are JSON objects terminated by `\n`. The protocol distinguishes two message types by the presence of a `command` field:

### Control Commands

Request contains `"command"` field:

```json
{"command":"<name>"}\n
```

### Prompt Requests

Request contains prompt fields (no `command`):

```json
{"cwd":"/path","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0}\n
```

## Control Command Reference

### `status`

Health check. Returns daemon metadata.

**Request:**
```json
{"command":"status"}
```

**Response:**
```json
{"status":"ok","pid":12345,"version":"0.1.0"}
```

**Used by:** Bash adapter (health check), CLI `debug`, `doctor`, Quattro panel (on connect).

### `reload_config`

Re-reads `config.toml` from disk and updates the daemon's in-memory config.

**Request:**
```json
{"command":"reload_config"}
```

**Response:**
```json
{"status":"ok"}
```

**Used by:** CLI `reload`, Quattro panel (after config save), integration tests.

### `reload_theme`

Reloads the Omarchy theme palette from `colors.toml`.

**Request:**
```json
{"command":"reload_theme"}
```

**Response:**
```json
{"status":"ok"}
```

**Used by:** `hooks/theme-set` (fan-out to all sockets on theme switch).

**Known issue:** Always calls `ThemePalette::load_omarchy()` regardless of `config.theme.source`. Custom/hybrid overrides are not re-applied.

### `invalidate_git`

Clears all cached git statuses. Next prompt request will re-query git.

**Request:**
```json
{"command":"invalidate_git"}
```

**Response:**
```json
{"status":"ok"}
```

**Used by:** Integration tests only. No CLI subcommand or UI button exposes this.

### `shutdown`

Graceful daemon shutdown. Responds and then calls `exit(0)`.

**Request:**
```json
{"command":"shutdown"}
```

**Response:**
```json
{"status":"bye"}
```

**Used by:** Bash adapter `__o10k_stop_daemon` (EXIT trap).

## Prompt Request/Response

### Request

```json
{
  "cwd": "/home/user/project",
  "exit_code": 0,
  "cmd_duration_ms": 1200,
  "cols": 120,
  "jobs": 0,
  "command": null
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `cwd` | string | Yes | Current working directory (absolute path) |
| `exit_code` | int | Yes | Last command's exit code |
| `cmd_duration_ms` | int | Yes | Last command's duration in milliseconds |
| `cols` | int | Yes | Terminal width in columns |
| `jobs` | int | Yes | Number of background jobs |
| `command` | string | No | Accepted but unused. When present and not a control command name, treated as prompt metadata. |

### Response

```json
{
  "left": "\u001b[1;38;2;122;162;247m~/project\u001b[0m ...",
  "right": null,
  "transient": "\u001b[38;2;86;95;137m❯\u001b[0m ",
  "git_stale": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `left` | string | Full prompt string with ANSI escape codes and OSC 133 markers |
| `right` | string? | Right prompt. Currently always `null`. |
| `transient` | string? | Transient replacement prompt. Present when `prompt.transient = true`. |
| `git_stale` | bool | Whether git data is from cache (stale). Currently always `false`. |

### Error Response

When prompt rendering fails:

```json
{"error":"description of what went wrong"}
```

## Client Implementations

### Bash Adapter (`shell/omarchy10k.bash`)

Uses `__o10k_socket_send` with socat or python3 fallback. Sends prompt requests directly — does not go through the CLI binary for the hot path.

```bash
# socat (preferred)
echo "$request" | socat -T2 - UNIX-CONNECT:"$socket"

# python3 (fallback)
python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(2)
s.connect('$socket')
s.sendall((sys.stdin.read().strip() + '\n').encode())
buf = b''
while b'\n' not in buf:
    buf += s.recv(4096)
print(buf.decode().strip())
s.close()
"
```

### CLI (`omarchy10k`)

Uses Tokio `UnixStream` for async socket I/O. Single request-response per connection.

### Quattro Panel (`Panel.qml`)

Uses Quickshell's `Socket` component with `SplitParser` for NDJSON line splitting. Keeps persistent connection while panel is open.

### Theme Hook (`hooks/theme-set`)

Fan-out sender. Globs all `omarchy10k-*.sock` files and sends `reload_theme` to each. Fire-and-forget — errors ignored.

## Transport Selection Matrix

| Client | Transport | When |
|--------|-----------|------|
| Bash adapter | socat > python3 | Every prompt render |
| CLI | Tokio UnixStream | On-demand subcommands |
| Quattro panel | Quickshell Socket | While panel open |
| Theme hook | socat > python3 | On theme switch |
| Integration tests | python3 | Test execution |

## Sequence Diagrams

### Normal Prompt Render

```
Bash                    Socket              Daemon
  │                       │                    │
  ├─ JSON request ───────►│                    │
  │                       ├─ parse request ───►│
  │                       │                    ├─ GitCache lookup
  │                       │                    ├─ collect_segments
  │                       │                    ├─ layout_resolve
  │                       │                    ├─ format + OSC 133
  │                       │◄─ JSON response ───┤
  │◄─ response ───────────┤                    │
  ├─ parse-prompt (left)  │                    │
  ├─ PS1 = left           │                    │
  │                       │                    │
```

### Config Reload (via Quattro)

```
Quattro Panel          Config File           Socket              Daemon
      │                     │                   │                    │
      ├─ write TOML ───────►│                   │                    │
      │                     │                   │                    │
      ├─ reload_config ────────────────────────►│                    │
      │                     │                   ├─ parse command ───►│
      │                     │                   │                    ├─ re-read config.toml
      │                     │                   │                    ├─ update RwLock<Config>
      │                     │                   │◄─ {"status":"ok"} ─┤
      │◄─ ok ──────────────────────────────────┤                    │
      │                     │                   │                    │
```

### Theme Switch (fan-out)

```
Omarchy Theme Engine     colors.toml          hooks/theme-set       Daemon 1    Daemon 2
        │                     │                     │                  │            │
        ├─ render template ──►│                     │                  │            │
        ├─ trigger hook ─────────────────────────►  │                  │            │
        │                     │                     ├─ glob sockets    │            │
        │                     │                     ├─ reload_theme ──►│            │
        │                     │                     ├─ reload_theme ──────────────►│
        │                     │                     │                  ├─ load      │
        │                     │                     │                  │  palette   ├─ load
        │                     │                     │                  │            │  palette
```
