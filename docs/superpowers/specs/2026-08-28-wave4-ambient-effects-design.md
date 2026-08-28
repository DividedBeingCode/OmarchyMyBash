# Wave 4 — Ambient & Effects: Design

Date: 2026-08-28
Status: Draft for review
Scope: `crates/omarchy10k` (CLI: effect, ambient, fetch), `shell/omarchy10k.bash` (event hooks, opt-in), `quattro/` (ambient overlay), `templates/themed/` (fastfetch), rice layer wiring, wiki.

## Context

Fourth and final wave of the 2026-terminal sequence (1 visual depth, 2 desktop moat, 3 workflow intelligence, this). Personality wave. Hard constraints inherited from the ratified kill list: **no motion in the prompt path**, no timer-driven PROMPT_COMMAND redraws, nothing that delays a render. Everything here is event-driven one-shots or user-invoked surfaces, all default-off.

## 1. Effects engine — `omarchy10k effect <name>`

One-shot ANSI animations printed **after** command completion, never inside PS1, never blocking:

- Implementation: adapter fires `omarchy10k effect run <name> <context>` detached post-command when configured; the CLI prints a ≤300ms frame loop to the TTY and exits. Bash stays responsive — the effect writes to the terminal between prompts, guarded by the same sync-output DEC 2026 wrapper the prompt uses.
- Effects v1: `success` (brief green sweep under the last prompt), `fail` (single red edge flash), `pushed` (branch → origin sweep; fired when a detected `git push` exits 0 — detection = `__O10K_LAST_CMD` prefix match), `theme-sweep` (palette-colored bar sweep; fired on `reload_theme` when enabled).
- Config:

  ```toml
  [effects]
  enabled = false          # master, default off — taste setting
  on = ["fail"]            # which events
  max_duration_ms = 300
  ```

- Reduction guard: effects never fire when `NO_COLOR` is set, when the terminal isn't Ghostty/foot/WezTerm-class (TermCaps gate), or when the last command ran <50ms (noise suppression).
- The effect palette derives from the theme (same endpoints as Wave 1's gradient) — re-themes free.

## 2. Ambient mode — `omarchy10k ambient`

User-invoked fullscreen idle surface; **no automatic trigger ships** (Hyprland idle config can bind it — documented, opt-in):

- Quattro overlay-kind surface: fullscreen, themed; big clock, date, load sparkline (Wave 1 segment data via `status`), current project, cava if installed (spawned with the o10k gradient config from the rice layer), Omarchy logo mark in the omarchy icon font.
- Exit: any key / mouse click. No timers, no network.
- CLI: `omarchy10k ambient` → IPC summon; `Esc`/click dismisses. Reuses the session-picker overlay registration pattern (v0.4 2.3).

## 3. Themed Fastfetch — `omarchy10k fetch`

- Ships `templates/themed/o10k-fastfetch.jsonc.tpl` through the existing rice layer (Omarchy theme engine renders it on every theme switch — zero daemon involvement).
- Content: Omarchy logo art, theme name, host, kernel, uptime, shell (bash + o10k version), packages, desktop, terminal, battery. All label/logo colors reference theme tokens via the template variables.
- `config/tools.sh` gains a guarded alias-less helper: `omarchy10k fetch` execs `fastfetch --config ~/.local/state/omarchy/current/theme/o10k-fastfetch.jsonc`; users opt into first-terminal-of-session triggers themselves in `.bashrc` (documented one-liner — no startup tax by default, per the research doc's rule).
- Requires `fastfetch` — package check in install.sh's tool list; doctor notes absence.

## 4. Wave-wide taste rules (enforced in review)

1. Every effect/ambient surface default-off or user-invoked.
2. Nothing animates between prompt draw and command start.
3. Every effect ≤300ms; ambient has zero animation loops except cava (external process, user-invoked).
4. All colors derive from the live palette; no hex in effects code.
5. TermCaps gate + `NO_COLOR` respect everywhere.

## Testing

- Unit: effect frame generation (durations, palette interpolation); event gating logic (`NO_COLOR`, TermCaps, min-duration); fastfetch template renders for dark/light palettes.
- Integration: forced `git push` success fires exactly one `pushed` effect (TTY capture — count cursor sequences); `omarchy10k fetch` exits 0 with fastfetch present, prints guidance when absent; ambient overlay opens/dismisses via IPC.
- Panel/overlay: qmllint clean; ambient overlay theme-follows (spot-check under a second palette).
- Performance: effect path adds zero prompt latency (detached, post-command); ambient/fetch never touch the render path.
- Wiki: `effects.md` short page (events, config, taste rules), glossary entries.
