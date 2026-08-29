# Platform Coexistence & Elevation (Wave 2): Design

Date: 2026-08-28
Status: Approved direction, pending implementation plan
Scope: `shell/omarchy10k.bash` (claim broker, prompt handoff), `config/tools.sh` (rewritten as claims), `crates/omarchy10k` (`layer` verb, `doctor`, `init` policy baking), `templates/themed/` (ghostty, blesh, delta), `install.sh` (terminal include wiring, stack presence checks), `config/default.toml`, wiki.
Protocol: **no changes.** No daemon involvement. Adapter + CLI + installer + templates only.

---

## 0. Orientation for an implementing agent

Read before touching code:

| Page | Path | Why |
|---|---|---|
| Bash Adapter | `docs/wiki/bash-adapter.md` | Hook broker, daemon lifecycle, init ordering |
| Theme | `docs/wiki/theme.md` | Rice layer, template deployment, theme-set hook, live re-source |
| CLI | `docs/wiki/cli.md` | Subcommand surface, doctor |
| Configuration | `docs/wiki/config.md` | Config key reference |
| Glossary | `docs/wiki/glossary.md` | Env vars, file paths |

Sibling spec, already written: `2026-08-28-extension-model-design.md` (plugins, custom segments, segment ordering). **These two waves touch the same file** — `config/tools.sh` here, and the compiled plugin bundle there, both land in the adapter's "Modern CLI Layer" section. Coordinate the init ordering: platform layer → claims → plugin bundle → user.

Update the wiki after implementation per `.cursor/rules/wiki-maintenance.mdc`.

### Platform facts this design is built on

All verified on a live Omarchy Quattro install. Re-verify against the target version before implementing — these are the load-bearing assumptions.

| Fact | Location | Consequence |
|---|---|---|
| Omarchy ships **starship** as the default prompt | `/usr/share/omarchy/default/bash/init:5-6`, `/usr/share/omarchy/config/starship.toml` | omarchy10k must displace a live prompt, not install into a vacuum |
| Omarchy ships a full Bash layer | `/usr/share/omarchy/default/bash/{rc,envs,shell,aliases,functions,init,completions,inputrc}` | Most of what a shell framework provides already exists |
| Omarchy aliases `ls` to **long-format** eza | `default/bash/aliases` — `eza -lh --group-directories-first --icons=auto` | o10k's short-format alias silently overrides it |
| Omarchy wraps `cd` with `zd()` + zoxide | `default/bash/aliases` (`zd()`, `alias cd=zd`), `default/bash/init` (`zoxide init bash`) | o10k's `zoxide init bash --cmd cd` kills the `zd` feedback wrapper |
| Omarchy sources fzf completion + key-bindings | `default/bash/init` | o10k's `eval "$(fzf --bash)"` double-binds |
| Omarchy sets `BAT_THEME=ansi` **deliberately** | `default/bash/envs` | `ansi` makes bat inherit the already-theme-synced terminal palette. o10k's rice layer overrides it with a hardcoded Catppuccin guess — a regression |
| Omarchy sets `MANPAGER` to bat | `default/bash/envs` | o10k redefines it in `tools.sh` |
| Omarchy ships a tuned `inputrc` | `default/bash/inputrc` — `TAB: menu-complete`, `completion-ignore-case`, `show-all-if-ambiguous`, `menu-complete-display-prefix`, `colored-stats` | The "no keybinding/completion layer" gap does **not** exist for Omarchy users. Do not add carapace or a competing completion layer here |
| Omarchy sources `bash-completion` | `default/bash/shell` | Already handled; do not re-source |
| Omarchy activates **mise** | `default/bash/init` | o10k does not; no conflict |
| Omarchy does **not** ship or reference ble.sh | grep of `/usr/share/omarchy/` returns only an agent skill doc | ble.sh cannot be the spine of a Quattro-native wave |
| Ghostty supports optional includes | `/usr/share/omarchy/config/ghostty/config` — `config-file = ?"~/.local/state/omarchy/current/theme/ghostty.conf"` | The seam for a personality include already exists and Omarchy uses it |
| foot supports includes | `/usr/share/omarchy/config/foot/foot.ini` — `include=~/.local/state/omarchy/current/theme/foot.ini` under `[main]` | Same seam. **Multiple-include support needs verification** — see §13 |
| Omarchy statically sets cursor style, padding, font for both terminals | Ghostty config, `foot.ini [cursor]`/`[main]` | Research §9's "dimensions beyond color" are **not unowned** — they are simply not theme-reactive |

### The user-visible symptom this wave removes

A working omarchy10k install on stock Omarchy currently requires two undocumented manual edits:

**1. Thirteen lines of `PROMPT_COMMAND` surgery in `~/.bashrc`:**

```bash
if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
  for prompt_hook in "${!PROMPT_COMMAND[@]}"; do
    PROMPT_COMMAND[$prompt_hook]="${PROMPT_COMMAND[$prompt_hook]//starship_precmd/:}"
    PROMPT_COMMAND[$prompt_hook]="${PROMPT_COMMAND[$prompt_hook]//__ghostty_hook/:}"
  done
else
  PROMPT_COMMAND="${PROMPT_COMMAND//starship_precmd/:}"
  PROMPT_COMMAND="${PROMPT_COMMAND//__ghostty_hook/:}"
fi
unset PS0
```

**2. A hand-edit in `~/.config/ghostty/config`:**

```
shell-integration = none
```

Neither is in `install.sh`, `doctor`, or the README. The README's principle 4 is *"One shell lifecycle… no more Mise clobbering Atuin clobbering Zoxide"* — the hook broker delivers that for tools, but installation onto the platform still makes the user do the clobbering by hand.

---

## 1. Design stance

**Compose and elevate. Delete nothing.**

The mechanism is not displacement. omarchy10k detects what the platform already provides, **extends it in place where extension is clean, defers where it is not**, and makes the resulting layering visible. Every capability currently in `tools.sh` is retained; it simply stops asserting itself where someone else already has an opinion.

This is the hook broker's philosophy pushed one level down — from `PROMPT_COMMAND` to aliases, tool inits, and environment variables.

### Decisions already made (do not re-litigate)

| Decision | Choice | Rationale |
|---|---|---|
| Wave framing | Coexistence and elevation, not theming completion | The audit found active regressions against platform defaults; those outrank new polish |
| ble.sh | Additive trim, never the spine | Omarchy neither ships nor references it; `v04-feature-intel.md` Watch List ratified "no new ble.sh-only capabilities" |
| Default precedence | **Extend where possible, defer otherwise** | Retains both sides' work; literally elevates what is built |
| User definitions | **Always win, not configurable** | The safety guarantee that makes automatic claiming safe to ship |
| Removal | Nothing is unset or deleted; only unhooked, reversibly | User's explicit direction |
| carapace / abbreviations | Out | Omarchy's `inputrc` already provides a good completion setup; these need independent justification |

## 2. Non-goals

- Terminal **modes** (SSH / root / production window identity, research §9's last section). Needs live terminal reconfiguration; belongs to the modes wave.
- `carapace`, ble.sh `sabbrev` abbreviations, or any competing completion layer. See the `inputrc` fact above.
- Alacritty and kitty include layers. Ghostty and foot only.
- Editing the user's `~/.bashrc` beyond the single existing init line.
- Any change to prompt rendering, segments, layout, or the daemon.
- Replacing Omarchy's palette generation for terminals. Colors stay Omarchy's; this wave never writes a terminal palette.

---

## 3. The claim broker

For each claimable item the adapter performs three steps.

**1. Detect** the current definition:

| Kind | Probe |
|---|---|
| alias | `alias -p` / `BASH_ALIASES[name]` |
| function | `declare -F name` |
| env var | `${name+set}` |
| readline binding | `bind -q <function>` / `bind -p` grep |

**2. Classify the owner:**

| Owner | Meaning |
|---|---|
| `platform` | Definition matches Omarchy's known shape (a signature match, not an exact string — see below) |
| `user` | Defined, but not Omarchy's shape |
| `none` | Undefined |

Platform signatures are matched on **substring markers**, never exact strings, so an Omarchy point release that reorders flags does not flip the classification to `user`. Example: `ls` is platform-owned if its alias contains `eza` and `--group-directories-first`. Signatures live in one table so a version bump is a data edit.

**3. Apply policy:**

| Owner | Policy `extend` (default) | Policy `defer` | Policy `own` | Policy `off` |
|---|---|---|---|---|
| `none` | o10k defines | o10k defines | o10k defines | nothing |
| `platform` | extend if an extender exists, else defer | leave alone | o10k redefines | nothing |
| `user` | **leave alone** | **leave alone** | **leave alone** | nothing |

**The `user` row is absolute and not overridable by config.** If someone defined `alias ls='ls --color'` in their `.bashrc` before the o10k init line, omarchy10k never touches it under any policy.

### Config

```toml
[shell.layer]
policy = "extend"          # extend | defer | own | off  — global default

[shell.layer.overrides]    # per-claim override of the global policy
ls = "extend"
grep = "defer"             # e.g. opt out of the rg-for-grep substitution
bat_theme = "defer"
```

Policy is read by `omarchy10k init bash` when it generates the adapter and baked into the emitted claim calls — no per-prompt cost, and no runtime config read in the shell. Same pattern the sibling spec's plugin bundle uses.

### Shell API

```bash
o10k_claim <name> --kind alias|func|env|bind \
                  --platform-sig <substring> \
                  --extend <function> \
                  --define <definition>
```

`--extend` names a function receiving the current definition on stdin and emitting the extended one. Absent `--extend`, `extend` policy degrades to `defer` for platform-owned items.

Every resolution is recorded in `__O10K_CLAIMS[name]="<owner>:<policy>:<result>"` — the array `doctor` and `omarchy10k layer` read.

---

## 4. Claim inventory

Derived from the live audit plus research §3 (Replace the boring Unix defaults) and §45 (Recommended default stack).

### 4.1 Contested — both define it today

| Claim | Omarchy | o10k today | Resolution |
|---|---|---|---|
| `ls` | `eza -lh --group-directories-first --icons=auto` | `eza --icons=auto --group-directories-first` | **extend** — keep Omarchy's flags, append `--git` |
| `lt` | `eza --tree --level=2 --long --icons --git` | `eza --tree --level=2 --icons=auto` | **defer** — Omarchy's is strictly richer |
| `cd` / zoxide | `zoxide init bash` + `zd()` + `alias cd=zd` | `zoxide init bash --cmd cd` | **defer** — detect `zd` or `_zoxide`, skip re-init entirely |
| fzf keys + completion | sources `/usr/share/fzf/{key-bindings,completion}.bash` | `eval "$(fzf --bash)"` | **defer** when already bound; detect via `bind -q` for `__fzf_history__` |
| `MANPAGER` | `sh -c 'col -bx \| bat -l man -p'` | same + `\|\| less` fallback | **defer** — define only when unset |
| `BAT_THEME` | `ansi` (deliberate: inherits the theme-synced terminal palette) | rice layer → hardcoded Catppuccin | **defer** — see §6 |

### 4.2 Uncontested — o10k owns, unchanged

Omarchy has no opinion on these. Current behavior is preserved exactly.

`ll`, `la`, `tree`, `cat`→`bat --paging=never`, `grep`→`rg`, `top`→`btop`, `du`→`dust`, `df`→`duf`, `ps`→`procs`, `y` (yazi cwd-follow function), atuin init.

**`grep`→`rg` carries a compatibility caveat** (ripgrep's flags are not grep's). It is existing shipped behavior and is retained, but it is classified as compatibility-sensitive: `doctor` labels it as such, and `[shell.layer.overrides] grep = "defer"` opts out cleanly.

### 4.3 Gap-fill — in the research stack, absent from `tools.sh`

| Tool | Addition | Note |
|---|---|---|
| `fd` | **No alias.** Presence check + `doctor` line only | `FZF_DEFAULT_COMMAND` in `o10k-env.sh.tpl` already depends on `fd` but nothing verifies it is installed. Aliasing `find`→`fd` is rejected: the CLIs are fundamentally incompatible, unlike the drop-in-ish substitutions above |
| `tldr` | `alias help='tldr'` when `help` is unclaimed | Research §3 "man discovery" |
| `lazygit` | `alias lg='lazygit'` | Already themed by `o10k-lazygit.yml.tpl`; had no alias |
| `lazydocker` | `alias lzd='lazydocker'` | Research §3 / §45 |
| `erdtree` | `alias et='erd'` when present | Research §3 tree alternative |
| `jq`, `xh`/`httpie` | Presence check + `doctor` only | No alias warranted; they are their own commands |

All gap-fills are `own`-class claims guarded by `command -v`, so a missing binary silently defines nothing.

### 4.4 Platform-only — o10k stays out

`mise` activation, Omarchy CLI completions, `inputrc`, history control (`histappend`, `HISTCONTROL`, `HISTSIZE`), `bash-completion` sourcing, `EDITOR`/`BROWSER`. Listed here so a future contributor does not "helpfully" add them.

---

## 5. Prompt ownership handoff

Replaces the user's `.bashrc:29-41` surgery. Runs once at adapter init, before `__o10k_install_hooks`.

- **starship** — detect `starship_precmd` in `PROMPT_COMMAND` (both array and string forms; the existing code in `__o10k_install_vanilla_hooks` already handles both shapes and is the reference). Remove that entry. **Leave the `starship_precmd` function defined and `starship` on `PATH`.**
- **Ghostty's injected hook** — detect and remove `__ghostty_hook` from `PROMPT_COMMAND`. The adapter already neutralizes `PS0` in `__o10k_preexec` (`shell/omarchy10k.bash:~578`); this moves the same defense to init time so the first prompt is clean too.
- Record each displacement in `__O10K_DISPLACED[]`.

**Nothing is unset, unaliased, or uninstalled — only unhooked.** Reversible in the current shell:

```
omarchy10k layer release prompt    # re-hooks starship, unhooks o10k
omarchy10k layer claim prompt      # takes it back
```

`install.sh` does **not** edit `~/.bashrc` beyond the existing init line. Instead `doctor` detects the now-redundant manual surgery by signature (`starship_precmd` string-substitution near an `unset PS0`) and reports that it is safe to remove. Telling the user beats editing their shell startup file.

---

## 6. Rice layer corrections and additions

### `BAT_THEME` — regression fix

Omarchy's `ansi` is the better default: bat inherits the terminal palette, which Omarchy already keeps theme-synced. o10k's override guesses a Catppuccin bundle and `o10k-env.sh.tpl` admits the guess in a comment.

```toml
[rice]
bat_theme = "ansi"     # ansi | themed   (default: ansi)
```

`o10k-env.sh.tpl` emits the `BAT_THEME` export **only** under `themed`. The capability is retained, not deleted — just off by default.

### ble.sh faces — additive, inert when absent

New `templates/themed/o10k-blesh.bash.tpl`, rendered by the theme engine like every other rice template, mapping research §12's syntax categories onto palette roles:

| ble.sh face | Palette role |
|---|---|
| `syntax_command` | `accent` |
| `syntax_filename` / valid path | `blue` |
| `syntax_quoted` (string) | `green` |
| `syntax_number` | `yellow` |
| `syntax_varname` | `accent` |
| `syntax_expr` / option | `foreground` |
| `syntax_comment` | `muted` |
| `syntax_error` | `red` |
| `auto_complete` (suggestion) | `muted` |

Sourced by the adapter **only when `BLE_VERSION` is set**. On a machine without ble.sh — including the author's — it is never read. This themes an optional component rather than adding a capability, which is what keeps it inside the ratified constraint.

### delta

`templates/themed/o10k-delta.gitconfig.tpl` → wired through `GIT_CONFIG_SYSTEM`-style layering or an `[include]` the user opts into. Genuinely unthemed by Omarchy or o10k today. Ship only if delta is installed; `doctor` reports otherwise.

---

## 7. Terminal include layer

Two files, because they have different lifetimes and different owners.

### Static — `~/.config/omarchy10k/ghostty.conf`

Installed once by `install.sh`, never regenerated. Holds settings that do not vary by theme:

```
# Omarchy10k requires ownership of Bash prompt/pre-exec integration.
# Ghostty's injected Bash hook uses PS0, which Omarchy10k treats as
# literal prompt text.
shell-integration = none
```

This is the currently-undocumented hand-edit, promoted to a shipped, explained, uninstallable file.

### Theme-rendered — `templates/themed/o10k-ghostty.conf.tpl`

Renders to `~/.local/state/omarchy/current/theme/o10k-ghostty.conf` on every theme switch. Holds **only theme-varying, non-color personality**: cursor accent, per-mode (`{{ mode }}`) opacity. **It never emits `background`, `foreground`, or `palette` lines** — those are Omarchy's, and duplicating them would fight the platform.

### Wiring

`install.sh` appends to `~/.config/ghostty/config`, **only when the lines are absent**, after a timestamped backup, printing exactly what was added:

```
config-file = ?"~/.config/omarchy10k/ghostty.conf"
config-file = ?"~/.local/state/omarchy/current/theme/o10k-ghostty.conf"
```

The `?` prefix makes both optional, so a missing file is not an error — the same convention Omarchy's own config uses.

**Precedence is positional.** Ghostty applies later values over earlier ones, so appending at the end gives o10k precedence over Omarchy's static defaults, while any user key placed after the includes still wins. Document this explicitly; do not attempt clever insertion-point logic.

foot gets the same treatment via `include=` under `[main]`, subject to the verification in §13.

`install.sh --uninstall` removes the appended lines and the static file.

---

## 8. `doctor` and `omarchy10k layer`

The visibility surface — the payoff for "elevate what's built." `doctor` gains a section:

```
Shell layer
  ls           Omarchy + o10k (--git)        extend
  lt           Omarchy                       defer
  cd           Omarchy (zd)                  defer
  fzf keys     Omarchy                       defer
  MANPAGER     Omarchy                       defer
  BAT_THEME    Omarchy (ansi)                defer
  grep         Omarchy10k (rg)               own · compatibility-sensitive
  top/du/df/ps Omarchy10k                    own
  ll/la/tree   Omarchy10k                    own
  lg/lzd/help  Omarchy10k                    own
  fd           installed ✓  (required by FZF_DEFAULT_COMMAND)
  tldr         not installed  — `help` alias skipped
  Prompt       Omarchy10k   (starship unhooked, reversible)
  Ghostty      include present ✓
  foot         include absent — run `omarchy10k layer install-terminal`
  ~/.bashrc    legacy prompt surgery detected — no longer needed, safe to remove
```

CLI:

```
omarchy10k layer                    # the map above
omarchy10k layer --json             # machine-readable, for scripts and the panel
omarchy10k layer release prompt     # hand the prompt back to starship
omarchy10k layer claim prompt       # take it back
omarchy10k layer install-terminal   # (re)wire the ghostty/foot includes
```

---

## 9. Config schema

New keys for `config/default.toml` and `docs/wiki/config.md`:

```toml
[shell.layer]
policy = "extend"          # extend | defer | own | off

# [shell.layer.overrides]
# ls = "extend"
# grep = "defer"
# bat_theme = "defer"

[rice]
bat_theme = "ansi"         # ansi | themed
blesh_faces = true         # render ble.sh face theming (inert without ble.sh)
delta = true               # render the delta theme template when delta is installed
```

Also fix, in the same change: `config/default.toml` currently has a stray bare block (`strategy`, `max_length`, `repo_root_style`, `unique`, `anchors`) after the `[segments.time]` header, so those keys parse into `[segments.time]` instead of `[directory]`. It duplicates the real `[directory]` block. Delete it. *(Also listed in the sibling extension-model spec §10 — whichever wave lands first fixes it; the second should confirm it is gone.)*

---

## 10. Failure modes

| Failure | Behavior |
|---|---|
| Omarchy absent entirely (non-Omarchy machine) | Every claim classifies `none`; o10k defines everything. This is the standalone install path and must be tested |
| Omarchy version changes an alias's flags | Substring signature still matches; extension re-applies. Worst case it classifies `user` and o10k defers — degraded, never broken |
| Omarchy renames or removes `zd()` | `cd` classifies `none`; o10k defines its own. Self-healing |
| User defined the alias themselves | Always left alone, under every policy |
| Extender function errors | Claim falls back to `defer`; original definition survives untouched |
| `starship_precmd` absent | No-op; nothing to unhook |
| Ghostty config unwritable | `install.sh` prints the two lines and asks the user to add them. Never fails the install |
| Terminal include file missing at launch | `?` prefix makes it optional; terminal starts normally |
| `fd` / `tldr` / `lazydocker` missing | Guarded by `command -v`; nothing defined, `doctor` notes it |

**Invariant: no claim resolution may leave the shell without a working `ls`, `cd`, or prompt.** Every path either extends, defers to something already functional, or defines fresh.

---

## 11. Testing

### Shell-level (the important ones)

A fixture stubs Omarchy's `default/bash/rc` (aliases, `zd()`, zoxide/fzf init, `envs`), sources it, then sources the o10k adapter, and asserts:

- `alias ls` contains **both** `-lh` and `--git`
- `lt` is unchanged from Omarchy's definition
- `cd` still resolves through `zd`; `zoxide init` ran exactly once
- fzf keybindings are bound exactly once
- `BAT_THEME` is `ansi`
- `MANPAGER` is Omarchy's
- `PROMPT_COMMAND` contains no `starship_precmd` and no `__ghostty_hook`
- `starship` remains callable and `starship_precmd` remains defined
- **a user `alias ls='ls --color'` set before the init line survives byte-for-byte** — the safety guarantee, tested directly, under every policy value
- with the Omarchy stub absent, o10k defines the full set (standalone path)
- `[shell.layer] policy = "own"` restores today's behavior exactly (the escape hatch works)
- `layer release prompt` re-hooks starship; `layer claim prompt` reverses it

### Rust unit

Policy parsing and precedence table resolution; claim-map serialization for `layer --json`; `doctor` section rendering as a snapshot test.

### Installer

Include lines appended exactly once (run `install.sh` twice, diff the config); backup created; `--uninstall` removes them; unwritable-config path prints instead of failing.

### Templates

`o10k-ghostty.conf.tpl` renders for both `mode = dark` and `mode = light` and emits **no** color keys. `o10k-blesh.bash.tpl` renders and is syntactically valid `bash -n`.

---

## 12. Implementation sequence

Each step is independently shippable.

1. **`config/default.toml` stray-block fix** (§9). Trivial; do it first.
2. **`BAT_THEME` + `MANPAGER` deference** (§6, §4.1). Smallest real user-visible fix, no new machinery — ships value immediately.
3. **Claim broker + `tools.sh` rewrite** (§3, §4). The core. Includes §4.3 gap-fills.
4. **Prompt ownership handoff** (§5), reversible.
5. **`doctor` Shell-layer section + `omarchy10k layer`** (§8).
6. **Terminal include layer + `install.sh` wiring** (§7).
7. **ble.sh face + delta templates** (§6).
8. **Wiki**: `bash-adapter.md` (claim broker, prompt handoff), `theme.md` (new templates, `BAT_THEME` policy), `cli.md` (`layer`), `config.md` (new keys), `glossary.md` (paths).

---

## 13. Open risks

| Risk | Mitigation |
|---|---|
| **foot multiple-`include=` support unverified** | Verify against the installed foot version before implementing §7's foot half. Fallback: print the snippet for manual addition rather than writing it |
| Omarchy bumps its bash layer and changes alias shapes | Signatures are substring-based and live in one table; a bump is a data edit. Worst case degrades to `defer` |
| Ghostty changes include semantics or precedence | Positional precedence is documented, not relied upon programmatically; `?` keeps a missing file harmless |
| `starship_precmd` detection misses a future starship init shape | Detection is additive; a miss means the old manual workaround still works and `doctor` reports the prompt as contested |
| Users who *want* o10k's short `ls` | `[shell.layer.overrides] ls = "own"` |
| Two waves editing the adapter's tail | Coordinate init ordering explicitly: platform layer → claims → plugin bundle → user (see §0) |

---

## 14. Companion work (separate specs)

- **Extension model** — `2026-08-28-extension-model-design.md`. Shares the adapter tail; see the ordering note in §0.
- **Terminal modes** — SSH / root / production / focus / presentation window identity, plus `[host.*]` config layering. Research §39 and §32. Needs live terminal reconfiguration, which is why it is not here.
- **Panel decomposition** — `Panel.qml` at 2350 lines. A `layer --json` consumer in the Control Center wants this done first.
