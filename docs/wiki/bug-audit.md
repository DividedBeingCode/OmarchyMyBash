# Bug Audit — 2026-08-27

Full-codebase correctness audit of Omarchy10k v0.3.0 against the wiki. Scope was
correctness only: defects that produce wrong output, crashes, hangs, corruption,
or silently dead features. Style, performance micro-nits, and design preferences
are out of scope except where they cause one of the above.

Every finding marked **Verified** was reproduced against a running daemon built
from the working tree at audit time (`cargo build`, 13/13 unit tests passing, 10
dead-code warnings). Findings marked **By inspection** are read from the source
and not separately executed.

| # | Severity | Area | Finding |
|---|----------|------|---------|
| [1](#1-prompt-escapes-are-not-marked-non-printing-so-bash-miscounts-prompt-width) | Critical | `render.rs` | Prompt escapes not marked non-printing — Bash miscounts prompt width |
| [2](#2-struct-tm-abi-mismatch-corrupts-the-stack-when-the-time-segment-is-enabled) | Critical | `segments/time.rs` | `struct tm` ABI mismatch corrupts the stack |
| [3](#3-two-utf-8-byte-slicing-panics-on-ordinary-non-ascii-input) | Critical | `git.rs`, `directory.rs` | Two UTF-8 byte-slicing panics on non-ASCII input |
| [4](#4-the-daemon-exits-immediately-when-o10k_parent_pid-is-unset) | Critical | `main.rs` | Daemon exits immediately without `O10K_PARENT_PID` |
| [5](#5-every-environment-derived-segment-is-frozen-at-daemon-start) | High | architecture | Every environment-derived segment is frozen at daemon start |
| [6](#6-the-daemon-is-never-restarted-once-it-dies) | High | `omarchy10k.bash` | Daemon is never restarted once it dies |
| [7](#7-the-cli-cannot-find-the-daemon-socket) | High | `omarchy10k/main.rs` | CLI cannot find the daemon socket |
| [8](#8-the-blesh-version-gate-never-passes) | High | `omarchy10k.bash` | ble.sh version gate never passes — enhanced mode is dead |
| [9](#9-the-right-prompt-duplicates-content-already-in-the-left-prompt) | High | `render.rs` | Right prompt duplicates left-prompt content |
| [10](#10-gitcache_ttl_ms-is-frozen-at-startup) | Medium | `server.rs` | `git.cache_ttl_ms` is frozen at startup |
| [11](#11-the-instant-prompt-cache-races-between-concurrent-shells) | Medium | `omarchy10k.bash` | Instant-prompt cache races between concurrent shells |
| [12](#12-a-type-less-prompt-request-carrying-command-is-misrouted-to-the-control-handler) | Medium | `server.rs` | Type-less prompt request carrying `command` is misrouted |
| [13](#13-home-prefix-substitution-is-not-path-aware) | Medium | `render.rs`, `directory.rs` | Home-prefix substitution is not path-aware |
| [14](#14-directoryrepo_root_style-is-ignored-and-the-bold-expression-is-inverted) | Medium | `directory.rs` | `directory.repo_root_style` ignored; `bold` expression inverted |
| [15](#15-command-timing-silently-returns-zero-in-comma-decimal-locales) | Medium | `omarchy10k.bash` | Command timing returns zero in comma-decimal locales |
| [16](#16-kill0-treats-eperm-as-parent-is-dead) | Medium | `main.rs` | `kill(0)` treats EPERM as "parent is dead" |
| [17](#17-shutdown-leaves-the-socket-file-behind) | Low | `server.rs` | `shutdown` leaves the socket file behind |
| [18](#18-osc-777-notification-text-is-injected-unescaped) | Low | `omarchy10k.bash` | OSC 777 notification text injected unescaped |
| [19](#19-quattros-fallback-config-writer-destroys-the-config-file) | Low | `Panel.qml` | Quattro's fallback config writer destroys the config file |
| [20](#20-smaller-confirmed-defects) | Low | various | Seven smaller confirmed defects |

---

## Critical

### 1. Prompt escapes are not marked non-printing, so Bash miscounts prompt width

**Where:** `crates/omarchy10kd/src/render.rs:86-111`, and every `fg_escape()` call
site in `crates/omarchy10kd/src/segments/*.rs`.

**Status:** Verified.

`render_with_ssh` wraps *only* the OSC 133 markers in the `\x01`/`\x02`
non-printing delimiters that Bash's readline requires:

```rust
const OSC_133_PROMPT_START: &str = "\x01\x1b]133;A\x07\x02";
const OSC_133_PROMPT_END: &str = "\x01\x1b]133;B\x07\x02";
```

Every other escape the daemon emits into `left` is bare: the truecolor
foreground sequences from `AnsiColor::fg_escape()`, the `\x1b[1m` bold, every
`\x1b[0m` reset, the OSC 2 window-title sequence, the undercurl pair in
`exit_status`/`character`, and the OSC 8 hyperlink wrapper in `directory`.

The bash adapter assigns this string straight to `PS1`
(`shell/omarchy10k.bash:161`, `:301`), so readline counts all of those bytes as
occupying columns.

Measured against a live daemon (`cols=120`, `exit_code=1`, no OSC 8):

```
left = '\x1b]2;~/Sync/OmarchyMyBash/omarchy10k\x07\x01\x1b]133;A\x07\x02\x1b[38;2;122;162;247m…'

ESC sequences in `left` : 15
\x01/\x02 marker pairs  : 2      (the OSC 133 markers only)

Bash thinks the last prompt line is : 25 columns
It actually renders as              :  2 columns  ('❯ ')
Phantom width                       : 23 columns
```

The wiki already documents the mechanism at `daemon.md:517` — *"The `\x01`/`\x02`
wrappers are Bash-specific non-printing character delimiters"* — it is simply
applied to one escape pair out of fifteen.

**Consequence:** readline believes the cursor sits 23 columns further right than
it does. Long command lines wrap early and at the wrong column, `Ctrl-R` history
recall and `Ctrl-A`/`Ctrl-E` leave visual debris, terminal resize repaints
incorrectly, and multi-line edits corrupt the display. The phantom width scales
with segment count, so it is worst on the wide default `omarchy` layout — and
worse still under Ghostty/Kitty, where `TermCaps.has_osc8` adds an unwrapped OSC 8
hyperlink around the directory segment.

**Note:** the `transient` string has the same defect, and the instant-prompt
cache persists a malformed `PS1` to disk, so the corruption survives into the
next shell's first prompt.

**Fix direction:** wrap at the point of emission — have `fg_escape()`/`bg_escape()`
and the constants for bold, reset, undercurl, OSC 2 and OSC 8 return
`\x01…\x02`-delimited strings, or post-process `left`/`transient` to delimit every
escape run before serializing. The `preview` response must *not* be wrapped
(Quattro strips escapes with `Model.stripAnsi`, which already discards `\x01`/`\x02`,
so either is tolerable there, but keeping preview clean is simpler).

---

### 2. `struct tm` ABI mismatch corrupts the stack when the time segment is enabled

**Where:** `crates/omarchy10kd/src/segments/time.rs:30-80`.

**Status:** Verified.

`local_time()` declares its own `#[repr(C)] struct Tm` with nine `i32` fields and
passes a `MaybeUninit<Tm>` to the libc `localtime_r`:

```rust
#[repr(C)]
struct Tm { tm_sec: i32, tm_min: i32, /* … */ tm_isdst: i32 }   // 36 bytes
```

Both glibc and the macOS libc define `struct tm` with two additional trailing
members — `long tm_gmtoff` and `const char *tm_zone`:

```
real struct tm    = 56 bytes
declared Rust Tm  = 36 bytes
```

`localtime_r` writes all 56 bytes. **20 bytes are written past the end of the
stack slot**, including a pointer-sized value into whatever lives after it.

**Consequence:** undefined behaviour on every prompt render while
`segments.time.enabled = true`. In practice this smashes an adjacent stack slot
in the connection-handling task — silent corruption, wrong values elsewhere, or a
crash, depending on stack layout and optimization level. It does not reproduce in
the unit tests because `test_format_time` only exercises `format_time`, never
`local_time`.

The segment is disabled by default (`config/default.toml:75`), which is the only
reason this has not surfaced — but it is editable from the Quattro Segments tab,
so any user toggling "Time" on turns it on.

**Fix direction:** depend on the `libc` crate and use `libc::tm`, or add the two
trailing fields (`tm_gmtoff: i64, tm_zone: *const c_char`) to the local
declaration. The project already hand-declares `gethostname` and `kill` in
several places; those are safe because they take no struct, but `struct tm` is
not a shape worth hand-rolling.

---

### 3. Two UTF-8 byte-slicing panics on ordinary non-ASCII input

**Status:** Verified — both panics reproduced.

**3a. Branch names** — `crates/omarchy10kd/src/segments/git.rs:163-168`:

```rust
fn truncate_branch(branch: &str, max_len: usize) -> String {
    if branch.len() <= max_len { branch.to_string() }
    else { format!("{}…", &branch[..max_len - 1]) }        // byte slice
}
```

`branch.len()` is a byte count but the slice index is applied as a byte offset.
Called with `max_len` 20 (compact) and 30 (expanded).

```
byte index 19 is not a char boundary; it is inside 'é' (bytes 18..20)
of `feature/naïve-café-ünïcode-branch`
```

**3b. Directory components** — `crates/omarchy10kd/src/segments/directory.rs:128-140`:

```rust
for len in 1..=target.len() {
    let prefix = &target[..len];                            // byte slice
```

`unique_prefix` walks byte offsets `1..=len` to find a disambiguating prefix, so
any non-ASCII path component panics on the first iteration. It is reached from
`smart_truncate`, the default `directory.strategy`, whenever the displayed path
exceeds `directory.max_length` (default 40).

```
byte index 1 is not a char boundary; it is inside 'Ü' (bytes 0..2) of `Übung`
```

**Consequence:** the panic unwinds the per-connection `tokio::spawn` task in
`handle_connection`. The client sees the connection close with no response: the
bridge coprocess blocks until its 2-second `read -t 2` timeout and then falls
back, so the shell hangs for two seconds and renders the plain fallback prompt —
on **every** prompt, for as long as the user stays on that branch or in that
directory. Non-ASCII paths and branch names are entirely ordinary (accented names,
CJK, emoji in branch names).

**Fix direction:** truncate on `char_indices()` boundaries, or use
`chars().take(n)`, in both functions. Note `truncate_branch` should also count
display width rather than chars if the intent is column budgeting.

---

### 4. The daemon exits immediately when `O10K_PARENT_PID` is unset

**Where:** `crates/omarchy10kd/src/main.rs:64-93`.

**Status:** Verified.

```rust
tokio::spawn(async move {
    monitor_parent().await;
    info!("parent process exited, shutting down");
    let _ = std::fs::remove_file(&sock_for_cleanup);
    std::process::exit(0);
});
```

`monitor_parent` returns immediately — not after a wait — when the PID is absent
or unparseable:

```rust
let ppid: u32 = ppid_str.parse().unwrap_or(0);
if ppid == 0 { return; }   // "no parent PID tracking"
```

The caller treats *any* return as "the parent died" and kills the process. The
early return means "we are not tracking a parent", but it is handled as
"the parent is gone".

Reproduced by launching the daemon with no `O10K_PARENT_PID`:

```
$ XDG_RUNTIME_DIR=/tmp/o10ktest ./target/debug/omarchy10kd &
$ sleep 3; pgrep -f omarchy10kd  →  DEAD
$ ls /tmp/o10ktest/             →  omarchy10k-73369.sock   (orphaned)
```

Two distinct problems are visible there. The process is gone within milliseconds,
**and** a stale socket file is left behind: `remove_file` in the shutdown task
races `UnixListener::bind` in `run_server`, and when the unlink loses the race the
socket file outlives the process. A subsequent client then connects to a socket
with nothing listening (`ECONNREFUSED`) rather than getting a clean `ENOENT`.

**Consequence:** the daemon cannot be run standalone at all — not for manual
debugging, not under systemd, not from an editor. The normal path through
`shell/omarchy10k.bash:110` does set `O10K_PARENT_PID=$$`, so day-to-day use is
unaffected; this bites anyone following the wiki's own debugging guidance or
trying to run the daemon outside the adapter.

**Fix direction:** have `monitor_parent` return a `bool` (or make it
`std::future::pending().await` when untracked) so the "not tracking" case never
triggers shutdown. Separately, remove the socket only after the listener is
bound, or drop the listener before unlinking.

---

## High

### 5. Every environment-derived segment is frozen at daemon start

**Where:** `segments/python_env.rs`, `toolchain.rs`, `nix.rs`, `container.rs`,
`k8s.rs`, `ssh.rs`, `render.rs:57-60`, `render.rs:204-220`, `terminal.rs:27-37`.

**Status:** By inspection (systemic; follows directly from the process model).

Seven segments read their state from `std::env::var` **inside the daemon
process**:

| Segment | Reads |
|---------|-------|
| `python_env` | `VIRTUAL_ENV`, `CONDA_DEFAULT_ENV` |
| `toolchain` | `MISE_NODE_VERSION`, `MISE_PYTHON_VERSION`, `MISE_RUBY_VERSION`, `MISE_GO_VERSION`, `MISE_RUST_VERSION` |
| `nix` | `IN_NIX_SHELL` |
| `container` | `DISTROBOX_ENTER_PATH`, `container` |
| `k8s` | `KUBECONFIG` |
| `ssh` | `SSH_TTY`, `SSH_CONNECTION` |
| title / directory | `HOME`, `USER`, hostname |

The daemon is spawned **once**, at shell startup (`shell/omarchy10k.bash:421`),
and lives for the whole session. It inherits the shell's environment as it was at
that moment and never sees it again. The `prompt` protocol message carries `cwd`,
`exit_code`, `cmd_duration_ms`, `cols`, `jobs` and `shell_integration` — no
environment.

**Consequence:** the four segments the wiki calls "v0.3 Context Segments" do not
respond to context.

- `source .venv/bin/activate` → the Python segment never appears.
- `mise` switching tool versions on `cd` → the toolchain segment never changes.
- `nix develop` / `nix-shell` → the Nix segment never appears.
- Entering a distrobox/toolbox from an existing shell → no container segment.
- `export KUBECONFIG=…` → the k8s segment keeps reading the old file.

Each of these only ever shows the state that happened to exist when the shell
started, which for a login shell is "none". `container` and `ssh` are largely
fine in practice because those conditions are true before the shell starts;
`python_env`, `toolchain` and `nix` are the ones users actively change and are
effectively non-functional.

This is not a small oversight to patch in one line — it is a protocol gap. Worth
recording as the top design item for v0.4.

**Fix direction:** add an `env` object to the `prompt` request carrying the small
allowlist of variables the segments need, populate it in the bash adapter (it is
a fixed set, so the cost is a handful of parameter expansions, no forks), and
have `SegmentContext` read from it instead of `std::env`. Bump `PROTOCOL_VERSION`
and keep `std::env` as the fallback for older clients.

---

### 6. The daemon is never restarted once it dies

**Where:** `shell/omarchy10k.bash:264-270`.

**Status:** By inspection.

```bash
if [[ -S "$__O10K_SOCKET" ]] && ! kill -0 "$__O10K_BRIDGE_PID" 2>/dev/null; then
    if ! __o10k_socket_send '{"command":"status"}' >/dev/null 2>&1; then
        rm -f "$__O10K_SOCKET"
        __o10k_start_daemon
        __o10k_start_bridge
    fi
fi
```

The entire recovery path is gated on `[[ -S "$__O10K_SOCKET" ]]` — the socket
still existing. But the daemon's own shutdown path **unlinks the socket on the
way out** (`main.rs:70`), and so does `__o10k_stop_daemon`. So in the common
failure — the daemon crashes or exits cleanly and removes its socket — the guard
is false, no restart is attempted, the `[[ -S … ]]` test at line 272 also fails,
and the function falls through to `PS1="$__O10K_FALLBACK_PS1"`.

The shell then renders the plain fallback prompt for the rest of its life. There
is no retry on any later prompt, and no message telling the user what happened.
The first-run hint at line 120 is one-shot and gated behind a marker file, so it
does not fire either.

The condition is also inverted with respect to the bridge: recovery runs *only
when the bridge is dead*. A live bridge holding a connection to a dead daemon is
never noticed here — it is handled by `bridge.rs`'s own reconnect, which retries
three times against a socket that no longer exists and then emits its fallback
forever.

**Fix direction:** invert the guard — attempt recovery when the socket is
*missing* or the status probe fails, not only when it exists. Rate-limit the
restart (e.g. at most one attempt every N seconds) so a genuinely broken install
does not spawn a daemon per prompt.

---

### 7. The CLI cannot find the daemon socket

**Where:** `crates/omarchy10k/src/main.rs:80-86` vs `shell/omarchy10k.bash:18,110`.

**Status:** By inspection.

The adapter names the socket after the shell's own PID:

```bash
__O10K_SOCKET="${__O10K_SOCKET_DIR}/omarchy10k-$$.sock"
O10K_PARENT_PID=$$ "$__O10K_DAEMON_BIN" &>/dev/null &
```

`O10K_PARENT_PID=$$ cmd` is a per-command environment prefix. It is visible to
the daemon and to nothing else — it is never `export`ed into the shell.

The CLI resolves the same path independently:

```rust
let ppid = std::env::var("O10K_PARENT_PID")
    .or_else(|_| std::env::var("PPID"))
    .unwrap_or_else(|_| "0".into());
```

From an interactive shell, `O10K_PARENT_PID` is unset (never exported) and `PPID`
is a Bash *shell* variable, not an environment variable, so it is not exported
either. Both lookups fail and the CLI computes
`$XDG_RUNTIME_DIR/omarchy10k-0.sock`, which never exists.

**Consequence:** `omarchy10k reload`, `omarchy10k debug`, `omarchy10k prompt` and
`omarchy10k benchmark` all fail to reach the running daemon from a normal shell.
`omarchy10k prompt` degrades quietly to the hardcoded fallback prompt (masking the
failure); `reload` and `debug` error out. `omarchy10k doctor` is unaffected because
`check_daemon` scans the runtime directory for `omarchy10k-*.sock` rather than
using `socket_path()` — which is also why doctor reports the daemon as healthy
while `reload` fails, a confusing pair of signals.

The hot path is unaffected: the adapter passes `--socket "$__O10K_SOCKET"`
explicitly to `omarchy10k bridge`.

**Fix direction:** `export O10K_PARENT_PID=$$` in the adapter (cheapest, and makes
the documented env var actually work as documented in `glossary.md`), and/or have
the CLI fall back to the doctor's directory-scan when the computed path is absent.

---

### 8. The ble.sh version gate never passes

**Where:** `shell/omarchy10k.bash:343`.

**Status:** By inspection.

```bash
if [[ -n "${BLE_VERSION:-}" ]] && (( ${BLE_VERSION%%.*} >= 4 )); then
    __o10k_install_blesh_hooks
```

ble.sh versions are `0.3.x` and `0.4.x` — `BLE_VERSION` looks like
`0.4.0-devel4`. `${BLE_VERSION%%.*}` strips everything from the first `.`, giving
`0`. The test is `0 >= 4`, which is false for every ble.sh release that has ever
existed. The check appears to intend "ble.sh 0.4 or newer" but compares against
the major version, which is always `0`.

**Consequence:** `__o10k_install_blesh_hooks` is unreachable. Every ble.sh user
silently gets the vanilla `PROMPT_COMMAND` + `DEBUG` trap path instead of ble.sh's
native `blehook PRECMD`/`PREEXEC`/`CHPWD`. Three advertised features are lost:

- `bleopt prompt_ps1_transient='always'` is never set → **transient prompt never
  works**, despite `prompt.transient = true` being the default and the daemon
  dutifully rendering a `transient` string on every response.
- `blehook CHPWD` → chpwd falls back to the polling emulation in
  `__o10k_check_chpwd`.
- `__o10k_update_rps1` is never registered → see finding 9.

`omarchy10k doctor` compounds the confusion by reporting
`ble.sh 0.4.0-devel4 ✓ enhanced mode available` whenever `BLE_VERSION` is set —
it checks a different condition than the adapter does.

**Fix direction:** compare against the minor version
(`${BLE_VERSION#*.}` → strip to `%%.*`), or simply test
`declare -F blehook >/dev/null`, which is what the code actually depends on.

---

### 9. The right prompt duplicates content already in the left prompt

**Where:** `crates/omarchy10kd/src/render.rs:143-179` vs
`segments/mod.rs:94-98` and `segments/git.rs`.

**Status:** Verified.

`render_right` builds the right prompt from command duration and git branch —
both of which `collect_segments` has already placed in the left prompt:

```
left  : ' ~/Sync/OmarchyMyBash/omarchy10k  main !23 ?1 9.0s\n❯ '
right : '9.0s  main'
```

`9.0s` and `main` each appear twice. There is no filtering between the two paths:
`render_right` re-derives from `ctx` rather than consuming what the layout engine
did not place.

Compounding this, `right` is currently unreachable for almost everyone. The
adapter stores it in `__O10K_LAST_RIGHT` (`:162`, `:302`) but the only consumer is
`__o10k_update_rps1`, which sets `bleopt prompt_rps1` — and that hook is only
registered inside `__o10k_install_blesh_hooks`, which never runs (finding 8).
Vanilla Bash has no right-prompt mechanism and the adapter does not emulate one.

So `prompt.right_prompt = true` (the default) currently costs a rendering pass per
prompt and produces a string that is displayed to nobody — and the moment finding
8 is fixed, ble.sh users will see the duplication.

**Fix direction:** decide which side owns duration and branch. Simplest is to have
`render_right` take the set of segments the layout engine dropped, or to filter
`command_duration`/`git` out of the left layout when `prompt.right_prompt` is on.
Fix in the same change as finding 8 so the duplication never reaches users.

---

## Medium

### 10. `git.cache_ttl_ms` is frozen at startup

**Where:** `crates/omarchy10kd/src/server.rs:79-88`, `:90-96`.

`GitCache::new(git_ttl_ms)` is called once in `DaemonState::new`, capturing
`config.git.cache_ttl_ms` into `GitCache::ttl`. `reload_config` replaces the
`Config` behind the `RwLock` but never rebuilds or updates the cache:

```rust
pub async fn reload_config(&self) -> anyhow::Result<()> {
    let new_config = Config::load(&self.config_path)?;
    let mut config = self.config.write().await;
    *config = new_config;            // git_cache.ttl untouched
```

Editing `cache_ttl_ms` in `config.toml`, via `omarchy10k reload`, or from Quattro
appears to succeed — the daemon logs `config reloaded` and `config_get` returns the
new value — but git status keeps using the old TTL until the shell is restarted.
`config.md:87` documents this key as fully implemented ("Yes").

**Fix direction:** make `GitCache::ttl` an `RwLock<Duration>`/atomic and update it
in `reload_config`, or read the TTL from `state.config` at `get_status` time.

---

### 11. The instant-prompt cache races between concurrent shells

**Where:** `shell/omarchy10k.bash:21-26`, `:163`, `:303`.

Every shell writes the cache through the **same fixed temp path**, on every
prompt, in a backgrounded subshell:

```bash
__O10K_CACHE="${__O10K_CACHE_DIR}/last_prompt"
…
{ printf '%s' "$PS1" > "$__O10K_CACHE.tmp" && mv "$__O10K_CACHE.tmp" "$__O10K_CACHE"; } 2>/dev/null &
```

With N terminals open, N processes interleave writes to `last_prompt.tmp`. The
`mv` is atomic but the `printf` into the shared temp file is not, so a shell can
rename a file another shell is midway through writing — publishing a truncated or
spliced `PS1` (containing partial escape sequences) as the cached instant prompt.
The next shell to start reads it with `PS1=$(<"$__O10K_CACHE")`.

Two secondary issues in the same lines: the `&` spawns a subshell per prompt (a
fork in a path the wiki describes as fork-free, plus job-table noise), and the
cache is global rather than per-directory, so a new shell's instant prompt shows
the *last* shell's working directory until the first real render lands.

**Fix direction:** include `$$` in the temp name (`"$__O10K_CACHE.$$.tmp"`) so each
shell renames its own file. Consider writing at most once per N seconds rather
than per prompt.

---

### 12. A type-less prompt request carrying `command` is misrouted to the control handler

**Where:** `crates/omarchy10kd/src/server.rs:58-70`, `:466-481`, `:22-23`.

**Status:** Verified.

`PromptRequest` declares a `command` field for the command text:

```rust
pub struct PromptRequest {
    …
    #[serde(default)] pub command: Option<String>,
```

`TypedMessage` also declares `command` at the top level, ahead of
`#[serde(flatten)] rest`. Named fields win over `flatten`, so `command` is
consumed by `TypedMessage` and never reaches `rest` — making
`PromptRequest.command` permanently `None` for typed messages, and dead code.

For **type-less** messages the fallback router keys on the same field:

```rust
None => { if let Some(ref cmd) = msg.command { handle_control(cmd, …) } … }
```

so a legacy prompt request that includes the command text is dispatched as a
control command:

```
→ {"cwd":"/tmp","exit_code":0,"cmd_duration_ms":0,"cols":120,"jobs":0,"command":"ls -la"}
← {"error":"unknown command: ls -la","status":"error","type":"control"}
```

The same payload without `command` renders correctly. `protocol.md:395` documents
the rule that causes this — *"`command` field present → Control command"* — which
is exactly the collision.

This is latent today: the bash adapter does not send `command`. It becomes a live
bug the moment anything populates the field the struct advertises (a
notification body, a title `{command}` placeholder, per-command timing).

**Fix direction:** rename the prompt field (e.g. `cmdline`), or route on the
presence of `cwd` before falling back to `command`. `TypedMessage.version` is
unused dead code as well (`cargo build` flags it).

---

### 13. Home-prefix substitution is not path-aware

**Where:** `crates/omarchy10kd/src/render.rs:93-97`,
`segments/directory.rs:10-14`.

```rust
let display_path = if !home.is_empty() && path.starts_with(home) {
    format!("~{}", &path[home.len()..])
```

This is a raw string-prefix test, not a path-component test. With
`HOME=/home/ian`, the path `/home/ian2/src` starts with `/home/ian`, so it renders
as `~2/src` — a path that does not exist and reads as nonsense. Any sibling home
directory whose name extends the user's own triggers it (`/home/ian` vs
`/home/iana`, `/Users/bob` vs `/Users/bobby`).

Both call sites have the identical bug, and `directory.rs`'s unit test
(`test_home_substitution`) reimplements the logic inline rather than calling the
function, so it would not catch a fix regressing either site.

**Fix direction:** require the match to end at a component boundary — compare
`Path::strip_prefix`, or check that the remainder is empty or starts with `/`.
Factor the logic into one shared helper and have the test call it.

---

### 14. `directory.repo_root_style` is ignored and the `bold` expression is inverted

**Where:** `crates/omarchy10kd/src/segments/directory.rs:26,31-32,78`.

The config value is threaded into `smart_truncate` and then discarded — the
parameter is named `_repo_root_style` and never read:

```rust
fn smart_truncate(path: &str, max_len: usize, _repo_root_style: &str) -> String {
```

The only other use is this expression:

```rust
let bold = strategy != "truncate" || ctx.config.directory.repo_root_style == "bold";
```

Because `||` short-circuits, `bold` is unconditionally `true` for the `smart` and
`full` strategies regardless of `repo_root_style`; the setting only has any effect
under `truncate`. Setting `repo_root_style = "none"` does not unbold anything on
the default strategy. The intent was presumably
`repo_root_style == "bold"` alone.

`config.md:74` documents this key as implemented ("Yes"). It is not.

**Fix direction:** either implement it (bold only the repo-root component, which
is what the name promises and what `smart_truncate` already detects at line 108),
or mark it unimplemented in `config.md`. Fixing the docs is done in this pass.

---

### 15. Command timing silently returns zero in comma-decimal locales

**Where:** `shell/omarchy10k.bash:204-226`.

```bash
local now="${EPOCHREALTIME:-$(date +%s%N)}"
if [[ "$now" == *.* ]]; then
    …compute duration…
else
    __O10K_CMD_DURATION=0
fi
```

Bash formats `EPOCHREALTIME` using the current `LC_NUMERIC` decimal separator. In
`de_DE`, `fr_FR`, `es_ES`, `pt_BR` and many others it is `1756330000,123456` — a
comma. The `*.*` test fails and the duration is silently set to `0` on every
command.

**Consequence:** for users in comma-decimal locales the `command_duration`
segment never appears, the right-prompt duration never appears, and long-running
command notifications (`segments.notification`) never fire — the threshold
comparison is always `0 > 10000`. All three features appear simply "not to work",
with no error.

**Fix direction:** accept either separator (`[[ "$now" == *[.,]* ]]` and split on
the same class), or read the time under `LC_ALL=C`.

---

### 16. `kill(0)` treats EPERM as "parent is dead"

**Where:** `crates/omarchy10kd/src/main.rs:88`.

```rust
let alive = unsafe { libc::kill(ppid as i32, 0) == 0 };
```

`kill(pid, 0)` returns `-1` both when the process does not exist (`ESRCH`) and
when it exists but the caller may not signal it (`EPERM`). The code treats any
non-zero return as death and exits the daemon.

PID reuse is the realistic trigger: the shell exits, its PID is recycled by a
process owned by another user (or by root), and the daemon — which would have
correctly exited anyway — exits for the wrong reason. More importantly it makes
the daemon un-runnable under any supervisor that hands it a parent PID it cannot
signal, which is how finding 4's reproduction failed on the first attempt
(`O10K_PARENT_PID=1` → `kill(1,0)` → `EPERM` → immediate exit).

**Fix direction:** distinguish `ESRCH` from `EPERM` via `errno`, treating only
`ESRCH` as death. Better still, on Linux use a pidfd or simply detect the socket
peer disconnecting.

---

## Low

### 17. `shutdown` leaves the socket file behind

`server.rs:181-185` responds `{"status":"bye"}` and calls `std::process::exit(0)`
without unlinking the socket. `__o10k_stop_daemon` does `rm -f` afterwards so the
adapter path is covered, but a `shutdown` sent from the theme hook, from Quattro,
or by hand orphans the socket file. The next client to try that path gets
`ECONNREFUSED` rather than `ENOENT`, and `omarchy10k doctor` reports the socket as
`✘ unresponsive` rather than absent. `run_server` does clean up a stale socket at
next bind, so this is cosmetic until something reads the socket list — which
`doctor`, `hooks/theme-set` and both Quattro socket-discovery paths all do.

### 18. OSC 777 notification text is injected unescaped

`shell/omarchy10k.bash:252-255`:

```bash
printf '\033]777;notify;Command finished;%s took %dms\007' "${__O10K_LAST_CMD:-command}" …
```

`__O10K_LAST_CMD` is the raw `$BASH_COMMAND`. A command containing `;` splits the
OSC parameter list; one containing `\a` or `\e` terminates or restarts the
sequence, letting arbitrary command text emit escape sequences to the terminal.
Running a command with a semicolon in a quoted argument is enough to garble the
notification. Strip or percent-encode control characters and `;` before
interpolating.

The adjacent OSC 7 emission (`:288`, `:311`, `:319`) has a milder version of the
same issue: `$PWD` is not percent-encoded, so paths containing spaces or `#`
produce a malformed `file://` URI.

### 19. Quattro's fallback config writer destroys the config file

`quattro/Panel.qml:166-181`. When the panel is not connected to a daemon,
`_flushSave` reconstructs `config.toml` from the flat parse:

```qml
var toml = Model.buildTOML(root._configFlat)
configWriter.exec(["sh", "-c", "mkdir -p '…' && printf '%s\\n' '" + escaped + "' > '" + _configPath + "'"])
```

`Model.parseTOML` keeps only `section.key = scalar` pairs — it strips every
comment and cannot represent the nested `[theme.custom]` table (`parseValue`
returns bare strings for anything it does not recognize). Round-tripping through
it therefore rewrites the user's `config.toml` with all comments gone and any
custom palette silently dropped. The shell quoting itself is correct
(`'` → `'\''`), so this is data loss rather than injection.

The daemon-connected path (`config set`) is safe — `server.rs:395-427` parses the
existing TOML, merges, and refuses to overwrite a file it cannot parse.

Note also that `Model.collectConfig` sends **every** key in `CONFIG_MAP` on each
save, not just changed ones. If a save fires before `config_get` has returned, the
panel's property defaults are written over the user's real settings.

### 20. Smaller confirmed defects

- **`kill -0 "$__O10K_BRIDGE_PID"` with PID `0`** (`omarchy10k.bash:264`)
  — `__O10K_BRIDGE_PID` is initialized to `0`, and `kill -0 0` signals the
  caller's *process group*, which always succeeds. Before the bridge first
  starts, the adapter therefore believes it is alive. Lines 142, 152 and 408
  guard with `(( __O10K_BRIDGE_PID > 0 ))` first; line 264 does not.
- **Git segment renders an empty branch on a cold cache miss**
  (`git.rs:96-101` + `segments/git.rs:73-78`). The cold-miss placeholder sets
  `is_repo: true` with an empty `branch`, so the first prompt in a new repo shows
  a leading separator and a bare `✓` with no branch name. `render_right` guards
  with `!branch.is_empty()`; the segment does not.
- **`detect_worktree` misidentifies submodules** (`git.rs:156-177`). It reports a
  worktree whenever `.git` is a *file*, but submodules also use a `gitdir:`
  pointer file. Every submodule is labelled with a worktree marker in the prompt.
- **`battery.show_above = 100` hides a full battery** (`battery.rs:11-14`). The
  test is `capacity >= show_above`, so at exactly 100% the segment disappears.
  `config.md:216` says the default "shows whenever a battery is detected".
  Corrected in this pass.
- **`Segment::display_width` counts escape bytes** (`layout.rs:19-21`). It runs
  `UnicodeWidthStr::width` over `content`, which for `directory` includes the OSC 8
  hyperlink and for `exit_status` the undercurl pair. It is currently only reached
  as the `compact_width` fallback for segments that always set `compact_content`,
  so it is unreachable today — but it is a trap for the next segment added
  without a compact form.
- **Background jobs are counted with a fork** (`omarchy10k.bash:275`).
  `jobs_count=$(jobs -p 2>/dev/null | wc -l)` forks a subshell and execs `wc` on
  every prompt, in the path the wiki describes as fork-free. `jobs -p` into an
  array and using `${#arr[@]}` avoids both.
- **Dead config keys.** `git.max_threads`, `daemon.socket` and
  `terminal.progress.enabled` are parsed and never read anywhere outside
  `config.rs` — all three are already documented as `No`/`Partial` in `config.md`,
  which is accurate. `git.stale_display` is likewise never read, though the
  `stale` flag it nominally controls *is* honoured unconditionally in both
  `segments/git.rs:34` and `render.rs:167`; the key is a no-op switch on
  always-on behaviour.

---

## Not bugs (checked and cleared)

Recorded so a later audit does not re-investigate them:

- `merge_toml_value` (`server.rs:496`) correctly deep-merges tables and replaces
  scalars; the `config set` write is atomic (tmp + rename) and refuses to
  overwrite an unparseable file.
- `parse_porcelain_v2` handles the `1`/`2`/`u`/`?` record types and `# branch.*`
  headers correctly, including detached HEAD.
- `Model.protocolAtLeast` compares dotted versions correctly, including
  differing component counts.
- `GitCache` in-flight deduplication is sound — the `in_flight` set is checked and
  inserted under a single write lock.
- The `__o10k_timer_stop` microsecond arithmetic handles the borrow across a
  second boundary correctly (negative microsecond deltas included). Its only
  defect is the locale issue in finding 15.
- `k8s.rs`'s reverse scan for `namespace:` correctly stops at the enclosing
  `- context:` entry boundary.
- The DEBUG-trap preexec guard (`__O10K_PREEXEC_READY`) correctly suppresses
  firing for `PROMPT_COMMAND`'s own commands, and DEBUG is not inherited into
  functions without `set -T`.

## Verification notes

Built and tested at audit time:

```
cargo build   → ok (10 dead-code warnings)
cargo test    → 13 passed; 0 failed
```

The existing 13 unit tests cover pure formatting helpers (`format_duration`,
`format_exit_code`, `format_time`, `parse_porcelain_v2`, hex parsing, palette
resolution). None of the findings above is covered by a test, and findings 1, 3,
9, 12 and 13 are each directly testable without a running daemon — that is the
cheapest place to start.
