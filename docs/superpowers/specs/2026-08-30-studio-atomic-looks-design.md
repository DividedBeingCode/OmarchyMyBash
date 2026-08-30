# Studio: atomic Looks, a legible glyph viewer, and a deeper preset library

Makes a preset apply as the atomic bundle it is presented as, makes the glyph
browser legible, splits the three over-stacked tabs, and grows the preset
library from 29 to 52.

**Date:** 2026-08-30
**Status:** approved for planning

## Problem

Three faults, measured rather than assumed.

### 1. A preset does not look like its card once applied

A smoke test over every ordered pair of Looks — apply A, then apply B, and
compare B's rendered prompt against B's gallery card — fails on **168 of 812
pairs**. The victims are the `structure` presets:

```
framed-focus      21/28 predecessors    lean-pure       20/28
powerline-classic 20/28                 zen-fade        20/28
dot-matrix        19/28                 mono-minimal    19/28
two-line-focus    19/28
```

Exactly three keys genuinely leak across an apply:

| key | pairs affected |
|-----|----------------|
| `style.frame.gap_char` | 100 |
| `style.frame.gap_gradient` | 100 |
| `prompt.newline` | 42 |

(`theme.custom` / `theme.source` / `theme.ramp` also appear in the raw counts;
those are an artifact of the probe using `Config::default()` as the card's
baseline rather than the live config. In the running Studio the card carries
the live theme, so they match.)

The cause is a mismatch between how a Look is *presented* and how it is
*applied*. A Look is described everywhere — the card, the docs, the README —
as an atomic appearance bundle. It is applied as a **delta**: `apply_transient`
and `write_config_patch` both merge the patch onto whatever is already there,
so every key a patch omits silently inherits from the previously applied
preset.

The card fix in `b12129f` (rendering cards on `base: "default"`) made the
gallery internally consistent but did not touch Apply — it moved the mismatch
to the point where the user could finally see it.

### 2. Glyphs are unreadable in the browser

`GlyphCell` renders its glyph at `Style.font.subtitle` — a fixed **13 px** —
inside a tile sized `(browser.width - gaps) / 10`, roughly **64 px** in the
Studio's left column. The glyph occupies about a fifth of its own cell, which
defeats the browser's stated purpose: to show a glyph as it will actually
render so that tofu and wrong codepoints are obvious.

Two smaller defects found while surveying the catalog:

- `dragon` is declared **twice** (once `category: "Animals"`, once
  `category: "Japan"`), so it appears twice in the grid.
- The daemon resolves 11 kaomoji; the Studio catalog lists 8. `kaomoji_relaxed`,
  `kaomoji_smirk` and `kaomoji_disapprove` are usable but not browsable — and
  the shipped `rose-classic` Look uses `kaomoji_disapprove`, so a preset
  depends on a glyph the picker cannot show.

### 3. Tabs stack more than a screen

`StudioPrompt.qml` (408 lines) renders one column in this order:

```
STYLE PRESET → SEPARATOR → PROMPT CHARACTER → ALL GLYPHS → BEHAVIOR
```

`ALL GLYPHS` is 76 tiles at 10 columns — eight rows, several hundred pixels —
and `BEHAVIOR` (the per-segment toggles, the controls changed most often) sits
below it. `StudioTheme.qml` (388 lines) and `StudioSystem.qml` (353) stack
three sections each in the same way.

### 4. The preset library under-uses its own material

29 Looks are built from 11 style presets, 14 separators, 39 curated palettes,
16 OS icons and 76 glyphs. Whole glyph families are unused: `ninja torii sushi
noodles rice tea fan mask drama katana`, `alien robot ghost`, `sakura crown
sword`, and every kaomoji but four.

## Design

### Section 1 — Looks become atomic

Introduce a canonical set of keys a Look owns, in `crates/omarchy10kd/src/looks.rs`:

```rust
/// The config paths a Look owns.
///
/// Applying a Look CLEARS these before merging its patch, so a Look is the
/// atomic bundle it is presented as. Without this, a patch is a delta: every
/// key it omits inherits from whatever preset was applied last, which is why
/// 168 of 812 ordered pairs rendered differently from their gallery card.
///
/// Deliberately excludes anything the user owns rather than the preset:
/// segment enable/disable, `git.mode`, `directory.*`, `terminal.*`, plugins,
/// rice and statusline all survive an apply untouched.
pub const LOOK_OWNED: &[&str] = &[
    "style",
    "prompt.layout",
    "prompt.newline",
    "prompt.blank_line",
    "segments.os.icon",
    "segments.character.success",
    "segments.character.error",
    "segments.character.transient",
    "git.branch_icon",
];
```

`theme` is **not** in the list. It stays governed by the existing
`structure`/`complete` rule: a `complete` Look's patch carries a `theme` block
and the existing "a patch that sets `theme.custom` replaces the palette
wholesale" rule applies; a `structure` Look carries none and leaves the live
theme alone.

Add one function:

```rust
/// Clear the Look-owned paths from a config table, so a subsequent patch
/// merge lands on defaults rather than on the last preset's leftovers.
///
/// Clears by REMOVAL rather than by writing explicit defaults: removal leaves
/// config.toml clean and keeps the panel's modified-vs-default ink honest.
/// Writing defaults would mark every owned key as user-modified.
pub fn clear_look_owned(doc: &mut toml::Table);

/// Apply a Look atomically: clear what a Look owns, then merge its patch.
pub fn apply_look(current: &Config, patch: &serde_json::Value) -> Result<Config, String>;
```

`apply_look` is `apply_transient` with a `clear_look_owned` call inserted
between the serialize and the merge. `apply_transient` stays for callers that
genuinely want a delta (the Look editor's working patch, project profiles).

**Both apply paths and the preview path call it.** This is the structural
point of the section: the gallery matches Apply because it is the same
function, not two paths that happen to agree today.

| Call site | File | Change |
|-----------|------|--------|
| `looks_apply`, transient | `server.rs` | `apply_transient` → `apply_look` |
| `looks_apply`, persistent | `server.rs` | `write_config_patch` → `write_look_patch` |
| Preview with a named Look | `server.rs::effective_preview_config` | `apply_transient` → `apply_look` |

`write_look_patch` is `write_config_patch` with `clear_look_owned(&mut doc)`
before the merge loop — the file gets the same treatment as the in-memory
config, or a restart would resurrect the leftovers.

**The `base` field is deleted.** `PreviewRequest.base`, its use in
`effective_preview_config`, the `base` plumbing through `Preview.js`,
`Service.qml` and `StudioLooks.qml`, and its row in `protocol.md` all go.
Once applying is atomic, a card rendered on the live config is *already*
stable across applies, so the workaround is redundant — and rendering on the
live config is strictly better, because the card then also reflects the user's
own segment toggles. Net: one protocol field removed rather than added.

`Preview.js::cacheKey` drops its `base` component; its two new tests in
`tests/preview_test.js` are replaced by the pair-stability test below.

### Section 2 — A legible glyph viewer

In `quattro/o10k/GlyphCell.qml`:

- Add `property real glyphSize: tile.width * 0.5`, and bind the `Text`'s
  `font.pixelSize` to it instead of `Style.font.subtitle`.
- Show the glyph's name under it when the tile is selected; hover keeps the
  existing tooltip. The name is `Style.font.caption` and does not change the
  tile's height (it overlays the lower strip), so the grid does not reflow on
  selection.

In `quattro/o10k/GlyphBrowser.qml`:

- `columns: 10` → `8`. At the Studio's left-column width this puts tiles near
  80 px and glyphs near 40 px.
- Add a row of category chips above the grid: `all · Prompt · Animals · Japan ·
  Kaomoji`, filtering on the `category` field the catalog already carries.
  Chips combine with the existing search box (chip narrows, then query
  filters).
- Add `property string category: ""` and fold it into `results`.

In `quattro/StudioPrompt.qml`:

- Remove the duplicate `dragon` entry (keep the `Animals` one; `Japan` already
  has ten entries without it).
- Add the three missing kaomoji — `kaomoji_relaxed`, `kaomoji_smirk`,
  `kaomoji_disapprove` — so the catalog covers everything the daemon resolves.
  The existing `catalog_parity_tests` gate is extended to fail when the daemon
  resolves a key the Studio does not list, which is what let this drift.

### Section 3 — A second-level rail

New component `quattro/o10k/SubRail.qml`: a horizontal chip row, visually
lighter than `Studio.qml`'s tab rail (smaller radius, no accent fill — the
active sub-tab is underlined), taking `property var tabs` and
`property int current`, emitting `switched(int)`.

Applied to the three tabs that stack:

| Tab | Sub-tabs | Sections moved |
|-----|----------|----------------|
| **Prompt** | Style · Glyphs · Segments | `STYLE PRESET`+`SEPARATOR`+`PROMPT CHARACTER` / `ALL GLYPHS` / `BEHAVIOR` |
| **Theme** | Themes · Palettes · Gradient | `OMARCHY THEMES` / `PIN TERMINAL COLORS` / `GRADIENT` |
| **System** | Sessions · Plugins · Layer | `SESSIONS` / `SEGMENT PLUGINS` / `SHELL LAYER` |

Each tab keeps its existing `Flickable` and its `WheelBoost`; the sub-tab
selects which section is instantiated. The pinned preview pane is unaffected —
it is owned by `Studio.qml`, not by the tab bodies.

Sub-tab selection is held in each tab component (`property int subTab: 0`) and
therefore persists while the Studio is open and resets when it closes. Not
persisted to config: it is navigation state, not a setting.

### Section 4 — 29 → 52 Looks

Each entry names a curated palette, a style preset, a separator shape and its
characters. Every key below was verified to exist in
`available_symbol_chars`, `available_separators` and `CURATED_PALETTES`.

**Ukiyo** (`complete`, uses the untouched `Japan` glyph family)

| name | palette | preset · separator | os icon / character |
|------|---------|--------------------|---------------------|
| `torii-dusk` | kanagawa | slanted · slanted | `torii` / `torii` |
| `sushi-bar` | rose-pine | classic · dot | `sushi` / `sushi` |
| `ramen-shop` | gruvbox | dense · vertical | `noodles` / `noodles` |
| `sakura-drift` | rose-pine-moon | gradient · fade | `sakura` / `sakura` |
| `tea-house` | everforest | lean · vertical | `tea` / `tea` |
| `steel-katana` | iceberg | powerline · flame | `katana` / `katana` |
| `noh-mask` | zenburn | framed · slanted | `mask` / `drama` |

**Sci-fi** (`complete`)

| name | palette | preset · separator | os icon / character |
|------|---------|--------------------|---------------------|
| `xenomorph` | scarlet-protocol | gradient · flame | `alien` / `alien` |
| `bot-farm` | oxocarbon | powerline · powerline | `robot` / `robot` |
| `ghost-shell` | poimandres | gradient · fade | `ghost` / `ghost` |
| `blue-cascade` | blue-matrix | dense · dot | none / `lambda` |
| `deep-space` | andromeda | rainbow · powerline | none / `triangle` |

**Expressive** (`complete`, uses the kaomoji family)

| name | palette | preset · separator | character |
|------|---------|--------------------|-----------|
| `shrug-life` | snazzy | lean · vertical | `kaomoji_shrug` |
| `sleepy-dev` | nightfox | pure · dot | `kaomoji_sleepy` |
| `hype-machine` | neon | rainbow · powerline | `kaomoji_cheer` |
| `zen-mode` | iceberg | minimal · none | `kaomoji_relaxed` |

**Regal**

| name | palette | preset · separator | os icon / character |
|------|---------|--------------------|---------------------|
| `crown-jewels` | aura | powerline · round | `crown` / `crown` |
| `swordsman` | horizon | slanted · slanted | `sword` / `sword` |

**Structure-only** (`structure` — respect whatever palette you are on)

| name | preset · separator | note |
|------|--------------------|------|
| `ascii-only` | classic · vertical | tagged `ascii-safe`; no Nerd Font glyph anywhere, `os.icon = none`, `branch_icon = text` |
| `single-line` | lean · dot | `prompt.newline = false`, `blank_line = false` |
| `wide-load` | dense · powerline_thin | every segment on one dense line |
| `round-trip` | powerline · round | the rounded-cap variant nothing currently uses |
| `diamond-cut` | powerline · diamond | ditto for `diamond` |

That is 23 new Looks, taking the library from 29 to 52. The README currently
says 28 and is already one behind; it is corrected as part of this work, along
with the Looks count in `INDEX.md`. `TAGS` is unchanged —
every entry uses the existing closed vocabulary.

## Testing

| Test | Location | Asserts |
|------|----------|---------|
| `a_look_looks_the_same_applied_as_it_did_in_the_gallery` | `server.rs` | For all 812 ordered pairs (A then B): the render of B's gallery card equals the render of B applied. This is the headline invariant and currently fails 168 times. |
| `the_leak_is_real_without_atomic_apply` | `server.rs` | The same sweep using `apply_transient` still leaks, so the invariant test cannot pass vacuously. |
| `applying_a_look_leaves_your_own_settings_alone` | `server.rs` | Segment enable/disable, `git.mode`, `directory.*` and `terminal.*` survive an apply unchanged. |
| `a_structure_look_keeps_the_palette_you_are_on` | `server.rs` | Existing test, retargeted at `apply_look`. |
| `clear_look_owned_removes_rather_than_defaults` | `looks.rs` | The cleared keys are absent from the table, not written as explicit defaults. |
| `every_daemon_glyph_is_browsable` | `style.rs` | Extends `catalog_parity_tests`: every key `available_symbol_chars` resolves appears in the Studio catalog. Fails today on three kaomoji. |
| `the_catalog_has_no_duplicate_keys` | `style.rs` | Fails today on `dragon`. |
| `patch_schema_tests` | `looks.rs` | Existing; every new Look's patch is walked against `Config::default()`. |
| `tst_subrail.qml` | `tests/qml/` | Sub-tab switching, bounds, and that an out-of-range index is clamped rather than blanking the tab. |
| `tst_glyphbrowser.qml` | `tests/qml/` | Category filter combines with the search query; `all` restores the full set; glyph size scales with tile width. |

The existing `qmllint` gate, the 812-pair sweep, and `patch_schema_tests`
together mean a bad glyph key, a bad config path, or a regressed apply fails
the build rather than shipping.

## Behaviour change to call out

Applying any preset now **resets** everything in `LOOK_OWNED`. A hand-tuned
separator, a `blank_line = false`, a custom prompt character — all revert when
the next preset is applied. This is the chosen semantics and it is what makes
the gallery trustworthy, but it is a real change from today, where those
survived.

Mitigation already in the product: the Look editor's "Save as new preset"
captures a tuned state as a Look of its own, and the bar popout keeps an undo
timeline.

## Out of scope

- Persisting sub-tab selection across Studio sessions.
- Reworking the bar popout's information architecture; only the Studio's tabs
  change.
- New palettes. The library grows in Looks, not colors.
- The `oh-my-posh` importer and Terminal Modes, both still open from earlier
  ricing work.
