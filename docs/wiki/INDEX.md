# Omarchy10k Project Wiki

Omarchy10k is a reactive shell UI runtime for Bash, purpose-built for the Omarchy Quattro desktop environment. It replaces Starship with a daemon-driven prompt architecture that renders in under 5ms, integrates natively with the Omarchy theme system, and provides a desktop Control Center through a Quattro bar plugin.

**v0.3** adds terminal capability detection (`TermCaps`), layout presets, git worktree support, instant prompt caching, live Quattro preview via the `preview` protocol message, theme color swatches via the `palette` control command, extended terminal integration (OSC 7/8/777, OSC 9;4 progress, DEC 2026 sync output, undercurl error styling), a one-script installer (`install.sh`), first-run diagnostics hints, notification threshold wiring from daemon config to bash, title format placeholder expansion (`{dir}`, `{user}`, `{host}`, `{branch}`), and TermCaps-gated OSC 8 hyperlinks.

**v0.4** adds the env channel (live env-derived segments — python/nix/mise/k8s/agent now respond to `activate`, `mise use`, `nix develop`), real notifications routed through `omarchy-notification-send`, an enriched `status` ambient snapshot, transient prompt wiring via the bridge's 4-field NUL framing, stale-aware git placeholder, true powerline/rainbow background-fill rendering, the `omarchy10k statusline` subcommand for Claude Code, the agent-signal segment, optional OSC 133;C/D semantic prompt emission, Quattro plugin IPC (`omarchy-shell call community.omarchy10k <method>`), a service-kind connection hub, a session-picker overlay, ANSI-colored live panel preview, and the `omarchy10k intro` first-run render. Protocol version is now **0.4**.

**v0.5** adds Looks — atomic appearance bundles in `[looks.<name>]` with 8 curated Looks compiled in (user entries shadow curated names) — exposed through the `looks`, `looks_apply` (persistent, or transient in-memory "Try"), `looks_save`, and `palettes` control verbs plus a `look` override on `preview` for dry-run renders; the enriched `status` snapshot (live git summary, battery, last command duration, session age); a configurable right prompt rail (`[prompt].right_segments`); the vi-mode prompt character delivered over the env channel (`KEYMAP` → `vi_mode`); the quick-action scripts CLI (`omarchy10k script list|run` via the daemon's `script_exec` module) and desktop hook dispatch (`omarchy10k hook-event`); the Quattro Looks Gallery overlay and the 4-bucket Control Center rail; session workspace labels; bar badges; and daemon hardening (64 KiB socket frame cap, LRU-bounded caches, `PR_SET_PDEATHSIG`). Protocol version is now **0.5**, crate version **0.4.0**.

Since the 0.5 doc sync, the crate gained seven more waves (all shipped as v0.4.0): **C1 stabilization** — daemon `status` gains an `agent` field ("claude"|"codex"|null, detected from the env channel), the BarWidget gains a robot-glyph agents badge, the git branch becomes an OSC 8 hyperlink to the normalized remote URL when the terminal supports it, the hook-event ETXTBSY race was fixed with write-then-rename, and 12 new integration assertions landed; **C2 platform coexistence** — `[shell.layer]` claim policy (extend/defer/own/off) resolved at `omarchy10k init bash` and baked into the adapter prelude, prompt handoff unhooks starship/Ghostty precmd hooks at init (reversible, recorded in `__O10K_DISPLACED[]`), `omarchy10k layer [--json]` prints the claim map, terminal include templates (o10k-ghostty.conf.tpl, o10k-foot.ini.tpl) plus o10k-blesh.bash.tpl and o10k-delta.gitconfig.tpl join the rice templates, and tools.sh keeps only uncontested aliases; **C3 Looks Studio** — the Gallery detail sheet became an editor (palette hex rows, cycle rows, Gradient Ramp Designer) rendering every edit through the `preview.patch` override (merge base → look → patch) with a new `looks_delete` verb; **C4 Panel decomposition** — Panel.qml shrank 2701→1277 lines into PanelLooks/PanelStyle/PanelBehavior/PanelSystem.qml + shared PanelKit.qml; **C5 parity + share** — vanilla-bash (non-ble.sh) gains an escape-aware right prompt rail and a rewritten transient prompt, and Looks export/install as portable TOML bundles (`look export` / `look install`, https-only, dry-run by default); **Tier C** — per-repo `.o10k.toml` project profiles (display-keys allowlist, `.git`-boundary detection), p10k-grade wizard depth (context previews, per-segment toggles, apply/Look/profile finish paths), and `theme.source = "terminal"` index-palette mode; **Tier D** — 8 new catalog segments with layered detection tiers, a plugin economy (`plugin add|list|enable|disable|remove|update` over `~/.config/omarchy10k/plugins/<name>/plugin.toml`), and `omarchy10k migrate <starship.toml>` mapping Starship config into a `migrated-starship` Look.

Since the increments above, the Control Center was rebuilt against a shared
component kit and service-owned state: the plugin now registers the **`panel`**
kind (`Studio.qml`, summonable via `omarchy-shell shell summon`), the service
owns config/Looks/palettes/preview/undo so surfaces cannot drift, the theme
**bind state** (sync/desync) is surfaced with a one-click resync, and four
daemon capabilities that were CLI-only gained a UI — quick actions
(`script_list`), segment plugins, the shell-layer claim map (`layer --json`)
and the rice layer's include wiring. A daemon watcher feedback loop that leaked
~9.5 MB/s per shell was fixed (see [Daemon](daemon.md)).

## Wiki Pages

| Page | Description |
|------|-------------|
| [Architecture](architecture.md) | System design, component graph, data flow diagrams, and design philosophy |
| [Daemon](daemon.md) | `omarchy10kd` module reference: server, config, theme, git, layout, segments, render, terminal (`TermCaps`) |
| [CLI](cli.md) | `omarchy10k` binary: subcommands, prompt client, doctor diagnostics |
| [Bash Adapter](bash-adapter.md) | Shell integration: hook broker, daemon lifecycle, instant prompt cache, timing, ble.sh mode |
| [Quattro Plugin](quattro.md) | Desktop Control Center: manifest, QML components, daemon IPC, live preview, theme swatches, config UI |
| [Configuration](config.md) | Full config key reference with types, defaults, and valid values (incl. Wave 1 visual-depth keys) |
| [Protocol](protocol.md) | Daemon IPC specification (protocol v0.5): NDJSON over Unix socket, prompt/preview/config/control/statusline messages, env channel, Looks verbs, enriched status |
| [Glossary](glossary.md) | Terms, concepts, environment variables, file paths (includes v0.3 terminal and API terms) |
| [v0.3 Feature Intel](v03-feature-intel.md) | Research-backed feature catalog that informed v0.3: 30 features, compatibility matrix, priority tiers |
| [Quattro QoL Intel](quattro-qol-intel.md) | Quality-of-life improvements for Quattro integration: live preview, bar intelligence, notifications |
| [Bug Audit](bug-audit.md) | Correctness audit of v0.3.0: 20 findings ranked by severity, with reproductions and fix directions |
| [Ricing Intel 2026](ricing-intel-2026.md) | What 2026 terminal ricing does that we do not: unused Kitty-graphics/OSC 52 capabilities, the glyph situation (animals fixed, anime honestly assessed), and a ranked order |
| [v0.4 Feature Intel](v04-feature-intel.md) | Next-release feature catalog: foundations, headliners, Omarchy desktop integration, onboarding — plus a ratified kill list and watch list |

> **Current known issues:** a full correctness audit of v0.3.0 is recorded in
> [Bug Audit](bug-audit.md). Four findings are rated critical — unwrapped prompt
> escapes corrupting readline width accounting, a `struct tm` ABI mismatch in the
> time segment, two UTF-8 slicing panics, and the daemon exiting immediately
> without `O10K_PARENT_PID`. Read it before changing `render.rs`, the segment
> layer, or the Bash adapter.

## Project Coordinates

| Property | Value |
|----------|-------|
| Language | Rust (daemon + CLI), Bash (shell adapter), QML/JS (Quattro plugin) |
| License | MIT |
| Version | 0.4.0 |
| Protocol version | 0.5 |
| Repository | `github.com/DividedBeingCode/OmarchyMyBash` |
| Plugin ID | `community.omarchy10k` |

## Repository Layout

```
omarchy10k/
├── Cargo.toml                       # Workspace manifest (crate 0.4.0)
├── install.sh                       # One-script installer (builds, binaries, shell, Quattro plugin, hooks, rice templates, tools)
├── .github/workflows/benchmark.yml  # CI benchmark workflow
├── config/
│   ├── default.toml                 # Default configuration (embedded in daemon)
│   └── tools.sh                     # Modern CLI alias layer (uncontested eza/bat/rg/zoxide/fzf/... aliases, policy-honoring)
├── crates/
│   ├── omarchy10k/                  # CLI client binary
│   │   └── src/{main,prompt,bridge,doctor,configure,intro,script,hook_event,statusline,update,layer,share,migrate,plugins_cli}.rs
│   └── omarchy10kd/                 # Persistent daemon binary
│       └── src/{main,server,config,git,layout,looks,style,render,script_exec,terminal,theme,profiles,plugins}.rs
│       └── src/segments/{mod,util,ai,battery,character,command_duration,container,directory,exit_status,git,jobs,
│           k8s,load,nix,os,python_env,ssh,time,toolchain,package_version,dir_writable,aws_profile,docker_context,
│           kubectl_context,terraform_workspace,vpn,gcloud_project}.rs
├── shell/omarchy10k.bash            # Bash adapter + hook broker (baked layer-policy prelude, prompt handoff)
├── hooks/                           # Omarchy hook drop-ins (installed to ~/.config/omarchy/hooks/<event>.d/omarchy10k)
│   ├── theme-set                    # Theme switch → reload_theme fan-out
│   ├── battery-low                  # Low battery → desktop notification toast
│   ├── post-update                  # After omarchy update → self-update + invalidate_git fan-out
│   └── font-set                     # Font switch → reload_theme fan-out
├── templates/
│   ├── omarchy10k.toml.tpl          # Theme bridge template
│   └── themed/                      # Rice-layer templates for ~/.config/omarchy/themed/:
│                                    #   o10k-env.sh, o10k-blesh.bash, o10k-delta.gitconfig,
│                                    #   o10k-ghostty.conf, o10k-foot.ini, lazygit, yazi, cava
├── quattro/                         # Quattro bar plugin
│   ├── manifest.json                # kinds: bar-widget, service, overlay, panel
│   ├── Studio.qml                   # Full-screen Control Center (panel kind, summonable)
│   ├── StudioPrompt.qml             # Studio: presets, separators, glyphs, toggles
│   ├── StudioRice.qml               # Studio: tool theming + include-wiring detection
│   ├── StudioTheme.qml              # Studio: Omarchy theme browser + palette pin
│   ├── StudioSystem.qml             # Studio: sessions, plugins, shell-layer map
│   ├── StudioWizard.qml             # Studio: wizard from `configure --describe`
│   ├── o10k/                        # Shared kit — Fx.js, Motion.js, Store.js,
│   │                                #   Card.qml, SettingRow.qml, ThemeBindRow.qml
│   ├── BarWidget.qml                # Bar glyph + badges (daemon status, git dirty, long-cmd chip, agent badge)
│   ├── Panel.qml                    # Control Center — 4-bucket rail (LOOKS · STYLE · BEHAVIOR · SYSTEM)
│   ├── PanelLooks.qml               # Looks bucket: cards, Looks Studio editor, delete
│   ├── PanelStyle.qml               # Style bucket
│   ├── PanelBehavior.qml            # Behavior bucket
│   ├── PanelSystem.qml              # System bucket
│   ├── PanelKit.qml                 # Shared QML controls (intentionally unbound components)
│   ├── StudioLooks.qml              # Preset browser: search, tags, live preview cards
│   ├── StudioLookEditor.qml         # Palette rows, ramp designer, save/overwrite/delete
│   ├── SessionPicker.qml            # Live session picker overlay
│   ├── Service.qml                  # Persistent connection hub (service-kind plugin)
│   └── Model.js                     # TOML parser, CONFIG_MAP, protocol helpers
├── tests/
│   ├── integration_test.sh          # Integration test suite
│   ├── qml/                         # Headless QML component tests (stubbed qs.Commons)
│   ├── qmllint.sh                   # Static gate: no errors, no unqualified in new code
│   ├── fx_test.js / motion_test.js / store_test.js   # o10k kit unit tests
│   ├── model_parity_test.js         # Model.js CONFIG_MAP round-trip parity harness
│   └── vanilla_transient_test.sh    # Vanilla-bash transient + right-rail fixture
├── docs/wiki/                       # This wiki
├── docs/img/                        # Screenshots (panel.png, gallery.png)
├── README.md                        # Quick start, controls table, screenshots
└── LICENSE
```

## Quick Start

```bash
# One-line install (builds, installs binaries, configures shell, sets up Quattro plugin + theme hook)
cd omarchy10k && ./install.sh

# Update to latest
omarchy10k update

# Browse and apply Looks (appearance bundles; persistent or try-in-memory)
omarchy10k look list
omarchy10k look apply tokyo-rainbow
omarchy10k look apply omnarchy --transient

# Shell layer: see which claims are extended/deferred/owned, then init
omarchy10k layer --json

# Plugins: install from a remote git URL (installs DISABLED — review, then enable)
omarchy10k plugin add https://github.com/user/o10k-plugin-example
omarchy10k plugin enable o10k-plugin-example

# Migrate a Starship config into a Look (dry-run first, --yes to save)
omarchy10k migrate ~/.config/starship.toml

# Share a Look: export to a portable TOML bundle, install from a file or URL
omarchy10k look export omnarchy --clipboard
omarchy10k look install ~/Downloads/omarch.look.toml --yes

# Quick actions and desktop hook events

omarchy10k script list
omarchy10k hook-event battery-low 15

# To uninstall
./install.sh --uninstall
```

Or manually:

```bash
cd omarchy10k && cargo build --release
cp target/release/omarchy10k target/release/omarchy10kd ~/.local/bin/
echo 'eval "$(omarchy10k init bash)"' >> ~/.bashrc
cp -r quattro/ ~/.config/omarchy/plugins/community.omarchy10k/
for event in theme-set battery-low post-update font-set; do
  mkdir -p ~/.config/omarchy/hooks/$event.d
  cp hooks/$event ~/.config/omarchy/hooks/$event.d/omarchy10k
  chmod +x ~/.config/omarchy/hooks/$event.d/omarchy10k
done
```

## Sibling Projects

Omarchy10k coexists with the **Omarchy Spatial UX** project (`~/syncthing/OMPSpacialUX`, wiki: `docs/wiki/INDEX.md` there) on the same Omarchy machine. Integration surfaces:

| Shared surface | Omarchy10k side | Spatial UX side | Coordination rule |
|---|---|---|---|
| Unix sockets | `$XDG_RUNTIME_DIR/omarchy10k-<pid>.sock` (per-shell) | `$XDG_RUNTIME_DIR/omarchy-uxd/{control,events}.sock` | Distinct namespaces; never collide them |
| Theme state | Theme bridge reads `~/.local/state/omarchy/current/theme/colors.toml` read-only | Theme reactor reads the same file read-only | Neither writes theme files; only the Omarchy theme engine does |
| Hooks | `<event>.d/omarchy10k` drop-ins (theme-set, battery-low, post-update, font-set) | `hooks/theme-set.d/spatial-ux-theme.sh`, `hooks/battery-low.d/spatial-ux-battery.sh` | Same `.d` dirs, distinct basenames; both fire per event |
| Quattro surfaces | `community.omarchy10k` (bar-widget/service/overlay) | `ijohnst.spatial-ux` (service/overlay/panel/menu) | Both may hold exclusive keyboard grabs when summoned — avoid simultaneous summon |

The two daemons are independent (`omarchy10kd` per bash session, `omarchy-uxd` single exec-once) with disjoint config/state dirs.

## Maintenance

This wiki is maintained alongside the codebase. When making changes to the project, update the relevant wiki pages to keep documentation in sync. See the project rule `.cursor/rules/wiki-maintenance.mdc` for the update protocol.
