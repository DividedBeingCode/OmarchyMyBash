# Daemon IPC Protocol

[← Index](INDEX.md) | [Daemon](daemon.md) | [Architecture](architecture.md)

Omarchy10k uses **newline-delimited JSON (NDJSON)** over **Unix domain sockets** for all inter-process communication. The protocol uses **typed messages with version negotiation** — clients send a `hello` handshake on connect to learn `protocol_version` and `server_version`. Protocol version **0.3** is current; untagged legacy messages remain supported for backward compatibility.

## Transport

| Property | Value |
|----------|-------|
| Socket type | Unix domain socket (`AF_UNIX`, `SOCK_STREAM`) |
| Socket path | `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock` |
| Framing | Newline-delimited (`\n`) |
| Encoding | UTF-8 JSON |
| Connection model | Persistent (server loops on read until EOF) |
| Timeout | Client-side: 2 seconds (socat `-T2`, python3 `settimeout(2)`) |
| Protocol version | `0.3` (`PROTOCOL_VERSION` in server) |

### Socket Path Convention

```
${XDG_RUNTIME_DIR:-/tmp}/omarchy10k-${PID}.sock
```

Where `PID` is the Bash shell's process ID (`$$`). The daemon receives this via `O10K_PARENT_PID` environment variable at startup. The CLI resolves it from `O10K_PARENT_PID` or `PPID`.

## Message Format

All messages are JSON objects terminated by `\n`. Messages can optionally include these envelope fields:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Message type: `hello`, `control`, `prompt`, `preview`, `config`, `error` |
| `id` | string | Request ID for correlation (echoed back in response) |
| `version` | string | Protocol version string |

The `type` field determines how the daemon routes and responds to the message. Legacy clients that omit `type` are handled via fallback routing (see [Backward Compatibility](#backward-compatibility)).

Responses include `"type"` and echo `"id"` when the request provided one.

### `hello`

Handshake. Returns protocol and server version.

**Request:**
```json
{"type":"hello"}
```

**Response:**
```json
{"type":"hello","status":"ok","protocol_version":"0.3","server_version":"0.3.0"}
```

**Used by:** Quattro panel (on connect), integration tests.

### Preview Messages

Render a prompt with simulated context for live UI preview. Added in v0.3. Unlike `prompt` messages, preview responses omit OSC 133 shell-integration markers and do not include `transient` or `git_stale` fields.

**Request:**
```json
{
    "type": "preview",
    "cwd": "~/projects/my-app",
    "exit_code": 0,
    "cmd_duration_ms": 0,
    "cols": 120,
    "jobs": 0,
    "in_ssh": false,
    "git_branch": "main",
    "git_staged": 2,
    "git_unstaged": 1
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `cwd` | string | No | `"~/projects/my-app"` | Simulated working directory |
| `exit_code` | int | No | `0` | Simulated last command exit code |
| `cmd_duration_ms` | int | No | `0` | Simulated command duration in milliseconds |
| `cols` | int | No | `120` | Simulated terminal width |
| `jobs` | int | No | `0` | Simulated background job count |
| `in_ssh` | bool | No | `false` | Simulated SSH session state |
| `git_branch` | string | No | `""` | Simulated git branch name. Empty string disables git repo simulation. |
| `git_staged` | int | No | `0` | Simulated staged file count |
| `git_unstaged` | int | No | `0` | Simulated unstaged file count |

**Response:**
```json
{
    "type": "preview",
    "status": "ok",
    "left": "rendered prompt string",
    "right": "rendered right prompt or null"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"preview"` |
| `status` | string | `"ok"` on success |
| `left` | string | Rendered left prompt with ANSI color codes (no OSC 133 markers) |
| `right` | string? | Rendered right prompt (duration + branch). `null` when `prompt.right_prompt = false` or nothing to show. |

Git state is synthesized from the request fields rather than queried from disk. The daemon uses current in-memory config and theme palette.

**Used by:** Quattro panel (live preview on Appearance tab, re-rendered on config or context toggle changes).

### Control Messages

**Request:**
```json
{"type":"control","command":"<name>","id":"2"}
```

**Legacy (still supported):**
```json
{"command":"<name>"}
```

### Prompt Messages

**Request (typed):**
```json
{"type":"prompt","id":"3","cwd":"/path","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0}
```

**Legacy (still supported):**
```json
{"cwd":"/path","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0}
```

### Config Messages

Config messages use `"type":"config"`. When no `command` field is present, defaults to get.

**Get config (`config_get`):**

Request:
```json
{"type":"config"}
```

Response:
```json
{"type":"config","status":"ok","config":{...}}
```

**Used by:** Quattro panel (primary read path).

**Set config (`config_set`):**

Request:
```json
{"type":"config","command":"set","config":{"git":{"enabled":false}}}
```

Response:
```json
{"type":"config","status":"ok"}
```

The daemon **recursively merges** the JSON patch into `config.toml` on disk and reloads in-memory config. Top-level sections are merged, not replaced — keys absent from the patch are preserved. For example, a patch containing `{"git":{"mode":"compact"}}` updates only `git.mode` without touching `git.cache_ttl_ms` or other git keys.

Behavior details:
- **Missing file:** If `config.toml` does not exist (fresh install), the daemon creates it with `create_dir_all` on the parent directory, seeds from an empty table, merges the patch, and writes.
- **Parse error:** If the existing file has TOML syntax errors, the daemon returns `{"type":"config","status":"error","error":"config.toml has syntax errors: ..."}` and **refuses to overwrite** the file.
- **I/O error:** Write failures return a structured error response instead of dropping the connection.
- **Atomic write:** Uses temp file + rename to prevent corruption on crash.
- **Theme reload:** When the patch touches the `[theme]` section, the daemon automatically calls `reload_theme()` after `reload_config()` so palette changes take effect immediately.

**Used by:** Quattro panel (primary write path).

## Control Command Reference

| Command | Direction | Since | Purpose |
|---------|-----------|-------|---------|
| `status` | client→daemon | 0.1 | Health check; returns pid, version, protocol_version, cwd |
| `palette` | client→daemon | 0.3 | Returns resolved theme palette as hex colors |
| `reload_config` | client→daemon | 0.1 | Re-read config.toml from disk |
| `reload_theme` | client→daemon | 0.1 | Re-resolve theme palette |
| `invalidate_git` | client→daemon | 0.1 | Clear git status cache |
| `shutdown` | client→daemon | 0.1 | Graceful daemon exit |
| `config_get` | client→daemon | 0.2 | Return full config as JSON (via typed `config` message) |
| `config_set` | client→daemon | 0.2 | Apply config patch (via typed `config` message) |

### `status`

Health check. Returns daemon metadata.

**Request:**
```json
{"type":"control","command":"status"}
```

**Response:**
```json
{"type":"control","status":"ok","pid":12345,"version":"0.3.0","protocol_version":"0.3","cwd":"/home/user"}
```

| Field | Type | Description |
|-------|------|-------------|
| `pid` | int | Daemon process ID |
| `version` | string | Server version (`CARGO_PKG_VERSION`) |
| `protocol_version` | string | Protocol version string |
| `cwd` | string | Daemon's current working directory at request time (added in v0.3) |

**Used by:** Bash adapter (health check), CLI `debug`, `doctor`, Quattro panel (on connect).

### `palette`

Returns the daemon's resolved theme palette as hex color strings. Added in v0.3.

**Request:**
```json
{"type":"control","command":"palette"}
```

**Response:**
```json
{
    "type": "control",
    "status": "ok",
    "palette": {
        "accent": "#7aa2f7",
        "foreground": "#c0caf5",
        "muted": "#414868",
        "background": "#1a1b26",
        "red": "#f7768e",
        "green": "#9ece6a",
        "yellow": "#e0af68",
        "blue": "#7aa2f7"
    }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `palette` | object | Resolved theme colors. Keys: `accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`. Values are lowercase hex strings (`#rrggbb`). |

Palette reflects the daemon's in-memory `ThemePalette`, updated on startup and via `reload_theme`.

**Used by:** Quattro panel (color swatch display on Appearance tab).

### `reload_config`

Re-reads `config.toml` from disk and updates the daemon's in-memory config.

**Request:**
```json
{"type":"control","command":"reload_config"}
```

Legacy:
```json
{"command":"reload_config"}
```

**Response (success):**
```json
{"type":"control","status":"ok"}
```

**Response (failure):**
```json
{"type":"control","status":"error","error":"reload failed: ..."}
```

**Used by:** CLI `reload`, Quattro panel (legacy fallback after direct TOML write), integration tests.

### `reload_theme`

Reloads the theme palette via `ThemePalette::resolve_palette(&config)` — respects `theme.source` and custom overrides.

**Request:**
```json
{"type":"control","command":"reload_theme"}
```

Legacy:
```json
{"command":"reload_theme"}
```

**Response:**
```json
{"type":"control","status":"ok"}
```

**Used by:** `hooks/theme-set` (fan-out to all sockets on theme switch).

### `invalidate_git`

Clears all cached git statuses. Next prompt request will re-query git.

**Request:**
```json
{"type":"control","command":"invalidate_git"}
```

Legacy:
```json
{"command":"invalidate_git"}
```

**Response:**
```json
{"type":"control","status":"ok"}
```

**Used by:** Integration tests only. No CLI subcommand or UI button exposes this.

### `shutdown`

Graceful daemon shutdown. Responds and then calls `exit(0)`.

**Request:**
```json
{"type":"control","command":"shutdown"}
```

Legacy:
```json
{"command":"shutdown"}
```

**Response:**
```json
{"type":"control","status":"bye"}
```

**Used by:** Bash adapter `__o10k_stop_daemon` (EXIT trap).

## Prompt Request/Response

### Request

```json
{
  "type": "prompt",
  "cwd": "/home/user/project",
  "exit_code": 0,
  "cmd_duration_ms": 1200,
  "cols": 120,
  "jobs": 0,
  "shell_integration": true
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `cwd` | string | Yes | Current working directory (absolute path) |
| `exit_code` | int | Yes | Last command's exit code |
| `cmd_duration_ms` | int | Yes | Last command's duration in milliseconds |
| `cols` | int | Yes | Terminal width in columns |
| `jobs` | int | Yes | Number of background jobs |
| `shell_integration` | bool | No | Emit OSC 133 markers (default: `true`) |
| `command` | string | No | Accepted but unused. When present and not a control command name, treated as prompt metadata. |

### Response

```json
{
  "type": "prompt",
  "left": "\u001b[1;38;2;122;162;247m~/project\u001b[0m ...",
  "right": "\u001b[38;2;86;95;137m1.2s\u001b[0m \u001b[38;2;122;162;247m\u001b[0m main",
  "transient": "\u001b[38;2;86;95;137m❯\u001b[0m ",
  "git_stale": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"prompt"` for typed responses |
| `left` | string | Full prompt string with ANSI escape codes and optional OSC 133 markers |
| `right` | string? | Right prompt (duration + branch). Present when `prompt.right_prompt = true`. |
| `transient` | string? | Transient replacement prompt. Present when `prompt.transient = true`. |
| `git_stale` | bool | Whether git data is from a stale or cold cache hit |

### Error Response

When prompt rendering fails:

```json
{"type":"error","error":"description of what went wrong"}
```

## Backward Compatibility

Messages without a `type` field are handled via fallback routing:

| Condition | Routed as |
|-----------|-----------|
| `"cwd"` field present | Prompt request (v0.3.0: checked first) |
| `"command"` field present (no `cwd`) | Control command |
| Neither | Attempt prompt parse, error on failure |

This ensures old clients (CLI, Bash adapter socket fallback, theme hook) continue to work with new daemons without modification. New clients should use typed messages and the `hello` handshake.

## Client Implementations

### Bash Adapter (`shell/omarchy10k.bash`)

Primary path uses the bridge coprocess (`omarchy10k bridge`):

```bash
coproc __O10K_BRIDGE { "$__O10K_BIN" bridge --socket "$__O10K_SOCKET"; }
# Write JSON request to coproc stdin
# Read two NUL-terminated fields from coproc stdout: left\0right\0
IFS= read -r -d $'\0' -t 2 -u "${__O10K_BRIDGE[0]}" left
IFS= read -r -d $'\0' -t 2 -u "${__O10K_BRIDGE[0]}" right
```

The bridge emits `left\0right\0` per response — the `right` field carries the right prompt for ble.sh's `prompt_rps1`. The right field may be empty when right prompt is disabled.

Fallback uses `__o10k_socket_send` with socat or python3:

```bash
# socat (preferred fallback)
echo "$request" | socat -T2 - UNIX-CONNECT:"$socket"

# python3 (last resort)
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

Uses Quickshell's `Socket` component with `SplitParser` for NDJSON line splitting. Keeps persistent connection while panel is open. Sends `hello` on connect, then uses `config_get` / `config_set` for config management (with TOML file I/O fallback for older daemons).

v0.3 additions:
- **`preview`** — live prompt preview with simulated context toggles (error state, SSH, long command duration)
- **`palette`** — fetches resolved theme colors for swatch display alongside preview

On config save or preview context change, the panel sends a `preview` request and updates the preview text (ANSI stripped for display).

### Theme Hook (`hooks/theme-set`)

Fan-out sender. Globs all `omarchy10k-*.sock` files and sends `reload_theme` to each. Fire-and-forget — errors ignored.

## Transport Selection Matrix

| Client | Transport | When |
|--------|-----------|------|
| Bash adapter | bridge coprocess > socat > python3 | Every prompt render |
| CLI | Tokio UnixStream | On-demand subcommands |
| Quattro panel | Quickshell Socket | While panel open (prompt, preview, palette, config) |
| Theme hook | socat > python3 | On theme switch |
| Integration tests | python3 | Test execution |

## Sequence Diagrams

### Normal Prompt Render (Bridge Path)

```
Bash                    Bridge Coproc         Socket              Daemon
  │                          │                  │                    │
  ├─ JSON request ──────────►│                  │                    │
  │                          ├─ forward ───────►│                    │
  │                          │                  ├─ parse request ───►│
  │                          │                  │                    ├─ GitCache lookup
  │                          │                  │                    ├─ collect_segments
  │                          │                  │                    ├─ layout_resolve
  │                          │                  │                    ├─ format + OSC 133
  │                          │                  │◄─ JSON response ───┤
  │                          │◄─ response ──────┤                    │
  │◄─ NUL-terminated left ───┤                  │                    │
  ├─ PS1 = left              │                  │                    │
  │                          │                  │                    │
```

### Normal Prompt Render (Socket Fallback)

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

### Config Update (via Quattro)

```
Quattro Panel          Socket              Daemon              Config File
      │                   │                    │                     │
      ├─ config_set ─────►│                    │                     │
      │                   ├─ parse message ───►│                     │
      │                   │                    ├─ apply JSON patch ─►│
      │                   │                    ├─ reload in-memory   │
      │                   │◄─ {"type":"config","status":"ok"} ──────┤
      │◄─ ok ─────────────┤                    │                     │
      │                   │                    │                     │
```

### Config Reload (Legacy Fallback)

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

### Live Preview (Quattro, v0.3)

```
Quattro Panel          Socket              Daemon
      │                   │                    │
      ├─ palette ────────►│                    │
      │                   ├─ read palette ────►│
      │                   │◄─ hex colors ──────┤
      │◄─ swatches ───────┤                    │
      │                   │                    │
      ├─ preview ────────►│                    │
      │  (simulated ctx)  ├─ synthesize git ───►│
      │                   │                    ├─ collect_segments
      │                   │                    ├─ layout_resolve
      │                   │                    ├─ format (no OSC 133)
      │                   │◄─ left + right ────┤
      │◄─ preview text ───┤                    │
      │                   │                    │
```

## Known Issues

Recorded by the [Bug Audit](bug-audit.md).

### `command` collides between prompt and control messages (v0.3.0: mitigated)

`PromptRequest` declares a `command` field (the command text), and `TypedMessage`
declares `command` at the top level ahead of `#[serde(flatten)] rest`. Named
fields win over `flatten`, so on a typed `prompt` message, `command` is swallowed
by `TypedMessage` and never reaches `PromptRequest`.

**v0.3.0 fix:** Type-less messages now check for `cwd` first (prompt requests
always have `cwd`) before checking `command`, so a type-less prompt request
carrying a `command` field is correctly routed to the prompt handler instead of
the control handler. See [Bug Audit #12](bug-audit.md#12-a-type-less-prompt-request-carrying-command-is-misrouted-to-the-control-handler).

### The `version` field is never read

`TypedMessage.version` is parsed and discarded — `cargo build` flags it as dead
code. `Model.buildHello` sends `"version":"0.3"`; the daemon ignores it and
answers with its own `protocol_version` regardless. There is no version
negotiation, only advertisement.
