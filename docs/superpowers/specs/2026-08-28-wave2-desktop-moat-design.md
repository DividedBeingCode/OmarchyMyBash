# Wave 2 — Desktop Moat: Design

Date: 2026-08-28
Status: Draft for review
Scope: `crates/omarchy10k` (CLI), `crates/omarchy10kd` (segments, server), `hooks/`, `quattro/` (bar widget, panel), `install.sh`, wiki. No protocol bump (reuses 0.4 status/config messages); one new CLI verb family.

## Context

Wave sequencing this session: Wave 1 prompt visual depth (spec written) → this wave → Wave 3 workflow intelligence → Wave 4 ambient & effects. The moat is what upstream Omarchy can't copy: our daemon + hook + widget integration. All inputs verified against the installed Omarchy hook system (`omarchy-hook <event>`, `~/.config/omarchy/hooks/<event>.d/`, events: `post-boot`, `post-update`, `theme-set`, `font-set`, `battery-low <pct>`).

## 1. Hook consumption — `omarchy10k hook-event <event> [args]`

One new CLI verb, installed hook scripts call it; it fans out to every live daemon socket (same pattern as the existing `theme-set` hook: socat NDJSON, fire-and-forget, `|| true`).

- `install.sh` drops `hooks/hook-bridge` into `~/.config/omarchy/hooks/battery-low.d/omarchy10k`, `post-update.d/omarchy10k`, `font-set.d/omarchy10k` — each a two-liner calling `omarchy10k hook-event <event> "$@"`.
- Daemon handling per event:
  - `battery-low <pct>` — battery segment (if enabled) renders urgent + a ⚡ suffix for the next N prompts (configurable `[segments.battery] low_pulse_prompts = 6`); bar widget dot flips urgent via the existing status polling; optional desktop notification through `omarchy-notification-send`.
  - `post-update` — clears the intro-shown marker's staleness, bumps a `last_omarchy_update` field in `status`; panel Advanced shows it.
  - `font-set` — daemon re-reads nothing (fonts are terminal-side) but triggers a single prompt re-render hint via the flags file; documents that Ghostty applies fonts on its own reload.
- New message `{"command":"hook_event","event":"...","args":[...]}` — additive, ignored by old daemons.

## 2. Keybind preset switcher — `omarchy10k preset <name>`

- CLI: `omarchy10k preset <name>` writes `style.preset` (+ the preset-consistent granular keys, same rule the panel uses) to `config.toml` via any live daemon socket (headless included), else direct file write; broadcasts config reload.
- `omarchy10k preset list` prints names. `omarchy10k preset --next` cycles (order = panel card order).
- Hyprland integration is user config (documented snippet: `bind = $mainMod SHIFT+P, exec, omarchy10k preset --next`) — no keybind installation by default; `install.sh --keybinds` optional.
- Panel parity: the Appearance "Preset" card grid gets a small "cycle" affordance reusing the same code path.

## 3. Omarchy icon font glyphs in prompt segments

- `GlyphCatalog` gains omarchy-font entries (`omarchy-logo`, agent marks) referencing the omarchy.ttf PUA codepoints; segments that take glyph keys (os icon, agent segment, git branch icon) accept them.
- Rendering is plain text — the terminal's fontconfig fallback resolves `omarchy` font for those codepoints (Ghostty/foot both fall back). `omarchy10k doctor` gains a check: glyph renders if `fc-match -f '%{family}' <codepoint-hex>` resolves to a font containing it; warns otherwise.
- Panel glyph pickers gain an "Omarchy" group only where the segment makes sense (OS icon).
- Wiki: document that these codepoints need the omarchy font installed (it ships with Omarchy).

## 4. Menu quick actions + `omarchy10k script`

- New CLI: `omarchy10k script` emits machine-readable JSON describing user-defined quick actions: source = `[scripts]` table in `config.toml` plus project profiles (Wave 3 integration point — profiles contribute actions when present).
  ```toml
  [scripts]
  update = { cmd = "omarchy update", icon = "reload", label = "Update system" }
  ```
- The Quattro panel's bar widget context actions (and later Wave 3 palette) list these; executing runs the command in the active shell via OSC 777-style... no — execution model: `omarchy10k script run <name>` executes via `sh -c` detached in the calling terminal when invoked interactively; from the bar widget it spawns a ghostty window in the session cwd (same launcher the session picker uses).
- Security: commands come only from the user's own config file — same trust level as `.bashrc`; nothing network-sourced.

## Watch items (from v0.4 intel, carried)

- Hook names/payloads: verified against installed Omarchy before merge (`omarchy-hook --help` + reading `omarchy-hook` source).
- Plugin API churn: bar-widget context actions feature-detect; degraded = actions listable via CLI only.

## Testing

- Unit: preset cycle order; scripts JSON shape; hook-event message serialization.
- Integration: fake hook script → daemon receives event → battery segment urgent flag present in next prompt render; `preset --next` writes config + reloads (file mtime + preview changes); `omarchy10k script` JSON matches schema.
- Panel: qmllint clean; actions list renders when `[scripts]` present, absent without.
- Wiki: glossary hook table, quattro.md actions section.
