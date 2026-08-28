# Omarchy10k v0.4 — Feature Intel

> Research-backed feature catalog for the next release cycle. Compiled via a
> five-division analysis (Omarchy deep integration, WOW/product, ecosystem
> intelligence, architecture enablers, adversarial feasibility) over the
> v0.3 codebase, the 2025-2026 prompt-tool ecosystem, and the Omarchy Quattro
> platform surface. Every idea is grounded in the current modules and protocol.
>
> Companion documents: [v0.3 Feature Intel](v03-feature-intel.md) (terminal
> protocol + segments — mostly shipped), [Quattro QoL Intel](quattro-qol-intel.md)
> (panel polish), [Bug Audit](bug-audit.md).

## The v0.4 Theme: From Prompt to Desktop Service

Omarchy10k's moat is not prompt features — Starship can ship any segment. The
moat is the **daemon + desktop integration no config-file competitor can
replicate**: a warm sub-5ms render engine on a Unix socket, a Quattro Control
Center, and root-position access to the Omarchy platform (hooks, IPC, plugin
kinds, icon font, notification wrapper). v0.4 makes that moat visible:

1. **Fix the context gap** — env-frozen segments are a correctness bug first,
   a feature gap second (Bug Audit #5).
2. **Finish the half-shipped** — true powerline rendering, transient prompt,
   notifications, and `git_stale` are 90% plumbed and 0% wired.
3. **Own the AI-statusline surface of 2026** — daemon-rendered, theme-native.
4. **Become a desktop service** — scriptable via `omarchy-shell call`,
   event-driven instead of polled.

## How to Read This Document

| Field | Meaning |
|---|---|
| **What / Why / How** | Same as the v0.3 intel doc — `How` references real modules |
| **Effort** | Hours, adversarially reviewed (estimates are usually 1.5-2x optimistic) |
| **Impact / Wow** | Transformative/high/medium/low, and a 1-5 screenshot factor |
| **Depends on** | Prerequisite from this list — dependencies are load-bearing |

---

## Tier 0 — Foundations (ship first; everything else builds on these)

### 0.1 Env Channel — live environment-derived segments

| | |
|---|---|
| **What** | Add an `env` object to the `prompt` request carrying a fixed allowlist of environment variables, populated by the bash adapter with zero forks; `SegmentContext` reads from it instead of `std::env`. |
| **Why** | Bug Audit #5. Today `source .venv/bin/activate`, `mise use`, `nix develop`, and `direnv allow` never change the prompt — four shipped segments (python_env, toolchain, nix, k8s) are dead weight. Starship gets fresh env for free; Oh My Posh's serve daemon added exactly this (#7774). |
| **How** | Adapter: pure parameter expansions in `__o10k_render_prompt` (no subprocesses) → `env:{...}` in the request JSON. Daemon: `PromptRequest.env: Option<HashMap<String,String>>`, `SegmentContext` prefers it, `std::env` stays as legacy fallback. Allowlist (~10 keys, config-overridable via `[env.watch]`): `VIRTUAL_ENV`, `CONDA_DEFAULT_ENV`, `MISE_NODE_VERSION`, `MISE_PYTHON_VERSION`, `MISE_RUBY_VERSION`, `MISE_GO_VERSION`, `MISE_RUST_VERSION`, `IN_NIX_SHELL`, `DISTROBOX_ENTER_PATH`, `container`, `KUBECONFIG`, `DIRENV_DIR`. |
| **Effort** | 4-6 hr |
| **Depends on** | Nothing. **This is step 1 of v0.4** — the agent-signal segment and every env-dependent idea depend on it. Bump `PROTOCOL_VERSION` to 0.4. |

### 0.2 Notification contract fix + `omarchy-notification-send` routing

| | |
|---|---|
| **What** | Implement the dead `[segments.notification]` stub properly: `[notifications] enabled, threshold_ms, unfocused_only`, delivered via `omarchy-notification-send` when available, OSC 777 otherwise. Includes fixing a verified defect: `segments.notification.enabled = false` is currently a no-op (daemon emits no threshold; the bash side keeps its default). |
| **Why** | Long-command notification is the most-requested feature in the category, and Omarchy explicitly forbids raw `notify-send` — its wrapper handles theming and do-not-disturb. The current stub ships a config key that lies. |
| **How** | Fix the no-op first (daemon emits empty threshold on disable; adapter must treat empty as *off*, not *default*). Then add the wrapper path: adapter prefers `omarchy-notification-send` (detected via `command -v`), falls back to OSC 777 (already sanitized, shell/omarchy10k.bash OSC 777 path). Focus gating via `omarchy10k doctor`-style terminal-focus detection stays bash-side. |
| **Effort** | 3-4 hr |
| **Depends on** | Nothing. Prerequisite for the bar badge (2.x) — do not build notification UX on the broken switch. |

### 0.3 Status enrichment — one-call ambient context API

| | |
|---|---|
| **What** | Grow the daemon `status` response into a full ambient snapshot: git branch/dirty/ahead, worktree name, last `cmd_duration_ms`, last exit code, session age, battery. |
| **Why** | Every desktop feature below (bar tooltip, session picker rows, menu actions, badge, third-party scripts) independently wants this data. Define it once in protocol 0.4 — no duplicate polling, one contract. Also gives `git_stale` its first real consumer. |
| **How** | Daemon tracks `last_render: Option<RenderSummary>` in `DaemonState`; `handle_status` merges it with the live `GitCache` entry. Pure additive response fields — backward compatible. |
| **Effort** | 3 hr |
| **Depends on** | Nothing. Prerequisite for 2.2 (Service plugin), 2.3 (overlay), 2.8 (script output). |

### 0.4 Wire the dead response fields — transient + stale-aware git placeholder

| | |
|---|---|
| **What** | (a) Extend the bridge NUL framing to a 4th field carrying the daemon's `transient` string and actually use it. (b) When git data is stale (large repo, expired cache), render the git segment immediately in muted style with a `⟳` marker instead of hiding it. |
| **Why** | Both fields are emitted today with zero consumers (verified). Transient collapse is the difference between a 3-line and a 40-line screenshot; the stale placeholder is the honest 90%-solution to streaming repaint (v0.3 intel 2.2) — bash cannot repaint PS1 in place, and the perceived-freshness problem is solved by a glyph, not a repaint protocol. |
| **How** | (a) bridge.rs: one extra `write_all(&[0])` pair mirroring the existing pattern; under ble.sh feed the transient string via `bleopt prompt_ps1_transient` hooks; non-ble.sh gets line-2 overwrite only (ship ble.sh-gated). (b) `segments/git.rs` stale branch renders `⟳ branch` in `palette.muted`; add `[git.stale_icon]`. |
| **Effort** | 4-5 hr both |
| **Depends on** | One bridge framing change — batch with 0.1 into the single protocol 0.4 bump. Full `segment_update` push-repaint is explicitly deferred (see Kill List / phasing note). |


---

## Tier 1 — Headliners (the WOW, ranked by value-per-effort)

### 1.1 True Powerline / Rainbow rendering

| | |
|---|---|
| **What** | Make `powerline` and `rainbow` render actual filled powerline segments: per-segment background fills, separators with flipped fg/bg, right separators, and cap glyphs closing the line ends. |
| **Why** | Today `powerline` and `rainbow` are visually identical to `lean` — space-joined foreground text. Filled segments are the most-screenshot aesthetic in the community, and **90% of the plumbing already exists and is unused**: `Segment.bg` (layout.rs:13) is never rendered, `right_separator` (style.rs:7) and `left_cap_start`/`right_cap_end` (style.rs:9-12) are resolved by StyleResolver but never consumed by `format_line1`. Combined with v0.3 theme hot-reload, the prompt visibly matches the wallpaper — the Omarchy10k signature screenshot. |
| **How** | `format_line1` (render.rs:322): apply `seg.bg` as SGR 48;2; render `right_separator` with fg flipped to the previous segment's bg; wire the caps. `rainbow` gets a bg-rotation scheme cycling palette accent/red/green/yellow/blue. Zero latency cost (string concat; separator width math already exists). Update Panel preset preview strings (Panel.qml:739, currently hardcoded fakes) to use the daemon `preview` message. |
| **Effort** | 7 hr |
| **Impact / Wow** | High / 5 — the single highest WOW-per-effort item in this document. |

### 1.2 `omarchy10k statusline claude-code` — daemon-rendered agent statusline

| | |
|---|---|
| **What** | A `starship statusline claude-code` competitor: subcommand consuming Claude Code's **documented** statusLine stdin JSON (model, context-window %, cost, rate limits, workspace dir, worktree) and rendering it through the daemon with the active Omarchy theme palette. |
| **Why** | The hottest prompt-adjacent surface of 2026: Starship shipped it (PR #7234), `ccusage` and a dozen blog tutorials prove mass demand. Starship's weakness is architectural — process-per-update (~15-30ms cold), no theme coupling. Our daemon is warm (<5ms cached) and theme-native. |
| **How** | New `statusline` protocol message (one type, part of the 0.4 bump) + CLI subcommand reading stdin JSON; tolerant parsing (`serde_json` unknown-field skip) with a contract test per schema bump; config `[statusline.claude-code]` format templates; context thresholds reuse exit_status color logic; pure-bash fallback render when the daemon is down. install.sh writes the `settings.json` snippet. |
| **Effort** | 8 hr |
| **Impact / Wow** | Transformative / 5. Deliberately consumes the *public* stdin contract, NOT undocumented session files (see Kill List). |

### 1.3 Agent signal segment (env-based MVP)

| | |
|---|---|
| **What** | One prompt glyph + bar dot when an AI coding agent is active in the session: detected via env vars (`CLAUDE_CODE_ENTRYPOINT`, `CODEX_*`), delivered through the 0.1 env channel. |
| **Why** | Omarchy's audience runs agents daily; no other Linux desktop shell signals agent state in prompt + bar together. This is the shrunk, unbreakable version of agent monitoring — env-var detection degrades to "hidden" and cannot break. |
| **How** | New `segments/ai.rs` reading the env channel; bar dot reuses 0.3 status enrichment. |
| **Effort** | 3-4 hr |
| **Depends on** | 0.1, 0.3. Full hook-event registry and session-file telemetry are deferred to v0.5 (watch list). |

### 1.4 OSC 133;C/D semantic prompt emission + Ghostty coordination

| | |
|---|---|
| **What** | Complete the OSC 133 story: emit `133;C` in preexec and `133;D;<exit>` in precmd (A/B already ship with `shell_integration`), unlocking Ghostty 1.3's click-to-position, prompt navigation, one-click output selection, and native notify-on-command-finish — plus Foot block navigation. |
| **Why** | Ghostty 1.3.0 (Mar 2026) shipped click-events and notify-on-finish driven by 133 markers; Zellij 0.45 parses 133; fish/nushell emit natively while bash needs a snippet — we already own the hook timing, so we are the natural snippet. |
| **How** | All four hook points already exist in the broker. Gate on a new `has_osc133` TermCaps bit (detect `GHOSTTY_SHELL_INTEGRATION_FEATURES`). **Mandatory coordination spike**: detect Ghostty's own injected bash integration and suppress one source — double-emission is a real corruption class. Degrade silently under tmux (does not forward 133). |
| **Effort** | 6 hr (includes the spike) |
| **Depends on** | Empirical coexistence test on Ghostty 1.3.x before default-on. |

---

## Tier 2 — Omarchy Desktop Integration (the moat made visible)

### 2.1 Plugin IPC target — `omarchy-shell call community.omarchy10k <method>`

| | |
|---|---|
| **What** | Register IPC methods on the Quattro plugin so any keybind, script, or third-party tool can drive Omarchy10k: `status`, `sessions`, `set-layout <preset>`, `toggle-transient`, `picker`, `invalidate-git`. |
| **Why** | The deepest integration Quattro offers and zero precedent in Starship/P10k/OMP: a Hyprland keybind `omarchy-shell call community.omarchy10k set-layout powerline` turns Omarchy10k from a prompt into a desktop service. |
| **How** | Bridge QML methods to the panel's existing daemonSocket + Model.js helpers; `status` answers from the bar widget's polled state; config-affecting methods go through `config_set`; spawn the CLI as fallback when no daemon. install.sh adds a Keybindings hint block. |
| **Effort** | 6 hr |
| **Risk** | QML IPC-registration API unverified — spike against `PluginRegistry.qml` / `omarchy-shell.md` first. |

### 2.2 Service-kind plugin — one persistent connection hub

| | |
|---|---|
| **What** | Add a `service` entry point to the manifest (headless singleton, survives panel open/close) holding persistent connections to every discovered daemon and exposing aggregated state (sessions, status, events) to BarWidget, Panel, and overlays. |
| **Why** | Today three independent connection lifecycles do the same work (bar polls 5s, panel reconnects, each re-runs discovery). A service kills the polling waste and is the substrate for push events (`long_command`, `git_stale`, `battery_low`) — the event channel full repaint was deferred for. |
| **How** | `Service.qml` reuses the socket-finder pattern but keeps connections alive; BarWidget/Panel consume its properties (delete `barPollTimer` duplication). Daemon-side push events ride the persistent socket. |
| **Effort** | 10 hr |
| **Depends on** | 0.3 (status enrichment); spike the service-kind plugin contract first (unverified platform surface — see Watch List). |

### 2.3 Session Picker overlay (overlay-kind plugin, keybind-summoned)

| | |
|---|---|
| **What** | Fullscreen overlay listing every live shell session (CWD, git branch, dirty state, last command duration, Hyprland workspace via `hyprctl -j clients`); pick one to focus its terminal window or open a floating terminal at that CWD. |
| **Why** | The #1 wow moment a prompt tool can offer a tiling desktop: all your shells as first-class desktop objects. The intel docs' 4.3 covered only *labels*; an overlay is the experience, and it uses the platform's overlay kind — untouched territory for a prompt tool. |
| **How** | Data from 2.2; rows read `~ work 2 • main ●` from the enriched `status`; focus via `hyprctl dispatch focuswindow pid:NNNN`; fallback `omarchy-launch-floating-terminal`. Summoned via 2.1's `picker` IPC method. |
| **Effort** | 10 hr |
| **Depends on** | 2.1, 2.2; hyprctl pid→window mapping needs a graceful non-Hyprland hide. |

### 2.4 Consume the rest of the hook system: battery-low, post-update, font-set

| | |
|---|---|
| **What** | `battery-low` → flash bar glyph red + force-show the urgent battery segment even in minimal presets. `post-update` → opt-in `omarchy10k update` self-sync so binary/plugin/hook/template stay in lockstep after Omarchy updates. `font-set` → re-check Nerd Font presence, auto-fallback separators to ASCII + warning instead of tofu. |
| **Why** | Omarchy fires these events for free; we are the only consumer positioned to act, and each maps to a real pain (the wrong-font broken-prompt screenshot is the classic one; post-update version skew is the top predicted support burden). |
| **How** | Three small hook scripts reusing the theme-set fan-out pattern in `theme-set.d`-style `.d` dirs. |
| **Effort** | 6 hr |
| **Risk** | Hook payload args unverified per installed Omarchy version; auto-update must never run on metered networks (opt-in). |

### 2.5 shell.json widget settings + installer auto-enable

| | |
|---|---|
| **What** | Declare bar-widget knobs (glyph char, git-dot toggle, poll interval) as inline shell.json settings so users configure the widget from Omarchy's own surface; installer calls `setPluginEnabled`/`setBarWidget` via omarchy-shell instead of printing manual "step 3" instructions. |
| **Why** | Widget-appearance knobs live in the wrong config today, and the install flow ends in a manual step. Presence in `bar.layout`/`plugins[]` IS the enabled state in Omarchy. |
| **Effort** | 5 hr |
| **Risk** | Community-plugin settings schema unverified; needs a precedence rule vs config.toml for widget-only knobs. |

### 2.6 Omarchy icon font glyphs in prompt segments

| | |
|---|---|
| **What** | Use `omarchy.ttf` private-use glyphs (Omarchy's own icon font) for the OS segment icon, bar glyph, and separators — falling back to Nerd Font/ASCII when absent. |
| **Why** | The prompt shares exact iconography with the bar/tray — the "panel, prompt, terminal feel like one thing" test at glyph level. |
| **How** | Glyph table in os.rs/character.rs for the omarchy.ttf PUA range (extract codepoints from the font, never guess); font-probe gate. |
| **Effort** | 3 hr |

### 2.7 Desktop-driven prompt theming: keybind switchers

| | |
|---|---|
| **What** | `omarchy10k theme-set`, `layout-set`, `palette` CLI/IPC methods so one keybind restyles the prompt in every open shell. |
| **Why** | Omarchy users switch themes obsessively; letting the desktop *drive* prompt style makes the terminal feel native. |
| **How** | Thin socket clients reusing the control-message path + the v0.3 `palette` command. |
| **Effort** | 4 hr |

### 2.8 Menu quick actions + `omarchy10k script` structured output

| | |
|---|---|
| **What** | Menu-kind entry on the bar widget (right-click): toggle transient, invalidate git, reload theme, copy config, open floating terminal here. Plus a stable `omarchy10k script --last-command --git --session` output contract for other tools. |
| **Why** | The bar widget has exactly one action today; context menus are the Omarchy ecosystem norm. The structured output makes Omarchy10k a data source for user scripts — integration leverage out of proportion to size. |
| **Effort** | 5 hr |
| **Depends on** | 0.3; menu-kind contract spike shared with 2.2. |

---

## Tier 3 — Onboarding & Panel (make the first 60 seconds sing)

### 3.1 ANSI-colored live preview in the Quattro panel

| | |
|---|---|
| **What** | The Control Center's live preview currently calls `Model.stripAnsi(resp.left)` and shows plain text — replace with `Model.ansiToRich()`, an ANSI-to-`Text.StyledText` converter, so the preview shows actual colors. |
| **Why** | The preview is the interactive 60-second wow for configurators and it is currently color-blind. Oh My Posh Studio's entire value proposition is this, and we already ship the harder half (daemon-side simulated rendering). |
| **How** | Same tokenizer as `stripAnsi` but keep SGR fg/bg as styled spans; the preview path already omits OSC 133/np-wrappers, so the escape stream is a clean SGR-only subset. Existing context toggle chips (error/git/duration) now visibly restyle. |
| **Effort** | 4 hr |

### 3.2 `omarchy10k gallery` — live preset showcase

| | |
|---|---|
| **What** | CLI command rendering all 10 style presets with rich simulated context, one per keypress; `--apply <name>` writes the choice via config_set. |
| **Why** | Preset discovery is blind today. Also drives adoption of 1.1 and doubles as a screenshot strip of the renderer's breadth. Zero new rendering — the `preview` message already does it. |
| **Effort** | 4 hr |

### 3.3 `omarchy10k intro` — first-run signature moment

| | |
|---|---|
| **What** | One-time themed welcome render on first shell start: rounded frame (the frame renderer *is* a banner renderer), palette swatch blocks, detected terminal capabilities, measured prompt latency ("renders in 2.9ms"). |
| **Why** | The "whoa in 60 seconds" moment currently has no answer, and it doubles as verification output — palette proves theme sync, latency proves the sub-5ms brand. |
| **How** | Reuses the preview path with `shell_integration=false`; one-shot marker file; `O10K_NO_INTRO` gate for CI; `--force` for demos. |
| **Effort** | 4 hr |

### 3.4 `omarchy10k configure` — bash-first P10k succession wizard

| | |
|---|---|
| **What** | Interactive TUI survey (layout, transient, git detail, segments) with live preview via the `preview` message, writing config.toml atomically. |
| **Why** | P10k is on life support; its signature onboarding is the missing piece for the migration wave. We already ship its two killer features (instant prompt, transient) for bash — a thing P10k never did. |
| **How** | dialoguer or hand-rolled TUI over existing config keys — a questionnaire, not a second config engine. Mirrors the Quattro panel structure so both stay in sync. |
| **Effort** | 6 hr |

### 3.5 Right-prompt rail

| | |
|---|---|
| **What** | Turn `render_right` from its two hardcoded items into a config-driven placement target any segment can occupy, via RPS1 under ble.sh and framed right-content otherwise. |
| **Why** | The asymmetric zsh/pure aesthetic screenshots extremely well, and the plumbing is nearly complete: the daemon returns `right`, the bridge frames it, the adapter applies it via `bleopt prompt_rps1` — but only under ble.sh, and `render_right` hardcodes duration+branch. |
| **How** | `rail: Right` placement field or `prompt.right_segments` config list; keep priority/width-fitting consistent with the left side. Bare-bash right-alignment is a UX decision, not an engineering one — gate it. |
| **Effort** | 6 hr |

### 3.6 Atuin-native context segment

| | |
|---|---|
| **What** | Read Atuin's local SQLite (read-only, WAL-safe) for "most-used command in this directory" / session command count instead of building our own history tracker. |
| **Why** | Atuin ships per-directory stats and (v18.13+) a daemon; Omarchy bundles it. Building our own stats (v0.3 intel 4.6) duplicates a tool our users already run — Ohno waste. |
| **Effort** | 3 hr |
| **Risk** | Atuin DB schema is versioned — pin fields, graceful absence. |

### 3.7 `omarchy10k img <file>` + one-shot `omarchy10k logo`

| | |
|---|---|
| **What** | Inline image preview CLI (kitty gfx on Ghostty/Kitty, sixel on Foot) for screenshots and album art via `playerctl metadata mpris:artUrl`; plus a one-shot `logo` command printing brand art + system info. |
| **Why** | The strongest remaining "whoa" — delivered as a *command*, never in the prompt (an image in PS1 desyncs bash cursor math; see Kill List). Gets the identical screenshot wow with none of the cursor hazards. |
| **How** | `image` crate + TermCaps `has_kitty_gfx`/`has_sixel` bits; sixel encoder is the long pole. |
| **Effort** | 6 hr |


## Kill List (ratified — do not re-litigate without new evidence)

| Idea | Verdict |
|---|---|
| WASM plugin system (v0.3 intel 4.3) | Real cost is a forever-versioned plugin ABI + wasmtime binary bloat + arbitrary-code-execution surface. Freezing a plugin API on a context model we know is wrong guarantees breakage. Revisit only if shrunk custom segments can't express real needs. |
| Kitty-graphics logo in the prompt (3.2/4.4) | Gimmick that damages the sub-5ms brand: payload per prompt, breaks on resize/scrollback, garbage on Foot. Replaced by 3.7's CLI approach — 80% of the wow, zero prompt-path risk. |
| DEC 2031 / OSC 11 light-dark detection (3.5) | Requires reading the TTY from a prompt hook — the classic hang/input-corruption trap, for near-zero value on a platform where the desktop theme-set hook already re-syncs the palette. |
| Tmux/Zellij status bridge (4.2) | Architecturally incoherent with per-shell daemons ("which daemon does tmux query?"), for a segment (multiplexer users) the Hyprland-first audience barely contains. Revisit on real demand. |
| Command history statistics (4.6) | Solved better by Atuin, which our users already run. Replaced by 3.6. |
| Prompt animations / motion in PS1 | Requires timer-driven PROMPT_COMMAND redraw storms; contradicts "fast enough to disappear". The only legitimate motion surface is QML in the bar widget. |
| Visual drag-and-drop prompt editor | Depends on segment ordering (doesn't exist) + fragile QML DnD. The segment toggle grid (QoL 3.1) delivers most of the value. v0.6+ at the earliest. |
| One-click tool setup in panel | The panel spawning `curl \| bash` is a security smell and a support liability. Doctor diagnoses; the fix is a copyable install command. |
| OSC 52 clipboard integration (3.6) | `pwd \| wl-copy` does this in one line of user config on Wayland. Document the keybinding, ship no code. |

**Phasing note — streaming repaint:** full daemon-pushed `segment_update` repaint (v0.3 intel 2.2) is deferred, not killed. v0.4 ships the honest slice (0.4's stale placeholder + `git_stale` wiring); the push channel arrives with the 2.2 Service plugin, and only then becomes worth its bash-redraw complexity.


## Watch List (external assumptions that could invalidate plans)

| Watch | Fallback |
|---|---|
| Omarchy Quattro / Quickshell plugin API churn (service/overlay/menu kinds, IPC registration, `qs.Commons`) — all young and unverified | Spike `PluginRegistry.qml` + `omarchy-shell.md` before 2.2/2.3; keep the plugin self-contained; feature-detect QML APIs; budget a 1 hr compatibility pass per Omarchy release |
| Ghostty capability changes (kitty gfx, 133 coexistence) | TermCaps bitfield + conservative unknown-terminal defaults — adapting is hours by design |
| Claude Code statusline JSON schema drift | Tolerant parsing + contract test per bump; env-var-only agent signal (1.3) cannot break |
| ble.sh bus factor (single maintainer) | Vanilla-bash mode stays strictly first-class; no new ble.sh-only capabilities |
| Omarchy hook names/payloads (battery-low, post-update, font-set) | Verify against the installed version before 2.4 |
| Omarchy upstream shipping its own prompt story | The daemon + desktop integration is the answer — prioritize moat features over generic prompt features |
| MISE_*/env-manager variable surface churn | Allowlist is data (one array); `.tool-versions` file reads as fallback |


## Recommended v0.4 Scope

**Phase 1 — Foundations (~15 hr):** 0.1 env channel → 0.2 notification fix → 0.3 status enrichment → 0.4 dead-field wiring (one protocol 0.4 bump, one bridge re-framing, shipped together).

**Phase 2 — Headliners (~24 hr):** 1.1 true powerline (ship-first candidate: biggest visible win) → 1.2 statusline claude-code → 2.1 IPC target (+ spike) → 1.3 agent signal → 1.4 OSC 133 (after the coexistence spike).

**Phase 3 — Desktop (~26 hr):** 2.2 service plugin → 2.3 session picker → 2.4 hooks → 2.5-2.8 as capacity allows.

**Phase 4 — Onboarding (~27 hr):** 3.1 ANSI preview first (smallest, most daily value), then 3.3 intro, 3.2 gallery, 3.4 configure, 3.5 right rail, 3.6 Atuin, 3.7 img.

Total full-catalog effort ≈ 92 hr. A realistic one-maintainer v0.4 is **Phase 1 + Phase 2 + 3.1 ≈ 43 hr** — that alone delivers: live context segments, real notifications, true powerline, the agent statusline, scriptability, and a colored live preview. Everything else is catalogued for v0.5+ with its dependencies recorded.

| Priority | Items | Hours |
|---|---|---|
| P0 — v0.4 core | 0.1-0.4, 1.1, 1.2, 2.1, 3.1 | ~43 |
| P1 — v0.4 stretch | 1.3, 1.4, 2.2, 2.3, 3.3 | ~33 |
| P2 — v0.5 backlog | 2.4-2.8, 3.2, 3.4-3.7 | ~38 |
| Killed | 9 ideas on the Kill List | — |


## Sources
