# Control Center Redesign + Looks Gallery: Design

Date: 2026-08-28
Status: Approved direction, pending implementation plan
Scope: `crates/omarchy10kd` (looks registry, protocol verbs, preview override), `crates/omarchy10k` (CLI `look`), `quattro/` (Panel IA restyle, Gallery overlay), `config/default.toml`, wiki. Protocol bump: **0.5, additive only** (new control verbs + optional preview `look` field).

## Context

The Control Center panel works but reads cheap: five flat tabs, ASCII-fake preset previews, no composite theming, no visibility of what changed. Research (SettingsUX + RicerUX wings, both with sources) converged on: real-render previews are the product; Looks should be atomic bundles; search + modified-ink make settings feel intelligent; gallery cards expand into full-transparency detail; never clobber silently.

User decisions: **full Looks bundles AND all granular settings remain** (two views of one state); the expanded surface is a **full overlay gallery** (overlay-kind, session-picker precedent).

Design approaches considered: daemon-native Looks (chosen), panel-side Looks (rejected — Looks invisible to CLI/doctor, ASCII-only previews), restyle-only (rejected — no composite theming).

## Non-goals

- Drag-and-drop segment canvas (oh-my-posh configurator parity) — later wave.
- p10k-style stepwise wizard — possible follow-up once Looks exist.
- Per-Look file storage; Looks live in `config.toml` like everything else.
- Any non-additive protocol change.

## 1. The Look model (daemon)

A **Look** is a named, atomic bundle of appearance keys stored in `config.toml`:

```toml
[looks.tokyo-rainbow]
label = "Tokyo Rainbow"
style = { preset = "rainbow", separators = { shape = "powerline" } }
glyphs = { os_icon = "arch", character = "❯", git_branch_icon = "powerline" }
frame = { enabled = false }
palette = "theme"        # "theme" | curated palette key | "keep"
prompt = { blank_line = true }
```

Look sections map onto existing config subtrees: `style.*`, `glyphs.*` (os_icon, character, git_branch_icon), `frame.*` (enabled, gap_char, gap_gradient), `palette` (theme source/curated/custom selection), `prompt.*`. Unknown keys inside a Look are ignored with a warn; `palette = "keep"` skips theme writes.

### Protocol verbs (0.5, control channel)

- `looks` → `{looks: [{name, label, patch}]} + {current}` — registry + which Look (if any) matches current state.
- `looks_apply {name, transient?}` → merges the Look's keys as ONE atomic config-set patch (single file write, single reload — the daemon's existing all-or-nothing merge guarantees no partial state). `transient: true` (the gallery's **Try**) applies to the in-memory config only — no file write — and is reverted by re-running `reload_config` when the overlay closes. `palette` handled per its value: `theme` → `{theme:{source:"omarchy"}}`; curated key → hybrid+custom; `keep` → theme keys omitted.
- **Curated palettes move daemon-side**: today `Model.js` owns `CURATED_PALETTES`; the daemon gains the same table (theme.rs) so `palette = "<curated key>"` resolves identically from CLI, gallery, and panel. The panel's existing palettes (its CURATED_PALETTES entries match daemon colors) keep working; a `palettes` control verb exposes the daemon table so the panel stops duplicating it.
- `looks_save {name}` → snapshots the current values of exactly the Look-mapped keys into `[looks.<name>]`.
- `preview` request gains optional `look: "<name>"` — the daemon renders with the Look's patch applied as a dry-run overlay (in-memory only, nothing persisted).

### Daemon surface

- `[looks.<name>]` tables parse into a `LooksRegistry` (name → label + patch). Malformed tables are skipped with `warn!`, never fail the config load.
- Curated Looks ship compiled-in (~8), composed from existing blocks — e.g. **Tokyo Rainbow** (rainbow + powerline + tokyo hybrid palette), **Framed Gradient** (framed + powerline_thin + gap_gradient full + accent), **Lean Pure** (lean + pure segments), **Slanted Dither**, **Classic Compact**, **Omnarchy** (omarchy preset + theme palette, the factory Look). Compiled-in Looks merge with user-saved Looks (user name collisions shadow curated).
- CLI: `omarchy10k look list|apply <name>|save <name>` — same verbs over the socket; works with the headless daemon, zero panels open.
- Explicit dependency: `glyphs.*` maps onto existing keys (`os_icon` → `segments.os.icon`, `character` → `segments.character.success/error/transient`, `git_branch_icon` → `git.branch_icon`); `style.*` and `frame.*` map 1:1 onto their config tables; `prompt.*` onto `[prompt]`.

## 2. Panel IA (restyle)

- **Left rail replaces the tab row**: LOOKS · STYLE · BEHAVIOR · SYSTEM (icon + label, selected pill, keyboard navigable). Content pane swaps. Smart buckets per the libadwaita/GNOME 46 consolidation pattern.
  - LOOKS: Look card row (real renders) + "Save current as Look" + **Expand** (gallery).
  - STYLE: preset grid (10 cards incl. gradient), separator shape picker (all keys incl. trapezoid/flame/dither), frame rows + gap gradient picker, palette strip + theme source (granular fine-tuning stays).
  - BEHAVIOR: prompt rows (transient/newline/blank/right-prompt) + context rows (git mode, duration, ssh, exit status) + segment toggle grid (incl. load) + notify threshold.
  - SYSTEM: integrations status, doctor/benchmark output boxes, daemon + session list, config actions (open/reload/copy/paste/reset).
- **Hero**: status dot + title + live preview + **backdrop toggle** (dark/light/transparent behind the real render) + **Expand** button + search field.
- **Intelligence layer** (all research-backed):
  - Rows differing from `config/default.toml` get a 3px accent ink bar on the left.
  - Hover reveals a per-row **reset chip** (writes the default, clears the bar).
  - Header **search box**: filters rows across all sections; `@modified` token shows only changed rows; selecting a result jump-navigates the rail + scrolls the pane.
  - Dense sections get **"Show advanced (n)"** progressive disclosure.
- All styling through real tokens (`Color.*`, `Style.spacing.*`, `Style.cornerRadius`, state fills) — zero hex fallbacks, zero font arithmetic (carried over from the token-hygiene pass).

## 3. Gallery overlay

- New overlay-kind registration (`Gallery.qml`, session-picker precedent): dark scrim, centered canvas ~900×640, Esc/scrim-click closes, opened via the hero **Expand** button and `omarchy-shell call community.omarchy10k gallery toggle`.
- **GridView of Look cards** — each renders the REAL prompt via the daemon dry-run (`preview` + `look` override); not ASCII fakes.
- Filters: All / Looks / Styles / Palettes + search field.
- Card click → **detail sheet**: large render + human-readable diff of what the Look changes (key: old → new, computed against current config) + three actions:
  - **Try** — live-apply via `looks_apply` on a shadow copy (in-memory only; auto-reverts when the overlay closes).
  - **Apply** — persistent `looks_apply`.
  - **Save current as Look** — `looks_save`.
- Never clobbers silently: the diff sheet IS the disclosure (Starship lesson), and Try/Apply are distinct.

## 4. Errors & testing

- Unknown Look name in apply → error response + panel toast. Malformed `[looks.x]` → skipped from the registry with `warn!`, config load unaffected. Apply atomicity = the daemon's existing single-patch merge (no partial state on failure).
- Tests (daemon): looks registry parse (curated + user + malformed + shadowing), apply round-trip (state keys change atomically), save snapshot fidelity, preview-with-look dry-run (no persistence), unknown-name error. CLI: `look list/apply/save` over the socket.
- Panel: qmllint clean; rail navigation; ink bar appears on modified rows; search jump works; gallery renders per-Look real previews.
- Integration: one `looks_apply` changes preset + glyphs + palette in a single render (no intermediate states); CLI apply with panel closed works; Try → Esc reverts.
- Performance: gallery previews render per card lazily (viewport), each <5ms daemon-side; no polling.

## Phasing (each phase ships usable)

1. **Looks model + CLI + panel rail IA/restyle** (daemon verbs, buckets, hero, ink/search basics).
2. **Gallery overlay** with real-render cards + detail sheet + Try/Apply.
3. **Search + modified-ink + smart links polish** (`@modified` filter, advanced disclosure, override notes).

## Rollout

Protocol 0.5 (additive; 0.4 clients ignore new verbs and the preview `look` field). No migration. Curated Looks visible immediately in LOOKS page and gallery.
