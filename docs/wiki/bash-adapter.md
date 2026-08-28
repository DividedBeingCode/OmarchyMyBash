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
2. Disables `promptvars` (`shopt -u promptvars`) so PS1 content — daemon/path/branch text — never undergoes parameter or command expansion
3. Loads instant prompt cache from `${XDG_CACHE_HOME:-$HOME/.cache}/omarchy10k/last_prompt` (if present)
4. Detects terminal shell integration and sets `__O10K_EMIT_OSC133`, then computes the `__O10K_EMIT_133CD` gate (see [Semantic Prompts](#osc-133cd-semantic-prompts-14))
5. Declares hook arrays (`__O10K_HOOKS_precmd`, `preexec`, `chpwd`, `shell_exit`)
6. Registers `EXIT` trap for `__o10k_cleanup`
7. Creates cache directory (`__O10K_CACHE_DIR`) if missing
8. Starts the daemon (`__o10k_start_daemon`)
9. Starts bridge coprocess (`__o10k_start_bridge`)
10. Installs hooks based on ble.sh availability (`__o10k_install_hooks`)
11. Launches the one-time intro in the background when the marker file is absent and `O10K_NO_INTRO` is unset (see [First-Run Intro](#first-run-intro))

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
2. Reads FOUR NUL-terminated fields from coprocess stdout:
   `left\0right\0notify_threshold_ms\0transient\0` (left: 2s timeout;
   right/threshold/transient: 0.5s each)
3. Sets `PS1` from the `left` field
4. Caches the `right` field in `__O10K_LAST_RIGHT` for ble.sh right prompt
5. Sets `__O10K_NOTIFY_THRESHOLD` via `__o10k_set_notify_threshold` — a value
   of `0` or empty means notifications are OFF and is stored as `0`, never
   falling back to the bootstrap default
6. Sets `__O10K_TRANSIENT` from the fourth field (may be empty)
7. Refreshes config flags from the bridge side-channel file
   (`__o10k_read_flags`) — the bridge writes it before its response fields,
   so the 133;C/D gate is current without a one-render lag
8. Writes the instant prompt cache in the background (see [Instant Prompt Cache](#instant-prompt-cache))

#### Config-flags side channel

The bridge relays `semantic_prompts` and `notify_unfocused_only` from the
daemon response through a side-channel file next to the socket
(`<socket>.flags`, atomic tmp+rename, written only when a value changes), so
the hot path keeps its frozen four-field NUL framing. The adapter reads the
file without forking. Old adapters ignore the file; old bridges simply never
create it.

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
When an existing integration is detected in `auto` mode, Omarchy10k defers OSC 133 emission to the terminal. The `__O10K_EMIT_OSC133` flag controls the `shell_integration` field of the prompt request (daemon-side OSC 133;A/B markers).

## OSC 133;C/D Semantic Prompts (1.4)

The adapter emits `\e]133;C` in preexec and `\e]133;D;<exit_code>` in precmd,
gated by `__O10K_EMIT_133CD`. The gate is computed by `__o10k_update_133cd`
(re-run at init and after every response/flag refresh):

1. `O10K_SHELL_INTEGRATION=off` → never; `force` → always (explicit user override)
2. In `auto` mode: `[terminal.semantic_prompts].enabled` from the daemon
   (`semantic_prompts` response field; default **OFF**)
3. `TMUX` unset (tmux does not forward 133)
4. `TERM_PROGRAM` is `ghostty` or `foot`
5. `GHOSTTY_SHELL_INTEGRATION_FEATURES` **and** `GHOSTTY_SHELL_FEATURES` unset
6. Ghostty's own integration not loaded (`__ghostty_precmd` undefined)

### Ghostty coexistence spike (2026-08, Ghostty 1.3.x)

Verified against Ghostty's `src/shell-integration/bash/ghostty.bash` and the
1.3.0 release notes:

- The real environment variable is **`GHOSTTY_SHELL_FEATURES`** (a
  comma-separated feature list such as `cursor,title`); the plan's
  `GHOSTTY_SHELL_INTEGRATION_FEATURES` does not exist in Ghostty source. The
  adapter checks both names so a future rename cannot re-enable double
  emission.
- Ghostty's auto-injected bash integration emits `133;A`/`P`/`B` in precmd
  (via PS1 wrapping), `133;D;<ret>` in precmd, and `133;C` in preexec —
  exactly the sequences we would emit. **Double emission is a real
  corruption class** (duplicate command-end regions break click-to-prompt
  and output selection), so our gate yields to Ghostty: unset
  `GHOSTTY_SHELL_FEATURES` + undefined `__ghostty_precmd` are both required.
- Under ble.sh, Ghostty switches to `133;P;k=i` instead of `133;A`; we emit
  only C/D, so no additional coordination is needed.
- Detection of a manually-sourced Ghostty integration (auto-injection
  disabled but script present) is covered by the `__ghostty_precmd`
  function check.

**Chosen default:** `[terminal.semantic_prompts] enabled = false` in
default.toml until an empirical coexistence test on Ghostty 1.3.x proves the
suppression heuristic on real installs; the gate stays conservative. Foot has
no auto-injection, so on Foot the adapter is the sole 133 source once enabled.

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
2. Emit OSC 133;D (marks end of previous command output, if `__O10K_EMIT_133CD`)
3. __o10k_timer_stop
4. If `__O10K_NOTIFY_THRESHOLD > 0` and cmd_duration_ms > threshold:
      desktop notification via `__o10k_notify` (see [OSC 777](#osc-777-desktop-notifications))
5. Emit DEC 2026 synchronized output ON (\033[?2026h)
6. __o10k_check_chpwd
7. __o10k_dispatch precmd
8. Emit OSC 9;4;0 (clear progress bar)
9. If socket exists:
   a. cols=${COLUMNS:-80}
   b. jobs_count=$(jobs -p | wc -l)
   c. Build JSON via `printf -v` (no fork):
      {"cwd","exit_code","cmd_duration_ms","cols","jobs","shell_integration","env"}
      — `shell_integration` reflects `__O10K_EMIT_OSC133` (daemon-side 133;A/B);
      `env` is the frozen allowlist snapshot (12 env keys + the three
      agent-signal keys `CLAUDE_CODE_ENTRYPOINT`, `CODEX_SANDBOX`,
      `CODEX_HOME` for the 1.3 ai segment) built with pure parameter
      expansions by `__o10k_env_json` (only non-empty values, control chars
      stripped, backslash/quote escaped via `__o10k_json_escape`)
   d. Refresh config flags (`__o10k_read_flags`) and re-run the 133;C/D gate
   e. Try bridge first (no fork/exec):
      __o10k_bridge_request "$request" → PS1 set + cache written
   f. Fallback: response=$(__o10k_socket_send "$request")
      Parse "left" from JSON:
      - Preferred: $__O10K_BIN parse-prompt (Rust, ~1-3ms)
      - Fallback: python3 JSON extraction (~5-10ms)
      PS1="$left" + cache written; also parses `notify_threshold_ms`
      (0/empty → OFF), `semantic_prompts`, `notify_unfocused_only`, and
      `transient` from the response
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
printf '\033]7;file://%s%s\033\\' "${HOSTNAME}" "$(__o10k_uri_encode "$PWD")"
```

`$PWD` is passed through `__o10k_uri_encode`, which percent-encodes URI-unsafe characters (space → `%20`, `%` → `%25`, `#` → `%23`, `?` → `%3F`) and strips control characters, so paths with special characters cannot truncate or corrupt the sequence.

### OSC 777 — Desktop Notifications

Notification routing (`__o10k_notify`), when
`__O10K_NOTIFY_THRESHOLD > 0 && __O10K_CMD_DURATION > threshold`:

1. If `[notifications].unfocused_only` is set (relayed via the bridge flags
   side channel as `notify_unfocused_only=1`), skip when the terminal is
   focused (`__o10k_terminal_focused`: Hyprland active-window pid must be in
   the shell's ancestor chain; undetectable → notify anyway)
2. Prefer `omarchy-notification-send` (detected once at init via
   `command -v`; called notify-send-compatibly as `title body`) — Omarchy
   forbids raw `notify-send`
3. Fallback: OSC 777 (`\033]777;notify;TITLE;BODY\007`), with the body
   sanitized (all C0 control characters and semicolons stripped) so it
   cannot break OSC 777 framing

A threshold of `0` (or empty) means the daemon disabled notifications and is
never replaced by a default — this fixes the verified 0.2 no-op defect where
`enabled = false` left the adapter's bootstrap default in force. Supported by
Ghostty, Foot, and WezTerm (OSC 777 path).

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
| 2 | `python3` via `sys.argv` (AF_UNIX, 2s timeout) | ~5ms |
| 3 | Return failure (fallback PS1 used) | — |

`/dev/tcp` fallback is commented in source but not implemented — Bash `/dev/tcp` doesn't support Unix domain sockets.

### Hardening Notes

- **JSON escaping:** The prompt request strips control characters from `$PWD` and JSON-escapes backslashes and double-quotes before embedding it in the request string (shared helper `__o10k_json_escape`); the same treatment is applied to every `env` allowlist value. Directories with special characters (e.g. `C:\Users`, paths containing `"`, or embedded newlines) no longer produce malformed JSON.
- **Python3 fallback safety:** The Python3 socket snippet accepts socket path and message as `sys.argv[1]` and `sys.argv[2]` instead of interpolating shell variables into the script. This eliminates code injection via crafted socket paths.
- **Daemon restart detection:** Before each prompt render, when the bridge coprocess is dead the adapter probes a present socket with a `status` command first. A healthy daemon's socket is never removed — only the bridge is restarted. A missing socket or a failed probe removes the stale socket and restarts both daemon and bridge, rate limited to one attempt per 5 seconds.
- **Bridge fallback NUL framing:** The bridge's `write_fallback()` function emits four NUL-terminated fields (`left\0right\0notify_threshold_ms\0transient\0`, with the last three empty) matching the normal protocol, so the bash reader never hangs waiting for the next field.

## Hook Installation

### Detection

```bash
if declare -F blehook → blesh hooks
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
- DEBUG trap chained with existing trap via `trap -p DEBUG` (all Bash versions)
- Previous versions used `PS0` for Bash 4.4+, but this ran in a subshell, causing timer, ready-gate, and notification state to be lost. The DEBUG trap runs in the main shell context.

### Preexec Ready Gate

`__O10K_PREEXEC_READY` prevents double-firing. Set to `1` at end of `PROMPT_COMMAND`. Checked and cleared at start of `__o10k_preexec`. Without this, DEBUG trap or PS0 would fire on every simple command in a pipeline.

### Preexec Transient, OSC 133 and Progress

The **vanilla** preexec handler additionally performs the non-ble transient
overwrite: when the daemon delivered a non-empty `transient` string and the
current PS1 is single-line (contains no `\n`), it moves the cursor up one
line, rewrites the previous prompt line as `\r<transient>\033[K`, and returns
the cursor to the fresh line. Multi-line prompts are left untouched — their
geometry is unknowable from bash. Under ble.sh the transient is handled
natively instead (see [Transient Prompt](#transient-prompt)).

Both vanilla and ble.sh preexec handlers then:

1. Emit OSC 133;C at command start (marks beginning of command output, if `__O10K_EMIT_133CD`)
2. Set `__O10K_LAST_CMD` from the command string
3. Emit OSC 9;4;3 (indeterminate progress bar)
4. Start command timer (`__o10k_timer_start`)
5. Dispatch preexec hooks

| Mode | Handler | Command source |
|------|---------|----------------|
| ble.sh | `__o10k_preexec_blesh` | `$1` (ble.sh PREEXEC argument) |
| vanilla | `__o10k_preexec` | `$BASH_COMMAND` |

## Transient Prompt

The daemon may deliver a `transient` string (the collapsed form of the
prompt) as the fourth bridge field / JSON response field:

- **ble.sh:** `__o10k_update_rps1` feeds it to `bleopt prompt_ps1_final`
  while `bleopt prompt_ps1_transient='always'` stays set. ble.sh replaces the
  collapsed prompt with `prompt_ps1_final` when non-empty; with an empty
  string the shipped vanish behavior is kept. (Verified against ble.sh's
  `blerc.template`: `prompt_ps1_transient` is a colon list
  `always|same-dir|trim` and `prompt_ps1_final` carries the replacement
  string.)
- **Non-ble:** the vanilla preexec overwrite described in
  [Preexec](#preexec-transient-osc-133-and-progress) — conservative
  single-line-prompt rewrite only.

## First-Run Intro

At init (after hooks are installed) the adapter launches `omarchy10k intro`
in a disowned background subshell writing to `/dev/tty` — non-blocking:

- Skipped when the marker file
  `${XDG_STATE_HOME:-$HOME/.local/state}/omarchy10k/intro_shown` exists
- Skipped when `O10K_NO_INTRO` is set (CI gate)
- Daemon down → the CLI exits silently; the marker stays unwritten so the
  next shell retries

## Right Prompt

When ble.sh is active, the adapter manages the right-aligned prompt via `__o10k_update_rps1`:

1. Registered as a ble.sh `PRECMD+` hook (runs after `__o10k_render_prompt`)
2. Reads `__O10K_LAST_RIGHT` which is populated during the prompt render cycle (from the bridge's `right\0` field or the socket fallback's JSON `right` extraction)
3. Sets `bleopt prompt_rps1` with the cached value, or an empty string if unavailable
4. Sets `bleopt prompt_ps1_final` from `__O10K_TRANSIENT` (see [Transient Prompt](#transient-prompt))

Right prompt content typically includes git branch and command duration when `prompt.right_prompt = true`.

## Runtime Globals

| Variable | Default | Purpose |
|----------|---------|---------|
| `__O10K_CMD_START` | `0` | Command start timestamp |
| `__O10K_CMD_DURATION` | `0` | Last command duration (ms) |
| `__O10K_LAST_CMD` | `""` | Last executed command name (for notifications) |
| `__O10K_NOTIFY_THRESHOLD` | `10000` bootstrap | Desktop notification threshold (ms); `0` = OFF |
| `__O10K_CACHE_DIR` | `$XDG_CACHE_HOME/omarchy10k` | Instant prompt cache directory |
| `__O10K_CACHE` | `$__O10K_CACHE_DIR/last_prompt` | Cached PS1 file path |
| `__O10K_EMIT_OSC133` | `0` or `1` | Whether to request daemon-side OSC 133;A/B markers |
| `__O10K_EMIT_133CD` | `0` or `1` | Whether to emit OSC 133;C/D markers (semantic gate) |
| `__O10K_SEMANTIC_PROMPTS` | `0` | Mirrors `[terminal.semantic_prompts].enabled` from the daemon |
| `__O10K_NOTIFY_UNFOCUSED_ONLY` | `0` | Mirrors `[notifications].unfocused_only` |
| `__O10K_TRANSIENT` | `""` | Daemon transient string for the prompt collapse |
| `__O10K_FLAGS` | `<socket>.flags` | Bridge config-flags side-channel file |
| `__O10K_LAST_RIGHT` | `""` | Last right prompt string (for ble.sh `prompt_rps1`) |
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
| `O10K_PARENT_PID` | Exported as `$$` at init time; used by daemon and CLI to find the per-shell socket |
| `HOSTNAME` | OSC 7 CWD reporting hostname |
| `BLE_VERSION` | ble.sh detection (legacy; v0.3.0 uses `declare -F blehook` instead) |
| `COLUMNS` | Terminal width |
| `EPOCHREALTIME` | Microsecond timing (Bash 5.0+) |
| `PWD` | Current directory for prompt + chpwd |
| `BASH_COMMAND` | Command string for preexec |
| `BASH_VERSINFO` | Bash version detection |
| `PROMPT_COMMAND` | Existing hooks (preserved) |
| `O10K_SHELL_INTEGRATION` | OSC 133 emission mode (`auto`, `force`, `off`) |
| `KITTY_SHELL_INTEGRATION` | Kitty shell integration detection |
| `O10K_NO_INTRO` | Set to skip the first-run intro launch (CI gate) |

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
| `ble.sh` (any version with `blehook`) | Optional | Enhanced hooks, transient prompt, right prompt |

## v0.3.0 Bug Fixes

The following issues identified by the [Bug Audit](bug-audit.md) have been fixed:

| Finding | Fix | Severity |
|---------|-----|----------|
| [#6 Daemon never restarted](bug-audit.md#6-the-daemon-is-never-restarted-once-it-dies) | Restart triggers when socket is missing OR bridge PID is dead; 5-second rate limiting prevents restart storms | High |
| [#7 CLI cannot find socket](bug-audit.md#7-the-cli-cannot-find-the-daemon-socket) | `O10K_PARENT_PID` is now `export`ed at init time so CLI tools can find the per-shell socket | High |
| [#8 ble.sh gate never passes](bug-audit.md#8-the-blesh-version-gate-never-passes) | Detection uses `declare -F blehook` (feature probe) instead of `BLE_VERSION` numeric comparison | High |
| [#11 Cache race between shells](bug-audit.md#11-the-instant-prompt-cache-races-between-concurrent-shells) | Cache temp files use PID-specific names (`$__O10K_CACHE.$$.tmp`) | Medium |
| [#15 Comma-decimal locales](bug-audit.md#15-command-timing-silently-returns-zero-in-comma-decimal-locales) | `EPOCHREALTIME` values are normalized (`comma→period`) before arithmetic | Medium |
| [#18 OSC 777 injection](bug-audit.md#18-osc-777-notification-text-is-injected-unescaped) | Command text is sanitized (all C0 control characters and `;` stripped) before OSC 777 interpolation | Low |
| [#20a kill -0 PID 0](bug-audit.md#20-smaller-confirmed-defects) | Restart logic guards PID > 0 before `kill -0` | Low |
| [#20f Fork-free jobs](bug-audit.md#20-smaller-confirmed-defects) | `jobs -p` output collected via `mapfile` + array length instead of `$(wc -l)` | Low |

## CSI-u Shift+Enter shim

Ghostty users may set `keybind = shift+enter=csi:13;2u` (lets TUIs distinguish Shift+Enter). At a bash readline prompt that sequence is unparseable and leaks as literal `;2u`. The adapter binds it to `accept-line` (interactive shells only, harmless no-op if the terminal never sends it). Remember the adapter is embedded in the `omarchy10k` binary via `include_str!` — rebuild the CLI after editing `shell/omarchy10k.bash`.

## Theme Env Re-Source

The adapter keeps running shells synchronized with theme switches. `__o10k_source_theme_env` is registered as a precmd hook and re-sources `~/.local/state/omarchy/current/theme/o10k-env.sh` (fzf/eza/less/bat/lazygit colors, rendered by the Omarchy theme engine — see [Theme Integration](theme.md)) whenever its mtime exceeds the stamp at `~/.cache/omarchy10k/env.stamp`. Cost when unchanged: one `stat` per prompt. A truncated source (racing the theme directory swap) skips the stamp so the next prompt retries. See the [Theme Integration rice layer](theme.md#rice-layer-theme-reactive-tool-configs) for the template pipeline.

## Modern CLI Layer

After hook installation the adapter sources `~/.config/omarchy10k/tools.sh` (installed by `install.sh` from `config/tools.sh` in the repo). It upgrades interactive Unix defaults with modern replacements that take on the standard command names — `ls`→eza, `cat`→bat (plus themed `MANPAGER`), `grep`→rg, `top`→btop, `du`→dust, `df`→duf, `ps`→procs, `cd`→zoxide (`--cmd cd`), plus atuin (Ctrl-R history, up-arrow untouched), fzf key bindings (`fzf --bash`), and the `y` yazi wrapper. Guards: every block checks `command -v` first, so missing tools degrade silently; the whole layer is skipped with `O10K_NO_TOOLS=1`; aliases never affect scripts (interactive shells only). Underlying packages are ensured by `install.sh` (skippable with `O10K_SKIP_TOOLS=1`).
