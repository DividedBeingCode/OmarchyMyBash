# Extension Model (Plugins + Custom Segments + Segment Ordering): Design

Date: 2026-08-28
Status: Approved direction, pending implementation plan
Scope: `crates/omarchy10kd` (new `plugins.rs`, `segments/mod.rs`, `layout.rs`, `config.rs`, `main.rs` watchers, `server.rs` control verbs), `crates/omarchy10k` (new `plugin.rs` CLI verb family, `main.rs`, `doctor.rs`, `init` bundle inlining), `shell/omarchy10k.bash` (bundle inlining point, completion paths), `config/default.toml`, `quattro/Panel.qml` (Plugins list + reorderable segment list), `tests/`, wiki.
Protocol: **additive to 0.5** — two new control verbs, no render-path message changes, no version bump required.

---

## 0. Orientation for an implementing agent

Read these before touching code. This spec assumes them.

| Page | Path | Why |
|---|---|---|
| Architecture | `docs/wiki/architecture.md` | System topology, segment plugin architecture, data flow |
| Daemon | `docs/wiki/daemon.md` | Module responsibilities in `omarchy10kd` |
| Protocol | `docs/wiki/protocol.md` | NDJSON message contracts, control verbs |
| Configuration | `docs/wiki/config.md` | Every config key, type, default, implementation status |
| Bash Adapter | `docs/wiki/bash-adapter.md` | Hook broker, env channel, bridge coproc, daemon lifecycle |
| CLI | `docs/wiki/cli.md` | Subcommand surface, doctor |

Update the wiki after implementation — see `.cursor/rules/wiki-maintenance.mdc`.

### What already exists that this design builds on

These are load-bearing prior art. Do not reinvent them.

| Existing thing | Location | How this design uses it |
|---|---|---|
| **Env channel** (protocol 0.4) | `shell/omarchy10k.bash` `__o10k_env_json()` (~line 438); allowlist in `config/default.toml` `[env.watch] keys` | Tier-1 plugin segments ride this. Zero forks per prompt. Plugin-declared `env` keys are unioned into the allowlist. |
| **`SegmentContext::env_get()`** | `crates/omarchy10kd/src/segments/mod.rs:47` | Prefers the request's env channel, falls back to process env. Tier-1 segments read through this unchanged. |
| **`collect_segments()`** | `crates/omarchy10kd/src/segments/mod.rs:57` | Where plugin-provided segments join the built-in segment list. |
| **`GitCache`** | `crates/omarchy10kd/src/git.rs:66` | The structural model for `CustomCache` (tier-2). TTL + keyed entries + never-block-the-render. Copy its shape. |
| **`git_stale` / stale rendering** | `crates/omarchy10kd/src/segments/git.rs`, `[git] stale_icon` in `config/default.toml` | The precedent and the visual treatment for "value not ready yet". Tier-2 reuses this contract exactly. |
| **`resolve_right_rail()`** | `crates/omarchy10kd/src/layout.rs:224` | The exact pattern `resolve_left_rail()` must mirror — name list in, typed segments out, unknown names skipped with `tracing::debug!`. |
| **`prompt.right_segments`** | `crates/omarchy10kd/src/config.rs:133` (default at `:144`) | The config shape `left_segments` mirrors. |
| **`Layout::segment_order(preset)`** | `crates/omarchy10kd/src/layout.rs:164` | Currently the authority on left-rail order. Becomes the *default* only. |
| **Hook broker** (`o10k_hook_add`) | `shell/omarchy10k.bash` (~line 95) | Plugin bash code registers through this so plugins compose instead of clobbering `PROMPT_COMMAND`. |
| **Config load + watcher** | `crates/omarchy10kd/src/config.rs:716` `load()`, `:736` `config_path()`; `crates/omarchy10kd/src/main.rs:157` `run_watchers()` | Registry loads alongside config. **Note:** the config watcher is `RecursiveMode::NonRecursive` on the config dir (`main.rs:175`) — `plugins/` is a subdirectory and needs its own watch. |
| **Control verb dispatch** | `crates/omarchy10kd/src/server.rs` (`reload_config` :280, `config_get` :508, `config_set` :517, `looks` :305) | Where the two new verbs go, and the merge/reload path `plugins_set` routes through. |
| **Omarchy plugin security posture** | `README.md` install section | Omarchy plugins land *disabled* so the user can review code before enabling. This design mirrors that posture deliberately. |

---

## 1. Context and motivation

Omarchy10k's README claims two lineages: Powerlevel10k's prompt intelligence and Oh My Zsh's batteries-included convenience. Only the first is built.

There is currently **no extension mechanism of any kind**. `config/tools.sh` is a static, installer-owned file of ~15 aliases. A user or third party cannot add a segment, an alias bundle, a completion, or a keybinding and have it be discoverable or toggleable. Oh My Zsh's entire value proposition is `plugins=(git docker kubectl)`.

The WASM plugin runtime was ratified onto the Kill List (`docs/wiki/v04-feature-intel.md`) on cost grounds: a forever-versioned plugin ABI, wasmtime binary bloat, and an arbitrary-code-execution surface. **That verdict killed the implementation, not the need.** This design delivers the need through mechanisms with none of those costs: declarative segment definitions (data, not code) and shell bundles (code, but at exactly the trust level `.bashrc` already has).

This subsystem is also load-bearing for downstream work:

- It grows the segment catalog (currently 18 segments vs P10k's ~100) **without** bloating the daemon.
- It forces left-rail segment ordering to become user-configurable, which is the blocker the Wave 4 kill list already identified behind "segment ordering (doesn't exist)".
- The reorderable segment list is the Control Center's headline missing feature.

### Decisions already made (do not re-litigate)

| Decision | Choice | Rationale |
|---|---|---|
| Plugin scope | Segments **and** shell bundle (aliases/functions/completions/keybindings) | The actual Oh-My-Zsh shape; what the README claims. |
| Segment execution | **Two tiers**, declared per segment: env-channel (zero fork) and daemon-side async command | Covers both cheap and expensive segments while keeping the sub-5ms guarantee absolute. |
| Left-rail ordering | **Config list** `[prompt] left_segments`, mirroring `right_segments` | Consistent with existing code, renders directly as a reorderable UI list, matches P10k's `LEFT_PROMPT_ELEMENTS` mental model. |
| Bash load model | **Compiled bundle inlined into `init bash`** | One eval regardless of plugin count. Avoids Oh My Zsh's cardinal sin (per-plugin file reads at every shell start). |
| Registry location | **Filesystem, read independently by daemon and CLI** | `init bash` runs *before* any daemon exists, so a daemon-owned registry is a chicken-and-egg. Also satisfies design principle 6, "files are the source of truth". |

Rejected alternatives, recorded so they aren't retried: daemon-owned registry (init-ordering deadlock); CLI-owned registry (loses hot-reload on plugin change); a `when = "<expression>"` string (see §3); `sh -c` command strings (see §3); lazy trigger-based bash loading (stub functions and `command_not_found_handle` games are a known source of subtle breakage).

## 2. Non-goals

- **`omarchy10k plugin add <git-url>`** and any remote catalog. That is a trust, versioning, and distribution design and it gets its own spec.
- Sandboxing plugin bash. Explicitly not claimed; see §11.
- WASM, or any compiled plugin ABI. Killed, stays killed.
- Drag-and-drop segment canvas in the panel. A plain reorderable list only.
- Per-segment drill-in settings UI in the panel.
- `carapace`, ble.sh `complete_menu` / `sabbrev`, or ble.sh face theming. These belong to the **theming / shell-UX wave** (a sibling spec) and must not be smuggled in here. §6 only creates the seam they will later use.
- Changing any render-path protocol message.

---

## 3. On-disk shape and `plugin.toml` schema

```
~/.config/omarchy10k/plugins/<name>/
  plugin.toml        # metadata + segment definitions   (daemon reads)
  plugin.bash        # aliases, functions, keybindings   (CLI compiles)
  completions/       # bash completion files             (CLI wires)
```

All three files are optional. A plugin may be segments-only (`plugin.toml` alone), shell-only (`plugin.bash` alone), or both.

```toml
# config.toml
[plugins]
enabled = ["git-aliases", "aws", "node"]
```

**Presence on disk means *available*. Presence in `enabled` means *active*.** Dropping a directory in never activates it.

### `plugin.toml`

```toml
[plugin]
name = "aws"
description = "AWS profile and region"
requires = []                    # binaries that must exist on PATH; else plugin is inert

# ── Tier 1: zero-fork, reads the env channel ──────────────────
[[segment]]
name       = "aws"
tier       = "env"
env        = ["AWS_PROFILE", "AWS_REGION"]   # unioned into [env.watch] keys
detect_env = ["AWS_PROFILE"]                 # show only when set and non-empty
format     = "☁ {AWS_PROFILE}"
style      = "yellow"                        # palette role name

# ── Tier 2: daemon-side, async, TTL-cached ────────────────────
[[segment]]
name         = "package"
tier         = "command"
command      = ["jq", "-r", ".version", "package.json"]   # argv array, NOT a shell string
detect_files = ["package.json"]              # gate evaluated BEFORE any fork
ttl_ms       = 30000
timeout_ms   = 2000
format       = "󰏗 {}"                        # {} is the command's captured output
style        = "muted"
```

### Two schema decisions that matter

**1. `command` is an argv array, never a shell string.** No `sh -c`. This eliminates quoting bugs, word-splitting surprises, and injection surface in one stroke.

**2. There is no `when` expression language.** Visibility is expressed through declarative predicates evaluated in-daemon with zero forks:

| Predicate | Semantics |
|---|---|
| `detect_env` | Every named var is present in the env channel and non-empty |
| `detect_files` | Any named file exists in cwd, or in an ancestor up to the repo root |
| `detect_extensions` | Any file in cwd has one of these extensions |
| `detect_folders` | Any named directory exists in cwd |

Multiple predicates on one segment are ANDed. This is Starship's proven `detect_*` model. An expression string was rejected because it is a language: you grow it, debug it, version it, and users write things it cannot parse. Predicates are data.

**`detect_*` gates run before any fork.** A tier-2 segment in a non-matching directory costs zero.

### Style and format

`style` names a palette role (`accent`, `foreground`, `muted`, `red`, `green`, `yellow`, `blue`) — never a hex value. Plugin segments must re-theme for free like every built-in segment. Reject hex in `style` at parse time with a `warn!`.

`format` substitutes `{VAR_NAME}` for tier-1 env values and bare `{}` for tier-2 captured output.

---

## 4. Daemon side

### 4.1 Registry (`crates/omarchy10kd/src/plugins.rs`, new)

- On config load, walk `~/.config/omarchy10k/plugins/`, parse each `plugin.toml`, filter to the names in `[plugins] enabled`, and produce `Vec<PluginSegmentDef>`.
- Malformed TOML, unknown keys, or an invalid `tier` → **skip that segment (or plugin) with `warn!`, never fail the config load.** A broken plugin must not take down a shell.
- A plugin whose `requires` binaries are absent from PATH is **inert**: its segments never render, it is reported as inert by `plugin list` and `doctor`.
- The registry rebuilds on `reload_config`.
- **Watcher change required:** `run_watchers()` (`main.rs:157`) watches the config dir with `RecursiveMode::NonRecursive` (`main.rs:175`). Add an explicit watch on the `plugins/` subdirectory so enable/disable/edit triggers a reload.

### 4.2 Tier 1 — env segments

Costs nothing new at render time.

- Plugin-declared `env` keys are unioned into the effective `[env.watch]` allowlist. **The adapter and the daemon must agree on this list**, which today is a frozen constant hardcoded on both sides. Resolution: the daemon writes the effective allowlist through the existing side-channel flags file that already relays `semantic_prompts` and `notify_unfocused_only`; the adapter reads it there. Do not hardcode plugin keys into the adapter.

  **Implementation note for the adapter:** `__o10k_env_json()` currently emits a fixed sequence of literal parameter expansions to stay fork-free. A dynamic key list requires iterating the list and using indirect expansion (`${!key}`), which is **also fork-free** — this does not compromise the zero-subprocess guarantee. Preserve that guarantee; it is the reason tier 1 exists.
- Rendering is `detect_*` evaluation plus format-string substitution against `SegmentContext::env_get()`.

### 4.3 Tier 2 — command segments

This is the part that must not break the sub-5ms brand.

```rust
CustomCache: HashMap<(SegmentName, Cwd), Entry {
    value: String,
    computed_at: Instant,
    generation: u64,
}>
```

Modelled directly on `GitCache` (`git.rs:66`). Behavior on render:

1. Evaluate `detect_*`. Not matched → segment absent. **No fork.**
2. Fresh cache entry (within `ttl_ms`) → render it.
3. Stale or missing entry → **return absent, or the stale value in muted style when `[git] stale_display`-equivalent behavior is on, reusing the `git_stale` visual treatment — then spawn a Tokio task to recompute.**

**The render path never awaits a command. Ever.** This is a hard invariant, not a goal.

- **In-flight dedupe:** at most one task per `(segment, cwd)` key, so a fast prompt loop cannot spawn a process storm.
- Hard `timeout_ms` (default 2000) enforced with `tokio::time::timeout`; on expiry the child is killed.
- Output capped to the **first line and 256 bytes**; trailing newline trimmed.
- Non-zero exit → segment absent, logged at debug.
- Working directory is the request's `cwd`. Environment is the env channel's contents plus the daemon's own — not the full user environment, which the daemon does not have.

**The documented contract, stated plainly in the wiki:** *a `command` segment may be blank on its first appearance in a directory and correct on every subsequent prompt.* This is the same trade `git_stale` already makes, and it is the price of an absolute latency guarantee.

### 4.4 Joining the segment list

Plugin segments are appended to the built-in list in `collect_segments()` (`segments/mod.rs:57`), then ordered by §5.

**Namespacing.** Plugin segments are referenced in `left_segments` with a leading `@` (see §5), so the plugin and built-in namespaces are disjoint by construction — a plugin may legally declare `name = "git"` without shadowing the built-in git segment.

The collision that *can* occur is **two enabled plugins declaring the same segment name**. Resolution: registry order wins (the order of `[plugins] enabled`), the loser is skipped with a `warn!`, and `doctor` reports the conflict by name so the user can see which plugin lost.

---

## 5. Segment ordering

Add `[prompt] left_segments: Vec<String>` to `config.rs`, mirroring `right_segments` (`config.rs:133`, default `:144`).

Add `resolve_left_rail()` beside `resolve_right_rail()` (`layout.rs:224`), with identical semantics: names in, typed segments out, **unknown names skipped with `tracing::debug!` rather than erroring**.

`Layout::segment_order(preset)` (`layout.rs:164`) stops being the authority and becomes the default:

- **`left_segments` unset** → today's preset default list, then enabled plugin segments appended in registry order. This preserves current behavior byte-for-byte for every existing user.
- **`left_segments` set** → exactly that list, in that order. Plugin segments are referenced by `@<name>`.

```toml
[prompt]
left_segments = [
  "os", "ssh", "directory", "git",
  "@aws",                    # plugin segment
  "exit_status", "command_duration",
]
```

`Layout::apply_filter()` (`layout.rs:183`) is updated to filter against the resolved list rather than the static preset list.

This is the change that unblocks the panel's reorder UI.

---

## 6. Shell side — the compiled bundle

### 6.1 Compilation

`omarchy10k plugin compile` concatenates the `plugin.bash` of every enabled plugin into `~/.cache/omarchy10k/bundle.bash`, prefixed with a header recording:

- the enabled plugin list, in order
- each source file's path, mtime, and size

Compilation must be **deterministic**: identical inputs produce a byte-identical bundle. This is a test assertion, not an aspiration.

### 6.2 Inlining

`omarchy10k init bash` emits the adapter **with the bundle inlined**, so shell startup is one `eval` regardless of plugin count — zero extra file reads, zero stat storm.

Inline point: the existing "Modern CLI Layer" section near the end of `shell/omarchy10k.bash`, after the hook broker and daemon lifecycle are established (so plugins can call `o10k_hook_add`) and alongside the existing `tools.sh` sourcing. Respect an `O10K_NO_PLUGINS=1` escape hatch mirroring `O10K_NO_TOOLS`.

Plugin bash registers lifecycle callbacks through `o10k_hook_add` (`precmd`, `preexec`, `chpwd`, `shell_exit`) so plugins compose instead of clobbering `PROMPT_COMMAND` — the same guarantee that already fixed the Mise/Atuin/Zoxide collision.

### 6.3 Staleness

**Shell startup performs no staleness check.** That is the entire point of the compiled-bundle model.

| Rebuild trigger | Mechanism |
|---|---|
| `omarchy10k plugin enable/disable` | Rebuild automatically as part of the verb |
| `omarchy10k update` | Rebuild as part of the update path |
| `omarchy10k plugin compile` | Explicit |
| Plugin file edited by hand | **Not detected at startup.** `doctor` compares the bundle header against disk and warns. |

A stale bundle works correctly — it is simply the previous version. `doctor` gains a "plugin bundle: current / stale (run `omarchy10k plugin compile`)" line.

---

## 7. Completions

- Enabled plugins' `completions/` directories are appended to `BASH_COMPLETION_USER_DIR`.
- The compiled bundle sources any `completions/*.bash` that bash-completion's dynamic loader will not pick up on its own.
- `doctor` gains a `bash-completion` presence check.

This creates the seam that `carapace` and ble.sh's `complete_menu` will later occupy. **Those are out of scope here** — see §2.

---

## 8. CLI surface

```
omarchy10k plugin list                 # available / enabled / inert (missing `requires`)
omarchy10k plugin enable <name>        # config_set + compile + reload
omarchy10k plugin disable <name>       # config_set + compile + reload
omarchy10k plugin info <name>          # metadata, segment defs, what plugin.bash defines
omarchy10k plugin compile              # force bundle rebuild
```

Follows the existing `Look { ... }` subcommand-family pattern in `crates/omarchy10k/src/main.rs`. Config mutation routes through the daemon socket when one is live (as `look apply` does), and falls back to a direct file write when headless.

`plugin enable` prints the absolute path of the `plugin.bash` being activated before activating it. See §11.

---

## 9. Panel surface

Deliberately minimal — `quattro/Panel.qml` is 2350 lines and needs its own decomposition pass before it can absorb more.

- **SYSTEM rail** gains a **Plugins** list: each plugin with an enable/disable toggle, a detail expander (description, segments provided, whether it ships bash), and an explicit *inert — missing `<binary>`* state.
- The existing segment toggle grid becomes a **reorderable list** bound to `[prompt] left_segments` (up/down affordance; no drag-and-drop).

Both read and write through the existing daemon-IPC config path (`Panel.qml` "Config Read / Config Write via daemon IPC" sections, ~lines 194 and 214).

---

## 10. Protocol and config

**Additive to 0.5. No version bump.** No render-path message changes — segment definitions reach the daemon via config reload, not via a message.

Two new control verbs in `server.rs`:

| Verb | Request | Response |
|---|---|---|
| `plugins` | `{"type":"control","command":"plugins"}` | `{plugins: [{name, description, enabled, inert, missing_requires, segments: [...], has_bash}]}` |
| `plugins_set` | `{"type":"control","command":"plugins_set","name":"aws","enabled":true}` | Routes through the existing `config_set` merge + reload path; returns the updated enabled list |

New config keys for `config/default.toml` and `docs/wiki/config.md`:

```toml
[plugins]
enabled = []                # names of plugins under ~/.config/omarchy10k/plugins/

[prompt]
# left_segments = [...]     # unset = preset default + enabled plugin segments
```

### Pre-existing bug to fix in the same change

`config/default.toml` currently has a bare block — `strategy`, `max_length`, `repo_root_style`, `unique`, `anchors` — sitting immediately after the `[segments.time]` header. Those five keys therefore parse into `[segments.time]`, not `[directory]`. It is a duplicate of the real `[directory]` block above it. Delete the stray block. It ships as the documented default config and is wrong.

---

## 11. Trust model

State this in the wiki in these words, not softer ones:

> `plugin.bash` is arbitrary shell code executed with the same privileges as your `.bashrc`. Omarchy10k does not sandbox it.

Consequences designed around that honesty:

- **Drop-in never auto-enables.** A directory appearing under `plugins/` does nothing until `plugin enable` names it. This mirrors Omarchy's own plugin posture, where plugins land disabled so the user can review the code first.
- `plugin enable` prints the path being activated before activating it.
- Tier-2 `command` argv arrays come only from the user's own config tree. Nothing is network-sourced — which is precisely why `plugin add <git-url>` is excluded from this spec (§2).
- No claim of isolation appears anywhere in the docs or the panel UI.

---

## 12. Failure modes

Every one of these resolves to a working prompt.

| Failure | Behavior |
|---|---|
| Malformed `plugin.toml` | Plugin (or just the bad segment) skipped, `warn!` logged, surfaced by `doctor` |
| Unknown `tier`, or hex in `style` | Segment skipped with `warn!` |
| Plugin segment name collides with a built-in | Skipped with `warn!` |
| `requires` binary missing | Plugin inert; listed as inert by `plugin list` and `doctor` |
| Tier-2 command times out | Segment absent, killed child, debug log; next prompt retries after TTL |
| Tier-2 command exits non-zero | Segment absent, debug log |
| Unknown name in `left_segments` | Skipped with `tracing::debug!` (same as `resolve_right_rail` today) |
| Stale bundle | Works — it is simply the previous version; `doctor` warns |
| Plugins dir missing entirely | Empty registry, no error |

**Invariant: no failure in this subsystem may break, delay, or blank a prompt render.**

---

## 13. Testing

### Unit

- Registry parse: well-formed, malformed TOML, unknown keys, invalid `tier`, hex in `style`, name collision with a built-in.
- Each `detect_*` predicate independently, plus AND-composition of several on one segment; `detect_files` ancestor walk stopping at the repo root.
- `CustomCache`: TTL expiry, in-flight dedupe (N concurrent renders spawn exactly one task), output truncation at first line / 256 bytes, non-zero exit, timeout.
- Bundle compilation determinism: identical inputs → byte-identical output.
- `resolve_left_rail()`: known names, unknown names skipped, `@plugin` names, unset → preset default plus appended plugin segments.
- Effective `[env.watch]` union includes plugin-declared keys.

### Integration (extend `tests/integration_test.sh`)

- Fixture plugin dropped on disk → **not** active until enabled.
- `plugin enable` → segment renders in the prompt; `plugin disable` → gone.
- `plugin compile` is idempotent (run twice, compare hashes).
- A deliberately-hanging tier-2 `command` **does not stall a render** — this is the single most important integration test in the suite.
- Inert plugin (fake `requires`) renders nothing and is reported inert.
- `doctor` reports a stale bundle after a hand-edit.

### Performance — the acceptance gate

Warm render with **five tier-2 plugins enabled stays under 5ms**, measured with `omarchy10k benchmark`. If it does not, the async design is wrong and must be fixed before merge. This is a blocking criterion, not a nice-to-have.

### Panel

`qmllint` clean. Plugins list renders with plugins present and with none. Reorder writes `left_segments` and the preview updates.

---

## 14. First-party plugins shipped in-tree

Five, chosen to exercise both tiers and to cover the highest-traffic Oh My Zsh territory:

| Plugin | Tier(s) | Contents |
|---|---|---|
| `git-aliases` | bash only | The Oh My Zsh `git` plugin's alias set — the single most-used component in all of OMZ |
| `aws` | env | `AWS_PROFILE` / `AWS_REGION` segment |
| `terraform` | env + command | Workspace segment; `detect_folders = [".terraform"]` |
| `node` | command | Version from `package.json` engines / `.nvmrc`; `detect_files` gated |
| `package` | command | Version from `Cargo.toml` / `package.json` / `pyproject.toml` — P10k's `package` segment |

These double as the fixtures for §13 and as the documentation-by-example for plugin authors.

---

## 15. Implementation sequence

Ordered so each step is independently shippable and testable.

1. **Fix the `config/default.toml` stray block** (§10). Trivial, unblocks nothing, do it first so it is not lost.
2. **`left_segments` + `resolve_left_rail()`** (§5). No plugins involved; pure refactor of `segment_order` from authority to default. Ship and verify zero behavior change for existing configs.
3. **Registry + tier-1 env segments** (§3, §4.1, §4.2). Includes the plugins-dir watcher and the env-allowlist union. Ship with the `aws` plugin as proof.
4. **Tier-2 command segments + `CustomCache`** (§4.3). The performance gate in §13 applies here. Ship with `package`.
5. **Bundle compilation + init inlining + completions** (§6, §7). Ship with `git-aliases`.
6. **CLI verb family** (§8) — can land alongside 3–5 incrementally, but `plugin list` should exist by step 3.
7. **Control verbs + Panel surface** (§9, §10).
8. **Remaining first-party plugins, wiki updates, doctor checks** (§14).

---

## 16. Companion work (separate specs — do not merge into this one)

- **Theming / shell-UX wave:** ble.sh `ble-face` syntax + autosuggestion theming through the rice layer, a terminal-emulator template (Ghostty/foot font, cursor, padding, opacity), `carapace`, ble.sh `sabbrev` abbreviations, a keybinding registry with conflict detection in `doctor`. This is the sibling wave already agreed with the user, and it consumes the completion seam from §7.
- **Plugin distribution:** `plugin add <git-url>`, update/remove, curated catalog, and the trust model that requires.
- **Panel decomposition:** `Panel.qml` at 2350 lines will resist per-segment drill-in and any richer plugin UI.
