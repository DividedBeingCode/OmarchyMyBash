# v0.4 Implementation Plan — 2026-08-28

Scope: P0 core + P1 stretch from docs/wiki/v04-feature-intel.md — 13 features:
0.1 env channel, 0.2 notification fix + routing, 0.3 status enrichment,
0.4 transient+stale wiring, 1.1 true powerline, 1.2 statusline claude-code,
1.3 agent signal segment, 1.4 OSC 133;C/D, 2.1 plugin IPC target, 2.2 service
plugin, 2.3 session picker overlay, 3.1 ANSI live preview, 3.3 intro.

Non-goals: 2.4-2.8, 3.2, 3.4-3.7, everything on the Kill List.

## Protocol 0.4 contract (frozen — all workers build to this)

- `PROTOCOL_VERSION = "0.4"` in server.rs; hello returns `protocol_version: "0.4"`.
- `PromptRequest` gains flattened `env: Option<HashMap<String,String>>`.
- Env allowlist default (adapter + daemon `[env.watch]` must match):
  VIRTUAL_ENV, CONDA_DEFAULT_ENV, MISE_NODE_VERSION, MISE_PYTHON_VERSION,
  MISE_RUBY_VERSION, MISE_GO_VERSION, MISE_RUST_VERSION, IN_NIX_SHELL,
  DISTROBOX_ENTER_PATH, container, KUBECONFIG, DIRENV_DIR.
- `status` response additive fields (old fields kept): `git: {branch, dirty,
  staged, unstaged, conflicted, ahead, behind, worktree, stale}`,
  `last_cmd_duration_ms: u64`, `last_exit_code: i32`, `session_age_secs: u64`,
  `battery: Option<{capacity, status}>`.
- New message type `statusline`: request `{type:"statusline", payload:{...}}`
  (payload = Claude Code statusLine JSON verbatim); response
  `{type:"statusline", status:"ok", left:"<ansi>"}` — rendered with current
  config+palette, `<50ms`, no OSC 133.
- Bridge stdout framing becomes FOUR NUL-terminated fields:
  `left \0 right \0 notify_threshold_ms \0 transient`. write_fallback emits
  empty 3rd/4th fields. Old 3-field readers still work (4th field ignored).
- Notifications: new `[notifications]` table — `enabled` (bool, default true),
  `threshold_ms` (u64, default 10000), `unfocused_only` (bool, default false).
  `[segments.notification]` stays parsed as deprecated alias. Daemon emits
  `notify_threshold_ms: 0` when disabled; adapter treats 0/empty as OFF
  (fixes the verified no-op bug).
- OSC 133;C/D: adapter-gated, default OFF via `[terminal.semantic_prompts]
  enabled = false` in default.toml until the coexistence spike result says
  otherwise. Gate condition when enabled: `GHOSTTY_SHELL_INTEGRATION_FEATURES`
  unset AND `TERM_PROGRAM` in {ghostty, foot}; never emit under TMUX.

## Worker ownership (disjoint files)

| Worker | Files | Features |
|---|---|---|
| W1 daemon | crates/omarchy10kd/src/**, config/default.toml, docs/wiki/{protocol.md,config.md} | 0.1 daemon, 0.2 daemon, 0.3, 1.1, 1.2 daemon, 1.3 segment, 0.4b stale icon |
| W2 cli/adapter | crates/omarchy10k/src/**, shell/omarchy10k.bash, install.sh, docs/wiki/{bash-adapter.md,cli.md} | 0.1 adapter, 0.2 routing, 0.4 bridge/adapter, 1.2 CLI, 1.4, 3.3 intro |
| W3 quattro | quattro/**, docs/wiki/quattro.md | 3.1, 2.1, 2.2, 2.3 |
| W4 tests | tests/integration_test.sh | New feature coverage per contract |

## Validation gate (main session, after all workers)

cargo build + cargo test --workspace; bash -n on all shell; integration suite
green; live smokes: env-object prompt renders venv segment, statusline stdin
round-trip, bridge 4-field framing, powerline emits SGR 48;2, status fields
present, notification disable emits threshold 0.

## Spikes (web, against basecamp/omarchy quattro branch)

W2: Ghostty shell-integration coexistence (GHOSTTY_SHELL_INTEGRATION_FEATURES
semantics) — decides 1.4 default. W3: PluginRegistry.qml + omarchy-shell.md
IPC registration contract — gates 2.1/2.2/2.3. If unverifiable, ship behind
feature detection and document the assumption in the wiki page.
