# Terminal Ricing Intel (2026)

[← Index](INDEX.md) | [Architecture](architecture.md) | [Quattro](quattro.md)

What the terminal-ricing world does in 2026 that Omarchy10k does not, assessed
against this project's two hard constraints: the **sub-5 ms prompt budget** and
the **shared Quickshell process** with the Spatial UX plugin on an integrated
GPU. Every claim below was verified on this machine, not taken from the web.

## Verified local facts

| Fact | Value |
|------|-------|
| Terminal | Ghostty 1.3.1-arch2 (Kitty graphics protocol: yes; Sixel: no) |
| Font | JetBrainsMono Nerd Font — the only Nerd Font installed |
| Nerd Fonts glyph index | 10,996 named glyphs (`glyphnames.json`) |
| Animal glyphs renderable in that font | **56** (project offered 23, of which 22 were mislabelled — now fixed at 38) |
| Anime/manga/kawaii/chibi glyphs | **none exist** in any Nerd Fonts collection |
| Anime-adjacent Unicode emoji | all present via Noto Color Emoji (installed) |
| Half-block sprite pipeline | verified: 22×24 px → 12 rows of pure ANSI |
| `TermCaps` capabilities detected but **never consumed** | `has_kitty_graphics`, `has_sixel`, `has_osc52` |

## 1. Inline graphics (Kitty protocol) — the biggest unused lever

`TermCaps::detect()` already sets `has_kitty_graphics`, and **nothing reads
it**. Ghostty and WezTerm both adopted Kitty's protocol, which has effectively
standardised it; Ghostty 1.3.1 here supports it.

**Where it must NOT go: the prompt.** A prompt re-renders on every keystroke
that redraws the line. Image escape sequences are large, they are not
readline-safe (they would wreck the width accounting the Bug Audit already
flags as an architectural weakness), and they would blow the 5 ms budget. This
is the tempting idea that must be refused.

**Where it fits well** — surfaces rendered once, off the hot path:

| Surface | Use |
|---------|-----|
| Looks Gallery cards | Real rendered thumbnails instead of ANSI text previews |
| `omarchy10k intro` | A proper splash on first run |
| `doctor` | Inline palette/preview strip |

Rust route: `viuer`, or the encoder inside `ratatui-image` (which supports
Kitty, Sixel, iTerm2 and Unicode half-blocks behind one API). Half-blocks are
the graceful fallback for terminals with no protocol — and the existing
`TermCaps` matrix is exactly the right place to choose between them.

**Sixel** (`has_sixel`) is lower value here: Foot supports it, Ghostty does
not, and Ghostty is the project's primary target.

## 2. OSC 52 clipboard — a cheap, immediate win

`has_osc52` is detected and unused. OSC 52 lets the *terminal* put data on the
system clipboard, which works over SSH where `wl-copy` does not.

Fits `look export --clipboard` directly: today it shells out to `wl-copy` then
`xclip`, both of which fail on a remote session. An OSC 52 path gated on
`TermCaps` makes "copy this Look and paste it on another machine" work over
SSH. Small, self-contained, no new dependency.

## 3. Glyphs: animals, and the honest answer on anime

### Animals — was broken, now fixed and expanded

The panel's animal picker had **22 of 23 codepoints wrong**. They all
*rendered*, which is why it went unnoticed — they rendered the wrong thing:

| Label | Codepoint | Actually was |
|-------|-----------|--------------|
| Cat | `U+F0B58` | `md-account_group_outline` |
| Panda | `U+F02E3` | `md-bed` |
| Sheep | `U+F1077` | `md-face_woman` |
| Fish | `U+F0143` | `md-chevron_up` |
| Dragon | `U+EE01` | `extra-progress_empty_mid` |

Only `bee` was right. The set is now generated from `glyphnames.json`,
preferring the Material Design family for visual consistency, with every
codepoint verified against the installed font via a fontconfig charset query.
23 → 38 entries.

**The lesson generalises:** any hand-typed Nerd Font codepoint in this repo is
suspect. A generated, font-verified table is the only safe way to ship glyphs.

### Anime glyphs — no, and here is what to do instead

Nerd Fonts aggregates Font Awesome, Material Design Icons, Octicons, Codicons,
Devicons and Weather Icons. All are **UI iconography**. A scan of all 10,996
names finds zero hits for anime, manga, kawaii, chibi, otaku or waifu. There is
no anime glyph to set, in any patched font.

Three real routes, in order of what I would actually do:

1. **Kaomoji as prompt characters** — `(◕‿◕)`, `(╯°□°)╯`, `ヽ(•‿•)ノ`, `( ˘ω˘ )`.
   Pure text, zero cost, works in every terminal and over SSH, and needs **no
   new code**: `GlyphCatalog::prompt_char()` already falls through to "the key
   itself as a custom string". This is shippable today as a curated preset
   list. Caveat: East Asian width — the daemon already uses `unicode-width`,
   so the layout engine measures them correctly, but they are wide.
2. **A Japan/geek-adjacent Nerd Font set** — 22 verified renderable glyphs
   exist and read as themed even if they are not anime: `md-ninja`,
   `fa-torii_gate`, `fae-sushi`, `md-noodles`, `md-rice`, `md-tea`, `md-fan`,
   `md-domino_mask`, `md-drama_masks`, `md-shield_sword`, `md-alien`,
   `md-robot_happy`, `md-ghost`, `md-flower_tulip`, plus 36 `md-emoticon_*`
   faces. Cheap to add as a second glyph category beside Animals.
3. **Real anime art via Kitty graphics** — the only way to get an actual
   character into the terminal. Belongs in the Gallery/intro (see §1), never
   in the prompt.

A fourth route, **custom font patching**, is possible (ship a patched font with
custom glyphs in the Private Use Area) but is a distribution problem, not a
code problem, and it breaks the moment the user picks another font. Not
recommended; the existing "custom string" escape hatch covers the same need.

### Other glyph libraries, surveyed

The question "is there another anime glyph library?" splits in two, because
the terminal and the Quattro GUI have completely different constraints. The
terminal can only render **font glyphs**; QML can render **any SVG**.

| Source | Anime content | Terminal? | Quattro GUI? | Verdict |
|--------|---------------|-----------|--------------|---------|
| Nerd Fonts (all 6 collections) | none | ✅ font | ✅ | Already used. No anime, 22 JP-adjacent glyphs verified |
| **Unicode emoji** (Noto Color Emoji, installed) | ⛩ 🏯 🎌 🍥 👘 🎎 🎏 👺 🦊 🌸 — **all verified present** | ✅ | ✅ | Real option, real caveats — see below |
| **Iconify** — [game-icons](https://icon-sets.iconify.design/game-icons/), 4,133 icons, CC BY 4.0 | Creatures, dragons, masks, katana — fantasy not anime | ❌ SVG only | ✅ | Best GUI-side option. Attribution required |
| Flaticon / IconScout / SVGRepo | Genuine anime characters | ❌ SVG only | ⚠ licence | Attribution-or-paid, per-icon terms. Not worth the compliance burden for a shipped default |
| **Half-block sprite rendering** | **anything**, including real anime art | ✅ pure ANSI | ✅ | The actual answer — see below |

**Unicode emoji caveats** (why they are not the default): they are
double-width, their colour is baked into the font so they ignore the theme
palette entirely — breaking the project's whole theme-native premise — and a
colour emoji font next to JetBrainsMono looks pasted-on. Fine as an opt-in
catalog; wrong as a default.

### Half-block sprites — the real way to get anime into a terminal

The ricing community solved this years ago and it is not a font problem.
`krabby`, `kingler`, `pokescript` and `pokeget` (all **Rust**) print Pokémon
sprites by converting a small PNG into coloured Unicode half-blocks: one `▀`
per character cell, foreground = upper pixel, background = lower pixel. It is
**pure ANSI text** — no graphics protocol, works over SSH, works in every
truecolor terminal.

Verified locally: a 22×24 px image becomes 12 rows × 22 columns of ANSI. The
technique generalises to *any* image, so "put my favourite character in the
terminal" is a sprite pipeline, not a glyph hunt.

Where it fits here:

- **`omarchy10k intro`** — a mascot on first run. Rendered once, off the hot
  path, and it degrades to nothing on a non-truecolor terminal.
- **Looks Gallery cards** — same half-block encoder covers the thumbnail idea
  in §1 *without* needing the Kitty protocol at all, which makes it work on
  Foot and Alacritty too. This is a better first step than Kitty graphics.
- **Not the prompt.** A 12-row sprite in `PS1` is unusable regardless of how
  cheap the encoding is.

Cost is small and self-contained: the encoder is ~40 lines against `image`,
or take `ratatui-image`'s, which picks Kitty/Sixel/iTerm2/half-blocks by
capability — and `TermCaps` already has the matrix to drive that choice.

**Licensing reality:** shipping actual anime sprites is a copyright question,
not a technical one. The defensible shape is what krabby does — ship the
*renderer*, let the user point it at their own image (`[intro] sprite =
"~/.config/omarchy10k/mascot.png"`), and ship at most a small
originally-drawn or permissively-licensed default.

## 4. A glyph browser in the Studio

We have 10,996 glyph names and the font locally. A searchable picker — type
"cat", see every matching glyph *as it will actually render in your font* —
is cheap (no network, one JSON read), high-value, and would have made the
mislabelled-animal bug impossible to ship.

This is the single best ricing feature-per-line-of-code available to us, and it
fits the Studio's Prompt tab naturally.

## 5. Terminal Modes (already designed, still unbuilt)

`next-level-brainstorm.md` Tier B1. Focus / Presentation / SSH / Production
personalities that re-render the terminal include (cursor accent, opacity,
background tint) plus a prompt patch. The Rice tab is now the natural home and
was laid out to host it.

Open spike, unchanged: **does Ghostty live-reload an include changed outside
`theme-set`?** Worth answering before designing further — the whole feature
rests on it.

## 6. Import from other prompt ecosystems

`omarchy10k migrate` handles `starship.toml`. **oh-my-posh** has a large JSON
theme ecosystem and is the other big configurable-prompt project; its themes
are structurally close to a Look (segments + styles + separators). A
`migrate --from ohmyposh` would widen the on-ramp for the same reason the
Starship importer did.

## 7. Things deliberately NOT recommended

| Idea | Why not |
|------|---------|
| Images in the prompt | Re-renders per keystroke, breaks readline width accounting, blows the 5 ms budget |
| Animated/spinner prompt | Bash cannot animate a static `PS1`; `OSC 9;4` progress already covers "something is running" |
| Sixel support | Ghostty (primary target) does not implement it |
| Shipping a patched font | Distribution burden, and it breaks when the user changes fonts |
| Cross-shell (zsh/fish) | Already on the ratified kill list — Omarchy is bash-first |

## Recommended order

1. **Glyph browser** in the Studio (highest value per line; prevents a whole bug class)
2. **Kaomoji + JP-adjacent glyph sets** (zero new machinery, pure catalog)
2b. **Half-block sprite encoder** — user-supplied image for `intro` and Gallery
    thumbnails. Works on every truecolor terminal, no graphics protocol, and
    supersedes the Kitty-thumbnail item below as the first step
3. **OSC 52** for `look export --clipboard` (small, fixes a real SSH gap)
4. **Kitty-graphics thumbnails** in the Gallery (visible payoff; strictly off the prompt path)
5. **oh-my-posh import** (on-ramp)
6. **Terminal Modes** (after the Ghostty live-reload spike)

## Sources

- [Nerd Fonts — glyph sets and code points](https://github.com/ryanoasis/nerd-fonts/wiki/Glyph-Sets-and-Code-Points)
- [Kitty terminal graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- [Kitty graphics protocol — terminal support matrix](https://terminfo.dev/extensions/kitty-graphics-protocol)
- [Terminal graphics protocols: Kitty, Sixel, iTerm2 and beyond](https://akmatori.com/blog/terminal-graphics-protocols)
- [ratatui-image (Kitty/Sixel/iTerm2/half-blocks in Rust)](https://github.com/ratatui/ratatui-image)
- [krabby — Pokémon sprites in the terminal, Rust](https://github.com/yannjor/krabby)
- [pokeget — faster sprite renderer, Rust](https://lib.rs/crates/pokeget)
- [Iconify game-icons — 4,133 CC BY 4.0 icons](https://icon-sets.iconify.design/game-icons/)
- [Iconify icon-sets — 200+ open source collections](https://github.com/iconify/icon-sets)
- [Terminal & shell tools 2026 deep dive](https://www.youngju.dev/blog/culture/2026-05-16-terminal-shell-tools-2026-ghostty-wezterm-alacritty-warp-fish-4-nushell-zellij-starship-deep-dive.en)
- [State of Linux terminal emulators in 2026](https://dev.to/shrsv/state-of-linux-terminal-emulators-in-2026-1gh5)
