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
2. Declares hook arrays (`__O10K_HOOKS_precmd`, `preexec`, `chpwd`, `shell_exit`)
3. Registers `EXIT` trap for `__o10k_cleanup`
4. Starts the daemon (`__o10k_start_daemon`)
5. Installs hooks based on ble.sh availability (`__o10k_install_hooks`)

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
2. `__o10k_stop_daemon` — graceful daemon shutdown

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

## Command Timing

### Timer Start (`__o10k_timer_start`)

Sets `__O10K_CMD_START` from `$EPOCHREALTIME` (Bash 5.0+) or `date +%s%N` fallback.

### Timer Stop (`__o10k_timer_stop`)

Computes millisecond delta from `$EPOCHREALTIME`. Handles fractional second padding (EPOCHREALTIME gives variable decimal places). Resets `__O10K_CMD_START` to 0.

Result stored in `__O10K_CMD_DURATION` for the prompt request.

## Prompt Rendering (`__o10k_render_prompt`)

Called from `PROMPT_COMMAND` or ble.sh `PRECMD`:

```
1. Capture exit_code=$?
2. __o10k_timer_stop
3. __o10k_check_chpwd
4. __o10k_dispatch precmd
5. If socket exists:
   a. cols=${COLUMNS:-80}
   b. jobs_count=$(jobs -p | wc -l)
   c. Build JSON: {"cwd","exit_code","cmd_duration_ms","cols","jobs"}
   d. response=$(__o10k_socket_send "$request")
   e. Parse "left" from JSON:
      - Preferred: $__O10K_BIN parse-prompt (Rust, ~1-3ms)
      - Fallback: python3 JSON extraction (~5-10ms)
   f. PS1="$left"
6. Else: PS1="$__O10K_FALLBACK_PS1"
```

### Fallback PS1

When the daemon is unreachable:
```bash
__O10K_FALLBACK_PS1='\[\e[1;34m\]\w\[\e[0m\] \[\e[1;32m\]❯\[\e[0m\] '
```

Blue working directory + green `❯`. The shell remains usable.

## Socket Communication (`__o10k_socket_send`)

Transport priority:

| Priority | Method | Latency |
|----------|--------|---------|
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

### Vanilla Bash Mode (`__o10k_install_vanilla_hooks`)

**PROMPT_COMMAND** (array-aware for Bash 5.1+):
- Prepend `__o10k_render_prompt` to array/string
- Append `__O10K_PREEXEC_READY=1`

**Preexec emulation:**
- Bash ≥ 4.4: `PS0='$(__o10k_preexec 2>/dev/null)'` — fires once per command line
- Bash < 4.4: DEBUG trap chained with existing trap via `trap -p DEBUG`

### Preexec Ready Gate

`__O10K_PREEXEC_READY` prevents double-firing. Set to `1` at end of `PROMPT_COMMAND`. Checked and cleared at start of `__o10k_preexec`. Without this, DEBUG trap or PS0 would fire on every simple command in a pipeline.

## Path Variables

| Variable | Default | Override |
|----------|---------|----------|
| `__O10K_BIN` | `omarchy10k` | `$O10K_BIN` |
| `__O10K_DAEMON_BIN` | `omarchy10kd` | `$O10K_DAEMON_BIN` |
| `__O10K_SOCKET_DIR` | `$XDG_RUNTIME_DIR` or `/tmp` | — |
| `__O10K_SOCKET` | `$__O10K_SOCKET_DIR/omarchy10k-$$.sock` | — |

## Environment Variables Read

| Variable | Purpose |
|----------|---------|
| `XDG_RUNTIME_DIR` | Socket directory |
| `O10K_BIN` | Override CLI path |
| `O10K_DAEMON_BIN` | Override daemon path |
| `O10K_PARENT_PID` | Set to `$$` when starting daemon |
| `BLE_VERSION` | ble.sh detection |
| `COLUMNS` | Terminal width |
| `EPOCHREALTIME` | Microsecond timing (Bash 5.0+) |
| `PWD` | Current directory for prompt + chpwd |
| `BASH_COMMAND` | Command string for preexec |
| `BASH_VERSINFO` | Bash version detection |
| `PROMPT_COMMAND` | Existing hooks (preserved) |

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
