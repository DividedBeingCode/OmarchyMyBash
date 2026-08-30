# Daemon IPC Protocol

[← Index](INDEX.md) | [Daemon](daemon.md) | [Architecture](architecture.md)

Omarchy10k uses **newline-delimited JSON (NDJSON)** over **Unix domain sockets** for all inter-process communication. The protocol uses **typed messages with version negotiation** — clients send a `hello` handshake on connect to learn `protocol_version` and `server_version`. Protocol version **0.5** is current; untagged legacy messages remain supported for backward compatibility.

## Transport

| Property | Value |
|----------|-------|
| Socket type | Unix domain socket (`AF_UNIX`, `SOCK_STREAM`) |
| Socket path | `$XDG_RUNTIME_DIR/omarchy10k-{shell_pid}.sock` |
| Framing | Newline-delimited (`\n`) |
| Frame limit | 64 KiB per NDJSON frame (`MAX_FRAME_BYTES` in `server.rs`); oversized frames are rejected with `frame too large` and the connection is closed |
| Encoding | UTF-8 JSON |
| Connection model | Persistent (server loops on read until EOF) |
| Timeout | Client-side: 2 seconds (socat `-T2`, python3 `settimeout(2)`) |
| Protocol version | `0.5` (`PROTOCOL_VERSION` in server) |

### Socket Path Convention

```
${XDG_RUNTIME_DIR:-/tmp}/omarchy10k-${PID}.sock
```

Where `PID` is the Bash shell's process ID (`$$`). The daemon receives this via `O10K_PARENT_PID` environment variable at startup. The CLI resolves it from `O10K_PARENT_PID` or `PPID`.

## Frame Limit and Error Responses

The server caps each incoming NDJSON frame at **64 KiB** (`MAX_FRAME_BYTES` in `server.rs`, `64 * 1024`). The socket reader is capped with `AsyncReadExt::take`, so a client streaming bytes without a newline cannot grow daemon memory unboundedly (OOM guard). A legitimate preview request is a few hundred bytes, leaving orders of magnitude of headroom.

Malformed and oversized input get structured errors instead of dropping the daemon:

| Condition | Response | Connection |
|-----------|----------|------------|
| Frame ≥ 64 KiB without a newline | `{"type":"error","error":"frame too large"}` | **Closed** |
| Invalid JSON on a line | `{"type":"error","error":"<serde_json error>"}` | Kept open (next line processed) |
| Unknown control command | `{"type":"control","status":"error","error":"unknown command: <name>"}` | Kept open |
| Unknown message `type` | `{"type":"error","error":"unknown message type: <type>"}` | Kept open |
| `control` message without `command` field | `{"type":"error","error":"control message requires 'command' field"}` | Kept open |

## Message Format

All messages are JSON objects terminated by `\n`. Messages can optionally include these envelope fields:

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Message type: `hello`, `control`, `prompt`, `preview`, `config`, `statusline`, `error` |
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
{"type":"hello","status":"ok","protocol_version":"0.5","server_version":"0.4.0"}
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
| `style_preset` | string | No | `""` | **v0.4 (additive).** Per-request preset override for the Quattro preset gallery: renders this preview with the named preset regardless of `style.preset`. Absent/empty = current config. |
| `look` | string | No | `""` | **v0.5.** Dry-run render a Look: resolves the named Look (user entries from `[looks.<name>]` shadow curated ones), applies its patch via the transient in-memory merge, and renders the result. Nothing is persisted; an unknown look name silently falls back to the current config. |
| `patch` | object | No | — | **v0.4.1 (Looks Studio).** `config_set`-shaped patch merged over the effective config for this render only — RAW config-tree keys, no glyph shortcut expansion. Merge order: base config → `look` → project profile of the previewed `cwd` → `patch` → wizard style knobs (later wins); see [Preview `patch` Override](#preview-patch-override-v041-looks-studio). |
| `style_separators` | string | No | `""` | **v0.5 (configure wizard).** Catalog key applied to both separators for this render only. |
| `style_frame` | string | No | `""` | **v0.5 (configure wizard).** Frame mode for this render: `none` \| `left` \| `right` \| `full`. |
| `prompt_newline` | bool | No | `""` | **v0.5 (configure wizard).** Two-line prompt toggle for this render. |

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

Git state is synthesized from the request fields rather than queried from disk. The daemon uses current in-memory config and theme palette; since Tier C, a preview also honors the project profile (`.o10k.toml`) of the previewed `cwd`.

### Preview `patch` Override (v0.4.1, Looks Studio)

The `preview` message also accepts an optional `patch` object (config_set-shaped, RAW config-tree keys — no glyph shortcut expansion). Tier C inserts a PROFILE layer between the Look and the client patch: the effective config builds as **base config → `look` → project profile of the previewed cwd → `patch` → wizard style knobs** (later wins), so Studio edits compose on top of a Look and the repo's `.o10k.toml`. Merge reuses the transient in-memory machinery — no file writes, no daemon state mutation. An unrepresentable patch (e.g. JSON `null`) → `{"type":"preview","status":"error","error":...}` (clients keep their last good render).

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
{"type":"prompt","id":"3","cwd":"/path","exit_code":0,"cmd_duration_ms":1200,"cols":120,"jobs":0,"env":{"VIRTUAL_ENV":"/home/u/.venv","MISE_NODE_VERSION":"22"}}
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

If any value in the patch cannot be converted to TOML (e.g. JSON `null`), nothing is written and the daemon responds instead:

```json
{"type":"config","status":"error","error":"values for keys a.b are not representable in TOML; nothing was written","failed_keys":["a.b"]}
```

`failed_keys` names the offending patch keys. The write is all-or-nothing: a patch containing even one unconvertible value leaves `config.toml` untouched.

The daemon **recursively merges** the JSON patch into `config.toml` on disk and reloads in-memory config. Top-level sections are merged, not replaced — keys absent from the patch are preserved. For example, a patch containing `{"git":{"mode":"compact"}}` updates only `git.mode` without touching `git.cache_ttl_ms` or other git keys.

Behavior details:
- **Missing file:** If `config.toml` does not exist (fresh install), the daemon creates it with `create_dir_all` on the parent directory, seeds from an empty table, merges the patch, and writes.
- **Parse error:** If the existing file has TOML syntax errors, the daemon returns `{"type":"config","status":"error","error":"config.toml has syntax errors: ..."}` and **refuses to overwrite** the file.
- **I/O error:** Write failures return a structured error response instead of dropping the connection.
- **Atomic write:** Uses temp file + rename to prevent corruption on crash.
- **Theme reload:** When the patch touches the `[theme]` section, the daemon automatically calls `reload_theme()` after `reload_config()` so palette changes take effect immediately.
- **Unconvertible values:** Values that fail JSON→TOML conversion (e.g. `null`) are collected up front; if any exist, the daemon responds `status:"error"` with `failed_keys` listing them and writes nothing (all-or-nothing).

**Used by:** Quattro panel (primary write path).

## Control Command Reference

| Command | Direction | Since | Purpose |
|---------|-----------|-------|---------|
| `status` | client→daemon | 0.1 | Health check; returns pid, version, protocol_version, cwd — **plus v0.4 ambient fields** (see below) |
| `palette` | client→daemon | 0.3 | Returns resolved theme palette as hex colors |
| `reload_config` | client→daemon | 0.1 | Re-read config.toml from disk |
| `reload_theme` | client→daemon | 0.1 | Re-resolve theme palette |
| `invalidate_git` | client→daemon | 0.1 | Clear git status cache |
| `shutdown` | client→daemon | 0.1 | Graceful daemon exit |
| `config_get` | client→daemon | 0.2 | Return full config as JSON (via typed `config` message) |
| `config_set` | client→daemon | 0.2 | Apply config patch (via typed `config` message) |
| `statusline` (message) | client→daemon | 0.4 | Render a Claude Code statusLine payload with the active theme (typed message, not a control command) |
| `looks` | client→daemon | 0.5 | List all Looks (curated + user) as `{name, label, patch}` entries |
| `looks_apply` | client→daemon | 0.5 | Apply a named Look atomically to disk, or transiently in memory with `transient: true` |
| `looks_save` | client→daemon | 0.5 | Save the current appearance as a user Look in `[looks.<name>]` |
| `palettes` | client→daemon | 0.5 | List the 8 curated palette keys with their `theme` patches |
| `defaults` | client→daemon | 0.5 | Return the full default `Config` as JSON (modified-vs-default comparison) |
| `script_list` | client→daemon | 0.5 | List executable user scripts in `~/.config/omarchy10k/scripts` |
| `script_run` | client→daemon | 0.5 | Execute a named user script with a hard timeout (default 30s) |

### `status`

Health check. Returns daemon metadata.

**Request:**
```json
{"type":"control","command":"status"}
```

**Response:**
```json
{
  "type": "control",
  "status": "ok",
  "pid": 12345,
  "version": "0.4.0",
  "protocol_version": "0.5",
  "cwd": "/home/user",
  "git": {"branch": "main", "dirty": true, "staged": 1, "unstaged": 2, "conflicted": 0, "ahead": 0, "behind": 0, "worktree": null, "stale": false},
  "last_cmd_duration_ms": 1200,
  "last_exit_code": 0,
  "session_age_secs": 300,
  "battery": {"capacity": 77, "status": "Discharging"},
  "agent": "claude"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `pid` | int | Daemon process ID |
| `version` | string | Server version (`CARGO_PKG_VERSION`) |
| `protocol_version` | string | Protocol version string |
| `cwd` | string | Daemon's current working directory at request time (added in v0.3) |
| `git` | object? | Ambient git snapshot (v0.4). `null` before the first prompt render. Prefers the live git cache entry, falls back to the last render summary. Fields: `branch`, `dirty`, `staged`, `unstaged`, `conflicted`, `ahead`, `behind`, `worktree` (string or null), `stale`. |
| `last_cmd_duration_ms` | int | Duration of the command active at the last render; `0` before the first render. |
| `last_exit_code` | int | Exit code at the last render; `0` before the first render. |
| `session_age_secs` | int | Seconds since the daemon (shell session) started. |
| `battery` | object? | `null` on batteryless machines; otherwise `{capacity: int, status: "Charging" \| "Discharging"}` from the same sysfs read the battery segment uses. |
| `agent` | string \| null | AI coding agent active at the last prompt render: `"claude"` when `CLAUDE_CODE_ENTRYPOINT` was in the prompt request's `env` channel, `"codex"` when `CODEX_SANDBOX` or `CODEX_HOME` was (added in the C1 Agents MVP wave). `null` when no agent env key was present or before the first prompt render. Latched from the `env` channel at render time and mirrored in `RenderSummary.agent` — it always agrees with the `ai` segment's detection (`segments/ai.rs`). Never updated by `preview` renders. |

All enrichment fields (v0.4 ambient set and the later `agent`) are **additive** — older clients ignore them safely.

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

### `looks` (v0.5)

List all Looks. Returns curated Looks first, then user Looks from `[looks.<name>]` in `config.toml`, sorted by name. A user entry with the same name as a curated Look shadows it (the curated entry is omitted).

**Request:**
```json
{"type":"control","command":"looks"}
```

**Response:**
```json
{
  "type": "control",
  "status": "ok",
  "looks": [
    {"name": "omnarchy", "label": "Omnarchy", "patch": {"style": {"preset": "omarchy"}, "...": "..."}},
    {"name": "my-look", "label": "My Look", "patch": {"style": {"preset": "framed"}}}
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `looks` | array | `{name, label, patch}` objects. `patch` is `config_set`-shaped (top-level config keys, glyph shortcuts expanded, palette resolved into a `theme` sub-patch), directly applicable via `config_set`. |

The 8 curated Looks are compiled in: `omnarchy`, `tokyo-rainbow`, `framed-gradient`, `lean-pure`, `slanted-owl`, `gruvbox-drift`, `rose-classic`, `polar-lean`.

**Used by:** CLI `look list`, Quattro panel Looks rail, gallery overlay.

### `palettes` (v0.5)

List the curated palettes (moved daemon-side from quattro/Model.js so Looks resolve identically from CLI, gallery, and panel).

**Request:**
```json
{"type":"control","command":"palettes"}
```

**Response:**
```json
{
  "type": "control",
  "status": "ok",
  "palettes": [
    {"key": "tokyo-night", "theme": {"source": "hybrid", "custom": {"accent": "#7aa2f7", "...": "..."}}}
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `palettes` | array | `{key, theme}` objects. `theme` is an `[theme]`-shaped patch: `source: "hybrid"` plus 11 `custom` colors (`accent`, `foreground`, `muted`, `background`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `orange`). |

Keys (all always present): `tokyo-night`, `catppuccin`, `gruvbox`, `nord`, `dracula`, `rose-pine`, `everforest`, `kanagawa`.

**Used by:** Quattro panel palette picker, gallery overlay.

### `defaults` (v0.5)

Return the **full default `Config`** (`Config::default()`) as JSON — the current config is not merged in. The panel diffs live config values against this snapshot to draw modified-vs-default ink on settings rows and to offer per-row reset.

**Request:**
```json
{"type":"control","command":"defaults"}
```

**Response:**
```json
{"type":"control","status":"ok","config":{"prompt":{"layout":"omarchy","...":"..."},"git":{"enabled":true,"...":"..."}}}
```

**Used by:** Quattro panel (modified-vs-default ink bars, per-row reset).

### `looks_apply` (v0.5)

Apply a named Look. Resolution uses user shadowing: a `[looks.<name>]` entry wins over a curated Look of the same name.

**Request:**
```json
{"type":"control","command":"looks_apply","name":"framed-gradient","transient":true}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Look name. Missing/empty → `{"status":"error","error":"looks_apply requires 'name'"}`; unknown name → `{"status":"error","error":"unknown look: <name>"}`. |
| `transient` | bool | No | `false` | Semantics switch, see below. |

**Transient (`transient: true`) — Try mode, in-memory only:**

The look patch is merged into the current **in-memory** config only (via `looks::apply_transient`, no file write), followed by `reload_theme()`. Response: `{"type":"control","status":"ok","transient":true}`. The change reverts the moment anything triggers `reload_config` — the daemon's config watcher on `config.toml`, a `reload_config` control command, a config write, or an atomically applied look. No disk state changes.

**Atomic (`transient: false`, default) — merged to disk:**

The look patch goes through `write_config_patch` — the same code path as `config_set`: recursive JSON→TOML merge into `config.toml`, atomic tmp+rename write, in-memory `reload_config`. Persistent across restarts. Response: `{"type":"control","status":"ok"}`. Errors (TOML syntax in the existing file, unrepresentable patch values, I/O failure) return `{"status":"error","error":...}` and nothing is written.

**Used by:** CLI `look apply <name> [--transient]`, Quattro panel LOOKS cards (Try vs Apply), gallery overlay detail sheet.

### `looks_delete`

Delete a USER look. `{type:"control",command:"looks_delete",name}` → `{status:"ok"}`; errors: `cannot delete curated look: <name>`, `unknown look: <name>`. Rewrites config.toml atomically (tmp+rename) and reloads. A user look shadowing a curated name deletes only the override. Added in v0.4.1 (Studio).

### `looks_save` (v0.5)

Save the daemon's **current in-memory appearance** as a user Look. The daemon captures a snapshot of the current style and writes a `[looks.<name>]` entry via the atomic `write_config_patch` path.

**Request:**
```json
{"type":"control","command":"looks_save","name":"my-look","label":"My Look"}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Look key. Missing/empty → `{"status":"error","error":"looks_save requires 'name'"}`. |
| `label` | string | No | `""` | Display label; empty label falls back to the name at resolution time. |

The captured patch covers: `style.preset`, `style.separators.shape/left/right`, `style.frame.enabled/gap_char/gap_gradient`, glyph shortcuts (`glyphs.os_icon`, `glyphs.character` from `segments.character.success`, `glyphs.git_branch_icon` from `git.branch_icon`), and `prompt.blank_line`. The entry gets the `palette: "keep"` directive (current palette retained). Option fields set to `None` (preset default) are normalized to preset defaults (`""` / `true`) instead of serialized as TOML-invalid `null` — the null would have failed the whole patch write on fresh configs.

**Response:** `{"type":"control","status":"ok"}` on success, `{"status":"error","error":...}` on write failure.

**Used by:** Quattro panel Save-as-Look, CLI `look save <name>`.

### `script_list` (v0.5)

List executable user scripts from `$XDG_CONFIG_HOME/omarchy10k/scripts` (the quick-actions registry). A script is listed only when it is a regular file with the executable bit set and a valid name: non-empty, no `/`, no `..` substring, not starting with `.` (traversal guard). Missing directory → empty list. Output is sorted by name.

**Request:**
```json
{"type":"control","command":"script_list"}
```

**Response:**
```json
{
  "type": "control",
  "status": "ok",
  "dir": "/home/user/.config/omarchy10k/scripts",
  "scripts": [{"name": "backup-notes.sh", "path": "/home/user/.config/omarchy10k/scripts/backup-notes.sh"}]
}
```

**Used by:** CLI `script list`, quick actions UI.

### `script_run` (v0.5)

Execute a user script daemon-side with output capture and a hard timeout. Trust model: scripts come only from the user's own config directory — same trust level as `.bashrc`, nothing network-sourced.

**Request:**
```json
{"type":"control","command":"script_run","name":"backup-notes.sh","timeout_secs":60}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | Yes | — | Script name (without directory). Resolved via the traversal guard: rejected if it contains `/` or `..` or starts with `.`, is not a regular file, or lacks the executable bit. |
| `timeout_secs` | int | No | `30` | Hard execution budget. The child process is killed (`kill_on_drop`) when the budget expires. |

**Response (success):** `{"type":"control","status":"ok","name":"<name>","output":"<trimmed stdout>"}`

**Response (failure):** `{"type":"control","status":"error","error":"..."}` — covers invalid/traversal names, missing/non-executable scripts, timeouts (`script timed out after <n>s and was killed`), and non-zero exits (error carries the exit status and trimmed stderr).

**Used by:** CLI `script run <name>`, quick actions UI.

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
  "shell_integration": true,
  "env": {"VIRTUAL_ENV": "/home/user/.venv", "KUBECONFIG": "/home/user/.kube/config"}
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
| `env` | object | No | **v0.4 env channel.** Flattened `{"KEY": "value", ...}` map of shell-exported variables. Keys come from the adapter's allowlist (must match `[env.watch]` in config). SegmentContext reads these instead of the daemon's own environment; absent keys fall back to `std::env` for legacy clients. |

### Response

```json
{
  "type": "prompt",
  "left": "\u001b[1;38;2;122;162;247m~/project\u001b[0m ...",
  "right": "\u001b[38;2;86;95;137m1.2s\u001b[0m \u001b[38;2;122;162;247m\u001b[0m main",
  "transient": "\u001b[38;2;86;95;137m❯\u001b[0m ",
  "git_stale": false,
  "notify_threshold_ms": 10000,
  "semantic_prompts": false,
  "notify_unfocused_only": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"prompt"` for typed responses |
| `left` | string | Full prompt string with ANSI escape codes and optional OSC 133 markers. With the `powerline`/`rainbow` presets (v0.4), includes SGR `48;2` background fills and flipped-color separators. |
| `right` | string? | Right prompt (duration + branch). Present when `prompt.right_prompt = true`. |
| `transient` | string? | Transient replacement prompt. Present when `prompt.transient = true`. |
| `git_stale` | bool | Whether git data is from a stale or cold cache hit |
| `notify_threshold_ms` | int | **v0.4.** Notification threshold in ms. Always a number: the configured threshold when `[notifications].enabled`, **`0` when disabled** — the adapter treats 0/empty as OFF. (Previously null/absent on disable, which the adapter misread as "keep default": the no-op bug.) |
| `semantic_prompts` | bool | **v0.4.** Whether the adapter may emit OSC 133;C/D (from `[terminal.semantic_prompts].enabled`, default `false`). Missing field = `false` for old daemons. |
| `notify_unfocused_only` | bool | **v0.4.** Restrict notifications to unfocused terminals (bash-side focus gating). |

### Error Response

When prompt rendering fails:

```json
{"type":"error","error":"description of what went wrong"}
```


### `statusline` Message (v0.4)

Render a Claude Code **statusLine** stdin JSON payload through the daemon with
the current config and theme palette. Added in v0.4. Target: `<50ms` warm
render, no OSC 133 markers, left-only output.

**Request:**
```json
{
  "type": "statusline",
  "id": "9",
  "payload": {
    "model": {"id": "claude-opus", "display_name": "Opus"},
    "workspace": {"current_dir": "/home/user/project", "project_dir": "/home/user/project"},
    "cost": {"total_cost_usd": 0.034, "total_duration_ms": 45000},
    "context_window": {"used_percentage": 41.7}
  }
}
```

The `payload` object is the Claude Code statusLine JSON verbatim. Parsing is
**tolerant**: unknown fields are skipped, missing sections simply render fewer
parts, and a flat payload (fields at the top level, no `payload` wrapper) is
accepted for manual/test invocations.

**Response:**
```json
{"type":"statusline","status":"ok","left":"<ansi>"}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"statusline"` |
| `status` | string | `"ok"` on success, `"error"` on unparseable payload |
| `left` | string | Left-only ANSI line: model name (accent), context % (green/yellow/red at `[statusline].context_warning_pct`/`context_critical_pct`, default 60/85), cost when present (muted), cwd basename (foreground) |

Errors keep the same shape with `"status":"error"` and an `error` string; the
CLI falls back to its builtin render on error or timeout.

**Used by:** CLI `omarchy10k statusline claude-code` (reads Claude Code stdin
JSON, forwards as payload).

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
# Read four NUL-terminated fields from coproc stdout:
#   left \0 right \0 notify_threshold_ms \0 transient
IFS= read -r -d $'\0' -t 2 -u "${__O10K_BRIDGE[0]}" left
IFS= read -r -d $'\0' -t 2 -u "${__O10K_BRIDGE[0]}" right
```

Since v0.4 the bridge emits **four** NUL-terminated fields per response: `left \0 right \0 notify_threshold_ms \0 transient`. `write_fallback` (no daemon) emits empty 3rd/4th fields. Old 3-field readers still work — the 4th field is simply ignored by a reader that stops after `right`. The `notify_threshold_ms` field is empty/`0` when notifications are disabled (**OFF**, not "keep default"), and the `transient` field carries the daemon's transient prompt string (ble.sh feeds it via transient hooks; non-ble.sh gets the line-2 overwrite). The `right` field may be empty when right prompt is disabled.

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

Since v0.4 the panel also sends a per-request `style_preset` override on
`preview` messages for the preset gallery cards (additive; absent = current
config preset).

v0.3 additions:
- **`preview`** — live prompt preview with simulated context toggles (error state, SSH, long command duration)
- **`palette`** — fetches resolved theme colors for swatch display alongside preview

On config save or preview context change, the panel sends a `preview` request and updates the preview text (ANSI stripped for display).

## Preview scenes (v0.6, additive)

`preview` takes an optional `scenes` array and renders every entry against
**one** effective config, returning them in a `renders` array.

```json
{"type":"preview","cwd":"~/projects/my-app","cols":88,"look":"polar-lean",
 "scenes":[{"label":"clean","git_branch":"main"},
           {"label":"failed","git_branch":"main","exit_code":127,
            "cmd_duration_ms":2400},
           {"label":"ssh","cwd":"~/dotfiles","in_ssh":true}]}
```

```json
{"type":"preview","status":"ok","left":"…","right":null,
 "renders":[{"label":"clean","left":"…","right":"…"}, …]}
```

Why it exists: a prompt has to survive a dirty repo, a failed command, an SSH
host and a deep path, and the Studio shows six such rows. Six requests would
be six round-trips and six entries in Quattro's preview broker, on a surface
that re-renders while you hover.

- Every scene field defaults, so a scene specifies only what it varies and
  inherits the rest from the request.
- Scene fields are `cwd`, `exit_code`, `cmd_duration_ms`, `cols`, `jobs`,
  `in_ssh`, `git_branch`, `git_staged`, `git_unstaged`, `label`. `label` is
  echoed back so the UI can caption a row without re-deriving it.
- `cols` is per-scene: gallery cards render at 44, the Studio pane at 88.
- Capped at `MAX_PREVIEW_SCENES` (12).
- **Omitting `scenes` returns the exact response shape it always did**, with
  top-level `left`/`right` and no `renders` key. The CLI, the bar panel and
  the configure wizard are unaffected.

Top-level `left`/`right` are always present, even alongside `renders`.

## Palettes verb (enriched)

`{"type":"control","command":"palettes"}` returns the curated table **plus one
palette derived from every installed Omarchy theme without a curated entry** —
30 entries on a stock install (16 curated + 14 derived).

```json
{"key":"osaka-jade","label":"Osaka Jade",
 "blurb":"Derived from the Osaka Jade theme, with muted adjusted for contrast.",
 "source":"derived","low_contrast":false,
 "colors":{"accent":"#509475","background":"#111c18", …},
 "theme":{"source":"hybrid","custom":{…}}}
```

`colors` is sent **flat** on purpose: without it a UI has to reconstruct
swatches out of the `theme` patch, which is precisely the duplicated
derivation this design removes. Curated entries lead the list; derived follow
in sorted order, so a picker does not reshuffle between opens.
`low_contrast` is true when derivation could not reach its floor for some
role — see [Theme](theme.md).

## Theme Hook (`hooks/theme-set`)

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

### Look Apply (transient Try → reload revert, v0.5)

```
Quattro Panel          Socket              Daemon              config.toml
      │                   │                    │                     │
      ├─ looks_apply ────►│                    │                     │
      │  name, transient:true                  │                     │
      │                   ├─ resolve look ────►│                     │
      │                   │                    ├─ apply_transient    │
      │                   │                    │  (merge into        │
      │                   │                    │   in-memory config, │
      │                   │                    │   no disk write)    │
      │                   │                    ├─ reload_theme       │
      │                   │◄─ ok, transient:true ──┤                 │
      │◄─ Try preview ────┤                    │                     │
      │                   │                    │                     │
      ├─ reload_config ──►│  (revert: watcher or explicit)           │
      │                   ├─ re-read config ──►│                     │
      │                   │                    ├─ update RwLock ◄────┤
      │                   │◄─ ok ──────────────┤   (disk copy wins)  │
      │◄─ previous look ──┤                    │                     │
```

The transient state lives only in `RwLock<Config>`. Any `reload_config` — the config.toml watcher, a `reload_config` control command, a `config_set`, or an atomic `looks_apply` — restores the on-disk config, discarding the Try.

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
