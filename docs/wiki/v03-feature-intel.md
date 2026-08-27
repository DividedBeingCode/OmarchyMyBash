# Omarchy10k v0.3 — Feature Intel

> Research-backed feature catalog for the next release cycle.
> Compiled from dev blogs, terminal emulator source, prompt framework changelogs,
> Reddit/HN discussions, and escape sequence specifications (2025–2026).
> All features evaluated against **Ghostty** and **Foot** — the two default
> terminal choices in Omarchy Quattro.

---

## How to Read This Document

Each feature includes:

| Field | Meaning |
|---|---|
| **What** | One-sentence description |
| **Why** | User-facing value / community demand signal |
| **Escape / API** | Exact escape sequence, env var, or syscall |
| **Ghostty** | Support status |
| **Foot** | Support status |
| **Safe unconditional?** | Can we emit without detection? |
| **Approach** | Implementation sketch referencing existing Omarchy10k modules |
| **Effort** | Rough estimate in hours |
| **Depends on** | Prerequisites from this list |

Features are organized into five tiers by impact × feasibility.

---

## Tier 0 — Terminal Protocol Foundations

Ship-with-v0.3 features. Both Ghostty and Foot support them, they cost almost
nothing to implement, and they dramatically improve the terminal integration
experience.

### 0.1 OSC 7 — CWD Reporting

| | |
|---|---|
| **What** | Tell the terminal the shell's current working directory so "new tab in same dir" works |
| **Why** | Without this, Ghostty/Foot open new splits at `$HOME`. Single highest-impact UX improvement for zero visual cost |
| **Escape** | `\033]7;file://HOSTNAME/url-encoded-path\033\\` |
| **Ghostty** | Yes — native support, uses for split/tab CWD inheritance |
| **Foot** | Yes — native support, uses for "pipe-command-output" CWD resolution |
| **Safe unconditional?** | Yes — unsupported terminals silently ignore it |
| **Approach** | Emit in `__o10k_render_prompt` after prompt is set. Use `$HOSTNAME` and percent-encode the path. Alternatively, emit from daemon in the prompt response as a prefix escape |
| **Effort** | ~30 min |
| **Depends on** | Nothing |

```
printf '\033]7;file://%s%s\033\\' "$HOSTNAME" "$PWD"
```

### 0.2 Synchronized Output (DEC Mode 2026)

| | |
|---|---|
| **What** | Wrap prompt output in begin/end markers so the terminal batches the render atomically |
| **Why** | Eliminates flicker during multi-line prompt redraws. Especially visible on SSH or slow connections |
| **Escape** | Begin: `\033[?2026h` — End: `\033[?2026l` |
| **Ghostty** | Yes |
| **Foot** | Yes |
| **Safe unconditional?** | Yes — terminals that don't understand the mode ignore it (DECRST fallback) |
| **Approach** | In `render.rs`, wrap the full `PromptResponse.left` string: emit `\033[?2026h` before line 1, emit `\033[?2026l` after prompt end marker. Or wrap in the bash adapter around `PS1=` assignment |
| **Effort** | ~15 min |
| **Depends on** | Nothing |

### 0.3 OSC 8 — Hyperlinked Directory Segment

| | |
|---|---|
| **What** | Make the directory path a clickable `file://` hyperlink in the terminal |
| **Why** | Ctrl+click opens the file manager at that path. P10k has this; Starship still doesn't. Adds polish and real utility |
| **Escape** | `\033]8;;URI\033\\VISIBLE_TEXT\033]8;;\033\\` |
| **Ghostty** | Yes |
| **Foot** | Yes |
| **Safe unconditional?** | Yes — unsupported terminals show the visible text normally (hyperlink is invisible metadata) |
| **Approach** | In `segments/directory.rs`, wrap the rendered path string with OSC 8 open/close. URI is `file://HOSTNAME/absolute-path` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

```
\033]8;;file://myhost/home/ian/project\033\\~/project\033]8;;\033\\
```

### 0.4 Desktop Notifications for Long Commands

| | |
|---|---|
| **What** | When a command takes longer than a configurable threshold and the terminal is unfocused, send a desktop notification |
| **Why** | `cargo build` finishes while you're in the browser — you want to know. Massive quality-of-life feature. Community request in every prompt framework's issue tracker |
| **Escape** | OSC 777: `\033]777;notify;TITLE;BODY\007` (Foot, VTE). OSC 99: `\033]99;i=ID:d=0;BODY\033\\` (Kitty spec). Ghostty also has built-in notify-on-command-finish via OSC 133 D |
| **Ghostty** | Yes — supports OSC 777 and has native long-command notification via OSC 133 |
| **Foot** | Yes — supports OSC 777 natively (sends to D-Bus notifications) |
| **Safe unconditional?** | Yes for OSC 777 — unsupported terminals ignore it. Should respect `O10K_NOTIFY=off` config |
| **Approach** | In the bash adapter's `__o10k_render_prompt`, after `__o10k_timer_stop`, check `$__O10K_CMD_DURATION` against threshold. Also add `[segments.notification]` config table with `threshold_ms` (default: 10000). Emit OSC 777 from bash (avoids daemon needing terminal focus state) |
| **Effort** | ~2 hr |
| **Depends on** | Nothing (but pairs well with 0.5 for conditional emission) |

### 0.5 Terminal Feature Detection Engine

| | |
|---|---|
| **What** | Probe terminal identity and capabilities at startup, store a capability bitfield for progressive enhancement |
| **Why** | Foundation for all conditional features (Kitty graphics yes/no, OSC 52 yes/no, underline styles yes/no). Prevents emitting unsupported sequences |
| **Escape / API** | `$TERM_PROGRAM`, `$GHOSTTY_RESOURCES_DIR`, `$FOOT_SOCK`, `$COLORTERM`, `XTVERSION` (DA3), `DECRQM` queries |
| **Ghostty** | Sets `TERM_PROGRAM=ghostty`, `GHOSTTY_RESOURCES_DIR`, responds to XTVERSION |
| **Foot** | Sets `TERM_PROGRAM=foot`, responds to XTVERSION. No `$FOOT_SOCK` but `$WAYLAND_DISPLAY` presence is signal |
| **Safe unconditional?** | N/A — this is the detection mechanism itself |
| **Approach** | New `terminal.rs` module in daemon. On startup, read env vars to identify terminal. Store `TermCaps` struct: `has_osc8`, `has_kitty_graphics`, `has_sixel`, `has_osc52`, `has_undercurl`, `has_osc777`. Expose via `status` response so adapter/Quattro can query. Progressive: known terminals get hardcoded caps; unknown terminals get conservative defaults |
| **Effort** | ~3 hr |
| **Depends on** | Nothing |

**Known terminal capability matrix (hardcoded):**

| Terminal | OSC 7 | OSC 8 | OSC 52 | OSC 777 | Kitty Gfx | Sixel | Undercurl | Sync Output |
|---|---|---|---|---|---|---|---|---|
| Ghostty | Y | Y | Y | Y | Y | N | Y | Y |
| Foot | Y | Y | Y | Y | N | Y | Y | Y |
| Kitty | Y | Y | Y | N | Y | N | Y | Y |
| WezTerm | Y | Y | Y | Y | Y | Y | Y | Y |
| Alacritty | Y | N | Y | N | N | N | N | N |

---

## Tier 1 — New Segments (High Community Demand)

New information segments that represent the most-requested features across
Starship, P10k, and Oh My Posh issue trackers.

### 1.1 Git Worktree Detection

| | |
|---|---|
| **What** | Detect when the user is inside a git worktree (not the main repo) and show an indicator |
| **Why** | Worktrees are the hottest git workflow in 2026. Without an indicator, users forget which worktree they're in. r/commandline top request |
| **Escape / API** | `git rev-parse --git-common-dir` returns a different path when inside a worktree. Also: presence of `$(git rev-parse --git-dir)/commondir` file |
| **Approach** | In `segments/git.rs` or `git.rs`, after finding repo root, check for `commondir` file. If present, extract worktree name from the repo root path (last component). Add `worktree: Option<String>` to `GitStatus`. Display as ` worktree-name` in the git segment |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

### 1.2 Container/Toolbox Detection

| | |
|---|---|
| **What** | Detect when running inside Docker, Podman, Toolbox, or Distrobox containers |
| **Why** | Essential for Fedora Silverblue/Kinoite users who live in toolboxes. Prevents confusion about which environment you're in |
| **Escape / API** | `/.dockerenv` exists (Docker), `/run/.containerenv` exists (Podman), `$container` env var (systemd-nspawn/Toolbox), `$DISTROBOX_ENTER_PATH` (Distrobox) |
| **Approach** | New `segments/container.rs`. Check indicators in order. Display: `⬡ container-name` or `📦 docker`. Config: `[segments.container]` with `enabled`, `icon` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

### 1.3 Python Virtualenv / Conda

| | |
|---|---|
| **What** | Show active Python virtual environment or Conda environment name |
| **Why** | Most-used language environment indicator across all prompts. Python devs expect this |
| **Escape / API** | `$VIRTUAL_ENV` (venv/virtualenv), `$CONDA_DEFAULT_ENV` (conda), `$CONDA_PREFIX` (conda prefix path) |
| **Approach** | New `segments/python_env.rs`. Read env vars. Extract environment name (basename of `$VIRTUAL_ENV`, or `$CONDA_DEFAULT_ENV` directly). Display: `🐍 env-name`. Config: `[segments.python]` with `enabled`, `show_version` (optional: also show `python --version`) |
| **Effort** | ~1.5 hr |
| **Depends on** | Nothing |

### 1.4 Language/Tool Version Detection (mise/asdf)

| | |
|---|---|
| **What** | Show active tool versions managed by mise, asdf, or nvm |
| **Why** | mise (formerly rtx) is exploding in adoption. Developers switching between projects need version context. Starship's most-used module category |
| **Escape / API** | `$MISE_NODE_VERSION`, `$MISE_PYTHON_VERSION`, `$MISE_RUBY_VERSION` etc. Also `.tool-versions` and `.mise.toml` files |
| **Approach** | New `segments/toolchain.rs`. Check mise env vars first (zero-cost). Fall back to reading `.tool-versions` if present. Show icons per language: ` 20`, `🐍 3.12`, `🦀 1.80`. Config: `[segments.toolchain]` with per-language enable/icon |
| **Effort** | ~4 hr |
| **Depends on** | Nothing |

### 1.5 Nix Shell / Flake Detection

| | |
|---|---|
| **What** | Detect when inside a Nix shell or devShell and show the environment type |
| **Why** | NixOS and nix flakes adoption is accelerating. Users need to know if they're in a pure or impure shell |
| **Escape / API** | `$IN_NIX_SHELL` (pure/impure), `$NIX_SHELL_PACKAGES`, presence of `flake.nix`/`shell.nix` in parent dirs, `$DIRENV_DIR` (often used with nix-direnv) |
| **Approach** | New `segments/nix.rs`. Check env vars, display `❄ pure` or `❄ impure`. If `$buildInputs` is set, we're in `nix-shell`. Config: `[segments.nix]` with `enabled` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

### 1.6 Kubernetes Context

| | |
|---|---|
| **What** | Show the active Kubernetes cluster and namespace |
| **Why** | Preventing accidental `kubectl delete` on production is the #1 reason people add k8s to their prompt. DevOps essential |
| **Escape / API** | `$KUBECONFIG` (or `~/.kube/config`), parse YAML for `current-context`, extract cluster/namespace |
| **Approach** | New `segments/k8s.rs`. Read `$KUBECONFIG` or default path. Parse only the `current-context` and `contexts[].context.namespace` fields (don't parse full YAML — use string scanning for speed). Display: `⎈ cluster/namespace`. Config: `[segments.k8s]` with `enabled`, `show_namespace` |
| **Effort** | ~3 hr |
| **Depends on** | Nothing |

### 1.7 Time / Clock Segment

| | |
|---|---|
| **What** | Display the current time in the prompt |
| **Why** | Simple but commonly requested. Useful for knowing when a command was run in scrollback |
| **Escape / API** | `chrono::Local::now()` or `std::time::SystemTime` |
| **Approach** | New `segments/time.rs`. Format per config: `%H:%M`, `%H:%M:%S`, `%I:%M %p`. Config: `[segments.time]` with `enabled`, `format` |
| **Effort** | ~1 hr |
| **Depends on** | Nothing |

### 1.8 Battery Segment

| | |
|---|---|
| **What** | Show battery level with threshold-based coloring |
| **Why** | Laptop users on Linux want battery awareness without looking at a taskbar. Especially relevant for tiling WM users (Omarchy's audience) |
| **Escape / API** | Linux: `/sys/class/power_supply/BAT0/capacity` + `status`. macOS: `pmset -g batt` |
| **Approach** | New `segments/battery.rs`. Read sysfs on Linux. Color: green >60%, yellow 20-60%, red <20%. Icon: 🔋/🔌 based on charging status. Config: `[segments.battery]` with `enabled`, `show_above` (only show when below threshold), `threshold_warning`, `threshold_critical` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

---

## Tier 2 — Architectural Improvements

Deeper changes to the daemon/adapter architecture that unlock performance
gains or new capability categories.

### 2.1 Native In-Process Git via gitoxide

| | |
|---|---|
| **What** | Replace `git status` subprocess with the `gix` (gitoxide) Rust library for in-process git operations |
| **Why** | Eliminates fork+exec overhead entirely. In large repos (Linux kernel, Chromium), `git status` subprocess can take 200-500ms. gitoxide does it in <50ms. Starship and Oh My Posh are both migrating to this. Oh My Posh's `native_status` option shipped in 2025 and showed 3-10x speedup |
| **Escape / API** | `gix` crate — `gix::open()`, `repo.head()`, `repo.status()` |
| **Approach** | Major refactor of `git.rs`. Replace `fetch_git_status` subprocess calls with `gix` API: `gix::open(repo_root)` for repo handle, `repo.head_ref()` for branch, `repo.status()` for porcelain-equivalent data. Keep `git stash list` as subprocess initially (gix stash API is limited). Gated behind `git.backend = "native" | "subprocess"` config. Ship with `subprocess` default, promote to `native` in v0.4 after testing |
| **Effort** | ~8 hr |
| **Depends on** | Nothing |

### 2.2 Streaming Segment Repaint (Async Prompt)

| | |
|---|---|
| **What** | Render fast segments immediately, show placeholder for slow segments, then in-place repaint when slow data arrives |
| **Why** | Git can take 100ms+ in large repos. Users see a frozen prompt during that time. With streaming, the prompt appears instantly with a placeholder (e.g., muted `...` or spinner), then the git segment fills in. Oh My Posh's `serve` daemon already does this. P10k's `instant prompt` is a simpler version |
| **Approach** | In bridge coprocess: send prompt request, daemon responds with `{left, right, pending: ["git"]}`. Bridge outputs prompt immediately. Daemon sends follow-up `{type: "segment_update", segment: "git", content: "..."}` when git resolves. Bridge sends SIGWINCH or uses readline `READLINE_LINE` injection to trigger repaint. Needs the bridge coprocess (already have it) |
| **Effort** | ~6 hr |
| **Depends on** | Bridge coprocess (v0.2 ✓) |

### 2.3 Instant Prompt (Cached Prompt)

| | |
|---|---|
| **What** | Cache the last rendered prompt and display it on shell startup before the daemon is ready |
| **Why** | Shell startup goes from "blank screen for 200ms" to "prompt appears in <5ms". P10k's signature feature and the primary reason it feels fast. Users consistently rate this as the most impactful perf feature |
| **Approach** | In `__o10k_start_daemon`, before waiting for socket, check for `$XDG_CACHE_HOME/omarchy10k/last_prompt`. If exists, set `PS1` to cached content. When daemon is ready and first real prompt renders, overwrite cache file. The bridge can write the cache atomically on every successful render |
| **Effort** | ~4 hr |
| **Depends on** | Nothing |

### 2.4 Custom User-Defined Segments

| | |
|---|---|
| **What** | Config-driven custom segments that run arbitrary shell commands |
| **Why** | Extensibility without recompiling. Users can add project-specific info, custom cloud status, internal tooling indicators. Starship's `[custom.*]` is their most-used advanced feature |
| **Approach** | Config: `[segments.custom.NAME]` with `command`, `when` (condition command or env var check), `format`, `style`, `shell` (default: sh). Daemon executes `command` via `tokio::process::Command`, captures stdout, applies format. Cache result per `cache_ttl` config. Security: sandboxed with timeout (default 500ms) |
| **Effort** | ~6 hr |
| **Depends on** | Nothing |

### 2.5 Layout Presets (Full Implementation)

| | |
|---|---|
| **What** | Fully implement the `prompt.layout` config key with real visual differences |
| **Why** | The config key exists, `LayoutPreset` struct was added in v0.2, but the actual rendering doesn't change separators or line structure per preset. Users expect `powerline` to use arrow separators, `minimal` to be single-line, etc. |
| **Approach** | In `layout.rs`, extend `LayoutPreset` to include separator style, line count, and segment arrangement. In `render.rs`, use the preset to determine: single vs two-line, separator characters (space, arrow ` `, bracket `[ ]`), right prompt inclusion. Presets: `omarchy` (current), `minimal` (dir+char, single line), `powerline` (arrows, all segments), `classic` (brackets), `pure` (async two-line a la Pure ZSH), `dense` (single-line compact) |
| **Effort** | ~4 hr |
| **Depends on** | Nothing |

---

## Tier 3 — Terminal-Native Enhancements

Features that leverage specific terminal capabilities for Ghostty and Foot,
using progressive enhancement (feature detection from 0.5).

### 3.1 Styled Error Indicators (Undercurl + Color)

| | |
|---|---|
| **What** | Use red wavy underline (undercurl) for error segments instead of just red text |
| **Why** | Visually distinctive error state that's immediately recognizable. Both Ghostty and Foot support this. Standard red text can be confused with other colored segments |
| **Escape** | Undercurl: `\033[4:3m`. Underline color: `\033[58:2::R:G:Bm`. Reset: `\033[4:0m\033[59m` |
| **Ghostty** | Yes — full SGR 4:3 + SGR 58 support |
| **Foot** | Yes — full support since 1.13 |
| **Safe unconditional?** | Mostly — terminals that don't understand `4:3` fall back to plain underline. The color escape may be ignored |
| **Approach** | In `segments/exit_status.rs` and `segments/character.rs`, when rendering error state, add undercurl escape before content and reset after. Add `error_style` config: `undercurl` (default where supported), `color` (just red text), `bold` |
| **Effort** | ~3 hr |
| **Depends on** | 0.5 (Terminal Feature Detection) for progressive enhancement |

### 3.2 Kitty Graphics Protocol — Distro Logo

| | |
|---|---|
| **What** | Display an inline image (OS/distro logo, custom icon) in the prompt using Kitty Graphics Protocol |
| **Why** | Visual flair that screenshots well. neofetch/fastfetch users love this. Ghostty supports Kitty graphics natively |
| **Escape** | `\033_Gf=100,t=d,a=T;BASE64_PNG_DATA\033\\` |
| **Ghostty** | Yes — full Kitty graphics protocol support (transmit, display, animate) |
| **Foot** | **No** — Foot explicitly does not support Kitty graphics. Supports Sixel as alternative |
| **Safe unconditional?** | **No** — must detect terminal. Unsupported terminals will show garbage |
| **Approach** | New `segments/logo.rs`. If terminal supports Kitty graphics (Ghostty), encode small PNG (16x16 or 32x32) as base64 and emit via Kitty protocol. If terminal supports Sixel (Foot), use Sixel encoding. If neither, fall back to Nerd Font icon (current behavior). Config: `[segments.logo]` with `enabled`, `source` (auto/kitty/sixel/icon), `image_path` (custom image) |
| **Effort** | ~6 hr |
| **Depends on** | 0.5 (Terminal Feature Detection) |

### 3.3 Terminal Title Management (OSC 0/2)

| | |
|---|---|
| **What** | Set the terminal tab/window title to show useful context (user@host, directory, git branch) |
| **Why** | Essential for tab-heavy workflows. When you have 5 Ghostty tabs, the title is the only way to find the right one |
| **Escape** | OSC 0 (icon+title): `\033]0;TITLE\007`. OSC 2 (title only): `\033]2;TITLE\007` |
| **Ghostty** | Yes |
| **Foot** | Yes |
| **Safe unconditional?** | Yes — universally supported, worst case: title is set but not visible |
| **Approach** | In `render.rs` or bash adapter, emit OSC 2 with formatted title string. Format configurable: `{user}@{host}: {dir}`, `{branch}: {dir}`, `{dir}`. Config: `[terminal.title]` with `enabled`, `format` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

### 3.4 Progress Bar in Tab Chrome (OSC 9;4)

| | |
|---|---|
| **What** | Show a progress indicator in Ghostty's tab bar during long-running operations |
| **Why** | Visual feedback without switching to the terminal tab. ConEmu/Windows Terminal pioneered this; Ghostty adopted it |
| **Escape** | Set: `\033]9;4;1;PERCENT\007`. Remove: `\033]9;4;0\007`. Indeterminate: `\033]9;4;3\007` |
| **Ghostty** | Yes — shows progress bar in tab title area |
| **Foot** | **No** — does not support OSC 9;4 |
| **Safe unconditional?** | Yes — Foot ignores unknown OSC sequences |
| **Approach** | In bash adapter preexec, set indeterminate progress (`\033]9;4;3\007`). In precmd, clear it (`\033]9;4;0\007`). For commands with known progress (e.g., `make` with `-j` output), could parse and report percentage. Config: `[terminal.progress]` with `enabled` |
| **Effort** | ~2 hr |
| **Depends on** | Nothing (but 0.5 helps decide whether to emit) |

### 3.5 Color Scheme Change Detection (DEC 2031 / DSR)

| | |
|---|---|
| **What** | Detect when the terminal switches between light and dark mode and update the prompt palette automatically |
| **Why** | macOS and GNOME users switch dark/light mode via system preferences. Without detection, the prompt colors become invisible on the wrong background |
| **Escape** | `DECRQM` for mode 2031: `\033[?2031$p`. Response `1` = dark, `2` = light. Ghostty also responds to `OSC 11` (background color query) |
| **Ghostty** | Yes — supports mode 2031 and sends mode update notifications |
| **Foot** | Partial — supports `OSC 11` background query but not DEC 2031 notifications. Foot does support Contour's `OSC 22` for color scheme name |
| **Safe unconditional?** | **No** — query-response protocol; must be handled in the terminal I/O layer |
| **Approach** | In daemon startup (or bridge init), query `OSC 11` for background color. Parse response RGB. If luminance > threshold, switch to light palette. Could register for mode 2031 change notifications on Ghostty. On Foot, re-query on SIGHUP or periodic timer. Config: `theme.source = "auto"` mode |
| **Effort** | ~4 hr |
| **Depends on** | 0.5 (Terminal Feature Detection) |

### 3.6 Clipboard Integration (OSC 52)

| | |
|---|---|
| **What** | Programmatic clipboard access for "copy path" or "copy last command" actions |
| **Why** | Enables keyboard-driven workflows: press a hotkey to copy the current directory path or the last command to clipboard without mouse |
| **Escape** | Set clipboard: `\033]52;c;BASE64_CONTENT\007`. Read clipboard: `\033]52;c;?\007` (rarely supported for security) |
| **Ghostty** | Yes — supports OSC 52 set (allows clipboard write). Read is blocked by default for security |
| **Foot** | Yes — supports OSC 52 set. Read controllable via `osc52-allow-read` config |
| **Safe unconditional?** | Write is safe (worst case: ignored). Read requires user opt-in in most terminals |
| **Approach** | New `omarchy10k clipboard` subcommand or daemon command. When invoked (e.g., via keybinding), emit OSC 52 with base64-encoded content. Integrate with ble.sh keybindings: `Ctrl+Y P` to copy path, `Ctrl+Y C` to copy last command |
| **Effort** | ~2 hr |
| **Depends on** | Nothing |

---

## Tier 4 — Stretch Goals / Future Vision

Higher-effort features for v0.4+ consideration.

### 4.1 AI / LLM Context Statusline

| | |
|---|---|
| **What** | Show Claude Code / Cursor / Copilot context: window %, cost, model name |
| **Why** | Starship shipped this as `statusline claude-code` in 2026. AI-assisted development is the primary workflow for Omarchy's audience. Context window depletion is the #1 surprise cost |
| **Escape / API** | `$CLAUDE_CODE_ENTRYPOINT`, `$CLAUDE_CODE_MODEL`, `$CLAUDE_CODE_SESSION_ID`. Or parse `~/.claude/sessions/` JSON files |
| **Approach** | New `segments/ai.rs`. Detect running AI tool from env vars. For Claude Code, read session metadata if accessible. Display: `🤖 claude-4 42%` or `🤖 $0.12`. Config: `[segments.ai]` with `enabled`, `show_cost`, `show_model` |
| **Effort** | ~4 hr |
| **Depends on** | Nothing |

### 4.2 Tmux / Zellij Status Line Bridge

| | |
|---|---|
| **What** | Export daemon state (git, directory, etc.) to tmux or Zellij status bars |
| **Why** | Users who run tmux/Zellij want the same git info in their status line without running a second prompt daemon. Share the cache |
| **Escape / API** | `tmux set-option -g status-right "#(omarchy10k tmux-status)"`. Or tmux control mode. Zellij: plugin API |
| **Approach** | New `omarchy10k tmux-status` subcommand. Queries daemon via socket, formats output for tmux. One-shot command that tmux calls periodically. Could also emit to a tmpfile that tmux reads (lower overhead). Config: `[integration.tmux]` with `enabled`, `format` |
| **Effort** | ~6 hr |
| **Depends on** | Nothing |

### 4.3 WASM Plugin System

| | |
|---|---|
| **What** | User-loadable WebAssembly modules for custom segments |
| **Why** | Ultimate extensibility. Users compile custom segments to WASM, drop them in `~/.config/omarchy10k/plugins/`, and they run sandboxed inside the daemon. No recompilation needed. Paneship (a Rust prompt) is exploring this model |
| **Approach** | Use `wasmtime` crate. Define a WASI interface for segment plugins: `fn render(ctx: &SegmentContext) -> Option<Segment>`. Load `.wasm` files at daemon startup. Sandbox: limit execution time (100ms), memory (16MB), no filesystem/network access. Config: `[plugins]` with `path`, `enabled` |
| **Effort** | ~20+ hr |
| **Depends on** | Stable segment API |

### 4.4 Sixel Fallback for Foot

| | |
|---|---|
| **What** | Sixel graphics rendering path for inline images on Foot (which doesn't support Kitty graphics) |
| **Why** | Foot supports Sixel but not Kitty graphics protocol. If we ship inline images via Kitty (3.2), Foot users get nothing without a Sixel path |
| **Escape** | Sixel: `\033Pq...SIXEL_DATA...\033\\` |
| **Foot** | Yes — full Sixel support |
| **Ghostty** | **No** — explicitly does not support Sixel (uses Kitty graphics instead) |
| **Approach** | Use `image` crate to load PNG, `sixel-rs` or hand-rolled encoder to convert to Sixel data. Emit conditional on terminal detection (Sixel for Foot, Kitty for Ghostty). In `segments/logo.rs`, add Sixel rendering path |
| **Effort** | ~6 hr |
| **Depends on** | 3.2 (Kitty Graphics), 0.5 (Terminal Feature Detection) |

### 4.5 Vi Mode Indicator

| | |
|---|---|
| **What** | Change prompt character based on vi editing mode (normal/insert/visual) |
| **Why** | Vi-mode users (especially ble.sh users) need visual feedback about which mode they're in. The prompt character is the ideal place |
| **Escape / API** | ble.sh: `$KEYMAP` variable (emacs, vi_imap, vi_nmap, vi_xmap). Also `$_ble_keymap_vi_IND` |
| **Approach** | In bash adapter, check `$KEYMAP` variable. Pass as field in prompt request. In `segments/character.rs`, change glyph: insert mode `❯`, normal mode `❮`, visual mode `v`. Color: green for insert, blue for normal, yellow for visual. Config: `[segments.character.vi_mode]` with `enabled`, per-mode glyphs |
| **Effort** | ~2 hr |
| **Depends on** | ble.sh integration |

### 4.6 Command History Statistics

| | |
|---|---|
| **What** | Track command success rate and frequency per directory. Show in right prompt or on-demand |
| **Why** | "This directory's commands fail 40% of the time" is useful signal. Could show most-used command for the directory |
| **Approach** | Track in daemon: `HashMap<PathBuf, DirStats>` with success/failure counts. Persist to `~/.local/state/omarchy10k/history.json`. Show in right prompt on-demand: `✓ 95%` or `✗ 3 fails`. Config: `[features.history]` with `enabled`, `show_in_prompt` |
| **Effort** | ~6 hr |
| **Depends on** | Nothing |

---

## Ghostty vs Foot Compatibility Matrix

Complete matrix for all features in this document:

```
Feature                         | Ghostty | Foot  | Safe Unconditional? | Tier
--------------------------------|---------|-------|---------------------|-----
OSC 7 (CWD reporting)          |   ✓     |   ✓   | Yes                 | 0
DEC 2026 (sync output)         |   ✓     |   ✓   | Yes                 | 0
OSC 8 (hyperlinks)             |   ✓     |   ✓   | Yes                 | 0
OSC 777 (notifications)        |   ✓     |   ✓   | Yes                 | 0
Terminal detection (env vars)   |   ✓     |   ✓   | N/A                 | 0
SGR 4:3 (undercurl)            |   ✓     |   ✓   | Yes (fallback)      | 3
SGR 58 (underline color)       |   ✓     |   ✓   | Yes                 | 3
Kitty graphics protocol        |   ✓     |   ✗   | No (detect!)        | 3
Sixel graphics                 |   ✗     |   ✓   | No (detect!)        | 4
OSC 0/2 (terminal title)       |   ✓     |   ✓   | Yes                 | 3
OSC 9;4 (progress bar)         |   ✓     |   ✗   | Yes (ignored)       | 3
OSC 52 (clipboard)             |   ✓     |   ✓   | Write: yes          | 3
DEC 2031 (theme detection)     |   ✓     |   ~   | No (query-response) | 3
OSC 11 (bg color query)        |   ✓     |   ✓   | No (query-response) | 3
Kitty keyboard protocol        |   ✓     |   ✓   | Needs opt-in        | —
SGR 53 (overline)              |   ✓     |   ✓   | Yes (ignored)       | —
```

---

## Recommended v0.3 Scope

Based on impact/effort analysis, the recommended v0.3 shipment is:

### Must-Ship (P0)
1. **0.1 OSC 7 CWD** — 30 min, massive UX win
2. **0.2 Synchronized Output** — 15 min, eliminates flicker
3. **0.3 OSC 8 Hyperlinks** — 2 hr, competitive differentiator
4. **0.4 Notifications** — 2 hr, high demand
5. **0.5 Terminal Detection** — 3 hr, foundation for everything
6. **1.1 Git Worktree** — 2 hr, most-requested git feature
7. **1.3 Python Env** — 1.5 hr, most common language env

### Should-Ship (P1)
8. **1.2 Container Detection** — 2 hr, essential for toolbox users
9. **1.7 Time Segment** — 1 hr, easy win
10. **2.3 Instant Prompt** — 4 hr, signature perf feature
11. **3.3 Terminal Title** — 2 hr, simple but impactful
12. **2.5 Layout Presets** — 4 hr, completes v0.2 stub

### Nice-to-Have (P2)
13. **1.4 Toolchain Versions** — 4 hr
14. **3.1 Undercurl Errors** — 3 hr
15. **3.4 Progress Bar** — 2 hr (Ghostty-specific but safe)
16. **2.2 Streaming Repaint** — 6 hr, hard but transformative

### v0.4+ Backlog
17. Everything in Tier 4
18. **2.1 gitoxide** — transformative but high risk
19. **2.4 Custom Segments** — high value but complex
20. **3.2 Kitty Graphics** — cool but detection-dependent
21. **3.5 Color Scheme Detection** — useful but complex I/O

---

## Total Estimated Effort

| Priority | Features | Hours |
|---|---|---|
| P0 (Must-Ship) | 7 features | ~11 hr |
| P1 (Should-Ship) | 5 features | ~13 hr |
| P2 (Nice-to-Have) | 4 features | ~15 hr |
| Backlog | 14 features | ~80+ hr |

**v0.3 target: P0 + P1 = ~24 hours of implementation work.**

---

## Sources

- Ghostty source & docs: [ghostty.org](https://ghostty.org), terminal.c escape handling
- Foot changelog & docs: [codeberg.org/dnkl/foot](https://codeberg.org/dnkl/foot)
- Oh My Posh v25 changelog: native git, serve daemon, Studio
- Starship 2026 releases: statusline module, claude-code integration
- Powerlevel10k: instant prompt implementation, transient prompt
- r/commandline, r/unixporn: community feature requests (2025-2026)
- XTerm control sequences: [invisible-island.net](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)
- Kitty protocol spec: [sw.kovidgoyal.net/kitty/graphics-protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)
- Terminal feature comparison: [Are We Sixel Yet](https://www.arewesixelyet.com/), [Terminal Capabilities DB](https://github.com/mawww/kakoune/wiki/Terminal-Capabilities)
