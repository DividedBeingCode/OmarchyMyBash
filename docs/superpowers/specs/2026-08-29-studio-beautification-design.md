# Omarchy10k Studio Beautification — Design

**Date:** 2026-08-29
**Status:** Approved for planning
**Supersedes nothing.** Extends `2026-08-29-control-center-rebuild-design.md`.

## Goal

Make choosing a terminal look in Omarchy10k a *visual* act. Today every
picker in the Control Center is grey text and you cannot see what you are
choosing until you open a new shell. The daemon can already render a real
prompt for any hypothetical config; this design puts that render in front
of the user everywhere a choice is made, gives the collection enough
breadth to be worth browsing, and collapses the duplicate Gallery
implementation that causes drifting buttons.

## Diagnosis

Observed by summoning each surface and screenshotting it.

| # | Fault | Evidence |
|---|---|---|
| D1 | Nothing previews | Prompt tab offers 11 presets, 8 separators, 8 prompt chars and 76 glyphs with no render of any of them |
| D2 | Colorless color UI | 22 Omarchy theme chips are plain grey text; palettes get a single 12px dot |
| D3 | ~65% dead space | `Studio.qml` canvas is hardcoded `1040x720`; Looks content ends around y=530 |
| D4 | Two implementations | `Gallery.qml` (1550 lines) owns its own `Socket`, IPC handler and widgets, duplicating `Service.qml` |
| D5 | The preview lies | `Model.js:_ANSI_FG` is a hardcoded Tokyo Night table, so ANSI-indexed colors render wrong under every other theme |
| D6 | 8 palettes, 22 themes | 14 shipped Omarchy themes have no matching prompt palette, and user-installed themes never will |

D4 is the cause of "the buttons don't all work": `Gallery.qml` reimplements
verbs that `Service.qml` already owns, so a fix in one does not reach the
other.

## Non-goals

- Rebuilding `Panel.qml` (the bar popout) from scratch. Its buckets are
  re-skinned on the shared kit; its structure stays.
- Changing the prompt renderer, the segment set, or the config schema.
- Writing theme files. Applying an Omarchy theme remains a shell-out to
  `omarchy theme set`; this project never writes into
  `~/.local/state/omarchy/` or `/usr/share/omarchy/`.
- Terminal Modes and the oh-my-posh importer (tracked separately in
  `docs/wiki/ricing-intel-2026.md`).

## Architecture

```
RUST  (source of truth; the CLI and every QML surface share it)
  palette_derive.rs   NEW  OKLCH + APCA. Map colors.toml roles -> prompt
                           palette, then repair contrast.
  looks.rs            EXT  curated Looks 8 -> 18, curated palettes 8 -> 16,
                           each gaining label/blurb/tags metadata.
  server.rs           EXT  preview{scenes:[...]} renders many lines in ONE
                           round-trip; palettes verb returns curated +
                           derived with full swatch data.

QML KIT  (quattro/o10k/ — reusable, unit-tested, no surface state)
  TerminalPreview.qml NEW  Framed terminal mock rendering N ANSI scenes on
                           the previewed palette's own background.
  PresetCard.qml      NEW  A card that IS its preview: live mini prompt
                           line, name, blurb, tag row.
  Swatches.qml        NEW  Palette strip (8 roles + bg/fg).
  Chip.qml            NEW  Extracted from the 5 copies across Studio*.qml,
                           with an optional swatch slot.
  Preview.js          NEW  Scene catalog + request builder + debounce.

SURFACES
  Studio.qml          Two-pane workbench; canvas grows to fit the screen.
  StudioLooks.qml     NEW  Absorbs the Gallery: search, tags, grid, editor.
  Studio{Prompt,Theme,Rice,System,Wizard}.qml   Gain the pinned preview.
  Panel{Looks,Style,Behavior}.qml               Re-skinned on the kit.
  Gallery.qml         DELETED (its summon route survives, see M1).
```

## Component design

### R1 — `palette_derive.rs`

Pure, no I/O, no config dependency. Input is a parsed `colors.toml` map
plus its `mode` field; output is a `DerivedPalette` with the same eleven
roles `looks::curated_palette` already emits.

Omarchy themes already name their roles, so derivation is **map then
repair**, not hue-bucketing:

1. **Map.** `accent`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`,
   `orange`, `muted`, `background`, `foreground` come straight across.
   Missing keys fall back in a fixed order: `orange` -> `yellow` ->
   `red`; `cyan` -> `blue`; `magenta` -> `accent`. (`white` ships no
   `orange`, so this path is exercised by a real theme.)
2. **Repair.** Each foreground role is checked against `background` with
   APCA. Below target, walk OKLCH lightness away from the background —
   lighter on `mode = "dark"`, darker on `mode = "light"` — in steps of
   L 0.02, preserving hue and chroma, until the target is met or L
   saturates. If the role's own `bright_*` variant already passes, prefer
   it over a synthesized value.
3. **Preserve character.** Hue and chroma are never invented. A
   monochrome theme (`vantablack`, `white`) stays monochrome; only
   lightness moves.

Contrast targets, in APCA Lc:

| Role group | Target Lc | Rationale |
|---|---|---|
| `character` (prompt char), error red | 75 | Must never be marginal; it is the thing you look at first |
| `foreground`, `accent`, and the six hue roles | 60 | APCA's body-text threshold |
| `muted` | 45 | Deliberately de-emphasized, but must stay legible |

APCA rather than WCAG 2.x because nearly every palette here is dark, and
WCAG 2.x "far overstates contrast for dark colors" — its prediction
degrades once the brightest color is darker than `#a0a0a0`, which is true
of most terminal foregrounds, and it "cannot be used for guidance
designing dark mode." APCA's Lc is perceptually uniform across the whole
range instead, so one threshold means the same thing on Vantablack and on
Catppuccin Latte. OKLCH is the repair space for the same reason: equal
lightness steps are equal perceived steps, so walking L does not drift a
hue brighter or duller than its neighbours.

- <https://gist.github.com/Myndex/069a4079b0de2930e72d5401bde9af98>
  (WCAG 2 vs APCA, the dark-color failure mode)
- <https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell.html>
  (Lc thresholds and their meaning)
- <https://auricartisan.com/library/learn/articles/2026-06-02-oklab-oklch-perceptual-color-spaces>
  (Oklab/OKLCH perceptual uniformity)

Public API:

```rust
pub struct DerivedPalette { /* eleven role fields, all String hex */ }
pub struct Oklch { pub l: f32, pub c: f32, pub h: f32 }

pub fn srgb_to_oklch(hex: &str) -> Option<Oklch>;
pub fn oklch_to_srgb(c: Oklch) -> String;          // clamped, "#rrggbb"
pub fn apca_lc(text_hex: &str, bg_hex: &str) -> f32;
pub fn derive(colors: &BTreeMap<String, String>, mode: &str)
    -> Option<DerivedPalette>;
```

### R2 — `looks.rs` extension

`LookDef` gains `blurb: String` and `tags: Vec<String>`. Tags are a closed
set so the UI can build filter chips without string soup:

`structure` | `complete` | `minimal` | `dense` | `powerline` | `framed` |
`two-line` | `nerd-font` | `ascii-safe`

Every curated Look is tagged `structure` (respects your palette) or
`complete` (brings its own). This is the whole of the "preset bundle"
concept — no third entity, because a Look patch can already carry `theme`
keys, which is how `tokyo-rainbow` works today.

Curated palettes move from the `match` in `curated_palette` to a table
carrying `key`, `label`, `blurb` and the eleven roles, so the same metadata
is available to the CLI and the GUI. The existing `curated_palette(key)`
signature is kept as a thin lookup over that table so no call site breaks.

### R3 — protocol (v0.6, additive only)

`preview` gains an optional `scenes` array. Each entry is the existing
per-request context fields (`cwd`, `exit_code`, `cmd_duration_ms`,
`git_branch`, `git_staged`, `git_unstaged`, `in_ssh`, `jobs`, `cols`), and
the shared `patch` / `look` / `style_*` fields apply to all of them. The
response gains `renders: [{left, right}, ...]`.

A request with no `scenes` behaves exactly as today and still returns
top-level `left` / `right`, so `Panel.qml` and the CLI are unaffected.

This matters for performance: the terminal mock needs six lines, and six
requests would be six round-trips plus six broker entries.

`palettes` response entries gain `label`, `blurb`, `source`
(`"curated"` | `"derived"`) and a flat `colors` object, so a surface can
draw swatches without reconstructing them from the `theme` patch.

### Q1 — `TerminalPreview.qml`

The hero component. Renders a framed terminal mock:

- Background is the **previewed palette's** `background`, not the panel's.
  A preview drawn on the panel surface is not a preview of a terminal.
- Content is `Text.StyledText` fed by `Model.ansiToRich`, in the terminal
  font, at a fixed column count shown in the frame caption.
- Scene rows come from `renders[]` in one response.
- A `Swatches` strip sits under the frame.
- Empty and error states are explicit (`no daemon`, `unrepresentable
  patch`) rather than a blank box.

Properties: `renders` (array), `palette` (object), `cols` (int),
`terminalFont` (string), `caption` (string), `state` (`"ok"` | `"loading"`
| `"error"`), `errorText`.

The component performs no I/O. It is handed renders; the surface fetches
them.

### Q2 — `Preview.js`

`.pragma library`, node-testable:

- `SCENES` — the catalog of scene contexts (clean repo, staged+unstaged,
  failed command with a duration, SSH host, deep path, empty dir).
- `buildScenesRequest(ctx, patch, look, scenes, id)` — one NDJSON line.
- `debouncer(ms)` — hover coalescing, returned as a closure so the timing
  is testable without a QML `Timer`.

### Q3 — `PresetCard.qml`

Replaces both the Studio Look card and the Gallery card. Shows a live
one-line prompt render on the preset's own background, the label, the
blurb, and its tags. Selected and hover states use `Card`'s existing
`surface` + `tint` composition — never a bare `Style.*Fill`, which is a
4-8% tint and would render a ~96% transparent card.

### Q4 — `Chip.qml`

The chip markup is currently copy-pasted in `Studio.qml`,
`StudioTheme.qml` (twice), `StudioWizard.qml` and `StudioPrompt.qml`, with
slightly different padding in each. Extracted once, with an optional
`swatch` color and `swatches` array so a theme chip can carry its actual
colors — which is the fix for D2.

### S1 — `Studio.qml` two-pane workbench

```
+----------------------------------------------------------+
| Omarchy10k Studio            * daemon running     [x]     |
| [Looks] [Prompt] [Rice] [Theme] [System] [Setup]          |
+---------------------------------+------------------------+
|                                 |  TerminalPreview       |
|   tab content (scrolls)         |  (pinned, never         |
|                                 |   scrolls away)         |
|                                 |                         |
|                                 |  Swatches               |
|                                 |  [Apply] [Revert]       |
+---------------------------------+------------------------+
```

Canvas sizing changes from `min(1040, w-64) x min(720, h-64)` to
`min(1440, w*0.9) x min(920, h*0.9)`, which removes D3 and gives the
preview pane room to be legible.

The preview pane is present on Looks, Prompt, Theme and Setup. Rice and
System have nothing to preview and use the full width.

### S2 — `StudioLooks.qml`

Absorbs `Gallery.qml`'s function on the shared `Service`:

- Search field and tag filter chips.
- `PresetCard` grid, responsive column count from available width.
- Selecting a card previews it; `Apply` commits through
  `service.applyLook`.
- Detail editor (per-field patch editing, ramp designer, save/overwrite/
  delete) behind a `Loader`, reusing `Service`'s existing verbs rather
  than `Gallery.qml`'s private socket.

### M1 — retiring `Gallery.qml`

`Studio.open()` currently routes `{"page":"gallery"}` to a `Loader` over
`SessionPicker.qml`. After this change it selects the Looks tab instead.
The summon contract is therefore unchanged for callers:

```
omarchy-shell shell summon community.omarchy10k '{"page":"gallery"}'
omarchy-shell call community.omarchy10k.gallery
```

both still open a Looks browser. `Service.qml:openGallery()` and
`PanelLooks`'s button keep working with no signature change; only the
surface they land on is different. `{"page":"sessions"}` continues to
delegate to `SessionPicker.qml`.

### M2 — the ANSI table (D5)

`Model.ansiToRich` gains an optional palette argument; `_ANSI_FG` /
`_ANSI_FG_BRIGHT` become the fallback used only when none is supplied.
Callers pass the palette the preview was rendered with. This is a
correctness fix, not cosmetics: today a Gruvbox user sees Tokyo Night
hexes in every gallery card.

## Performance

Target remains the shared ThinkPad T480 / UHD 620 running one Quickshell
process alongside the Spatial UX plugin.

- **One round-trip per preview**, via `scenes` (R3), served from the
  broker cache already in `Service.qml`. Browsing 18 Looks costs at most
  18 renders, then zero.
- **Hover debounce** at `Motion.MICRO_MS` (90ms), so a mouse crossing the
  grid issues one request, not eighteen.
- **No new offscreen buffers.** `RectangularShadow` only; no
  `layer.enabled`; `Text.StyledText` rather than full rich text.
- **Daemon cost is bounded** by the existing prompt budget: a render is
  the same work the shell does on every prompt, with a 5ms budget, so six
  scenes is well inside a frame.
- The detail editor stays behind a `Loader`, and inactive tabs stay
  uninstantiated as they are today.

## Error handling

| Condition | Behavior |
|---|---|
| No daemon | Preview pane shows `no daemon — start a shell with the Omarchy10k prompt`; cards fall back to name-only; nothing throws |
| Preview returns `status:"error"` | Pane shows the daemon's message; the broker key is released so the next hover retries |
| Theme has no `colors.toml` | That theme is offered but marked `no palette`; derivation is skipped rather than guessed |
| Derived role cannot reach its Lc target | The best achievable value is used and the palette is tagged `low-contrast`, surfaced in the UI rather than silently shipped |
| Socket drops mid-request | Existing `previewRelease` path; no stranded keys |

## Testing

**Rust**
- OKLCH round-trip within tolerance across a fixed color corpus.
- `apca_lc` against published APCA reference pairs.
- **Every theme in `/usr/share/omarchy/themes/` derives a palette whose
  roles meet their Lc targets** — the test enumerates the directory, so a
  new shipped theme that breaks derivation fails CI.
- `white` (no `orange`) exercises the fallback chain.
- `vantablack` and `white` stay monochrome after repair (chroma unchanged).
- `hackerman`'s invisible `muted` is measurably repaired.
- Preset table well-formed: unique keys, non-empty labels/blurbs, tags
  from the closed set, every `structure` Look free of `theme` keys.
- `scenes` request renders N results; omitting `scenes` preserves the
  current top-level `left`/`right` response shape.

**JS (node)**
- `ansiToRich` with an injected palette maps indexed colors to it, and
  without one falls back to the built-in table.
- Scene catalog well-formed; request builder emits one newline-terminated
  line (the missing-newline bug that made Looks never load).
- `debouncer` coalesces a burst into one call.

**QML (`qmltestrunner`)**
- `TerminalPreview` renders one row per render, and its background equals
  the palette background, not the panel's.
- `PresetCard` surface is opaque (`color.a === 1`).
- `Chip` renders its swatches when given them and omits the slot when not.
- `qmllint` gate extended to the new files, including the `\uXXXX`
  4-digit rule.

**Live**
- Screenshot every tab after `omarchy restart shell`. `rescanPlugins`
  does not reload changed QML, so a restart is mandatory before believing
  anything.
- `omarchy plugin validate community.omarchy10k` stays VALID.

## Increments

Each produces a working, testable surface.

1. `palette_derive.rs` + tests. No UI.
2. `looks.rs` metadata and the expanded curated tables + tests.
3. `preview{scenes}` and the enriched `palettes` verb + tests.
4. Kit: `Chip`, `Swatches`, `Preview.js` + tests.
5. `TerminalPreview` + `PresetCard` + tests.
6. `Studio.qml` two-pane workbench; preview live on Prompt and Theme.
7. `StudioLooks.qml`; delete `Gallery.qml`; re-point the summon route.
8. Re-skin `PanelLooks` / `PanelStyle` / `PanelBehavior` on the kit.
9. Wiki update (`quattro.md`, `protocol.md`, `theme.md`, `architecture.md`,
   `config.md`, `INDEX.md`).
