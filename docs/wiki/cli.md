# CLI Reference (`omarchy10k`)

[← Index](INDEX.md) | [Protocol](protocol.md) | [Bash Adapter](bash-adapter.md)

The `omarchy10k` binary is a thin async client (crate `omarchy10k`, version 0.4.0). It does not render prompts — it routes requests to the daemon over the Unix socket and handles user-facing subcommands. The subcommand set below mirrors the `Commands` enum in `crates/omarchy10k/src/main.rs` (ground truth).

## Subcommands

Subcommands in enum order: `prompt`, `init`, `layer`, `doctor`, `reload`, `look`, `plugin`, `migrate`, `benchmark`, `debug`, `bridge`, `statusline`, `intro`, `configure`, `update`, `script`, `hook-event`, plus two hidden: `parse-prompt`, `benchmark-shell`.

### `omarchy10k prompt`

Requests a prompt render from the daemon.

| Flag | Default | Description |
|------|---------|-------------|
| `--cwd` | `"."` | Working directory (resolved to absolute if `"."`) |
| `--exit-code` | `0` | Last command exit code |
| `--cmd-duration-ms` | `0` | Last command duration in milliseconds |
| `--cols` | `80` | Terminal width |
| `--jobs` | `0` | Background job count |

Sends a JSON prompt request to the daemon socket, extracts the `left` field from the response, and prints it to stdout. Unlike the Bash adapter's request, the CLI omits `shell_integration` and `env` — the daemon treats both as optional.

**Fallback:** If the daemon is unreachable, prints a hardcoded fallback PS1 (same string as the adapter's `__O10K_FALLBACK_PS1`):
```
\[\e[1;34m\]\w\[\e[0m\] \[\e[1;32m\]❯\[\e[0m\]
```

If the daemon responds but the JSON has no `left` field, the raw response is printed instead (prompt.rs `render`).

### `omarchy10k init bash`

Emits the Bash adapter source to stdout. Integration point for `.bashrc`:

```bash
eval "$(omarchy10k init bash)"
```

Prints the full contents of `shell/omarchy10k.bash` via `include_str!`. Only `"bash"` is accepted — any other shell name prints an error and exits with code 1.

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

Unlike the other subcommands, `check_daemon` does **not** use `socket_path()`: it scans the runtime directory for every `omarchy10k-*.sock` and probes each with a `status` command, reporting one line per running shell daemon.

### `omarchy10k reload`

Sends `{"command":"reload_config"}` to the daemon. Prints `"config reloaded"` on success.

### `omarchy10k look`

Browse and apply named appearance bundles (Looks). A Look is a named patch over the config tree plus a palette directive; curated Looks ship compiled-in, user Looks live in `[looks.<name>]` tables in `config.toml`, and user entries shadow curated names (daemon module `looks.rs`). The daemon side of the three verbs is specified in [Protocol](protocol.md).

| Subcommand | Daemon verb | Description |
|------------|-------------|-------------|
| `look list` | `looks` | Lists available Looks (curated + user-saved) with name, label, and patch |
| `look apply <name>` | `looks_apply` | Applies a Look atomically — the patch is merged into `config.toml` (tmp+rename) and the daemon's fs watcher reloads it |
| `look apply <name> --transient` | `looks_apply` | In-memory only; reverted by the next config reload (`omarchy10k reload`) |
| `look save <name> [--label <label>]` | `looks_save` | Snapshots the current appearance as a named user Look written to `[looks.<name>]` (palette directive `keep`) |
| `look export <name> [--out FILE] [--clipboard]` | `looks` + `looks_save`-style local read | Emits the Look as a portable TOML bundle — stdout by default, `--out FILE`, or `--clipboard` (wl-copy, then xclip, then **OSC 52** written to `/dev/tty` — the terminal-level clipboard escape, and the only one of the three that works over SSH; declines rather than truncates past the terminal payload cap). User entries export verbatim; curated Looks need a reachable daemon (`export_curated`) |
| `look install <file\|https-url> [--yes] [--force] [--as NAME]` | `config set` (looks table) | Installs a Look bundle. Local files or **https:// URLs only** (fetched with `curl`; any other scheme refused before network activity — share.rs `fetch_bundle`). The bundle is validated (exactly one `[looks.<name>]` table, entry keys checked, valid name) and by default only the resolved patch is **printed (dry run)**. `--yes` writes via the daemon's atomic config patch, falling back to a local atomic tmp+rename write; `--force` overwrites an existing user Look of the same name, `--as NAME` installs under a different name |

```bash
omarchy10k look list
omarchy10k look apply tokyo-night --transient
omarchy10k look save my-rice --label "My Rainbow"
omarchy10k look export my-rice --out my-rice.toml
omarchy10k look install my-rice.toml --as borrowed-rice
```

### `omarchy10k benchmark`

Performance test. Sends repeated prompt requests and reports latency statistics.

| Flag | Default | Description |
|------|---------|-------------|
| `--iterations` | `100` | Number of prompt requests |

Uses current directory (or `/tmp`) as cwd. Fixed params: `exit_code=0`, `cmd_duration_ms=0`, `cols=120`, `jobs=0`.

Output: average, p50, p95, p99 in milliseconds. Pass/fail against a 5 ms p50 target.

### `omarchy10k debug`

Dumps daemon state for debugging. Sends the `status` control verb and prints the full JSON response. Since the 0.3 status enrichment the response includes:

| Field | Content |
|-------|---------|
| `pid`, `version`, `protocol_version` | Daemon process identity (crate version 0.4.0, protocol 0.5) |
| `cwd` | Daemon's working directory |
| `git` | Merged summary of the last render's git state + the live git cache entry for that cwd |
| `last_cmd_duration_ms` | Duration of the last command rendered by this daemon |
| `last_exit_code` | Exit code of that command |
| `session_age_secs` | Seconds since the daemon started |
| `battery` | `{capacity, status}` from a sysfs read, or `null` when absent |

### `omarchy10k bridge`

Runs as a persistent bridge coprocess between Bash and the daemon. Long-running process that amortizes socket connection overhead across prompt renders.

| Flag | Default | Description |
|------|---------|-------------|
| `--socket` | auto-detected | Unix socket path (same resolution as other subcommands) |

**Behavior:**

1. Connects to the daemon socket with bounded retry (10 attempts)
2. Reads JSON requests from stdin line-by-line (tab-separated `key=value` lines are converted to JSON internally via `parse_kv_to_json`)
3. Forwards each request to the daemon via the Unix socket
4. Extracts the response into FOUR NUL-terminated stdout fields:
   `left\0right\0notify_threshold_ms\0transient\0`
5. Relays `semantic_prompts`/`notify_unfocused_only` via a `<socket>.flags` side-channel file (atomic tmp+rename, written only when a flag value changes) — the frozen four-field framing stays untouched
6. If a request cannot be answered, emits the same four fields with the last three empty (`write_fallback`) so the Bash reader never blocks

The Bash adapter starts this as a coprocess:

```bash
coproc __O10K_BRIDGE { "$__O10K_BIN" bridge --socket "$__O10K_SOCKET"; }
```

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
statusline can never hang Claude Code.

install.sh merges (never overwrites) the snippet into
`~/.claude/settings.json` idempotently — an existing third-party `statusLine`
is left untouched; ours is detected by its exact command string. A backup is
written to `settings.json.o10k.bak` before the first merge, and a hint is
printed when `~/.claude` is absent.

### `omarchy10k intro`

One-time themed welcome (3.3). Renders a rich simulated prompt through the
daemon `preview` path (`~/projects/my-app`, `cmd_duration_ms: 2345`,
`jobs: 1`), framed in a rounded box, followed by palette swatches
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

### `omarchy10k configure`

Interactive setup wizard (full-screen alt-buffer TUI via crossterm). One
question per screen — style preset, separators, one-line vs two-line prompt,
frame mode, transient prompt, prompt character, and OS icon — each with a
**live prompt preview rendered by the real daemon renderer**:

- Reuses a live daemon if a probe `preview` request succeeds (700 ms timeout, sockets scanned in sorted order)
- Otherwise spawns a transient `omarchy10kd` next to the CLI binary with `O10K_PARENT_PID=<wizard pid>`, which dies with the wizard
- Preview requests carry per-request style overrides (`style_preset`, `style_separators`, `style_frame`, `prompt_newline`) without touching the config

On completion the chosen keys are written to `~/.config/omarchy10k/config.toml` (any existing file is backed up first); the daemon's fs watcher picks the change up immediately — no restart needed.

**v0.4.1 depth additions**: new steps after appearance — context preview (cycle Clean / Failed / Dirty repo / SSH with live daemon previews), per-segment enable toggles (arrows + space, live preview via the patch payload), and three finish paths: apply now (config.toml), save as Look (config_set on the looks table), or save as project profile (`.o10k.toml` in cwd). Every wizard preview carries a config_set-shaped patch so prompt char, transient, OS icon, and segment toggles render live.

Wizard invariants worth keeping (each was broken once and is now covered by a unit test):

| Invariant | Why |
|-----------|-----|
| Segment toggles write the config path that actually gates the segment | `git` and `directory` are **top-level** tables and `python_env` is spelled `segments.python`. `SegmentsConfig` has no `deny_unknown_fields`, so `segments.git.enabled` deserialized into nothing and those three toggles were silent no-ops |
| All three finish paths serialize from one source (`full_patch_value`) | `render_config` used to hand-write only style/separators/frame/prompt/character, so the *default* finish path discarded every segment toggle and the OS icon |
| Segment defaults mirror the daemon's `Config::default()` | Pre-checking everything turned on `k8s`/`time`/`battery`/`load` and the whole default-off Tier D catalog — including four segments that spawn a subprocess per TTL window |
| The project-profile path strips exec-tier `enabled` flags | The daemon rejects such a profile *wholesale*, which would silently discard every other choice in it |
| `q` quits, and the prompt-character step runs | Both regressed to dead code: nothing produced `Key::Quit` while every screen advertised "[q] quit", and `step_prompt_char` had been dropped from the step chain, pinning the glyph to `chevron` |


### `omarchy10k configure --describe`

Prints the wizard's step data as JSON and exits: the step list, every option
catalog, segment metadata (`all`, `default_on`, `exec_tier`) and the finish
paths.

This exists so the wizard's options live in **one** place. The Quattro
Studio's Setup tab renders this rather than restating the catalogs in QML —
the CLI wizard had already drifted badly (segment toggles writing config paths
the daemon never read, the prompt-character step dropped from the chain, `q`
no longer quitting) precisely because nothing else consumed the data and
nothing checked it. `describe_exposes_every_catalog_the_wizard_offers` pins
the contract.

```bash
omarchy10k configure --describe | jq .steps
```

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
7. Reload the Quattro shell (`omarchy restart shell`; falls back to
   `omarchy-shell shell rescanPlugins` with a warning). **A rescan alone is
   not enough** — it re-reads the plugin list but does not invalidate QML's
   component cache, so changed `.qml` files keep serving their previous code
   and an update appears to succeed while changing nothing.
8. Copy `hooks/theme-set` to hook directory
9. Copy `templates/omarchy10k.toml.tpl` to `~/.local/share/omarchy/templates/` (theme bridge)
10. Send `shutdown` command to all running daemon sockets (`omarchy10k-*.sock`)
11. Print version change summary

Running daemons auto-restart on the next prompt render via the Bash adapter's existing reconnection logic.

### `omarchy10k script`

List and run user-defined quick actions from `$XDG_CONFIG_HOME/omarchy10k/scripts` (same trust level as `.bashrc`; the directory matches the daemon's).

| Usage | Behavior |
|-------|----------|
| `script list` | Reads the scripts directory **directly** (no daemon) and prints machine-readable pretty JSON: `{"dir": ..., "scripts": [{"name": ..., "path": ...}]}` — only executable files, same validation as the daemon side |
| `script run <name>` | **Daemon-first:** sends a `script_run` control verb to the daemon socket (CLI round-trip budget 60 s; the daemon enforces a 30 s execution timeout on the script). Prints the daemon's captured `output` on success |
| `script run <name>` (daemon unreachable) | **Local fallback:** executes the script directly with the same hard-timeout model (30 s), after announcing the fallback on stderr |

Script names are validated everywhere: non-empty, no `/`, no `..`, must not start with `.` — paths are never accepted, only names inside the scripts directory.

```bash
omarchy10k script list
omarchy10k script run update-system
```

An unknown action (anything but `list` or `run`) exits with an error listing the valid actions.

### `omarchy10k hook-event`

Dispatch a desktop hook event to Omarchy's hook system:

```bash
omarchy10k hook-event battery-low 42
omarchy10k hook-event post-update
```

| Argument | Description |
|----------|-------------|
| `name` | Event name, e.g. `battery-low`, `post-update`, `font-set` |
| `args...` | Event arguments (e.g. battery percentage), passed through verbatim |

Two delivery paths, tried in order (hook_event.rs):

1. **`omarchy-hook <event> [args…]`** — Omarchy's own dispatcher, when present on `PATH`. It fans out to every registered `<event>.d/` consumer and handles logging itself. Its exit code is propagated (non-zero → CLI error).
2. **Fallback** — run the flat `~/.config/omarchy/hooks/<event>` file first (if executable), then every executable in `~/.config/omarchy/hooks/<event>.d/`, all with the same arguments (hook root honors `XDG_CONFIG_HOME`). The flat-file-then-directory ordering mirrors Omarchy's own runner, so a hook installed the flat way is not silently skipped when `omarchy-hook` is absent. Individual hook failures are logged to stderr but do not abort the remaining hooks — a desktop event must never be dropped because one consumer is broken. Non-executable files are ignored.

### `omarchy10k parse-prompt` (hidden)

Internal helper used by the Bash adapter's socket-fallback path. Reads a JSON daemon response from stdin and prints the `left` field raw; non-JSON input prints nothing (exit 0). Used as the fast Rust alternative to a `python3` JSON extraction in `__o10k_render_prompt` (see [Bash Adapter](bash-adapter.md#prompt-rendering-__o10k_render_prompt)).

### `omarchy10k benchmark-shell` (hidden)

Shell-level end-to-end latency benchmark. Despite the name it does not spawn Bash: it opens **one persistent UnixStream connection** to the daemon and replays the same fixed request per iteration over that single stream — simulating the bridge coprocess pattern (no per-request connect), which is the path real prompts take.

| Flag | Default | Description |
|------|---------|-------------|
| `--iterations` | `100` | Number of render cycles |
| `--adapter` | — | Accepted for CLI compatibility; currently unused (`_adapter`) |

Fixed request params: `exit_code=0`, `cmd_duration_ms=0`, `cols=120`, `jobs=0`.

Output: min, avg, p50, p95, p99, max in milliseconds. Pass requires **p50 < 5 ms AND p95 < 10 ms**; on failure the offending targets are printed and the process exits with code 1 (CI-usable).

## `layer`

Shell-layer claim map (C2). Reads `[shell.layer]` from the user config, resolves the claim inventory (contested: ls extend, lt/cd/fzf_keys/manpager/bat_theme defer; uncontested/gap-fill: own) and prints a human map, or `--json` for the panel. `init bash` bakes the resolved policy as `__O10K_LAYER_POLICY` plus per-item `__O10K_LAYER_OVERRIDES_<name>` prelude lines ahead of the adapter body — the shell does trivial detection only, no dynamic broker.

```
omarchy10k layer            # human map (claim / kind / category / action / notes)
omarchy10k layer --json     # machine-readable claim table
```

## `plugin`

Segment-plugin economy (v0.4.1). Plugins live in `~/.config/omarchy10k/plugins/<name>/` with a `plugin.toml` (name, description, version, author, [[segments]] with tier env|command). `plugin add <git-url>` shallow-clones (https/git/git@ remotes only — local paths refused), installs DISABLED, and prints what it adds. The clone is staged *beside* the plugins directory, not inside it, and removed by a drop guard on every error path — a staging dir under the plugins root is enumerated by the daemon's `load_plugins` (warned about on every reload if leaked) and, since that directory is watched recursively, the clone's own file traffic churns `reload_config` while it runs; `plugin list`; `plugin enable|disable <name>` (config `[plugins].enabled`, daemon registry reloads live); `plugin remove` (refuses while enabled); `plugin update` (git pull + commit summary). Enabled plugin segments join the render pipeline as `plugin.<plugin>.<segment>`.

Convert a starship.toml into an o10k Look: `omarchy10k migrate <starship.toml> [--yes]`. Dry-run by default — prints the mapping table (directory, git, cmd_duration, character, time, battery, jobs, aws, gcloud, kubernetes, terraform, package, docker_context, hostname/username→ssh, python/conda→python_env, nodejs/rust/golang/ruby/java→toolchain; collapsed rows like git_branch+git_status→git) and an honest unmapped list ($fill, $memory_usage, $custom…). `--yes` saves Look `migrated-starship` via the daemon (local atomic fallback). Style/symbol formatting inside starship modules is not translated — segments land with o10k defaults.

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

Priority: `O10K_PARENT_PID` > `PPID` > `"0"`. The Bash adapter exports `O10K_PARENT_PID=$$` at init time, so CLI processes launched from an adapter-initialized shell resolve the correct per-shell socket. From an arbitrary process that only inherits Bash's `PPID` shell variable (never exported), resolution falls through to `omarchy10k-0.sock` and fails — `doctor` and `configure` are unaffected because they scan the runtime directory for `omarchy10k-*.sock` instead.

## Module: `prompt.rs`

### `render(socket_path, cwd, exit_code, cmd_duration_ms, cols, jobs)`

Builds the five-field prompt JSON, sends it via `send_request`, prints the `left` field from the response (raw response when `left` is absent; hardcoded fallback PS1 when the daemon is unreachable).

### `send_command(socket_path, command)`

Sends `{"command":"<name>"}\n`, prints full response. Used by `reload`, `debug`, `look list`, and doctor's daemon probe.

### `send_request(socket_path, request) -> String`

Core transport: `UnixStream::connect` → write request + `\n` → read one line response (trimmed). No timeout at this layer — `statusline` wraps it in its own 300 ms timeout.

### `benchmark(socket_path, iterations)`

Timed loop of prompt requests with percentile computation.

### `benchmark_shell(socket_path, iterations, adapter)`

Single persistent-connection latency benchmark (see the hidden subcommand above); the `adapter` argument is currently ignored.

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

## Module: `configure.rs`

### `run()`

Setup wizard (see the `configure` subcommand above): `DaemonHandle::connect` reuses or spawns a daemon, `preview_request` builds override-carrying preview bodies, and the final choices are written to `config.toml` with a backup.

## Module: `script.rs`

### `run(socket_path, action, name)`

`list` = direct local directory scan; `run` = daemon-first `script_run` with local fallback (see the `script` subcommand above).

## Module: `hook_event.rs`

### `run(event, args, dispatcher, hook_root)`

Desktop-event dispatch (see the `hook-event` subcommand above). `find_dispatcher()` resolves `omarchy-hook` on `PATH`; `default_hook_root()` resolves `$XDG_CONFIG_HOME/omarchy/hooks`.

## Module: `doctor.rs`

Pure diagnostic module. Each check is an independent function that probes the environment via:
- Environment variables (`BASH_VERSION`, `COLORTERM`, `BLE_VERSION`, etc.)
- Binary availability (`which`, `Command::new(...).output()`)
- File existence (`~/.local/share/blesh/ble.sh`, theme files)
- Daemon socket health: directory scan for `omarchy10k-*.sock`, each probed with `send_command("status")`

## Module: `bridge.rs`

### `run(socket_path)`

The bridge coprocess implementation (see the `bridge` subcommand above): stdin reader thread → channel → async forward loop, `extract_prompt_parts` for the four-field split, `update_flags` for the side-channel file, `write_fallback` for the failure framing.

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | Argument parsing with derive macros |
| `tokio` | Async Unix socket I/O |
| `serde` / `serde_json` | JSON parsing (parse-prompt, looks verbs, response handling) |
| `anyhow` / `thiserror` | Error propagation |
| `directories` | XDG config path resolution (doctor, script dir) |
| `crossterm` | configure wizard terminal I/O |
| `unicode-width` | Frame/drawing width math (intro, configure) |
| `toml` | Config round-trip (configure output) |
| `tracing` + `tracing-subscriber` | Logging |

## Known Issues

Recorded by the [Bug Audit](bug-audit.md).

### `doctor` reports ble.sh availability the adapter does not honour

`check_blesh` prints `✓ enhanced mode available` whenever `BLE_VERSION` is set.
The adapter gates on `declare -F blehook` (feature probe), which is the
supported detection path since v0.3.0. Doctor and the adapter still test
different conditions, so doctor advertises a mode that may not be installed.
See [Bug Audit #8](bug-audit.md#8-the-blesh-version-gate-never-passes).

### Socket discovery outside an adapter-initialized shell

Fixed for the common case in v0.3.0 (Bug Audit #7): the adapter now `export`s
`O10K_PARENT_PID=$$` at init, so CLI subprocesses inherit the per-shell PID
and resolve the right socket. The remaining gap is cosmetic: `PPID` is a Bash
shell variable, not an environment variable, so a CLI invoked from a process
tree that never had the adapter's export (e.g. an editor's embedded terminal
before `init bash` ran) still computes `omarchy10k-0.sock`. `prompt` degrades
silently to the hardcoded fallback string in that case, masking the failure.
