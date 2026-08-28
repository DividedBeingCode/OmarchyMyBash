# Wave 3 — Workflow Intelligence: Design

Date: 2026-08-28
Status: Draft for review
Scope: `crates/omarchy10k` (CLI: palette, profile, q), `crates/omarchy10kd` (profiles, project segment data), `shell/omarchy10k.bash` (palette widget binding), `config/tools.sh` (fzf plumbing), `quattro/` (HUD overlay), wiki. Depends on Wave 2's `omarchy10k script` actions model. Protocol: additive bump to 0.5 for the `qbox` message and the `project` block in `status`; palette uses no new message (CLI aggregates via `actions` endpoint).
## Context

Third wave of four (1 visual depth ✓ spec'd, 2 desktop moat ✓ spec'd, this, 4 ambient). The 2026-terminal idea: the terminal starts acting on your behalf. Two flagship surfaces (HUD, command palette) plus project awareness. Design stance: **reuse, don't rebuild** — fzf/atuin/zoxide are already installed, themed (rice layer), and loved; our palette is a source-aggregator over them, not a competing TUI.

Later waves (separate specs, sequenced this session): Wave 1 prompt visual depth (spec committed), Wave 2 desktop moat (spec committed), Wave 4 ambient & effects (sibling spec). Non-goals this wave: Atuin-native context segment (deferred — needs atuin internals spike), history search UI (atuin owns it), any new interactive TUI binary.

## 1. Command palette — `omarchy10k palette`

- **Surface: fzf.** `omarchy10k palette` prints a source-aggregated action list (JSON lines → formatted rows); a bash widget pipes it through the themed fzf; the chosen action executes. No new TUI binary, theme coupling free, `FZF_DEFAULT_OPTS` already ours.
- Bash widget: `__o10k_palette` bound to `Ctrl-Space` (configurable escape hatch: `O10K_PALETTE_KEY`), installed by `config/tools.sh`.
- Sources (each a provider, JSON line: `{id, label, category, action, exec_model}`):
  - static commands (`[scripts]` table + project profile actions — Wave 2/3 models)
  - zoxide top dirs (action: `cd`)
  - git branches (action: `git checkout`), when in a repo
  - recent dirs (from zoxide data)
  - Omarchy actions (theme cycle, preset cycle — Wave 2 verb, rice refresh)
  - shell history is explicitly NOT duplicated — Ctrl-R/atuin own that
- `exec_model`: `shell` (typed into the prompt line via `READLINE_LINE` semantics — works in vanilla readline via `bind -x` and ble.sh), `exec` (run + notify), `navigate` (cd).
- Daemon's role: profile-aware action aggregation endpoint `{"command":"actions"}` so the CLI stays thin and future QML surfaces (bar widget menu) reuse it.

## 2. Project profiles — `.omarchy10k.yml`

- Detection: walk up from cwd to repo root (anchor files); profile file = `.omarchy10k.yml` (name reserved; `.quattro.yml` also accepted for continuity with the research doc).
- Schema (denied keys ignored, never executed automatically):
  ```yaml
  name: quattro
  actions:
    test: { cmd: "cargo test", label: "Run tests" }
    lint: { cmd: "cargo clippy" }
  env:
    show: [RUST_LOG]
  ```
- Daemon: reads profile on cwd change (same cache slot as the sibling/git caches, keyed by project root); exposes `project: {name, actions}` in the `status` response and the palette `actions` endpoint; optional prompt chip `[segments.project] enabled=false` shows the profile name with a ⌘-family glyph.
- Panel: Context tab shows detected project + its actions (display-only this wave; execution lives in palette/bar per Wave 2's model).
- Validation: parse errors → ignored with a doctor warning, never a broken prompt.

## 3. HUD — `omarchy10k hud`

- Quattro **overlay-kind** surface (session picker precedent, v0.4 2.3): translucent themed card, top-center, auto-dismiss 6s (or keypress).
- Content per row: CPU%, RAM, battery, network iface state, current project + branch, running jobs across live sessions (from the service hub's per-session `status`). Data source: one fan-out of the existing `status` command to all sessions — no new polling daemon.
- Summon: `omarchy10k hud` CLI (IPC to the shell → panel overlay `summon`), plus an optional Hyprland bind snippet (documented, like Wave 2 presets).
- Refresh: single snapshot at open; explicit `r` re-query in v1 (no live timer — QML motion stays event-driven per the kill list's spirit).

## 4. Smart output adapters — `omarchy10k q <subject>`

Deliberately minimal MVP (the research doc's "opt-in, never intercept" rule):

- `omarchy10k q git-status` — renders `git status --porcelain` + branch/ahead/behind as a themed box (daemon-rendered via a `qbox` protocol message reusing statusline machinery).
- `omarchy10k q sys <unit>` — `systemctl status <unit>` summarized: active state, uptime, memory, last 3 log lines. Requires `systemctl` (always present on Omarchy).
- Raw output escape hatch: `--raw` passthrough flag on both.
- Alias wiring: user adds `alias qgit='omarchy10k q git-status'` if they want it; nothing intercepted globally.
- Deferred (explicitly): docker, ip addr, package manager adapters — each is a schema + renderer; add on demand.

## Testing

- Unit: palette line formatting; profile parse (valid/malformed/denied keys); q git-status porcelain→box mapping; HUD row assembly from fake status payloads.
- Integration: `.omarchy10k.yml` in a fixture repo → `status` contains project block; palette JSON includes profile action; `q git-status` box renders under forced palette.
- Panel/overlay: HUD opens via IPC, auto-dismisses, qmllint clean.
- Performance: palette aggregation <50ms warm (daemon-cached sources); profile cache hit = 0 I/O.
- Wiki: new pages `palette.md` (sources, exec models), updates to config.md/status/protocol sections.
