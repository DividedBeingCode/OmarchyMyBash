# Wave 1 — Prompt Visual Depth: Design

Date: 2026-08-28
Status: Approved direction, pending implementation plan
Scope: `crates/omarchy10kd` (style, segments, render, config), `quattro/Panel.qml`, `quattro/Model.js`, `config/default.toml`, wiki updates. No protocol bump. No adapter changes.

## Context

Omarchy10k renders through a warm per-shell daemon with palette hot-reload and truecolor output. v0.4 shipped powerline/rainbow fills, frames with a no-bottom-border typing line, and the rice layer. This wave deepens the prompt's visual language — p10k-class polish — entirely inside the daemon, with zero new surfaces and zero platform risk.

Features (all user-selected): unique-path shortening, separator geometry family, gradient fill-line, gradient preset, load sparkline. Vi-mode prompt char explicitly cut.

Later waves (separate specs, sequenced this session): Wave 2 desktop moat (hooks, keybind switchers, icon-font glyphs, menu actions), Wave 3 workflow intelligence (HUD, command palette, project profiles, smart output), Wave 4 ambient & effects. Engine-depth items (gitoxide, streaming repaint) attach to whichever wave they serve.

## Non-goals

- Vi-mode prompt char (user decision).
- Per-character gradient SGR, streaming repaint, custom user segments, prompt animations (kill list).
- Protocol changes: every feature is render/config-side; preview requests already carry style overrides.

## 1. Unique-path shortening

`segments/directory.rs` gains `shorten_unique`:

- Split cwd into components. A component whose directory contains any anchor file (`.git`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `Gemfile`, `flake.nix`, `README.md`) is an **anchor**: never shortened.
- Every other component shortens to the fewest leading characters that stay unambiguous among its sibling directories in the same parent (`~/w/p/src` for `~/Work/project/src`). Single-entry components (only child) shorten to 1 char. Home dir renders as `~` as today.
- Sibling tables are computed once per cwd and cached in `DaemonState` alongside the git cache; key = cwd. Cache entry holds per-ancestor `Vec<String>` of sibling names plus an `Instant` stamp; entries older than 30s recompute. Warm renders do zero filesystem reads.
- Ordering: anchor detection → unique shortening → existing compact/max_length logic applies to the shortened path.

Config (`config/default.toml` + `config.rs`):

```toml
[segments.directory]
unique = false          # opt-in
anchors = [".git", "Cargo.toml", "package.json", "pyproject.toml", "go.mod", "Gemfile", "flake.nix", "README.md"]
```

Failure mode: any read error → fall back to the full component (never empty).

## 2. Separator geometry family

`GlyphCatalog` separator keys extended: existing `powerline`, `powerline_thin`, `slanted`, `round`, `vertical`, `fade`, `fade_rev` stay; add `round_thick` (`E0B4/E0B5` pair), `trapezoid` (`E0D2/E0D4`), `trapezoid_rev` (`E0D3`/`E0D5` mirrored), `flame` (`E0C0/E0C2`), `dither` (`E0C4/E0C6`).

New config layer:

```toml
[style.separators]
shape = "auto"   # auto | <catalog key>
```

- `auto` (default): preset default separators — current behavior, byte-for-byte.
- A set shape overrides **both directions and the matching end caps together** (`GlyphCatalog::caps_for(key)` provides the cap pair; frames pick caps from the same family). Raw `left`/`right` keys still override individually for custom mixes; precedence: explicit left/right > shape > preset.
- Panel separator picker (existing GlyphRow) gains the new keys; single picker continues to set both sides.

Consumption in `style.rs resolve()`: shape resolves before left/right defaults; caps resolve from shape when set, else preset defaults.

## 3. Gradient fill-line

The gap fill (`style.gap_char` + frame path) interpolates between two palette-derived colors across its width.

```toml
[style.frame]
gap_gradient = "off"   # off | subtle | full
```

- Endpoints computed once at palette load: `subtle` = accent → accent blended 60% toward background; `full` = accent → complement. Complement rule: if the accent's blue channel ≥ red, complement = palette magenta; else palette cyan. Deterministic from any palette. Recomputed on `reload_theme` — re-themes free.
- Rendering: per-8-cell blocks. One `SGR 38;2` prefix per block of `gap_char`, interpolated linearly by block center position. A gap narrower than 8 cells renders solid accent (current behavior). Width math unchanged — gradient is foreground-only, so frame budget and overflow logic are untouched.
- Applies to the framed top line's gap; unfilled presets ignore it.

## 4. Gradient preset

`preset_defaults("gradient")` joins the preset table: `filled = true`, separators `powerline_thin`, frame off, and a **stepped background ramp**: segment i of n samples the two-stage lerp accent → magenta → cyan at t = i/(n−1) (t ≤ 0.5 lerps accent→magenta, else magenta→cyan, via `AnsiColor::blend`). Existing rainbow stays as-is (cycled palette hues); gradient is the smooth-ramp sibling.

Panel: 9th preset card (`{ name: "gradient", preview: "▛▜▛▜", desc: "Ramp" }`) — the live preset-preview path already renders per-card via the `style_preset` override, so no preview plumbing changes.

## 5. Load sparkline segment

New `segments/load.rs`:

- Per render: read `/proc/loadavg`, take the 1-minute load. Microseconds; failure → segment hidden.
- `DaemonState` holds a 16-slot ring (pushed once per render). Sparkline = braille `▁▂▃▄▅▆▇█` mapped over the ring with autoscale (max of ring, floor 1.0). Ring only advances while the shell renders prompts — an idle shell freezes history, which is the honest reading.
- Config:

```toml
[segments.load]
enabled = false
width = 16
```

- Segment name `load`, added to the segment allowlist/order plumbing and the panel Segments toggle grid.

## Panel & config surface

- Panel Appearance tab: separator picker gains the new shape keys (chips already flow). `unique`, `gap_gradient`, and the sparkline toggle ride the existing Segments toggle grid / config docs — no new bespoke controls required this wave.
- `docs/wiki/config.md`: new keys. `docs/wiki/daemon.md`: sibling cache + load ring notes. `INDEX.md`: wave note.

## Testing

- Unit (daemon): `shorten_unique` — collisions, anchors, single-child, unicode components, deep paths; gradient endpoint derivation + block splitting (widths 0, 1, 7, 8, 100); sparkline ring autoscale + braille mapping; shape→caps resolution precedence.
- Integration (`tests/integration_test.sh`): preview with `unique=true` shows a shortened path; `gap_gradient=full` preview contains ≥2 distinct `SGR 38;2` runs; `gradient` preset preview renders; `load` segment hidden when disabled, braille when enabled.
- Panel: qmllint clean; gradient card appears and live-previews via the existing `style_preset` path.
- Performance guard: existing benchmark — warm prompt latency stays <5ms median (all features are string work + one `/proc` read).

## Rollout

All defaults opt-in except the gradient preset card (visible, not selected). No migration. Ships as one protocol-compatible release (config additions only).
