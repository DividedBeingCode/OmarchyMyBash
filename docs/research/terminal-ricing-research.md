# Terminal Ricing: Prompt Theming Techniques

*Survey compiled 2026-08-28. Focus: how p10k-class prompts are actually built — glyphs,
colors, gradients, separator geometry — what zsh gives you for free, what bash needs
faked, and the wider "everything matches" rice around the prompt.*

---

## 1. The landscape

| Tool | Lang | Shells | Model | Notes |
|---|---|---|---|---|
| **Powerlevel10k** | zsh | zsh only | zsh theme, in-process | The aesthetic reference. `p10k configure` wizard, gitstatusd backend, instant prompt, transient prompt. Maintainer has publicly described it as on life support; still works. |
| **Starship** | Rust | bash, zsh, fish, pwsh, nu, elvish, ion, tcsh, xonsh, cmd | External binary, one `starship.toml` | The current default recommendation. Format-string template model + named palettes. |
| **oh-my-posh** | Go | same breadth + Windows-first heritage | External binary, JSON/YAML/TOML theme | Richer segment *styles* (plain/powerline/diamond/accordion), Go templating with conditional color templates, drag-and-drop web configurator. |
| **Pure** | zsh | zsh | Minimalist theme | The anti-powerline: two lines, no backgrounds, one accent color. p10k ships a `pure` preset that clones it. |
| **Spaceship** | zsh | zsh | Segment-based theme | Older, slower, large segment library. |
| **Tide** | fish | fish | fish's p10k equivalent | Interactive configure wizard, same visual grammar. |
| **powerline / powerline-shell** | Python | many | The original | Historically slow (Python startup per prompt). Its font-patching legacy is why we have Nerd Fonts. |
| **liquidprompt** | bash/zsh | bash, zsh | Pure-shell, no binary | Interesting for bash: proves a rich prompt is doable with zero compiled deps. |
| **ble.sh** | bash | bash | Full line editor replacement | Not a prompt theme — a *substrate*. Gives bash right-prompt, transient prompt, syntax highlighting, autosuggestions. See §7. |

**The shape of the market:** two compiled cross-shell engines (Starship, oh-my-posh) have
absorbed the aesthetic that p10k invented, because a compiled binary sidesteps the
fork-per-prompt cost that killed Python powerline and makes the same config work in every
shell. Everything below is technique-level and tool-agnostic.

---

## 2. Anatomy of a riced prompt

The vocabulary is remarkably consistent across tools. Learning p10k's variable names is the
fastest way to learn the whole design space, because it named every part.

```
╭─ ⌂ ian ▓ ~/Work/project ▓  main ▓ !2 ?1 ────────── ⏱ 2.4s  14:32 ─╮
╰─ ❯
   ^  ^                                                              ^
   |  prompt char                                            frame right
   frame left
```

### Parts list

| Part | p10k variable | Starship equivalent | What it does |
|---|---|---|---|
| Segment | `POWERLEVEL9K_LEFT_PROMPT_ELEMENTS` | `format = "$dir$git_branch..."` | A unit of information with its own fg/bg |
| Segment separator (color changes) | `LEFT_SEGMENT_SEPARATOR=''` | manual glyph in `format` | The powerline arrow between differently-colored segments |
| Subsegment separator (same color) | `LEFT_SUBSEGMENT_SEPARATOR=''` | manual | Thin divider inside a color run |
| Frame | `MULTILINE_FIRST_PROMPT_PREFIX='%242F╭─'`, `NEWLINE_..._PREFIX='├─'`, `LAST_..._PREFIX='╰─'` (right side: `─╮ ─┤ ─╯`) | manual in `format` | Box-drawing bracket around a multi-line prompt |
| Gap / fill | `MULTILINE_FIRST_PROMPT_GAP_CHAR` | `$fill` module (`[fill] symbol = "─"`) | Stretches the first line to the terminal edge, pushing right-side content right |
| Right prompt | `RIGHT_PROMPT_ELEMENTS` (uses zsh `RPROMPT`) | `right_format` | Second cluster anchored to the right margin |
| Prompt char | `PROMPT_CHAR_..._CONTENT_EXPANSION='❯'` / `'❮'` / `'▶'` per vi mode | `[character]` | The thing you type after. Colored by exit status. |
| Newline before prompt | `PROMPT_ADD_NEWLINE=true` | `add_newline = true` | Breathing room between commands |
| Transient prompt | `TRANSIENT_PROMPT=always` | `enable_transience` (not bash) | After you hit Enter, collapse the fat prompt to a bare `❯` in scrollback |
| Instant prompt | `INSTANT_PROMPT=verbose` | n/a | Draw a cached prompt immediately at shell start, reconcile once rc finishes |

### Two structural choices that define the look

1. **Filled vs. unfilled.** "Rainbow"/powerline style paints a **background** on every
   segment, so separators are the interesting problem. "Lean"/Pure style colors only the
   **foreground**, uses spaces as separators, and has no glyph dependency at all. p10k ships
   both; Starship's `plain-text-symbols` / `no-nerd-font` presets are the lean end.
2. **One line vs. two (or three).** Two-line with a frame is the p10k signature: information
   on line 1, typing on line 2, so your command never gets pushed to column 90. The frame
   glyphs (`╭ ├ ╰`) are what makes it read as a coherent object rather than two stray lines.

---

## 3. The glyph layer

### 3.1 Nerd Fonts code point map (v3.5.0)

Everything decorative comes out of the Private Use Area. Ranges worth knowing:

| Set | Range | Use |
|---|---|---|
| **Powerline Symbols** | `E0A0`–`E0A2`, `E0B0`–`E0B3` | The canonical separators + branch/lock/line glyphs |
| **Powerline Extra Symbols** | `E0A3`, `E0B4`–`E0C8`, `E0CA`, `E0CC`–`E0D7`, `2630` | Every alternate separator geometry (§3.2) |
| Pomicons | `E000`–`E00A` | Pomodoro |
| Font Awesome Extension | `E200`–`E2A9` | |
| Weather | `E300`–`E3E3` | Actual use case: a weather segment |
| Seti-UI + Custom | `E5FA`–`E6BB` | Filetype icons |
| Devicons | `E700`–`E958` | Language/tool logos — the ` `, ` `, ` ` you see in toolchain segments |
| Codicons | `EA60`–`EC84` | VS Code's set |
| Font Awesome | `ED00`–`F2FF` (gaps) | Relocated in v3 — **v2 configs break here** |
| Octicons | `F400`–`F533`, `2665`, `26A1` | Git-flavored |
| Font Logos | `F300`–`F384` | Distro logos (the Arch ` `) |
| Material Design | `F500`–`FD46`, plus `F0001`–`F1AF0` | 6000+ icons, the extended plane is where v3 moved them |
| IEC Power | `23FB`–`23FE`, `2B58` | Power/battery |
| Progress indicators | `EE00`–`EE0B` | Spinner frames — usable for async segment "loading" state |
| Braille | `2800`–`28FF` | Not Nerd Font specific, but the sparkline/spinner workhorse |
| Box drawing | `2500`–`259F` | Frames, fills, gradient blocks (`░▒▓█`) |

> **v3 migration gotcha:** Nerd Fonts v3 remapped Font Awesome and Material Design. Old
> configs from 2021-era blog posts render as boxes. If a config's icons are broken, this is
> almost always why.

### 3.2 Separator geometry — the actual design lever

This is the single most under-appreciated ricing knob. Everyone uses `E0B0`. The extra set
gives you a dozen distinct silhouettes:

| Codepoint | Informal name | Character |
|---|---|---|
| `E0B0` / `E0B1` | Hard / thin triangle (right) |  /  |
| `E0B2` / `E0B3` | Hard / thin triangle (left) |  /  |
| `E0B4` / `E0B5` | Half-circle right (hard/thin) |  /  |
| `E0B6` / `E0B7` | Half-circle left (hard/thin) |  /  |
| `E0B8` / `E0B9` | Lower-left triangle (angular 1) |  /  |
| `E0BA` / `E0BB` | Lower-right triangle |  /  |
| `E0BC` / `E0BD` | Upper-left triangle (angular 2) |  /  |
| `E0BE` / `E0BF` | Upper-right triangle |  /  |
| `E0C0` – `E0C3` | Flame |     |
| `E0C4` – `E0C7` | Pixelated / dithered |     |
| `E0C8`, `E0CA` | Ice waveform |   |
| `E0CC` – `E0CF` | Honeycomb / hexagon |     |
| `E0D0` – `E0D1` | Lego |   |
| `E0D2` / `E0D4` | Trapezoid (right / left) |  /  |

Render them yourself with the real font to pick:

```bash
for cp in $(seq 0xE0A0 0xE0D7); do
  printf 'U+%X %b\n' "$cp" "\\U$(printf '%08X' "$cp")"
done | column -c "$(tput cols)"
```

**Design notes on geometry:**

- **Half-circles (`E0B6` … `E0B4`) as end-caps** are the "capsule/pill" look — open with
  `E0B6` in the segment's bg-as-fg, close with `E0B4`. Reads softer than triangles and is the
  basis of oh-my-posh's `diamond` style. Your current starship config uses exactly this
  (`[](fg:blue)` ... `[](fg:blue)`).
- **Trapezoids (`E0D2`/`E0D4`)** are the "tab bar" look — segments read as browser tabs.
  Underused, very distinctive.
- **Upper vs. lower triangles** (`E0B8` vs `E0BC` families) let you build a *zigzag* — alternate
  them and the prompt appears to lean. Nobody does this; it looks great.
- **Thin variants** are for *same-color* runs. Using a thin separator between two different
  backgrounds looks broken; using a hard separator between two identical backgrounds wastes a
  cell. p10k's split into SEGMENT vs SUBSEGMENT separator exists precisely to encode this rule.
- **Pixelated/dithered (`E0C4`–`E0C7`)** is a *gradient in one cell* — it fakes a soft blend
  between two backgrounds. Combined with `░▒▓` block fills you can build a true multi-cell
  fade between segment colors without any real gradient support (§4.4).

### 3.3 The separator color trick

The mechanic everyone re-derives: a right-facing separator between segment A (bg `X`) and
segment B (bg `Y`) is drawn as a character with **foreground `X` and background `Y`**. The
glyph is a solid triangle; painting its foreground with the *previous* segment's background
makes the fill appear to continue and then taper.

```
segment A text   separator   segment B text
bg=X fg=text     fg=X bg=Y   bg=Y fg=text
```

Corollary: for an **end cap** into the terminal background, the separator is `fg=X` with
*no* background — which is exactly the `[](fg:blue)` idiom in your starship config.

### 3.4 Font mechanics that bite

- **Misalignment is endemic.** Powerline glyphs are patched into fonts at a size derived from
  the host font's ascender/descender metrics; terminals that use strict font metrics leave a
  1px seam or a vertically-clipped arrow. Reported continuously since 2012 across
  powerline, powerlevel9k, WezTerm, Zed. Mitigations: use a font where the patch was done
  well (MesloLGS NF is p10k's recommendation precisely because romkatv controlled the patch),
  adjust cell height / line-height in the terminal, or use terminals that *synthesize*
  powerline glyphs instead of using the font's (kitty and Ghostty draw box-drawing and some
  powerline glyphs themselves, sidestepping the whole problem).
- **Ambiguous width.** Many Nerd Font icons are double-width or ambiguous-width. If the shell
  and the terminal disagree about a glyph's cell count, the cursor lands in the wrong column
  and line editing corrupts. This is *the* reason to prefer glyphs from ranges your terminal
  handles predictably, and to test each icon before committing it.
- **A symbols-only font as fallback** (`Symbols Nerd Font`, the `NerdFontsSymbolsOnly`
  release) lets you keep any text font you like and only fall back for the PUA. Good escape
  hatch if you don't want a patched JetBrains Mono.

---

## 4. Color techniques

### 4.1 The three color depths

| Depth | Escape | When to use |
|---|---|---|
| 16 (ANSI) | `\e[3Xm` / `\e[9Xm` | **Underrated.** Colors follow the terminal theme, so the prompt re-themes for free when the user changes colorscheme. p10k's "rainbow" preset deliberately uses palette indices 0–15/0–255, not hex. |
| 256 | `\e[38;5;Nm` | The compromise. p10k defaults here (`DIR_BACKGROUND=4`, `FOREGROUND=254`). |
| 24-bit truecolor | `\e[38;2;R;G;Bm` (fg), `\e[48;2;R;G;Bm` (bg) | Required for gradients and for pinning an exact brand palette. Detect via `$COLORTERM` containing `truecolor` or `24bit`. |

The strategic choice: **hardcoded hex locks your prompt's look; palette indices let it inherit.**
Riced setups that follow the wallpaper (pywal/matugen, §8.3) *must* use indices or regenerate
the prompt config. p10k's rainbow preset is index-based for this reason; Starship's
`[palettes.x]` blocks are hex-based and therefore need regeneration.

### 4.2 Named palettes as the unit of design

Both modern engines support indirection: define colors once, reference by name.

```toml
palette = "p10k"
[palettes.p10k]
blue = "#7047a8"
text = "#0d0914"
# ...
[directory]
style = "bold fg:text bg:cyan"
```

This is the single highest-leverage structural feature — it makes "swap the whole prompt to
Gruvbox" a one-line change, and it is what allows Starship to ship
`gruvbox-rainbow` / `tokyo-night` / `catppuccin-powerline` as variations on *the same layout*.
Starship's official presets: `catppuccin-powerline`, `tokyo-night`, `gruvbox-rainbow`,
`pastel-powerline`, `jetpack`, `pure-preset`, `nerd-font-symbols`, `bracketed-segments`,
`no-empty-icons`, `no-nerd-font`, `no-runtime-versions`, `plain-text-symbols`.

### 4.3 Semantic color — the part that earns its keep

Decorative color is noise; **color that encodes state** is the actual feature. The
established mappings:

- **Exit status** → prompt char green/red. Nearly universal, highest value per character.
- **Git state** → segment background. p10k: `VCS_CLEAN_BACKGROUND=2` (green),
  `VCS_MODIFIED_BACKGROUND=3` (yellow), untracked a third color. You read repo state
  peripherally, without parsing text.
- **Root / elevated** → red username background (`style_root`).
- **SSH / remote** → hostname segment appears *only* when remote, in a distinct color. The
  best segments are the ones that are usually invisible.
- **Slow command** → duration segment appears only above a threshold.
- **Danger contexts** → prod kubernetes context, a `--no-verify` env, a dirty submodule.
  oh-my-posh's `background_templates` / `foreground_templates` are built for exactly this:
  a Go template that picks the color from the segment's own data.

### 4.4 Gradients — how they're actually done

No mainstream prompt engine has a native gradient primitive. Four real techniques:

1. **Stepped palette across segments.** Assign each segment a color sampled along a ramp
   (`#7047a8 → #a66de0 → #cba6f7`). With hard separators the eye reads it as a single
   gradient bar. This is what "pastel-powerline" and "gruvbox-rainbow" actually are. Cheapest,
   most robust, and the one to reach for first.
2. **Per-character interpolation.** Emit one truecolor SGR per character:
   ```bash
   # linear fg gradient across a string
   grad() { local s=$1 r1=$2 g1=$3 b1=$4 r2=$5 g2=$6 b2=$7 n=${#1} i
     for ((i=0;i<n;i++)); do
       printf '\e[38;2;%d;%d;%dm%s' \
         $((r1+(r2-r1)*i/(n-1))) $((g1+(g2-g1)*i/(n-1))) $((b1+(b2-b1)*i/(n-1))) "${s:i:1}"
     done; printf '\e[0m'; }
   ```
   Fine for a fixed banner. In a *prompt* it is a trap in bash: every one of those escapes has
   to be wrapped in `\[ \]` or line-wrap math breaks (§7.2), and the prompt string balloons.
3. **The fill-line gradient.** The `$fill` module / gap char stretches across the terminal.
   Gradient *that* instead of the segments — you get a wide, smooth ramp with a stable
   character count and no interaction with segment text. Underexplored and visually strong.
4. **Block-character dithering.** `█▓▒░` plus `E0C4`–`E0C7` gives you a 4–8 step fade between
   two backgrounds using ordinary background colors. Works at 16-color depth. This is how you
   get a gradient look that survives a colorscheme swap.

`lolcat` (and the `rainbow-bash-prompt` / `rainbow-zsh-prompt` / `risbow` projects) pipe the
prompt string through a colorizer each redraw. It's the pure-shell approach and it works, but
it is a subprocess per prompt — the exact cost every fast prompt exists to avoid.

### 4.5 Contrast discipline

The one rule that separates good rices from garish ones: **foreground is derived from
background, not chosen independently.** p10k's rainbow preset uses a small set of near-black
and near-white foregrounds (`254`, `0`) against saturated backgrounds. Your starship config
does the same (`text = "#0d0914"` used as fg on every colored bg). Pick two text colors —
one for light backgrounds, one for dark — and never deviate.

---

## 5. Layout & information-design techniques

- **Directory truncation strategies.** The best idea in p10k and the one most often missed.
  `POWERLEVEL9K_SHORTEN_STRATEGY=truncate_to_unique` shortens each path component to the
  fewest characters that stay unambiguous among its siblings (`~/w/p/src` instead of
  `~/Work/project/src`). Combined with **anchor files** — `POWERLEVEL9K_DIR_ANCHOR` patterns
  like `.git`, `package.json`, `Cargo.toml` — components that mark a project root are never
  abbreviated. Result: paths stay short *and* readable. Alternatives: `truncate_from_right`,
  `truncate_middle`, `truncate_to_last`, `truncate_absolute`, plus `DIR_MAX_LENGTH`.
- **Conditional / collapsing segments.** Show a language version only inside a project of that
  language. Show the k8s context only when `kubectl` config differs from default. oh-my-posh's
  `accordion` style is a distinct twist: it renders powerline-style but *collapses* rather than
  disappearing, so the prompt width stays stable and the eye doesn't have to re-locate segments.
- **Transient prompt.** After Enter, the just-used prompt is rewritten to something minimal
  (`❯` or `directory ❯`). Scrollback becomes clean and copy-pasteable; the live prompt stays
  fat. Arguably the highest quality-of-life feature in p10k and the one that most changes how
  the terminal *feels*. p10k: `TRANSIENT_PROMPT`. Starship: `enable_transience`
  (zsh/fish/pwsh; bash only via ble.sh — §7.3). oh-my-posh: `transient_prompt` block.
- **Instant prompt.** p10k dumps a rendering of the prompt to a cache file, and at next
  startup prints it *before* sourcing the rest of `.zshrc`, then reconciles. Makes a 400ms rc
  file feel like 0ms. Requires careful handling of anything that writes to stdout during init.
- **Right-aligned metadata.** Duration, time, battery, jobs go right; identity and location go
  left. Keeps the left edge stable so your eye always lands in the same place.
- **The gap/fill line.** `$fill` between left and right clusters, drawn with `─`, `·`, or space.
  A fill character (rather than space) makes the two clusters read as one bar and gives you a
  place to hang a gradient (§4.4.3).
- **Prompt char as mode indicator.** `❯` insert, `❮` command, `▶` overwrite, `V` visual. If you
  use vi mode, this is a free, zero-width mode display.
- **Empty line before prompt** (`add_newline`). Cheap, and does more for readability than any
  glyph.

---

## 6. Performance techniques

A riced prompt that adds 200ms per Enter gets deleted within a week. The known techniques:

- **Never fork if you can help it.** The Python-powerline lesson. Every `$(...)` in `PS1` is a
  subprocess. A compiled prompt binary is *one* fork; a shell prompt with eight command
  substitutions is eight.
- **gitstatusd.** `git status` is the expensive part of every prompt. romkatv's `gitstatusd`
  is a persistent C++ daemon (patched libgit2, multithreaded, early-terminating because it
  only needs *whether* there are dirty files, not the list). Benchmarked ~10x faster than
  `git`, ~46x on the `diff_index_to_workdir` hot path; 291ms cold / 30.9ms hot vs git's
  876/295. **It ships bindings for both zsh and bash** (`gitstatus.plugin.sh`) — one of the
  few p10k pieces directly reusable from bash.
- **Async segments.** Render the prompt immediately with a placeholder, compute the slow thing
  in a background job, redraw when it lands. Native in zsh (zle + `zle -F` on a fd). In bash
  this needs ble.sh or a redraw hack.
- **Caching keyed on cheap state.** Recompute the git segment only when `$PWD` changed or
  `.git/HEAD` mtime moved. Recompute the toolchain segment only when the project root changed.
- **Timeouts.** Starship's `command_timeout` (you have it at 500ms) — a segment that can't
  answer in time is dropped rather than blocking. Essential for anything touching the network
  or a slow filesystem.
- **Precompute at config-load, not at prompt-time.** Colors, glyph strings, and separator
  sequences are constant; build them once into variables at init.

---

## 7. zsh vs bash — what has to be faked

This is the crux. zsh's advantages are *structural*, not cosmetic — the colors and glyphs port
trivially; the machinery does not.

### 7.1 Feature-by-feature

| Capability | zsh | bash | Verdict |
|---|---|---|---|
| Prompt re-evaluated on every redraw | `setopt PROMPT_SUBST` — `PS1` is expanded each time zle redraws | `PS1` is expanded once per prompt, but *not* on redraw/resize | **Different.** Bash can't repaint a live prompt on window resize. |
| Prompt escapes | `%F{#7047a8}`, `%K{}`, `%B`, `%~`, `%(?.ok.err)` ternaries — the shell knows these are zero-width | `\[`…`\]` manual wrapping around every raw escape | **Faked, error-prone.** §7.2 |
| Right prompt | `RPROMPT` — native, resize-aware, auto-hides when the line grows | None. Cursor-save/restore (`tput sc`/`rc`) hacks | **Effectively unavailable.** §7.3 |
| Transient prompt | zle widget rewrites the accepted line | Nothing native | **ble.sh only.** |
| Instant prompt | Possible because zsh can print before rc completes and reconcile via zle | Nothing native; bash also starts faster, so less needed | **Mostly moot.** |
| `precmd` / `preexec` hooks | Native `add-zsh-hook` | `PROMPT_COMMAND` (precmd only); preexec via `bash-preexec` `DEBUG` trap | **Faked well.** `bash-preexec` is production-grade — Ghostty and iTerm2 ship it. |
| Async fd callbacks | `zle -F fd handler` | None | **ble.sh or polling.** |
| Line editor extensibility | zle widgets | readline (`bind -x`), much weaker | **ble.sh replaces readline entirely.** |
| Fast git status | gitstatusd zsh bindings | **gitstatusd bash bindings exist** | **Direct port.** |
| `PROMPT_COMMAND` as array | n/a | Bash 5.1+ allows an array of hooks — composes cleanly with other tools | **Bash advantage**, actually. |

### 7.2 The `\[ \]` problem

In bash, every non-printing byte in `PS1` must be bracketed: `\[\e[38;2;112;71;168m\]`.
Bash counts unbracketed bytes as visible width. Get it wrong and:

- long command lines wrap at the wrong column,
- `Ctrl-A`/`Ctrl-E` land in the wrong place,
- history recall overwrites the prompt,
- the damage is *silent* until the line is long enough to wrap.

Consequences for a riced bash prompt:

- **Generate, don't hand-write.** Build `PS1` programmatically with a helper that always emits
  `\[`…`\]` around escapes, so it's structurally impossible to forget.
- If your prompt is emitted by an **external binary** (starship, oh-my-posh, a custom Rust/Go
  prompt), that binary must know it's targeting bash and bracket accordingly — this is exactly
  what `starship init bash` / `oh-my-posh init bash` handle for you.
- **Never `echo` from `PROMPT_COMMAND`.** Characters printed outside `PS1` aren't counted at
  all. This is the documented failure mode of every "bash right prompt" tutorial.

### 7.3 Right prompt in bash: read the postmortem

oh-my-posh **shipped and then removed** bash rprompt support. Their stated reasons are the
best available summary of why this is a dead end:

- easy to break readline's cursor-position calculation, and hard to debug when it breaks;
- if the previous command's output lacks a trailing newline, the right prompt renders on
  *that* line;
- at the bottom of the scrollback buffer it renders on the prompt line itself;
- it broke history navigation on some platforms;
- multiline needed separate repositioning logic — the *only* issue they could actually fix.

Their recommendation was to use a shell with native support. The realistic bash options:

1. **Don't.** Put the metadata on the left, or on the first line of a two-line prompt with a
   fill. A `$fill`-style stretch on line 1 gets you 90% of the visual effect of a right prompt
   with none of the cursor risk, because it's all inside `PS1` and bash counts it correctly.
2. **ble.sh.** It replaces readline wholesale and implements right prompt properly, because it
   owns the cursor. Also gives you transient prompt (`bleopt prompt_ps1_transient=` with
   `always` / `same-dir` / `trim`), syntax highlighting, autosuggestions, and vim modes. This
   is the single decision that closes most of the zsh gap.

### 7.4 A practical bash porting checklist

Everything in §3 (glyphs), §4 (color), §5's layout ideas, and §6's performance work ports to
bash **unchanged**. What needs adaptation:

- [ ] Emit `\[`/`\]` around every escape — mechanically, from a generator.
- [ ] Use `PROMPT_COMMAND` (array form on 5.1+) to rebuild `PS1` before each prompt.
- [ ] Add `bash-preexec` if you need pre-command hooks (command timing, terminal title).
- [ ] Use gitstatusd's bash bindings for the git segment.
- [ ] Compute `COLUMNS` yourself for fill/gap alignment (`${COLUMNS}` is maintained by bash
      when `checkwinsize` is set) — and recompute per prompt, since bash won't redraw on resize.
- [ ] Emit OSC 133 marks yourself (§8.1) — zsh frameworks often do it for you.
- [ ] Decide early: ble.sh or not. It's the fork in the road for right prompt and transience.
- [ ] Handle `PS0` if you want a "command started" hook without bash-preexec (Ghostty already
      uses it — worth checking for conflicts).

---

## 8. Beyond the prompt: the rest of the rice

### 8.1 Shell integration (OSC 133) — the invisible win

The "Final Term" / FTCS protocol. Your shell emits marks around prompt and output:

```
\e]133;A\a   prompt start
\e]133;B\a   prompt end / command start
\e]133;C\a   command output start
\e]133;D;<exit>\a  output end
```

Supported by Ghostty, kitty, WezTerm, iTerm2, VS Code, Windows Terminal. Enables jump-to-prompt
(`Ctrl+Shift+J/K` in Ghostty), click-to-select-an-entire-command's-output, scroll-by-command,
and exit-status decorations in the gutter. Costs four escape sequences and is worth more than
any glyph. Note that **tmux does not forward these by default** — a long-standing open request.

### 8.2 Terminal emulator layer

- **Font:** the ligature question (JetBrains Mono, Fira Code, Cascadia, Monaspace's "texture
  healing"), plus a Nerd Font patch or a symbols-only fallback.
- **Padding & line height.** Generous window padding and ~1.1–1.2 line height do more for
  "this looks designed" than any prompt change. Also mitigates powerline seams.
- **Background opacity + blur** (Ghostty `background-opacity` / `background-blur-radius`,
  kitty, WezTerm). The classic rice move; costs legibility, so pair with a *higher-contrast*
  palette than you'd otherwise use.
- **Custom shaders.** Ghostty exposes user GLSL shaders — CRT effects, bloom, and the current
  trend: **cursor trails / smear cursors** (`ghostty-cursor-shaders`, "smear cursor"). Ghostty
  is moving generalized cursor-trail support into core. WezTerm doesn't have user shaders yet.
  Practical argument for it: a trailing cursor is genuinely easier to track when screensharing.
- **Terminal-drawn glyphs.** kitty and Ghostty synthesize box-drawing and some powerline glyphs
  rather than using the font's, which eliminates the seam/misalignment class of bug entirely.

### 8.3 One palette, many templates

The architectural pattern behind every coherent rice: define the colorscheme **once**, generate
every app's config from templates.

- **base16 / Tinted Theming** — the schema (16 named slots) plus hundreds of schemes and
  per-app templates. Widely criticized now for being *only* 16 colors, which can't express the
  granular modern themes people want; the community keeps proposing a 24–32 slot successor.
- **Stylix** (NixOS) — a module that applies one scheme + font + wallpaper across the whole
  system, built on `base16.nix`. The most complete implementation of the idea.
- **pywal / wallust / matugen** — *derive* the palette from the wallpaper. `wallust` is the
  actively-maintained pywal successor; `matugen` generates Material You palettes and supports
  base16 output. This is the "everything follows the wallpaper" rice.
- **Catppuccin / Tokyo Night / Gruvbox / Rosé Pine as distributed ports** — the pragmatic
  alternative: an org maintains hand-tuned ports for 100+ apps. Less automatic, better-looking.

**Design consequence:** if you want your prompt to participate in this, its config must be
either (a) index-based (16/256 colors, inherits automatically) or (b) template-generated from
the palette source. A hardcoded-hex prompt config is an island.

### 8.4 Tool theming

The cluster people theme together, and the mechanism for each:

| Tool | Theming mechanism |
|---|---|
| `eza` | `EZA_COLORS` env (LS_COLORS-like), plus `--icons` (Nerd Font) |
| `bat` | `.tmTheme` files in `~/.config/bat/themes` + `bat cache --build` |
| `delta` (git diff) | `[delta]` in gitconfig, syntax-theme shared with bat |
| `fzf` | `FZF_DEFAULT_OPTS --color=...`, usually 16-color ANSI so it inherits |
| `lazygit` | `gui.theme` in its yaml |
| `btop` | `.theme` files |
| `yazi` | `theme.toml`, flavors system |
| `zoxide` | inherits fzf's |
| `less` / `man` | `LESS_TERMCAP_*` escapes |
| `LS_COLORS` | `vivid` generates it from a YAML theme — the clean way to do it |

Note the split: tools that take **16-color ANSI** re-theme for free when the terminal palette
changes; tools that take **hex** need regeneration. Prefer ANSI where the tool allows it.

### 8.5 tmux

If tmux is in play it owns a whole second status bar with the same design grammar (segments,
powerline separators, semantic colors). Frameworks: `catppuccin/tmux`, `tokyonight-tmux`,
`tmux-powerkit` (43 themes / 71 variants), the older `tmux-powerline`.

**Trap:** a theme plugin and a status plugin both rewrite `status-left`/`status-right`; running
two produces flicker and duplicated segments. Pick exactly one owner of the status line.

Also: tmux doesn't forward OSC 133 by default, so shell integration silently degrades inside it.

### 8.6 Fetch tools

`fastfetch` (the maintained `neofetch` successor) — the screenshot centerpiece. Configurable
JSON modules, custom ASCII/image logos, kitty/sixel image protocol support for actual images.

---

## 9. Idea bank — techniques worth stealing

Ranked roughly by payoff-to-effort:

1. **Transient prompt.** Changes the feel of the terminal more than anything else on this list.
2. **Semantic color on git state** (clean/dirty/untracked as *background* color).
3. **`truncate_to_unique` + anchor-file directory shortening.** Nobody outside p10k does this
   and it's strictly better than `truncate_from_left`.
4. **OSC 133 marks.** Four escapes, unlocks prompt-jumping and output selection.
5. **Two-line + frame + fill.** The frame is what makes it read as one designed object.
6. **Alternate separator geometry.** Trapezoid tabs or a zigzag of upper/lower triangles instead
   of the universal `E0B0`. Instant differentiation for zero cost.
7. **Stepped palette across segments** as the gradient (§4.4.1) rather than per-char interpolation.
8. **Gradient on the fill line**, not the segments. Wide, smooth, cheap, rare.
9. **Segments that are usually invisible** — SSH host, prod k8s context, slow-command duration,
   nonzero jobs. Information density without clutter.
10. **Block-dither fades** (`░▒▓` + `E0C4`–`E0C7`) for gradients that survive at 16 colors.
11. **Braille sparklines** (`2800`–`28FF`) in a segment — load average, battery history, build
    status over time. Almost nobody does this in a prompt.
12. **Progress-indicator glyphs** (`EE00`–`EE0B`) as an async-segment "still computing" state.
13. **Index-based colors** so the prompt inherits the terminal theme instead of fighting it.
14. **Prompt char as vi-mode indicator.**
15. **`command_timeout` on every segment that touches I/O.**

---

## 10. Pitfalls

- **Nerd Fonts v3 remapping** broke Font Awesome and Material Design code points from older configs.
- **Ambiguous-width glyphs** desync the cursor. Test every icon in the target terminal.
- **Powerline seams** are a font-metrics problem, not a config problem — fix with the terminal
  (line height, or a terminal that draws its own glyphs), not with more config.
- **Unbracketed escapes in bash `PS1`** corrupt line editing silently.
- **Printing from `PROMPT_COMMAND`** is the same bug wearing a hat.
- **Forks per prompt** — the death of Python powerline. Budget one.
- **Two owners of tmux's status line** flicker.
- **Hardcoded hex** makes the prompt an island in a wallpaper-driven rice.
- **Opacity/blur** eats contrast; compensate in the palette.
- **Copy-paste**: powerline glyphs paste as PUA garbage into chat/docs. Transient prompt is the
  mitigation — the scrollback you copy is the *lean* prompt.
- **p10k specifically** is zsh-only and on reduced maintenance; it remains the best *reference
  design* even where it isn't the right dependency.

---

## 11. Sources

Prompt engines
- [romkatv/powerlevel10k](https://github.com/romkatv/powerlevel10k) · [README](https://github.com/romkatv/powerlevel10k/blob/master/README.md) · [p10k-rainbow.zsh](https://raw.githubusercontent.com/romkatv/powerlevel10k/master/config/p10k-rainbow.zsh)
- [Starship presets](https://starship.rs/presets/) · [Gruvbox Rainbow](https://starship.rs/presets/gruvbox-rainbow) · [Catppuccin Powerline](https://starship.rs/presets/catppuccin-powerline) · [Pastel Powerline](https://starship.rs/presets/pastel-powerline) · [Advanced config](https://starship.rs/advanced-config/)
- [oh-my-posh: Segment](https://ohmyposh.dev/docs/configuration/segment) · [Transient prompt](https://ohmyposh.dev/docs/configuration/transient) · [Deprecating the bash rprompt](https://ohmyposh.dev/blog/deprecating-bash-rprompt) · [themes/schema.json](https://github.com/JanDeDobbeleer/oh-my-posh/blob/main/themes/schema.json)
- [Powerlevel10k is on Life Support. Hello Starship!](https://hashir.blog/2025/06/powerlevel10k-is-on-life-support-hello-starship/)

Glyphs & fonts
- [Nerd Fonts — Glyph Sets and Code Points](https://github.com/ryanoasis/nerd-fonts/wiki/Glyph-Sets-and-Code-Points)
- [ryanoasis/powerline-extra-symbols](https://github.com/ryanoasis/powerline-extra-symbols)
- [Nerd Fonts #296 — powerline glyph misalignment](https://github.com/ryanoasis/nerd-fonts/issues/296) · [powerlevel9k #922](https://github.com/Powerlevel9k/powerlevel9k/issues/922) · [wezterm #670](https://github.com/wezterm/wezterm/issues/670)
- [Patching another font for Powerline? With Kitty you don't need to](https://benfrain.com/patching-another-font-for-powerline-with-kitty-you-dont-need-to/)

Color
- [True colour support in terminals (kurahaupo gist)](https://gist.github.com/kurahaupo/6ce0eaefe5e730841f03cb82b061daa2) · [termstandard/colors](https://github.com/kurahaupo/termstandard-colors)
- [Terminal Colors Demystified](https://unixy.io/blog/terminal-colors-demystified/)
- [rainbow-bash-prompt](https://github.com/dosentmatter/rainbow-bash-prompt) · [rainbow-zsh-prompt](https://github.com/dosentmatter/rainbow-zsh-prompt) · [risbow](https://github.com/waddupp00/risbow)

Bash mechanics
- [ArchWiki — Bash/Prompt customization](https://wiki.archlinux.org/title/Bash/Prompt_customization)
- [Bash right prompt (mina86)](https://mina86.com/2015/bash-right-prompt/)
- [rcaloras/bash-preexec](https://github.com/rcaloras/bash-preexec)
- [akinomyoga/ble.sh](https://github.com/akinomyoga/ble.sh) · [Manual §4 Editing](https://github.com/akinomyoga/ble.sh/wiki/Manual-%C2%A74-Editing)
- [romkatv/gitstatus](https://github.com/romkatv/gitstatus) (zsh **and** bash bindings)

Shell integration
- [OSC 133 — Shell Integration (FTCS)](https://docs.otty.sh/vt/osc/osc-133) · [terminfo.dev OSC](https://terminfo.dev/osc)
- [ghostty #5932 — OSC 133 semantic prompt regions](https://github.com/ghostty-org/ghostty/issues/5932) · [wezterm shell integration](https://wezterm.org/shell-integration.html) · [tmux #5237 — forward OSC 133](https://github.com/tmux/tmux/issues/5237)

Wider rice
- [tinted-theming/base16-schemes](https://github.com/tinted-theming/base16-schemes) · [Stylix](https://github.com/danth/stylix) · [matugen](https://github.com/InioX/matugen)
- [Colorscheme tooling discussion (Lemmy)](https://lemmy.ml/post/39598755)
- [ghostty-cursor-shaders](https://github.com/sahaj-b/ghostty-cursor-shaders) · [Ghostty cursor uniforms discussion](https://github.com/ghostty-org/ghostty/discussions/6901) · [Make your terminal swoosh](https://bogdan-calapod.github.io/posts/ghostty-smear-cursor/)
- [tmux-powerkit](https://github.com/fabioluciano/tmux-powerkit) · [tokyonight-tmux](https://github.com/zMoooooritz/tokyonight-tmux) · [tmux themes overview](https://tmuxai.dev/tmux-themes/)
- [CLI++: Upgrade Your Command Line (KDAB)](https://www.kdab.com/cli-upgrade-your-command-line-with-a-new-generation-of-everyday-tools/)
