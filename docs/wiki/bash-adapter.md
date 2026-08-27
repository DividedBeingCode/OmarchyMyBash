# Bash Adapter Reference

[← Index](INDEX.md) | [Architecture](architecture.md) | [Protocol](protocol.md)

The Bash adapter (`shell/omarchy10k.bash`) is the sole shell integration surface. It is emitted by `omarchy10k init bash` and sourced into the user's Bash session. It manages the daemon lifecycle, brokers hooks for shell tools, captures command timing, and sets `PS1` on every prompt.

## Initialization

```bash
eval "$(omarchy10k init bash)"
```

Guard rails prevent double-init and non-interactive shells:

```bash
[[ $- == *i* ]] || return 0              # interactive only
[[ -n "${__O10K_INITIALIZED:-}" ]] && return 0
__O10K_INITIALIZED=1
```

### Startup Sequence

After guards pass, the adapter:

1. Sets up path variables (`__O10K_BIN`, `__O10K_DAEMON_BIN`, `__O10K_SOCKET`)
2. Loads instant prompt cache from `${XDG_CACHE_HOME:-$HOME/.cache}/omarchy10k/last_prompt` (if present)
3. Detects terminal shell integration and sets `__O10K_EMIT_OSC133`
4. Declares hook arrays (`__O10K_HOOKS_precmd`, `preexec`, `chpwd`, `shell_exit`)
5. Registers `EXIT` trap for `__o10k_cleanup`
6. Creates cache directory (`__O10K_CACHE_DIR`) if missing
7. Starts the daemon (`__o10k_start_daemon`)
8. Starts bridge coprocess (`__o10k_start_bridge`)
9. Installs hooks based on ble.sh availability (`__o10k_install_hooks`)

## Instant Prompt Cache

On startup, before the daemon is ready, the adapter loads the last successfully rendered prompt from disk so the shell shows a styled prompt immediately instead of a blank line:

```bash
__O10K_CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/omarchy10k"
__O10K_CACHE="${__O10K_CACHE_DIR}/last_prompt"

if [[ -f "$__O10K_CACHE" ]]; then
    PS1=$(<"$__O10K_CACHE")
fi
```

After each successful prompt render (bridge path or socket fallback), the adapter writes the cache atomically in the background:

```bash
{ printf '%s' "$PS1" > "$__O10K_CACHE.tmp" && mv "$__O10K_CACHE.tmp" "$__O10K_CACHE"; } 2>/dev/null &
```

The cache directory is created during init (`mkdir -p "$__O10K_CACHE_DIR"`). Static fallback PS1 is not cached.

## Daemon Lifecycle

### Start (`__o10k_start_daemon`)

1. If socket exists and `status` command succeeds → daemon already running, return
2. Remove stale socket file
3. Start daemon: `O10K_PARENT_PID=$$ "$__O10K_DAEMON_BIN" &>/dev/null &` + `disown`
4. Poll for socket creation: 20 attempts × 100ms sleep (2 second timeout)

### Stop (`__o10k_stop_daemon`)

1. If socket exists: send `{"command":"shutdown"}`
2. Remove socket file

### Cleanup (`__o10k_cleanup`)

EXIT trap handler:
1. `__o10k_dispatch shell_exit` — fires all registered shell_exit hooks
2. Kills bridge coprocess (`kill "$__O10K_BRIDGE_PID" 2>/dev/null`)
3. `__o10k_stop_daemon` — graceful daemon shutdown

## Bridge Coprocess

The bridge coprocess provides the lowest-latency path to the daemon by keeping a persistent `omarchy10k bridge` process alive for the shell session.

### Start (`__o10k_start_bridge`)

Starts `omarchy10k bridge` as a Bash coprocess and stores its PID in `__O10K_BRIDGE_PID`:

```bash
coproc __O10K_BRIDGE { "$__O10K_BIN" bridge --socket "$__O10K_SOCKET"; }
__O10K_BRIDGE_PID=$!
```

Called at init time alongside daemon startup.

### Request (`__o10k_bridge_request`)

1. Writes the JSON request to the coprocess stdin
2. Reads a NUL-terminated response from coprocess stdout (2 second timeout)
3. Sets `PS1` directly from the response
4. Writes the instant prompt cache in the background (see [Instant Prompt Cache](#instant-prompt-cache))

If the bridge is unavailable or times out, the adapter falls back to `__o10k_socket_send` + `parse-prompt`.

## Shell Integration Detection

### Detection (`__o10k_detect_terminal_integration`)

Checks for existing terminal shell integrations that already emit OSC 133 markers:

| Terminal | Detection |
|----------|-----------|
| Ghostty | `__ghostty_precmd` function defined |
| VTE (GNOME Terminal, etc.) | `__vte_prompt_command` function defined |
| WezTerm | `__wezterm_set_user_var` function defined |
| Kitty | `$KITTY_SHELL_INTEGRATION` set |

### `O10K_SHELL_INTEGRATION` Environment Variable

| Value | Behavior |
|-------|----------|
| `auto` (default) | Detect existing integrations; defer OSC 133 when one is found |
| `force` | Omarchy10k always emits OSC 133 markers |
| `off` | Omarchy10k never emits OSC 133 markers |

When an existing integration is detected in `auto` mode, Omarchy10k defers OSC 133 emission to the terminal. The `__O10K_EMIT_OSC133` flag controls whether OSC 133 markers are included in prompt output.

## Hook Broker

The adapter provides a unified hook system that eliminates `PROMPT_COMMAND` conflicts between shell tools. Instead of each tool fighting to own `PROMPT_COMMAND`, they register callbacks through the broker.

### Hook Events

| Event | When | Arguments |
|-------|------|-----------|
| `precmd` | Before each prompt render | None |
| `preexec` | After user enters command, before execution | `$BASH_COMMAND` |
| `chpwd` | When `$PWD` changes | None |
| `shell_exit` | On shell EXIT trap | None |

### Public API

```bash
o10k_hook_add <event> <function_name>
o10k_hook_remove <event> <function_name>
```

Example: a Zoxide integration would call `o10k_hook_add chpwd _zoxide_hook`.

### Dispatch

```bash
__o10k_dispatch <event> [args...]
```

Iterates `__O10K_HOOKS_${event}` array, calls each registered function with `"$@"`. Stderr suppressed.

### Chpwd Emulation

Bash has no native chpwd hook. The adapter emulates it:

```bash
__o10k_check_chpwd() {
    if [[ "$PWD" != "$__O10K_LAST_PWD" ]]; then
        __O10K_LAST_PWD="$PWD"
        __o10k_dispatch chpwd
    fi
}
```

Called at the start of every `__o10k_render_prompt`.

## Command Timing and Name Tracking

### Timer Start (`__o10k_timer_start`)

Sets `__O10K_CMD_START` from `$EPOCHREALTIME` (Bash 5.0+) or `date +%s%N` fallback.

### Timer Stop (`__o10k_timer_stop`)

Computes millisecond delta from `$EPOCHREALTIME`. Handles fractional second padding (EPOCHREALTIME gives variable decimal places). Resets `__O10K_CMD_START` to 0.

Result stored in `__O10K_CMD_DURATION` for the prompt request.

### Command Name (`__O10K_LAST_CMD`)

The adapter captures the current command name in both preexec paths for use by desktop notifications:

| Hook | Assignment |
|------|------------|
| Vanilla (`__o10k_preexec`) | `__O10K_LAST_CMD="$BASH_COMMAND"` |
| ble.sh (`__o10k_preexec_blesh`) | `__O10K_LAST_CMD="$1"` |

Set before timer start and preexec hook dispatch.

## Prompt Rendering (`__o10k_render_prompt`)

Called from `PROMPT_COMMAND` or ble.sh `PRECMD`:

```
1. Capture exit_code=$?
2. Emit OSC 133;D (marks end of previous command output, if __O10K_EMIT_OSC133)
3. __o10k_timer_stop
4. If cmd_duration_ms > __O10K_NOTIFY_THRESHOLD:
      emit OSC 777 desktop notification
5. Emit DEC 2026 synchronized output ON (\033[?2026h)
6. __o10k_check_chpwd
7. __o10k_dispatch precmd
8. Emit OSC 9;4;0 (clear progress bar)
9. If socket exists:
   a. cols=${COLUMNS:-80}
   b. jobs_count=$(jobs -p | wc -l)
   c. Build JSON: {"cwd","exit_code","cmd_duration_ms","cols","jobs","shell_integration"}
      (shell_integration reflects __O10K_EMIT_OSC133)
   d. Try bridge first (no fork/exec):
      __o10k_bridge_request "$request" → PS1 set + cache written
   e. Fallback: response=$(__o10k_socket_send "$request")
      Parse "left" from JSON:
      - Preferred: $__O10K_BIN parse-prompt (Rust, ~1-3ms)
      - Fallback: python3 JSON extraction (~5-10ms)
      PS1="$left" + cache written
10. Else: PS1="$__O10K_FALLBACK_PS1"
11. Emit OSC 7 CWD report (file://hostname/path)
12. Emit DEC 2026 synchronized output OFF (\033[?2026l)
```

The bridge path avoids per-prompt fork/exec overhead. Socket + parse-prompt is used only when the bridge coprocess is unavailable.

OSC 7 CWD reporting runs after **every** successful PS1 assignment — bridge path, socket fallback, and static fallback — so terminals can track the working directory regardless of daemon availability.

### Fallback PS1

When the daemon is unreachable:
```bash
__O10K_FALLBACK_PS1='\[\e[1;34m\]\w\[\e[0m\] \[\e[1;32m\]❯\[\e[0m\] '
```

Blue working directory + green `❯`. The shell remains usable.

## Terminal Escape Sequences

The adapter emits several terminal control sequences beyond OSC 133 shell integration. All are no-ops or pass-through on unsupported terminals.

### OSC 7 — CWD Reporting

After every successful PS1 assignment, the adapter reports the current working directory:

```bash
printf '\033]7;file://%s%s\033\\' "${HOSTNAME}" "$PWD"
```

Enables Ghostty, Foot, and WezTerm to open new tabs/splits in the shell's current directory. Emitted on bridge success, socket fallback success, and static fallback paths.

### OSC 777 — Desktop Notifications

When a command's duration exceeds `__O10K_NOTIFY_THRESHOLD` (default 10000 ms), the adapter notifies the terminal:

```bash
printf '\033]777;notify;Command finished;%s took %dms\007' \
    "${__O10K_LAST_CMD:-command}" "$__O10K_CMD_DURATION"
```

Supported by Ghostty, Foot, and WezTerm. Threshold is configurable via the `O10K_NOTIFY_THRESHOLD` environment variable or `segments.notification.threshold_ms` in config (Quattro panel).

### OSC 9;4 — Progress Bar

| Phase | Sequence | Meaning |
|-------|----------|---------|
| Preexec | `\033]9;4;3\007` | Indeterminate progress (command running) |
| Render prompt | `\033]9;4;0\007` | Clear progress indicator |

Supported natively by Windows Terminal and ConEmu; other terminals pass the sequence through harmlessly.

### DEC 2026 — Synchronized Output

The entire prompt render cycle is wrapped to prevent screen tearing:

```bash
printf '\033[?2026h'   # begin synchronized update
# ... chpwd, precmd, daemon request, PS1 assignment ...
printf '\033[?2026l'   # end synchronized update
```

Supported by Ghostty, Foot, Kitty, and WezTerm.

## Socket Communication (`__o10k_socket_send`)

Transport priority:

| Priority | Method | Latency |
|----------|--------|---------|
| 0 | Bridge coprocess (NUL-terminated stdout) | ~0.1ms |
| 1 | `socat -T2 - UNIX-CONNECT:"$sock"` | ~1ms |
| 2 | `python3` inline (AF_UNIX, 2s timeout) | ~5ms |
| 3 | Return failure (fallback PS1 used) | — |

`/dev/tcp` fallback is commented in source but not implemented — Bash `/dev/tcp` doesn't support Unix domain sockets.

## Hook Installation

### Detection

```bash
if BLE_VERSION major ≥ 4 → blesh hooks
else → vanilla Bash hooks
```

### ble.sh Mode (`__o10k_install_blesh_hooks`)

ble.sh provides proper shell lifecycle hooks:

| blehook | Callback |
|---------|----------|
| `PRECMD+` | `__o10k_render_prompt` |
| `PREEXEC+` | `__o10k_preexec_blesh` |
| `CHPWD+` | `__o10k_dispatch chpwd` |
| `PRECMD+` | `__O10K_PREEXEC_READY=1` |

Also enables transient prompt: `bleopt prompt_ps1_transient='always'`

Registers `__o10k_update_rps1` as an additional `PRECMD+` hook for right prompt updates.

### Vanilla Bash Mode (`__o10k_install_vanilla_hooks`)

**PROMPT_COMMAND** (array-aware for Bash 5.1+):
- Prepend `__o10k_render_prompt` to array/string
- Append `__O10K_PREEXEC_READY=1`

**Preexec emulation:**
- Bash ≥ 4.4: `PS0='$(__o10k_preexec 2>/dev/null)'` — fires once per command line
- Bash < 4.4: DEBUG trap chained with existing trap via `trap -p DEBUG`

### Preexec Ready Gate

`__O10K_PREEXEC_READY` prevents double-firing. Set to `1` at end of `PROMPT_COMMAND`. Checked and cleared at start of `__o10k_preexec`. Without this, DEBUG trap or PS0 would fire on every simple command in a pipeline.

### Preexec OSC 133 and Progress

Both vanilla and ble.sh preexec handlers:

1. Emit OSC 133;C at command start (marks beginning of command output, if `__O10K_EMIT_OSC133`)
2. Set `__O10K_LAST_CMD` from the command string
3. Emit OSC 9;4;3 (indeterminate progress bar)
4. Start command timer (`__o10k_timer_start`)
5. Dispatch preexec hooks

| Mode | Handler | Command source |
|------|---------|----------------|
| ble.sh | `__o10k_preexec_blesh` | `$1` (ble.sh PREEXEC argument) |
| vanilla | `__o10k_preexec` | `$BASH_COMMAND` |

## Right Prompt

When ble.sh is active, the adapter manages the right-aligned prompt via `__o10k_update_rps1`:

1. Registered as a ble.sh `PRECMD+` hook (runs after `__o10k_render_prompt`)
2. Queries the daemon for right prompt data (via bridge or socket)
3. Sets `bleopt prompt_rps1` with the result, or an empty string if unavailable

Right prompt content typically includes git branch and command duration when `prompt.right_prompt = true`.

## Runtime Globals

| Variable | Default | Purpose |
|----------|---------|---------|
| `__O10K_CMD_START` | `0` | Command start timestamp |
| `__O10K_CMD_DURATION` | `0` | Last command duration (ms) |
| `__O10K_LAST_CMD` | `""` | Last executed command name (for notifications) |
| `__O10K_NOTIFY_THRESHOLD` | `10000` | Desktop notification threshold (ms) |
| `__O10K_CACHE_DIR` | `$XDG_CACHE_HOME/omarchy10k` | Instant prompt cache directory |
| `__O10K_CACHE` | `$__O10K_CACHE_DIR/last_prompt` | Cached PS1 file path |
| `__O10K_EMIT_OSC133` | `0` or `1` | Whether to emit OSC 133 markers |
| `__O10K_BRIDGE_PID` | `0` | Bridge coprocess PID |

## Path Variables

| Variable | Default | Override |
|----------|---------|----------|
| `__O10K_BIN` | `omarchy10k` | `$O10K_BIN` |
| `__O10K_DAEMON_BIN` | `omarchy10kd` | `$O10K_DAEMON_BIN` |
| `__O10K_SOCKET_DIR` | `$XDG_RUNTIME_DIR` or `/tmp` | — |
| `__O10K_SOCKET` | `$__O10K_SOCKET_DIR/omarchy10k-$$.sock` | — |
| `__O10K_CACHE_DIR` | `$XDG_CACHE_HOME/omarchy10k` | — |

## Environment Variables Read

| Variable | Purpose |
|----------|---------|
| `XDG_RUNTIME_DIR` | Socket directory |
| `XDG_CACHE_HOME` | Instant prompt cache directory |
| `O10K_BIN` | Override CLI path |
| `O10K_DAEMON_BIN` | Override daemon path |
| `O10K_NOTIFY_THRESHOLD` | Desktop notification threshold (ms) |
| `O10K_PARENT_PID` | Set to `$$` when starting daemon |
| `HOSTNAME` | OSC 7 CWD reporting hostname |
| `BLE_VERSION` | ble.sh detection |
| `COLUMNS` | Terminal width |
| `EPOCHREALTIME` | Microsecond timing (Bash 5.0+) |
| `PWD` | Current directory for prompt + chpwd |
| `BASH_COMMAND` | Command string for preexec |
| `BASH_VERSINFO` | Bash version detection |
| `PROMPT_COMMAND` | Existing hooks (preserved) |
| `O10K_SHELL_INTEGRATION` | OSC 133 emission mode (`auto`, `force`, `off`) |
| `KITTY_SHELL_INTEGRATION` | Kitty shell integration detection |

## Runtime Dependencies

| Tool | Required | Purpose |
|------|----------|---------|
| `omarchy10kd` | Yes | Background daemon |
| `omarchy10k` | Yes | `parse-prompt` subcommand |
| `socat` | Preferred | Fast socket I/O |
| `python3` | Fallback | Socket + JSON parsing |
| `bash` ≥ 4.4 | Required | PS0 preexec, basic functionality |
| `bash` ≥ 5.0 | Recommended | EPOCHREALTIME timing |
| `bash` ≥ 5.1 | Recommended | Array PROMPT_COMMAND |
| `ble.sh` ≥ 4 | Optional | Enhanced hooks, transient prompt |
