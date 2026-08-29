# Control Center Rebuild — Design

**Date:** 2026-08-29
**Status:** Approved for planning
**Scope:** `quattro/` (Quickshell plugin), `install.sh`, theme/rice templates
**Crate impact:** no daemon protocol changes. One small CLI addition is
likely — a machine-readable description of the `configure` wizard steps so
the Studio wizard consumes them rather than restating them.

## Purpose

Omarchy10k is a terminal-ricing environment for Bash on Omarchy Quattro —
p10k's configurability, delivered through the desktop rather than a dotfile.
The Control Center is where that promise is kept or broken, and today it
breaks it in five ways: it does not look like Omarchy, it does not behave
like Omarchy, it rices only the prompt, it silently desynchronizes your
terminal from your desktop theme, and half the daemon's capabilities have no
UI at all.

This rebuild makes the Control Center the place you rice the whole terminal
environment, in a surface indistinguishable from a first-party Omarchy panel.

## Findings that motivate the rebuild

Each was verified against the code or the live system, not assumed.

| # | Finding | Evidence |
|---|---------|----------|
| 1 | The Control Center is not summonable | `manifest.json` declares `bar-widget, service, overlay` — not `panel`. Five first-party plugins use the `panel` kind (`omarchy.dev-gallery`, `omarchy.disk-speedtest`, …) and are summoned with `omarchy-shell shell summon <id>`. Ours opens only by clicking the bar glyph: no keybind, no menu entry. |
| 2 | It does not use Omarchy's UI kit | 34 hand-rolled `Rectangle`+`MouseArea` controls across the plugin; zero uses of `Button`, `Toggle`, `Dropdown`, `PanelSlider`, `PanelSectionHeader`, `PanelActionButton`, `ConfirmDialog`, `SearchableDropdown`, `NumberField`, `PopupCard`. Consequence: no panel keyboard cursor, and `Style.styleOverrides` — Omarchy's themeable normal/hover/selected/focus token vocabulary — never reaches our controls. |
| 3 | Every card is a hard rectangle | `Style.cornerRadius` defaults to `0` (`/usr/share/omarchy/shell/Commons/Style.qml`), mirroring Hyprland `decoration:rounding`, which Omarchy ships at 0. We use it in 52 places with no radius floor. Zero elevation. Exactly one animation in 5,932 lines of QML (a 300 ms toast fade). |
| 4 | Palette cards silently unbind from the desktop theme | Applying a curated palette writes `[theme.custom]` + `source = "hybrid"`, permanently desyncing terminal from desktop, with no indicator and no way back. `architecture.md` states the opposite principle: *"Colors come from the Omarchy desktop theme, not hardcoded palettes."* |
| 5 | The rice layer is invisible | Eight theme-reactive templates (`o10k-{blesh,cava,delta,env,foot,ghostty,lazygit,yazi}`) render on every theme switch and have no Control Center surface. |
| 6 | `PanelLooks` does not use the `looks` verb | It hardcodes the 8 curated names in a static array; a code comment defers wiring to the gallery. User-saved Looks never appear in the Looks bucket. `look export` / `look install` are CLI-only. |
| 7 | The segment plugin economy has no UI | `omarchy10k plugin add\|list\|enable\|disable\|remove\|update` exists with a full manifest format; nothing surfaces it. |
| 8 | Quick actions have no UI | `script_list` / `script_run` are daemon verbs with a traversal guard and a 30 s timeout; CLI-only. |
| 9 | The shell-layer claim map has no UI | `omarchy10k layer [--json]` resolves and prints who owns `ls`, `lt`, `cd`, `fzf_keys`, `manpager`, `bat_theme` (extend / defer / own / off). No QML references it. Already designed as **B2** in `2026-08-29-next-level-brainstorm.md`, blocked there on Panel decomposition — which the C4 wave completed, so it is unblocked. |

Two things already correct and to be preserved: `omarchy plugin validate quattro`
**passes**, and the plugin has **zero `layer.enabled` uses**, which keeps it
inside the integrated-GPU contract shared with Spatial UX.

## Decisions

| Decision | Choice |
|---|---|
| Theme relationship | **User-controlled bind.** Colors either follow the Omarchy theme or are explicitly pinned, with the state always visible and a one-click resync. Not an architectural forcing in either direction. |
| Scope | **One rebuild**, not staged waves. Risk is managed by build order and gates, not by reducing scope. |
| Surface | **Quick Panel + full-screen Studio**, as two views over one state object and one component set. |
| Architecture | **Service-owned state** (Approach 1): `Service.qml` becomes the single owner; all surfaces are views. |

### The theme bind is already modelled daemon-side

The Look schema's `palette` directive already encodes exactly three states, so
this is a UI and state-visibility job, not a protocol change:

| Directive | Effect | UI state |
|---|---|---|
| `"theme"` | `theme.source = "omarchy"` | **Bound** |
| `"keep"` | leave color untouched | unchanged |
| curated key | `source = "hybrid"` + 11 custom colors | **Pinned** |

## Architecture

### State ownership

`Service.qml` grows from a status/discovery hub into the single owner of
sockets, config, Looks, palettes, defaults, the preview broker, the undo
stack, and the headless-daemon spawn. Every other component binds read-only
and mutates only through service functions.

```
Service.qml  ── owns ──►  sockets · config · looks · palettes · defaults
                          preview broker · undo stack · headless daemon
     ▲                            │
     │ bind (read-only)           │ mutate via service functions only
     ├── BarWidget                │
     ├── QuickPanel  ◄────────────┘
     ├── Studio
     └── SessionPicker
```

This is not a new architecture — it completes the one already chosen.
`Service.qml` already survives panel open/close, already caches `_cfgFlat`,
and already exposes `openGallery()` via `shell.summon`. It simply never got
ownership of config, Looks and preview, which is why Panel and Gallery drifted.

What it fixes, each a cost that exists today:

1. **Three sockets become one.** Panel, Gallery and Service each open their own.
2. **One preview broker.** A config change currently fires ~10 preset previews
   from the Panel while the Gallery fires its own per-card previews. Gallery
   already has the right machinery (`previewCache`, `_inFlight`, `_reqSeq`);
   this promotes proven code to the service: dedupe by `(ctx, patch)` key,
   cancel superseded requests by sequence, share the cache across surfaces.
3. **One dirty set and one debounce.** The delta-save discipline — send only
   changed keys, never a full stamp — exists because a full stamp clobbers
   edits made outside the panel. Two surfaces with independent dirty sets and
   300 ms timers would race each other's saves.
4. **Undo spans surfaces.** It is panel-local today.
5. **One headless daemon.** The panel owns that spawn today; two surfaces
   would risk spawning two.

### Entry points

```json
"kinds": ["bar-widget", "service", "overlay", "panel"],
"entryPoints": {
  "barWidget": "BarWidget.qml",
  "service":   "Service.qml",
  "overlay":   "SessionPicker.qml",
  "panel":     "Studio.qml"
}
```

The `panel` kind matters structurally, not just for discoverability: a plugin
gets exactly **one** `overlay` entry point, and `SessionPicker.qml` is already
an ad-hoc router (`page: "sessions" | "gallery"` → `Loader`). Adding the
Studio there would make it a three-way router. Giving the Studio its own entry
point follows first-party precedent and makes it summonable, keybindable and
menu-reachable.

`Gallery.qml` folds into the Studio's Looks tab. `SessionPicker.qml` keeps the
overlay slot to itself and stops being a router.

### Component layer

A new `o10k/` kit. Three load-bearing files plus thin wrappers over the
first-party controls:

| File | Purpose |
|---|---|
| `Fx.qml` | Radius **floor** (fixes the hard-rectangle bug on a stock install), elevation via `RectangularShadow`, material and state fills driven by `Style`'s existing `normal` / `hover-cursor` / `selected` / `focus` tokens |
| `Motion.qml` | Duration and easing tokens, values mirrored from Spatial UX's `lib/Motion.qml` |
| `SettingRow.qml` | Label + control + modified-vs-default ink + per-row reset + help tooltip + keyboard focus, as one component |

`SettingRow` is the highest-leverage piece: `isModified()` and
`resetConfigKey()` already exist in `Panel.qml` but are applied by hand and
inconsistently. As a component, every setting in both surfaces gets them for
free, and the Studio's larger surface area stops multiplying the
inconsistency.

Everything else wraps `Button`, `Toggle`, `Dropdown`, `PanelSlider`,
`PanelSectionHeader`, `PanelActionButton`, `ConfirmDialog`,
`SearchableDropdown`, `NumberField`, `TextField`, `PanelToolTip`. This is what
buys the panel keyboard cursor and lets theme `styleOverrides` finally reach
our controls.

**Constraint carried forward from the C4 decomposition:** the shared kit stays
*unbound*. `quattro.md` records it as that wave's key finding — bound inline
components cannot be instantiated cross-file. Consumer files keep
`pragma ComponentBehavior: Bound`; kit definitions must not rely on
outer-scope binding.

## Information architecture

### Quick Panel (bar-anchored, ~360 px)

```
❯ Omarchy10k                    ● 2 sessions
┌──────────────────────────────────────────┐
│  ~/projects/my-app  main ✚2 ⇡1      1.2s │  live preview
│  ❯                                       │
│  [Error] [SSH] [Long cmd] [Jobs]         │  context toggles
└──────────────────────────────────────────┘
  🔗 Colors follow Tokyo Night      [Sync ↻]
  ── Looks ─────────────────────────────────
  ◂ [Omnarchy] [Tokyo Rainbow] [my-rice] ▸    ← real `looks` verb
       Try · Apply
  ── Quick ─────────────────────────────────
  Transient    ●━━    Two-line      ━━●
  Right rail   ●━━    Git mode  ‹adaptive›
  ── Actions ───────────────────────────────
  ⚡ backup-notes.sh   ⚡ deploy               ← script_list
                              [ Open Studio ↗ ]
```

### Studio (full-screen, `panel` kind)

| Tab | Contents |
|---|---|
| **Looks** | Gallery grid, live dry-run previews, detail-sheet editor, Gradient Ramp Designer, import/export wired to `look export` / `look install`, delete |
| **Prompt** | Presets, separators, glyphs, frame, per-segment enable and reorder, right rail |
| **Rice** | Terminal, Git, Files, System tool theming — see below |
| **Theme** | Omarchy theme browser, bind state, palette overrides, per-role editing |
| **System** | Doctor, benchmark, sessions, segment plugins, **shell layer map**, tools, config import/export, reset |

The **shell layer map** is the B2 design from `next-level-brainstorm.md`, landing
here rather than as its own rail bucket because the full-screen Studio has the
room: one row per claim (`ls`, `lt`, `cd`, `fzf_keys`, `manpager`, `bat_theme`)
showing owner × resolved policy, with row toggles writing
`[shell.layer.overrides]` through the same delta-save path as every other
setting. It reads `omarchy10k layer --json`, which already emits exactly this
table.

**The organizing rule:** the Quick Panel is a subset *view* of Studio state,
never a second implementation. A control appearing in both is the same
component with the same binding.

## Rice tab

`~/.config/omarchy/themed/` is a first-party Omarchy mechanism — Omarchy ships
`alacritty.toml.tpl.sample` there. Our eight templates sit beside it and are
rendered into `~/.local/state/omarchy/current/theme/` on every theme switch.

Omarchy already themes `alacritty`, `foot`, `ghostty`, `kitty`, `btop`,
`helix`, `neovim`, `chromium`, `icons`, `keyboard-rgb`, `vscode` and
`obsidian`. Omarchy10k adds `ble.sh`, `cava`, `delta`, `env`, `lazygit`,
`yazi`, plus non-color personality includes for `ghostty` and `foot`.

The tab shows that whole picture honestly rather than pretending we own
theming:

```
 THEMED BY OMARCHY            THEMED BY OMARCHY10K          AVAILABLE
 ─────────────────────        ────────────────────────      ──────────────
 ghostty  foot  kitty         ✓ ble.sh      ● wired         ○ btop-cava sync
 alacritty  btop              ✓ delta       ● wired         ○ starship migr.
 helix  neovim  chromium      ✓ lazygit     ● wired
 icons  keyboard-rgb          ✓ yazi        ● wired
 vscode  obsidian             ✓ cava        ● wired
                              ⚠ ghostty personality — template installed,
                                but ~/.config/ghostty/config has no include
                                line.                     [ Add include ]
```

Two rules:

- **The Rice tab never writes theme files.** It manages *templates* in
  `~/.config/omarchy/themed/` and *include lines* in user configs. Rendering
  stays the Omarchy theme engine's job.
- **It surfaces the silent failure.** An o10k include only takes effect if the
  user's terminal config references it. "Template installed but not included"
  is invisible today.

This posture is already encoded in `o10k-ghostty.conf.tpl`, which deliberately
emits no colors: *"Omarchy's own theme include owns background, foreground,
and palette — emitting those here would fight the platform."*

## Theme tab and the bind state machine

Three states, surfaced in both the Quick Panel and the Studio:

| State | Indicator | Config |
|---|---|---|
| **Bound** | 🔗 Colors follow *Tokyo Night* | `theme.source = "omarchy"` |
| **Pinned** | 📌 Pinned to *Gruvbox* — desktop is *Tokyo Night* · **[Sync ↻]** | `source = "hybrid"` + custom |
| **Index** | ▦ Terminal palette | `source = "terminal"` |

Contents:

- A grid of installed Omarchy themes from `omarchy theme list`, current one
  marked. Applying calls `omarchy theme set <name>` and is **labelled as
  desktop-wide** — no pretending it is terminal-only.
- A "pin terminal colors" section fed by the daemon's existing `palettes`
  verb, plus per-role color editing.
- A prominent **Sync to desktop** action sending `{theme: {source: "omarchy"}}`.
- Look cards carry a 🔗 / 📌 badge derived from their `palette` directive, so
  the user knows *before* clicking whether applying a Look will unbind colors.

## Wizard

`omarchy10k configure` already owns the step logic and the real-daemon preview
path. The Studio wizard shares the **daemon contract and the step data**, not a
second copy of the choices — the same discipline that prevents the Panel and
Gallery drift from recurring. It runs on first launch and from a Restart
button, and offers the same three finish paths: `config.toml`, save as Look,
or save as project profile.

Sharing step data means the wizard's steps, option catalogs and defaults stop
living only in `configure.rs`. The likely shape is a hidden
`omarchy10k configure --describe` emitting them as JSON, which the Studio
renders. This is the one place the rebuild is expected to touch the Rust
crates, and it is worth it: the CLI wizard has already drifted once — segment
toggles wrote config paths the daemon does not read, the prompt-character step
fell out of the chain, and `q` stopped quitting — precisely because nothing
else consumed that data.

## Feel

Restrained, matching Spatial UX's "premium over loud":

- Preview cross-fades between renders instead of snapping
- Look cards settle with a short stagger on tab open
- **🎲 Surprise me** — a random Look applied in Try mode, one click to keep or revert
- **A/B compare** — two Looks pinned side by side, which the full-screen Studio has room for
- Applying a Look briefly shows what changed
- The bar glyph pulses once when a long command finishes (the status stream already carries duration)

## Install and update

`install.sh` currently hand-copies into `~/.config/omarchy/plugins/` and
`hooks/<event>.d/`. It should prefer the native paths when present, falling
back to the current behavior:

- `omarchy plugin add <git-url>` / `omarchy plugin enable` instead of manual copy
- `omarchy hook install <event> <file>` instead of `cp`
- `omarchy plugin update` owns the plugin half; `omarchy10k update` keeps owning the Rust binaries
- Registering the `panel` kind is what makes the Studio reachable from
  `omarchy plugin enable`, a keybind, and the menu

## Coexistence with Omarchy Spatial UX

Both plugins load into the same `omarchy-shell` Quickshell process.

| Surface | Rule |
|---|---|
| GPU cost | `RectangularShadow` only — never `MultiEffect` / `DropShadow`, which need an offscreen buffer per surface. The zero-`layer.enabled` property is a standing constraint; introducing one requires explicit justification. |
| Motion | Their `lib/Motion.qml` and `lib/Fx.qml` are plugin-private and cannot be imported. Mirror the token *values* so both plugins read as one product. |
| Keyboard grabs | Both may hold exclusive grabs when summoned. The Studio must yield rather than fight if a Spatial UX overlay is up. |
| Sockets | `omarchy10k-<pid>.sock` vs `omarchy-uxd/{control,events}.sock` — never converge the namespaces. |
| Hooks | Same `<event>.d` directories, distinct basenames (`omarchy10k` vs `spatial-ux-*`). |
| Theme files | Neither project writes them. Only the Omarchy theme engine does. |

## Testing and gates

This is one large change to a 5,932-line QML surface with no component tests
today, so the gates are the risk control:

| Gate | Covers |
|---|---|
| `omarchy plugin validate quattro` in CI | Manifest schema conformance; passes today and must keep passing |
| `tests/model_parity_test.js` extended | CONFIG_MAP round-trip for new keys (32 keys today) |
| New headless QML smoke harness | Instantiates every kit component and every Studio tab, so a rename cannot silently break a tab nobody opened |
| `tests/integration_test.sh` extended | Daemon verbs the UI newly depends on: `looks_delete`, `palettes`, `defaults`, `script_list`, plugin verbs |

**Build order** (each step leaves the plugin loading and validating):
`o10k/` kit → service state ownership → Quick Panel → Studio tabs → Rice and
Theme tabs → wizard → install.sh native paths.

This is deliberately one design, not one implementation plan. The work is
large enough that planning should decompose it along that build order, with
the kit and service-ownership steps landing first — they are what every later
step depends on, and they are where a mistake is most expensive to unwind.

## Relationship to prior specs

This design continues two documents already in `docs/superpowers/specs/`
rather than restarting from them.

**`2026-08-28-control-center-redesign-design.md`** produced what exists today:
the Look model and its protocol verbs, the four-bucket rail, the Gallery
overlay, modified-ink and per-row reset. All of that is kept. This design
**supersedes only its §2 Panel IA**, on three grounds it could not have seen
at the time:

- It required "all styling through real tokens … `Style.cornerRadius`", which
  is correct but insufficient — the token is `0` on a stock Omarchy install,
  so honoring it faithfully is what produces the hard-rectangle rendering.
  Hence the radius floor in `Fx.qml`.
- It did not anticipate that a second Looks surface (panel bucket + gallery)
  would drift. It did, immediately: `PanelLooks.qml` hardcodes the Look list.
  The "one state, one component set" rule exists to close that.
- Its non-goals deferred the p10k-style wizard "once Looks exist." Looks
  exist, so the wizard is in scope here.

**`2026-08-29-next-level-brainstorm.md`** ranked the idea backlog. This design
picks up **B2** (shell-layer map, unblocked now that B3 Panel decomposition has
shipped as the C4 wave) and **B5** (Look share/import, whose CLI half shipped
as `look export` / `look install` and whose UI half is the Looks tab). Its
Tier C entries for the p10k-grade wizard and index-palette mode have since
shipped in the crate; this design gives the wizard a graphical surface.

**Deferred, and why:** Tier B1 **Terminal Modes** (Focus / Presentation / SSH
personalities re-rendering the terminal include) is the natural next occupant
of the Rice tab, but it needs a new daemon concept plus an unresolved spike —
whether ghostty live-reloads an include changed outside `theme-set`. It stays
out of this rebuild; the Rice tab should be laid out so it can host modes later
without restructuring.

## Non-goals

- No daemon protocol changes. Every verb the design needs already exists.
- No long-lived user-session daemon. The `headless` daemon spawn stays as-is;
  replacing it is its own project.
- Omarchy10k does not generate or write Omarchy themes. Applying a theme
  shells out to `omarchy theme set`.
- No re-theming of tools Omarchy already themes.
