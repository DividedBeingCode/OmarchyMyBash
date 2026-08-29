# Next-Level Brainstorm: Ricing, Control, and Folding Into Omarchy

Date: 2026-08-29
Status: Brainstorm synthesis — Manhattan Project over the platform-coexistence spec + full competitive/rice/panel/adversarial analysis. Not yet an implementation plan.
Inputs: platform-coexistence-design.md, extension-model-design.md, wave1-4 specs, terminal-ricing-research.md, quattro-terminal-experience.md, quattro-qol-intel.md, v03/v04 feature intel, shipped state as of 0.4.0 (see `~/OmarchyMyBash/INSTALL_AND_FIXES.md`).

---

## 0. Verdict on the platform-coexistence spec

**Direction approved; mechanism simplified.** The spec's philosophy (compose and elevate, user definitions always win, nothing unset — only unhooked) is right and ratified. Three structural changes from the colloquium:

1. **No dynamic Bash claim broker.** The spec's `o10k_claim` engine (signature tables, classifier, per-item policy plumbing, extension pipes) is over-engineered for ~20 mostly-static claims: 14 are static `own`, 3 are pure `defer` probes, exactly 1 (`ls`) extends. The adapter is already ~758 lines of bash; it must stay strictly declarative. **Detection, classification, and policy resolution run once in Rust at `init bash` / `doctor` time**; the emitted adapter contains only a flat, pre-resolved definition table. Same zero-per-prompt-cost property, none of the bash-string-parsing risk. Policy config (`[shell.layer]`) is read at init and baked, exactly as the spec proposed.
2. **Prompt handoff happens at startup only.** The spec's `layer release prompt` / `claim prompt` runtime PROMPT_COMMAND surgery across subshells is fragile (hook arrays, trap registrations, ble.sh/ghostty injections). Ship the handoff at init; `doctor` tells the user how to revert (remove the init line — starship takes over automatically). The `layer` CLI reports state; it does not mutate a live shell.
3. **`config/default.toml` is documentation, not config.** `Config::load()` never parses it; keys added there without serde fields are dead. The spec's §9 keys (`[shell.layer]`, `[rice]`) must land in `config.rs` (serde) + `docs/wiki/config.md`, with `default.toml` updated only as the commented reference. (Its duplicate `[directory]` table is already fixed in-tree.)

Also adopt from critique: budget a real bash fixture harness for §11 testing (none exists today — the integration suite greps adapter output only); add a `doctor --fix` concept (guided, printed, user-executed — never silent edits); treat terminal-include wiring as opt-in and strictly subordinate to Omarchy themes (never emit `background`/`foreground`/`palette`).

---

## 1. The unified idea backlog (colloquium-ranked)

Ranked by consensus across divisions (resonance = multiple independent analyses converged). Scope: S ≤ a session, M ≤ a few, L = planned wave.

### Tier A — high consensus, do first (after the simplified coexistence wave)

| # | Idea | Pitch | Substrate | Scope |
|---|------|-------|-----------|-------|
| A1 | **Looks Studio** | Gallery detail sheet becomes an editor: tap palette roles/patch rows, live dry-run re-render per edit, save as user Look. Closes the *create* loop — gallery → design tool. | Gallery.qml + `looks_save` + one protocol field: arbitrary `patch` override in PreviewRequest (generalizes the existing style/look overrides). Optionally `looks_delete`. | M |
| A2 | **Gradient Ramp Designer** | Two color picks → live stepped-ramp preview across segments and gap fill → save as a Look patch. The panel's ricing centerpiece; rides the shipped lerp/ramp_color/gap_gradient engine. | Same PreviewRequest patch field as A1 — A1 and A2 are one feature surface. | M (same PR as A1) |
| A3 | **System dashboard cards** | Parse doctor into per-subsystem cards (daemon/tools/shell layer/terminal/hooks) with green/amber/red chips; benchmark as a braille sparkline. | QML-only; Model.js parsing; no protocol change. | S |
| A4 | **Doctor remediation cards** | Missing tools (fd, tldr, delta, blesh) as copyable-command cards — honors the kill-list ruling against panel-spawned installers while keeping friction low. Also resolves the existing tension with the v0.3 one-click install buttons. | QML-only + toolDetector. | S |
| A5 | **Coexistence spec, simplified** (§0 above) | Claim table resolved in Rust; static adapter definitions; prompt handoff at init; BAT_THEME→ansi deference; ghostty/foot optional includes; gap-fill aliases (lg, lzd, help→tldr) with `command -v` guards; doctor Shell-layer section. | Adapter (emitted), CLI init/doctor, installer. | M |
| A6 | **Stabilization pass** | B-Adversary checklist: E2E coverage for today's manually-verified surfaces (bar badges, gallery Try/Apply, script verb E2E, rail rendering) before new waves; bash fixture harness seed. | tests/. | S–M |

### Tier B — the "next level" swings (each needs a spike or a prerequisite)

| # | Idea | Pitch | Prereq | Scope |
|---|------|-------|--------|-------|
| B1 | **Terminal Modes** (minimal lovable) | `omarchy10k mode <name>` (Focus/Presentation/SSH/Production): mode enum → theme-rendered ghostty/foot include re-render (cursor accent, opacity, background tint) + prompt personality patch + panel/bar indicator. "Looks, one level up." | Coexistence include seam; **spike: ghostty live reload on include change outside theme-set**. | L |
| B2 | **Shell-Layer Map page** | Fifth rail bucket rendering the claim map (owner × policy per item), row toggles writing `[shell.layer.overrides]` via the existing delta-save path; release/claim state shown. | A5 + **Panel.qml decomposition**. | M |
| B3 | **Panel decomposition** | Extract bucket Components + shared row/card library from the 2350-line Panel.qml. Hard prerequisite for B2 and risk-reduction for everything else (today's crash history: color_bg, configKey, wheel Boost). | — | M–L |
| B4 | **Agents view** | Sessions with detected AI agents (ai.rs env detection already works): glyph, project, age, state dot later. One status field (`agent`) added to the hub's status stream. | Status field only; presence-only MVP is S. State telemetry is speculative until agent-side hooks exist. | S (MVP) |
| B5 | **Look share/import** | `look export` (portable TOML bundle / gist URL / clipboard) + `look install <url-or-paste>`. Turns Looks into the community share format. | A1's patch schema makes bundles canonical; needs a small fetch/trust design. | M |
| B6 | **OSC 8 clickable prompt** | Hyperlink directory → file manager, branch → remote URL, toolchain → docs, gated by TermCaps. "The clickable prompt" — high wow, low cost. | OSC 8 already used for directory links in one form; generalize per-segment. Verify current shipped coverage first. | S–M |
| B7 | **Vanilla-bash transient/right parity** | First-class transient collapse + right-rail on bare bash (no ble.sh): closes the gap for refugees who skip ble.sh. | Bridge 4th field + OSC133; starship proves transience works without ble.sh. | M |

### Tier C — differentiators worth designing, not building yet

- **Per-directory/project profiles** (`[profiles.<name>]` + `.o10k.toml` in repo root; auto-apply on chpwd via env-channel cwd). L; needs profile-resolution design + config watcher work.
- **p10k-grade configure wizard depth**: segment-by-segment add/remove/reorder flow, context-rich previews (exit≠0, dirty, ssh), finish = "save as profile." L; the wizard is appearance-only today.
- **Index-based / inherit-terminal palette mode** (`theme.source = "terminal"` reading the live ghostty palette file — no TTY query, kill-list-safe). M; makes wallpaper-driven retheming free.
- **Dither gap fill for 16-color/SSH terminals** (░▒▓ between backgrounds — "the gradient that survives"). S–M; distinctive.
- **Cursor persona + per-theme font/spacing personality** via the include seam (user override always wins). M; font changes are high-visual-risk — opt-in only.
- **Migration importer** (`omarchy10k migrate` from starship.toml / .p10k.zsh / omz). SPECULATIVE — foreign-schema mapping is unbounded; revisit on demand.
- **Plugin economy distribution** (`plugin add <git-url>`, trust model, curated catalog). L; extension-model spec explicitly defers it — the loudest remaining omz-refugee draw.

### Explicitly rejected (colloquium consensus)

- **Cross-shell (zsh/fish) adapters** — anti-Quattro; Omarchy is bash-first, starship/tide own that space.
- **Dynamic bash claim broker** — replaced by Rust-side resolution (§0).
- **Runtime in-session prompt mutation** — startup-time only.
- **High-frequency ambient session watchers / agent socket polling in the prompt path** — kill-list adjacent; agent state must ride the existing status cadence or push events, never the prompt.
- **Upstream/ISO fold-in now** — requires multi-terminal verification (Ghostty/Foot/Alacritty/Kitty), CJK/UTF-8 width validation, hardened ABI contract, and zero terminal-specific user edits first.

---

## 2. Recommended sequencing

```
Wave C1 (stabilize + quick wins)   A6 + A3 + A4 + B4(MVP) + B6
Wave C2 (coexistence, simplified)  A5 (+ claim map groundwork in doctor)
Wave C3 (create loop)              A1 + A2 (Looks Studio + ramp designer)
Wave C4 (decompose)                B3 (Panel components + shared card library)
Wave C5 (control surfaces)         B2 (Shell-Layer page) + B7 (vanilla parity) + B5 (share/import)
Wave C6 (modes, after spike)       B1 (Terminal Modes) — gated on ghostty reload spike
Backlog                            Tier C items; plugin distribution spec; importer on demand
```

Protocol surface total across all waves: PreviewRequest `patch` override field, optional `looks_delete`, one `agent` status field, optional `layer`/`modes`/`history` verbs — all incremental against the v0.5 feature-gated handshake, no breaking bump.

---

## 3. Division provenance

| Division | Contribution |
|---|---|
| B-Rice | Rice techniques beyond shipped (ramp designer, dither fill, OSC 8, index palette, cursor/font persona, modes-minimal) |
| B-Compete | p10k/omz refugee gaps (wizard depth, share economy, profiles, instant-prompt robustness, vanilla parity, importer) |
| B-Panel | Control Center surface (Shell-Layer page, Looks Studio, modes switcher, agents view, timeline, dashboard cards, per-session targeting, remediation cards) |
| B-Platform | Spec critique (broker simplification, startup-only handoff, default.toml reality, test-infra budget, upstream/packaging/doctor --fix gaps) |
| B-Adversary | Anti-recommendations (no bash broker, no panel expansion pre-decomposition, no include override without platform sync, no ambient watchers, no premature ISO fold-in) + stabilization checklist |
