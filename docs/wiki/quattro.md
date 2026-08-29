# Quattro Plugin Reference

[← Index](INDEX.md) | [Protocol](protocol.md) | [Configuration](config.md)

The Quattro plugin provides a desktop Control Center for Omarchy10k, surfaced as a bar widget in the Omarchy Quattro panel. It reads and writes config files, communicates with running daemon instances over Unix sockets, detects installed shell tools, and previews prompt output in real time.

**Protocol version:** 0.3 hello handshake (feature gating). The v0.4 daemon adds `style_preset` and `look` override fields on `preview` requests; the v0.5 daemon adds the `looks` / `looks_apply` / `looks_save` / `palettes` / `defaults` control verbs.

## Manifest (`quattro/manifest.json`)

```json
{
  "schemaVersion": 1,
  "id": "community.omarchy10k",
  "name": "Omarchy10k",
  "version": "0.4.0",
  "author": "Ian Johnston",
  "license": "MIT",
  "description": "Control Center for the Omarchy10k shell experience. Configure prompt style, theme, segments, and shell integrations from the Quattro bar.",
  "kinds": ["bar-widget", "service", "overlay"],
  "entryPoints": {
    "barWidget": "BarWidget.qml",
    "service": "Service.qml",
    "overlay": "SessionPicker.qml"
  },
  "barWidget": {
    "displayName": "Omarchy10k",
    "category": "Shell",
    "allowMultiple": false,
    "defaultSection": "right"
  }
}
```

- `kinds` ↔ `entryPoints` are 1:1: `bar-widget`→`BarWidget.qml`, `service`→`Service.qml`, `overlay`→`SessionPicker.qml` (all relative, inside the plugin dir)
- `allowMultiple: false` — only one bar widget instance; `defaultSection: "right"`
- The `service` kind mounts `Service.qml` at shell startup **when the plugin is enabled** — for third-party plugins that means the id is present in `shell.json` `plugins[]` (`omarchy plugin add`/`enable` does this)
- The `overlay` kind is loaded on summon (`omarchy-shell shell summon community.omarchy10k '<payload>'` or the plugin's own `picker` IPC method)
- Older Quattro hosts that do not know `service`/`overlay` kinds simply ignore them (manifest validation only requires a non-empty `kinds` array and safe relative `entryPoints` paths) — the bar-widget path keeps working unchanged
- Panel path is not declared in manifest — `BarWidget.qml` loads `Panel.qml` via `Loader`

## Component Hierarchy

```
Service (Item, service kind — mounted at shell startup)
├── daemonStatus / sessions / lastStatus reactive state
├── eventReceived(var) signal bus
├── serviceSocketFinder Process → discovers omarchy10k-*.sock with liveness
│   filter: owning shell PID must be alive (kill -0) AND the socket must
│   accept a connection (socat probe) — dead shells and SIGKILLed daemons
│   leave socket files behind that would otherwise surface as ghost sessions
├── Instantiator → one persistent Socket per session (hello → status)
├── controlSocket Socket (config_get/config_set/invalidate_git for IPC)
└── IpcHandler target "community.omarchy10k"

BarWidget (qs.Ui.BarWidget)
├── omarchyService (shell.serviceFor("community.omarchy10k"), feature-detected)
│   └── present → mirrors daemonStatus, poll timer off
│   └── absent  → barSocketFinder + barStatusSocket + barPollTimer (5s), unchanged
├── status-stream badges: health-colored glyph, git dirty dot, ⏱ long-cmd chip
├── Loader → Panel.qml
└── WidgetButton (❯ glyph, ✓/✗ tooltip) + IpcHandler "community.omarchy10k.panel"

Panel (qs.Ui.Panel, manageIpc: false)
├── Header: connection dot + title + "◧ bg" backdrop toggle + ↩ Undo button
├── Live prompt preview (ANSI → StyledText colors) + Error/SSH/Long cmd toggles
├── Rail bar: Looks · Style · Behavior · System (4 buckets, Loader-switched)
├── omarchyService mirror (daemonStatus + sessionList, feature-detected)
├── 4× Component buckets (Looks / appearance / behavior / system)
└── Processes/Sockets/Timers as in v0.3 (own daemonSocket still drives preview/config)

SessionPicker (Item, overlay kind — summoned on demand)
├── open(payloadJson) / close() entry-point contract
├── page mode: payload {"page":"gallery"} → Loader → Gallery.qml; else sessions
├── PanelWindow (WlrLayer.Overlay, exclusive keyboard) + scrim + Escape/dismiss
├── ListView of live sessions (CWD, branch, dirty, last duration, age, ws label)
├── hyprctl -j clients → workspace id per shell pid (no timer, skipped off-Hyprland)
├── focuswindow pid:<shellPid> via hyprctl → fallback omarchy-launch-floating-terminal
└── graceful empty state (no sessions / service not loaded / non-Hyprland)

Gallery (Item, loaded inside SessionPicker's gallery page)
├── open(payloadJson) / close() overlay contract; Escape/scrim dismiss
├── `looks` verb → category chips + search + card grid (live dry-run previews)
├── detail sheet: old → new patch rows + large preview + Try (transient) / Apply
└── own IpcHandler target "community.omarchy10k.gallery" (toggle/open/close)
```

## BarWidget (`BarWidget.qml`)

The bar widget mirrors the Service hub's state when the host loaded `Service.qml`; otherwise it maintains its own daemon connection independent of the panel, so connection status is visible even when the Control Center is closed.

### Properties

| Property | Purpose |
|----------|---------|
| `barDaemonStatus` | `"running"`, `"stopped"`, `"error"`, `"not running"`, or `"unknown"` |
| `barSocketPath` | Path to the first discovered `omarchy10k-*.sock` |

### Daemon Status: Service hub first, poll fallback

`readonly property var omarchyService: root.bar.shell.serviceFor("community.omarchy10k")` — feature-detected (guarded on `bar`, `bar.shell`, and `typeof serviceFor === "function"`):

- **Service present:** `barDaemonStatus` tracks `omarchyService.daemonStatus` (via `Connections.onDaemonStatusChanged`), the widget's own status socket is disconnected, and `barPollTimer.running: !root.opened && !root.omarchyService` stops the 5s poll — the hub's persistent connections replace it.
- **Service absent (old host, plugin not enabled as a service, or hub failed):** exactly the v0.3 path — `barPollTimer` fires every 5 seconds while the panel is closed:
  1. `discoverBarSocket()` — lists sockets, takes the first match
  2. Connects `barStatusSocket` to that path
  3. Sends `hello` handshake, then `status` command
  4. Updates `barDaemonStatus` from the response

  Polling stops while the panel is open (`running: !root.opened`) to avoid duplicate IPC with the panel's `daemonSocket`.

### Tooltip

The bar glyph tooltip reflects live daemon status:

- `"Omarchy10k ✓"` when `barDaemonStatus === "running"`
- `"Omarchy10k ✗"` otherwise

### Bar Intelligence (status-stream badges, no new timers)

All three badges are pure bindings over the `status` payload the widget already receives (Service hub's `lastStatus` in service mode, the existing 5s poll response otherwise — no new timers):

1. **Glyph health color** — `barGlyphColor`: `Color.accent` while the daemon answers, `Color.urgent` when disconnected/not running. Replaces the default bar foreground on the ❯ glyph.
2. **Git dirty dot** — 6 px dot next to the glyph, driven by `status.git` `{branch, dirty, staged, unstaged}`: accent while staged-only, urgent once the tree is dirty/unstaged (`barGitHot`); hidden entirely when git is absent/clean or the daemon is down.
3. **Long-cmd chip** — `⏱ <duration>` in urgent color while the last command's `last_cmd_duration_ms` outlives the threshold; the next fresh status clears it by rebinding. Threshold comes from the hub's `notifyThresholdMs` (0 = notifications off), falling back to `barLongCmdFallbackMs` = 10000.

## Quickshell Imports

| Import | Components Used |
|--------|----------------|
| `QtQuick` | Core QML types |
| `Quickshell` | Base types, `Instantiator`, `PanelWindow` (SessionPicker) |
| `Quickshell.Io` | `Process`, `Socket`, `StdioCollector`, `SplitParser`, `IpcHandler` |
| `qs.Commons` | `Color` (accent, background, muted, green, red) |
| `qs.Ui` | `BarWidget`, `Panel`, `WidgetButton`, `KeyboardPanel`, `PanelKeyCatcher`, `Style` |
| `Model.js` | `parseTOML`, `buildTOML`, `buildCommand`, `buildPreview`, `protocolAtLeast`, etc. |

## Panel Lifecycle

| Event | Actions |
|-------|---------|
| User clicks bar glyph | `toggle()` → `controller.show()` |
| Panel opens | Load config, discover all sockets, detect tools, request preview + palette |
| Config change | Snapshot undo stack → update property → debounce 300ms → `config_set` → request preview |
| Panel closes | `controller.hide()`, disconnect `daemonSocket` |
| Tab switch | `Loader` swaps among 5 tab components |
| Undo click | Pop last config snapshot from circular buffer, re-apply, save |

### Body layout conventions (fixed to match first-party panels)

- Content is wrapped in a `ScrollView` (`ScrollBar.vertical` AsNeeded, `interactive` only when content overflows) — tall tabs scroll instead of clipping or stretching the card.
- `contentHeight: panel.fittedContentHeight(content.implicitHeight, Style.space(560))` — the second cap argument stops the card ballooning per tab.
- The card supplies the padding (`KeyboardPanel` popupPadding); the content Column adds none — no double-inset, dividers use `PanelSeparator` at full width.
- Toast/error are out-of-flow overlays anchored to the card bottom, so appearing notices never resize the panel.

## UX Features (v0.3)

### P0 — Core QoL

| Feature | Implementation |
|---------|----------------|
| Connection status indicator | Green/yellow/red dot in panel header; green = running, yellow = reconnecting, red = not running |
| Hero backdrop toggle | "◧ bg" button in the header cycles a `backdropMode` flag on the header row (visual mode switch for the hero preview area) |
| Doctor output in panel | Scrollable monospace `TextEdit` in the System bucket after "Run Doctor" |
| Live prompt preview | Preview box above the rail; `preview` IPC with simulated context; auto-updates on config save |
| Preview toggles | Error / SSH / Long cmd pill buttons modify preview context and re-request |
| Config diff toast | `"Changed key → value"` accent toast with 2s fade on every config change |

### P1 — Enhanced UX

| Feature | Implementation |
|---------|----------------|
| Dynamic bar glyph | `BarWidget` polls daemon via independent socket; tooltip shows ✓/✗ |
| Floating terminal button | Terminal icon (`\uf120`) on session rows opens shell in that CWD via `floatingTermLauncher` |
| Config diff toast | Same toast as P0; surfaced on every `setConfigValue` call |

### P2 — Polish

| Feature | Implementation |
|---------|----------------|
| Theme color preview | `palette` command on connect/theme change; swatch row for 8 colors |
| Segment toggle grid | New Segments grid (Behavior bucket) with 2-column pills for 8 segment flags |
| One-click tool setup | "Install Atuin" / "Install Mise" buttons in the System bucket when tools missing |
| Config undo | Circular buffer of last 10 config states; "↩ Undo" button in header |
| Config import/export | "Copy Config" / "Paste Config" in the System bucket via xclip/wl-copy |
| Degradation labels | Per-feature: preview shows "Live preview requires daemon v0.3+" and palette shows "Palette preview requires daemon v0.3+" when protocol < 0.3; System bucket shows "full (v0.3+)" / "degraded (upgrade daemon)" |
| Benchmark display | "Run Benchmark" button with scrollable results (`omarchy10k benchmark --iterations 50`) |

## Live Prompt Preview (ANSI-colored, v0.4)

Above the rail, a hero preview box shows the rendered left prompt with simulated context. On socket connect and after each debounced save, the panel sends a `preview` message:

```javascript
{
  type: "preview",
  cwd: "~/projects/my-app",
  exit_code: previewError ? 1 : 0,
  cmd_duration_ms: previewLongCmd ? 5000 : 0,
  cols: 120,
  jobs: previewJobs,
  in_ssh: previewSsh,
  git_branch: "main",
  git_staged: 2,
  git_unstaged: 1
}
```

The daemon response `left` field is converted with `Model.ansiToRich()` (not stripped) and rendered in a monospace `Text` with `textFormat: Text.StyledText`, so SGR colors (3x/9x, 38;5, 38;2, 48;* backgrounds, bold/italic/underline) show as real colors. Pill toggles (Error, SSH, Long cmd) flip boolean preview properties and call `requestPreview()`. `Model.stripAnsi()` remains in use for doctor/benchmark output where color is not wanted.

### `Model.ansiToRich(text)`

Same tokenizer family as `stripAnsi`, but SGR state is carried across the stream and emitted as inline `<span style="…">` markup:

- HTML entities (`&`, `<`, `>`) are escaped first, so prompt text renders literally
- `30–37`/`90–97` → themed ANSI palette, `38;5;n` → xterm-256 (cube + grayscale computed), `38;2;r;g;b` → truecolor; `48;*` mirrors each as `background-color`
- `0` resets all, `1/3/4` set bold/italic/underline, `22/23/24/39/49` clear individual attributes, `2` clears bold
- Every other escape (OSC strings, cursor movement, private modes, `\x01`/`\x02` readline delimiters) is dropped
- Output is a single span per contiguous SGR run; spans are only emitted when a style is active

### Preset Gallery Live Previews

The Style bucket's 8 style cards render **live daemon previews** instead of hardcoded strings: `requestPresetPreviews()` sends one `preview` request per preset with the id `preset-<name>` and the v0.4 `style_preset` override field (daemon renders one-shot with that preset; no config mutation — see [protocol.md](protocol.md)). Responses route by id into `presetPreviews[name]`; the card shows the rich preview (`textFormat: Text.StyledText`), falling back to the static glyph string when the daemon is unreachable or older than the override. Requires daemon ≥ 0.3 for previews, ≥ 0.4 for per-card distinct presets.

## v0.4 Spike: Quattro Plugin Platform Contract (verified upstream)

Verified 2026-08-27 against `basecamp/omarchy` branch `quattro`:

- Manifest/kinds/entryPoints validation + enable semantics: [`shell/services/PluginRegistry.qml`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/shell/services/PluginRegistry.qml)
- Plugin platform + IPC docs: [`docs/omarchy-shell.md`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/docs/omarchy-shell.md)
- Plugin catalogue: [`shell/plugins/README.md`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/shell/plugins/README.md)
- Loader/service wiring: [`shell/shell.qml`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/shell/shell.qml) (`ensureService`, `_syncServices`, panel `Instantiator`)
- IPC-by-plugin example: [`shell/plugins/background/Background.qml`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/shell/plugins/background/Background.qml) (`IpcHandler { target: "background" }`)
- Overlay example: [`shell/plugins/emojis/Emojis.qml`](https://raw.githubusercontent.com/basecamp/omarchy/quattro/shell/plugins/emojis/Emojis.qml)

Contract facts extracted (all load-bearing for this plugin):

1. **Kinds** (1:1 with `entryPoints` keys): `bar-widget`→`barWidget`, `bar`→`bar`, `panel`→`panel`, `overlay`→`overlay`, `menu`→`menu`, `service`→`service`. Entry points are QML `Item`s; `panel`/`overlay`/`menu` must expose `open(payloadJson)` / `close()`.
2. **Injection** — the host sets, when the property is declared on the instance: `omarchyPath`, `shell`, `manifest`, `barWidgetRegistry`, `pluginRegistry`; panel/overlay/menu items additionally get `service` = `shell.serviceFor(<own plugin id>)`.
3. **Service lifecycle** — services are instantiated at shell startup by `shell._syncServices()` for every *enabled* manifest declaring kind `service`. Third-party plugins are enabled ⇔ present in `shell.json` `plugins[]`, so a third-party service needs that entry (plugin add/enable provides it). Services are destroyed when the plugin is disabled/removed; editing plugin files hot-reloads.
4. **Overlay lifecycle** — overlays (like panels/menus) load on demand; a `Loader` stays active while the overlay is open (or forever with `keepLoaded: true`). Summon/hide/toggle: `omarchy-shell shell summon|hide|toggle <id> '<payloadJson>'`; the overlay's `open(payloadJson)` receives the payload, `close()`/`shell.hide(id)` closes. `WlrLayershell.layer: WlrLayer.Overlay` + `WlrKeyboardFocus.Exclusive` is the fullscreen-overlay pattern used by first-party overlays.
5. **IPC registration** — a plugin declares `IpcHandler { target: "<plugin-id>"; function method(arg: type): returnType {} }`. The CLI `omarchy-shell call <id> <method> [args]` dispatches to the function **by its QML name** (camelCase); methods answer on stdout with exit 0. First-party per-widget targets are named for the plugin (`omarchy.clock`), so the third-party convention is `target: "community.omarchy10k"`.
6. **Consuming a service from another component** — bar widgets reach services via `bar.shell.serviceFor("<id>")` (upstream `omarchy.media` widget pattern); panel/overlay components can declare `property var service` and receive their own plugin's service via injection.

## IPC Target: `community.omarchy10k` (2.1)

Registered by `Service.qml` (persistent, so the target exists whether or not any panel is open):

```bash
omarchy-shell call community.omarchy10k status
omarchy-shell call community.omarchy10k sessions
omarchy-shell call community.omarchy10k setLayout powerline   # intel doc's "set-layout"
omarchy-shell call community.omarchy10k toggleTransient        # intel doc's "toggle-transient"
omarchy-shell call community.omarchy10k picker                 # opens the session picker overlay
omarchy-shell call community.omarchy10k invalidateGit          # intel doc's "invalidate-git"
omarchy-shell call community.omarchy10k gallery                # opens the Looks Gallery (payload {"page":"gallery"})
```

| Method | Returns | Behavior |
|--------|---------|----------|
| `status()` | JSON object | `ok`, `daemon`, `sessions`, plus the primary session's enriched `status` fields (pid, version, protocol_version, cwd, git, last_cmd_duration_ms, last_exit_code, session_age_secs, battery). `ok:false` when no daemon is running. |
| `sessions()` | JSON array | One object per live socket: `shell_pid`, `pid`, `cwd`, `branch`, `dirty`, `last_cmd_duration_ms`, `session_age_secs`. |
| `setLayout(preset)` | JSON object | Queues `config_set {"style":{"preset":…}}` on the hub's control socket. |
| `toggleTransient()` | JSON object | Reads cached `prompt.transient` (from `config_get` at connect), queues the flipped value via `config_set`. |
| `picker()` | `"ok"` or JSON error | `shell.summon("community.omarchy10k", "{}")` — opens the overlay. |
| `gallery()` | `"ok"` or JSON error | `shell.summon("community.omarchy10k", {"page":"gallery"})` — opens the Looks Gallery overlay page. |
| `invalidateGit()` | JSON object | Queues the `invalidate_git` control command on the primary socket. |

Notes:

- Method names are the QML function names (camelCase, per upstream's `IpcHandler` dispatch). The intel doc's kebab names (`set-layout`, `toggle-transient`, `invalidate-git`) map onto `setLayout` / `toggleTransient` / `invalidateGit` — QML function names cannot contain dashes.
- **Config methods never throw.** No-daemon / write-failure returns `{"ok":false,"error":"…"}`; a successfully queued write returns `{"ok":true,"queued":true}` (the daemon's ok/error reply is consumed asynchronously; persistent failures surface on the next status poll and via the `eventReceived` signal bus).
- `config_set` writes go through `Model.buildConfigSet` over the hub's own persistent control socket — the panel does not need to be open. (Deviation from the original task wording, which assumed the *panel's* `daemonSocket`: the panel disconnects on close, so an always-loaded hub socket is the only path that keeps the target answering when no panel is open.)
- install.sh keybind hints are W2's scope, not documented here.
- **Second target on the bar widget:** `BarWidget.qml` registers `IpcHandler { target: "community.omarchy10k.panel" }` with `toggle()` / `open()` / `close()` — opens the Control Center popout without a pointer click (mirrors first-party widgets that register one target each). `omarchy-shell community.omarchy10k.panel toggle`.

## Service Plugin: `Service.qml` (2.2)

One persistent connection hub replacing the three duplicated poll/reconnect lifecycles:
- **Per-session sockets:** a `Quickshell` `Instantiator` holds one persistent `Socket` per path; each handshakes (`hello`) and issues `status`. Responses update `sessions[i]` (`pid`, `cwd`, `branch`, `dirty`, `lastCmdMs`, `ageSecs`) and, for the primary session, `lastStatus` + `daemonStatus = "running"`.
- **Derived state:** `notifyThresholdMs` — `[notifications].threshold_ms` from the cached flat config (with the deprecated `[segments.notification]` alias and `notifications.enabled === false` → 0); consumed by BarWidget's long-cmd chip. `openGallery()` summons the Looks Gallery page via the host (`shell.summon(<id>, {"page":"gallery"})`).
- **Control socket:** a dedicated connection to the first socket for `config_get` (seeds the transient/threshold cache) / `config_set` / control commands used by the IPC target.
- **State surface:** `daemonStatus` (`"running"`/`"not running"`), `sessions`, `lastStatus`, and the `eventReceived(var)` signal bus — currently emitted for `config_set_error` (a failed queued `config_set` from the IPC target); daemon push events (`long_command`, `git_stale`, `battery_low`) remain reserved.
- **Consumers:** `BarWidget` mirrors `daemonStatus` (poll timer off); `Panel` mirrors `daemonStatus` + `sessionList` on the hub's change signals; `SessionPicker` reads `sessions` via host injection.
- **Graceful fallback:** every consumer feature-detects the hub (`typeof shell.serviceFor === "function"` + null check). Without it, BarWidget polls as in v0.3, the panel re-discovers on its 5s `reconnectTimer`, and the overlay shows its "service not loaded" empty state. On old hosts that ignore the `service` kind, `Service.qml` is simply never instantiated.

## Session Picker Overlay: `SessionPicker.qml` (2.3)

Fullscreen overlay (summoned by the `picker` or `gallery` IPC method, a Hyprland keybind, or `omarchy-shell shell summon community.omarchy10k '<payloadJson>'`). The payload's `page` field selects the mode: `{"page":"gallery"}` activates the gallery `Loader` (Gallery.qml) instead of the session card; any other/absent payload shows the session list.

- Lists every live session with **CWD, git branch (+ dirty dot), last command duration, session age**, pid, and — under Hyprland — the **workspace label** (`ws <id>`), resolved by one `hyprctl -j clients` call per refresh cycle into `workspaceByPid` (pid→workspace id). No timer; outside Hyprland or on hyprctl failure the map is empty and rows simply omit the suffix.
- **Enter / click** activates: under Hyprland (`HYPRLAND_INSTANCE_SIGNATURE` set) it runs `hyprctl dispatch focuswindow pid:<shellPid>`; on nonzero exit (or non-Hyprland) it falls back to `omarchy-launch-floating-terminal` from the session's CWD, then closes.
  - *Assumption:* the socket-name PID is the shell session PID; `focuswindow pid:` matches client windows exactly, so the pid→window hit is best-effort with the terminal fallback as guarantee.
- **Escape / scrim click** closes (`shell.hide(<id>)` when the host is available, plain hide otherwise).
- **Empty state** when no sessions are live or the service isn't loaded (with a hint); non-Hyprland adjusts the help line to the terminal-fallback behavior.
- Rendering follows the first-party overlay pattern: `PanelWindow` anchored to all edges, transparent, `WlrLayer.Overlay`, exclusive keyboard focus, scrim + centered card, with `Color.menu.*` theme tokens falling back to `Color.*` then hard-coded values.

## Looks Gallery Overlay: `Gallery.qml`

The gallery lives in the plugin tree but is **not** a manifest entry point: the manifest's `overlay` entry point is `SessionPicker.qml`, and the gallery is the `page: "gallery"` mode of that overlay (loaded via `Loader` from SessionPicker; the payload `{"page":"gallery"}` routes there). Gallery.qml additionally registers its own `IpcHandler { target: "community.omarchy10k.gallery" }` with `toggle()` / `open()` / `close()` — callable while the overlay component is loaded (`omarchy-shell call community.omarchy10k.gallery toggle`).

### Data flow

1. `open()` → `ensureConnection()` discovers the first live `omarchy10k-*.sock` and connects a single shared `gallerySocket`.
2. On connect: `buildHello("gallery-handshake")` → on the hello reply, sends `looks` (list) and `config_get` (seeds old values for the diff sheet).
3. Each Look card lazily requests a **real daemon dry-run preview**: a `preview` message with `look: "<name>"` (protocol 0.4+ `look` override — the daemon renders one-shot with that Look, no config mutation). Responses route by `id` (`look-<name>`) into `previewCache`; the first `eagerPreviews` cards and the visible grid are fetched eagerly. A daemon that ignores `look` renders the current look on every card (graceful degradation, same as the preset cards).

### UI

- **Category chips** — derived from each Look's patch top-level keys (`theme→Theme`, `style→Style`, `segments`/`os→Segments`, `frame→Frame`, `git→Git`, `directory→Directory`, `prompt→Prompt`), always prefixed by `All`; clicking a chip filters the grid and re-runs eager previews.
- **Search field** filters by Look name/label; daemon-down state disables it ("No omarchy10k daemon running").
- **Card grid** — preview box (StyledText live render) + label per Look; keyboard navigable (arrows + Enter opens the detail sheet).
- **Detail sheet** — large real render plus `_patchSummary`: every patch leaf as `old → new` (old values from the flattened live `config_get` when available).
- **Try (transient)** → `looks_apply {name, transient:true}` (in-memory; reverted by `reload_config`); **Apply** → persistent `looks_apply`. Confirmation/failure via toast; per-request id tracks completion.

### Curated Looks

8 curated Looks ship compiled-in (`crates/omarchy10kd/src/looks.rs::curated()`): omnarchy, tokyo-rainbow, framed-gradient, lean-pure, slanted-owl, gruvbox-drift, rose-classic, polar-lean. User entries in `[looks.<name>]` shadow curated names (`looks::all` filters shadowed curated entries). Curated **palettes** moved daemon-side (`looks.rs::curated_palette`), so the CLI, gallery, and panel resolve them identically; the `palettes` control verb exposes `{key, theme}` rows from that table (see [protocol.md](protocol.md)).

CLI: `omarchy10k look list|apply <name> [--transient]|save <name>` (main.rs `LookAction`).

## Config Undo

`setConfigValue()` pushes a JSON snapshot of `_configFlat` onto `_undoStack` before each change. The stack holds at most 10 entries (FIFO eviction). The "↩ Undo" button in the header is visible when the stack is non-empty; clicking pops the last snapshot, re-applies properties, and triggers a debounced save.

## Modified-Ink and Per-Row Reset (v0.5)

On panel open (and after a reload) the panel sends the daemon's `defaults` control verb and caches the reply in `defaultFlat` — the factory-default value for every CONFIG_MAP key. Two derived behaviors ride on it:

- **Ink bar** — each `ControlRow` shows a 3 px accent bar on its left edge while its `configKey`'s current value diverges from the default (`isModified(key)`).
- **Reset chip** — a `↺` chip appears after the options on modified rows; clicking calls `resetConfigKey(key)`, which writes the default back via `setConfigValue` (undo snapshot + toast + debounced save as usual).

The snapshot is empty until the first successful `defaults` fetch, in which case no row shows ink or a reset chip.

## Socket Discovery and Daemon IPC

### Discovery

```javascript
socketFinder.exec(["sh", "-c",
    "ls '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock 2>/dev/null"])
```

Enumerates **all** `omarchy10k-*.sock` files in `$XDG_RUNTIME_DIR` (or `/tmp`). Each discovered socket is parsed to extract shell PID and added to the `sessionList` model. The user can select between sessions in the System bucket.

The bar widget uses a separate finder that takes only the first socket (`head -1`) for lightweight status polling.

### Connection Flow

```
socketFinder returns all socket paths
  → sessionList populated with path, shellPid, PID, CWD per session
  → user selects session (or first is auto-selected)
  → daemonSocket.path = selected path
  → daemonSocket.connected = true
    → onConnectedChanged → send hello (version 0.3)
      → response: protocol_version, server_version
      → daemonProtocolVersion stored
    → requestPreview() + requestPalette()
    → sendDaemonCommand("status")
      → response parsed → daemonStatus = "running", pid, version set
    → loadConfig() via config_get
```

### Reconnection

`reconnectTimer` fires every 5 seconds when panel is open and daemon is not running. Re-runs socket discovery.

On socket `error` (stale socket file, dead daemon), the panel marks the session dead (`daemonSocket.connected = false`, `daemonStatus = "not running"`) and removes the failed path from `sessionList`, so the timer re-discovers instead of spinning against a stale socket. The bar widget does the same on its status socket (clears `barSocketPath`; the next poll re-discovers).

On panel close, socket is explicitly disconnected.

### Protocol

Outbound messages built via Model.js helpers. Inbound: `Model.parseDaemonResponse(json)` → parsed JSON object.

Commands and message types used by the panel:

| Message | When |
|---------|------|
| `hello` | On socket connect (handshake, version `"0.3"`) |
| `config_get` | Panel open, reload |
| `config_set` | Debounced config save |
| `status` | After hello handshake |
| `preview` | On connect, after save, preview toggle change |
| `palette` | On connect, theme source change |
| `reload_config` | Manual reload, after reset (fallback) |

Commands available in daemon but not used by panel: `reload_theme`, `invalidate_git`, `shutdown`.

### Protocol Version Gating

`Model.protocolAtLeast(current, min)` compares dotted version strings. The System bucket daemon info card shows:

- `"full (v0.3+)"` when connected daemon protocol ≥ 0.3
- `"degraded (upgrade daemon)"` when protocol < 0.3

Preview and palette features require a v0.3+ daemon.

## Config Management

### Read Flow

Primary path uses the daemon config API:

```
open() → send config_get message
  → daemon returns full config as JSON
  → Model.flattenConfig(nested) → flat {key: value} object
  → _applyParsedConfig(flat) → set QML properties
  → _configFlat = flat (master copy)
```

**Fallback:** If the daemon does not support the config API (older version), falls back to direct TOML file I/O:

```
configReader.exec(["cat", configPath])
  → StdioCollector captures stdout
  → Model.parseTOML(text) → flat object
  → _applyParsedConfig(flat)
```

### Write Flow

Primary path uses `config_set` with a JSON patch:

```
setConfigValue(key, value)
  → push undo snapshot (if value changed)
  → _configFlat[key] = value
  → QML property updated
  → show config diff toast
  → saveTimer.restart() (300ms debounce)
    → _flushSave()
      → patch = Model.unflattenPatch(Model.collectConfig(root))
      → send {type:"config", command:"set", config: patch}
      → daemon recursively merges patch into config.toml (preserving unmentioned keys)
      → daemon reloads config in-memory
      → requestPreview()
```

On daemon error, `lastError` is set and the red error toast appears for 5 seconds.

**No offline writes:** If `daemonSocket` is not connected, the debounced save is
refused — the panel never rebuilds or overwrites `config.toml` itself. Instead
`lastError` is set, the red error toast tells the user that saving settings
requires a running omarchy10k daemon, and the change stays in the panel
properties for when a daemon reconnects. All config writes go through the
daemon's `config_set` (see [architecture.md](architecture.md), "Data Flow:
Config Change via Quattro").

### CONFIG_MAP

Maps TOML keys to QML property names (32 keys):

| TOML Key | QML Property |
|----------|-------------|
| `prompt.layout` | `cfgLayout` |
| `prompt.transient` | `cfgTransient` |
| `prompt.newline` | `cfgNewline` |
| `prompt.blank_line` | `cfgBlankLine` |
| `prompt.right_prompt` | `cfgRightPrompt` |
| `style.preset` | `cfgStylePreset` |
| `style.separators.left` | `cfgSepLeft` |
| `style.separators.right` | `cfgSepRight` |
| `style.frame.enabled` | `cfgFrameEnabled` |
| `style.frame.gap_char` | `cfgFrameGapChar` |
| `theme.source` | `cfgThemeSource` |
| `git.mode` | `cfgGitMode` |
| `git.enabled` | `cfgGitEnabled` |
| `git.branch_icon` | `cfgGitBranchIcon` |
| `segments.os.icon` | `cfgOsIcon` |
| `segments.character.success` | `cfgCharSuccess` |
| `segments.character.error` | `cfgCharError` |
| `segments.character.transient` | `cfgCharTransient` |
| `segments.exit_status.show_signal_name` | `cfgExitSignalNames` |
| `segments.command_duration.show_above_ms` | `cfgCmdDurationMs` |
| `segments.ssh.show` | `cfgSshShow` |
| `segments.container.enabled` | `cfgContainerEnabled` |
| `segments.python.enabled` | `cfgPythonEnabled` |
| `segments.toolchain.enabled` | `cfgToolchainEnabled` |
| `segments.nix.enabled` | `cfgNixEnabled` |
| `segments.k8s.enabled` | `cfgK8sEnabled` |
| `segments.time.enabled` | `cfgTimeEnabled` |
| `segments.load.enabled` | `cfgLoadEnabled` |
| `segments.time.format` | `cfgTimeFormat` |
| `segments.battery.enabled` | `cfgBatteryEnabled` |
| `segments.notification.threshold_ms` | `cfgNotifyThresholdMs` |
| `terminal.title.enabled` | `cfgTitleEnabled` |

### Import / Export

**Copy Config** serializes current QML properties via `Model.collectConfig()` → `Model.buildTOML()` and pipes to clipboard (`xclip` or `wl-copy`).

**Paste Config** reads clipboard (`xclip -o` or `wl-paste`), parses TOML, applies properties, and triggers a debounced save.

## Panel Rail: Looks · Style · Behavior · System

The old five-tab bar (`Appearance / Context / Segments / Shell / Advanced`) is now a **4-bucket rail**: `Repeater model: ["Looks", "Style", "Behavior", "System"]` in `Panel.qml`, with the active bucket's `Component` Loader-switched (`looksTab` / `appearanceTab` / `behaviorTab` / `systemTab`). The previous tab content lives on under the new buckets:

| Old tab | New home |
|---------|----------|
| Appearance (style gallery, glyphs, frame, theme) | **Style** |
| Context (git, duration, ssh, exit status) | **Behavior → Context** |
| Segments | **Behavior → Segments** |
| Shell (tool detection) | **System** |
| Advanced (config actions, daemon info) | **System** |

### Looks Bucket

- **Curated Look cards** — 2-column grid of the 8 compiled-in Looks (omnarchy, tokyo-rainbow, framed-gradient, lean-pure, slanted-owl, gruvbox-drift, rose-classic, polar-lean). Clicking a card calls `applyLook(name)` → `looks_apply` control command (persistent apply).
- **Save current as Look** — a `TextField` for the name plus a "Save current as Look" action button → `saveLook(name)` → `looks_save {name, label}` (snapshots the currently mapped CONFIG_MAP keys).
- **Expand gallery** — calls `omarchyService.openGallery()` (host summon with payload `{"page":"gallery"}`); falls back to emitting the panel's `galleryRequested()` signal when the hub is absent.
- An "Identity" note points palette/theme fine-tuning to the Style bucket.

### Style Bucket

#### Style Gallery

A 4-column grid of 8 preset cards. Each card's preview line renders the **live daemon preview** for that preset (see [Preset Gallery Live Previews](#preset-gallery-live-previews)); the static glyphs below are the offline fallback.

| Card | Preview | Description |
|------|---------|-------------|
| omarchy | `~ ❯` | Clean |
| powerline | `~ ▶ git` | Classic |
| rainbow | `~ ▶▶▶` | Vibrant |
| framed | `╭─ ~ ─╮` | Framed |
| classic | `~ │ git` | Divided |
| lean | `~/src` | Minimal |
| dense | `~ git ❯` | Compact |
| slanted | `~ ╲ git` | Modern |

Clicking a card sets `style.preset` **and re-stamps the preset-controlled granular keys** (`style.frame.enabled`, `style.frame.gap_char`, empty `style.separators.left/right`) — `_flushSave` writes every CONFIG_MAP key, so without this a stale frame/separator toggle from an earlier preset would silently override the new one.

#### Glyph Pickers

| Picker | TOML Key(s) | Options |
|--------|------------|---------|
| OS Icon | `segments.os.icon` | 13 distro icons (Arch, Ubuntu, Debian, Fedora, NixOS, macOS, Win, Linux, Omarchy, Alpine, Void, Gentoo) + None |
| Git Icon | `git.branch_icon` | Powerline, Octicon, Nerd, git:, None |
| Separator | `style.separators.left` + `.right` | Default, Arrow, Thin, Slant, Round, Bar, Dot, Diamond |

(Prompt Char pickers moved to the Behavior bucket's Glyphs section.) The Separator picker uses a custom handler that sets left and right separators at once.

#### Frame Controls

| Control | TOML Key | Options |
|---------|----------|---------|
| Frame Lines | `style.frame.enabled` | On / Off |
| Gap Fill (visible when Frame is On) | `style.frame.gap_char` | Line ─, Dots ·, Ellipsis ⋯, None |

#### Curated Palette Cards

A 4-column grid of the 8 curated prompt palettes (Tokyo Night, Catppuccin, Gruvbox, Nord, Dracula, Rosé Pine, Everforest, Kanagawa), each rendered as a mini swatch strip. These come from `Model.CURATED_PALETTES` (client-side); the daemon-side equivalent for Looks lives in `looks.rs::curated_palette` and backs the `palettes` verb (see [Looks Gallery](#looks-gallery-overlay-galleryqml)). Applying a palette writes `[theme.custom]` with `source = "hybrid"` so it layers over the Omarchy theme.

#### Theme Section

| Control | TOML Key | Options |
|---------|----------|---------|
| Source | `theme.source` | omarchy, custom, hybrid, terminal |
| Theme swatches | (from `palette` IPC) | accent, foreground, muted, background, red, green, yellow, blue |

Changing theme source triggers `requestPalette()` to refresh the color swatch row. Swatches appear when `paletteColors` is populated from the daemon response.

### Behavior Bucket

#### Prompt

| Control | TOML Key | Options |
|---------|----------|---------|
| Lines | `prompt.newline` | Two-line / One-line |
| Spacer | `prompt.blank_line` | On / Off |
| Transient | `prompt.transient` | On / Off |

#### Glyphs

| Picker | TOML Key(s) | Options |
|--------|------------|---------|
| Prompt Char | `segments.character.success` + `.error` + `.transient` | Chevron ❯, Arrow ➜, Lambda λ, $, >, %, ▶, # |
| Animals | `segments.character.success` (+ error/transient) | Nerd-font animal glyphs (cat, penguin, fox, owl, duck, butterfly, ladybug, bee, dog, rabbit, …) |

#### Context

| Control | TOML Key | Options |
|---------|----------|---------|
| Git | `git.mode` | adaptive, compact, expanded, hidden |
| Duration | `segments.command_duration.show_above_ms` | 500, 1000, 1500, 3000, 5000 ms |
| SSH | `segments.ssh.show` | auto, always, never |
| Exit Status | `segments.exit_status.show_signal_name` | Signal names / Codes only |

#### Segments

Two-column toggle grid. Clicking a pill toggles the boolean config value via `setConfigValue()`:

| Label | TOML Key | QML Property |
|-------|----------|-------------|
| Container | `segments.container.enabled` | `cfgContainerEnabled` |
| Python | `segments.python.enabled` | `cfgPythonEnabled` |
| Toolchain | `segments.toolchain.enabled` | `cfgToolchainEnabled` |
| Nix | `segments.nix.enabled` | `cfgNixEnabled` |
| Kubernetes | `segments.k8s.enabled` | `cfgK8sEnabled` |
| Time | `segments.time.enabled` | `cfgTimeEnabled` |
| Load | `segments.load.enabled` | `cfgLoadEnabled` |
| Battery | `segments.battery.enabled` | `cfgBatteryEnabled` |
| Terminal Title | `terminal.title.enabled` | `cfgTitleEnabled` |

Enabled segments render with accent background; disabled segments use muted styling. Pills are filtered by the panel's `searchQuery` property (case-insensitive label match) — the settings-search hook; no visible search input is currently rendered in the panel (inferred).

**Time Format selector** — visible when Time is enabled. Three options: `HH:MM` (`%H:%M`), `HH:MM:SS` (`%H:%M:%S`), `hh:mm AM/PM` (`%I:%M %p`).

#### Notifications

**Notify After** — `ControlRow` selector with `5s`, `10s`, `30s` options, mapped to `segments.notification.threshold_ms` (5000/10000/30000). The daemon includes `notify_threshold_ms` in prompt responses; the bash adapter updates its threshold from this field.

### System Bucket

Opens with a note that shell integrations are configured through their own tools (Omarchy10k coordinates their lifecycle via the hook broker).

**Tool detection** — `StatusRow`s for five tools with conditional install actions:

| Tool | Detection | Install Action |
|------|-----------|----------------|
| ble.sh | `command -v blesh` | — |
| Atuin | `command -v atuin` | "Install Atuin" → `curl setup.atuin.sh` |
| Mise | `command -v mise` | "Install Mise" → `curl mise.run` |
| Zoxide | `command -v zoxide` | — |
| fzf | `command -v fzf` | — |

Install buttons appear only when the tool status contains `✗ not found`. After any install runner completes, `detectTools()` is called automatically and a success toast is shown.

**Daemon info card** — status, PID, version + protocol version (with the full/degraded protocol label), session count, and the session list with per-row floating-terminal buttons.

**Actions:**

| Action | Behavior |
|--------|----------|
| Open Config File | Opens `$TERMINAL` (default: `foot`) running `$EDITOR` (default: `nano`) via `Process.startDetached()` |
| Run Doctor | `omarchy10k doctor`; output shown in scrollable monospace area below |
| Copy Config | Serialize config to clipboard |
| Paste Config | Parse clipboard TOML, apply, save |
| Reload Config | Re-fetch config via `config_get` + `reload_config` |
| Run Benchmark | `omarchy10k benchmark --iterations 50`; results in scrollable area |
| Reset to Defaults | Backup to `.bak`, delete config, reload |

### Multi-Session

When multiple shells are running, each has its own daemon socket. The System bucket provides a session selector:

When the v0.4 service hub is loaded, this list mirrors `Service.sessions` (live branch/dirty/duration data per row) and the panel's own `socketFinder` only maintains the working connection. Without the hub the behavior below applies unchanged.

- Lists all discovered `omarchy10k-*.sock` files
- Each entry shows shell PID, working directory, and a floating-terminal icon
- Clicking a row switches the active session (disconnect + reconnect)
- Clicking the terminal icon opens a new shell in that session's CWD (single quotes in CWD are escaped as `'\''` for safe interpolation)
- Config changes apply to the selected session's daemon

## Process Components

| ID | Command | Trigger |
|----|---------|---------|
| `serviceSocketFinder` (in `Service.qml`) | `ls $XDG_RUNTIME_DIR/omarchy10k-*.sock` | Service startup + 10s timer |
| `hyprctlClients` (in `SessionPicker.qml`) | `hyprctl -j clients` | Session-picker refresh (per refresh cycle, no timer) |
| `hyprctlFocus` (in `SessionPicker.qml`) | `hyprctl dispatch focuswindow pid:<shellPid>` | Session picker activation |
| `focusLauncher` (in `SessionPicker.qml`) | `cd '<cwd>' && omarchy-launch-floating-terminal` | Picker fallback focus |
| `configReader` | `cat config.toml` | Panel open, reload, reset |
| `socketFinder` | `ls $XDG_RUNTIME_DIR/omarchy10k-*.sock` | Panel open, reconnect |
| `barSocketFinder` | `ls … \| head -1` | BarWidget init, bar poll |
| `toolDetector` | 5× `command -v` | Panel open |
| `editorLauncher` | `$EDITOR config.toml` | System bucket button |
| `doctorRunner` | `omarchy10k doctor` | System bucket button |
| `benchRunner` | `omarchy10k benchmark --iterations 50` | System bucket button |
| `installRunner` | curl install scripts | System bucket install buttons |
| `floatingTermLauncher` | `cd '$cwd' && exec $SHELL` | Session row terminal icon |
| `clipboardCopy` | xclip / wl-copy | Copy Config |
| `clipboardPaste` | xclip -o / wl-paste | Paste Config |
| `resetProc` | Backup + rm config.toml | System bucket button |

## Model.js

Stateless helper library (`.pragma library`):

| Function | Purpose |
|----------|---------|
| `configDir(xdgConfigHome, home)` | `$XDG_CONFIG_HOME/omarchy10k` (falls back to `$HOME/.config/omarchy10k`); env values passed in from QML via `Quickshell.env()` |
| `configPath(xdgConfigHome, home)` | `configDir()/config.toml` |
| `runtimeDir(xdgRuntimeDir)` | `$XDG_RUNTIME_DIR` or `/tmp`; env value passed in from QML |
| `stripComment(line)` | Removes an unquoted `#` comment, preserving `#` inside quoted values |
| `buildCommand(name, id)` | JSON control command string with newline |
| `buildHello(id)` | Hello handshake message (version `"0.3"`) |
| `buildConfigGet(id)` | Builds config_get request |
| `buildConfigSet(patch, id)` | Builds config_set request with JSON patch |
| `buildPreview(context, id)` | Builds preview request with simulated context |
| `stripAnsi(str)` | Removes ANSI/OSC escape sequences (doctor/benchmark output) |
| `ansiToRich(text)` | Converts SGR ANSI into `Text.StyledText` span markup (live preview, preset cards) |
| `escapeHtml(str)` | Escapes `&`, `<`, `>` (used by `ansiToRich`) |
| `protocolAtLeast(current, min)` | Dotted version comparison (e.g. `"0.3" >= "0.2"`) |
| `flattenConfig(nested)` | Flattens nested config object to dotted keys; skips null leaves |
| `unflattenPatch(flat)` | Unflattens dotted keys to nested object |
| `parseDaemonResponse(json)` | Safe JSON parse with error wrapping |
| `parseTOML(text)` | Subset TOML parser → flat key-value object |
| `buildTOML(flat)` | Flat object → sectioned TOML string |
| `CONFIG_MAP` | TOML key ↔ QML property mapping (32 keys) |
| `CURATED_PALETTES` | Client-side curated prompt-palette table (8 palettes) used by the Style bucket's palette cards; the Looks system's curated palettes moved daemon-side (`looks.rs::curated_palette`) |
| `applyConfig(flat, target)` | Load parsed config into QML properties; skips undefined/null values |
| `collectConfig(source)` | Export QML properties to flat object |
| `parseToolOutput(text)` | Parse `name=path\|missing` format |

### Parity Gate: `tests/model_parity_test.js`

A Node harness (no QML runtime needed) that strips the `.pragma library` directive, evaluates `Model.js` with `new Function`, and proves every `CONFIG_MAP` key survives `unflattenPatch` → `flattenConfig` round-trips without rename, drop, value mutation, or sibling clobbering. Run directly with `node tests/model_parity_test.js`; `tests/integration_test.sh` runs it when `node` is available (skip when not) and fails the suite on nonzero exit.

### TOML Parser Limitations

The `parseTOML()` implementation supports:
- `[section]` headers
- `key = "string"`, `key = true/false`, `key = integer`
- `#` comments (quote-aware — `#` inside quoted values is preserved)

It does not support: nested tables, arrays, inline tables, multi-line strings, dotted keys.

## Reactive State Properties

Key QML properties on `Panel.qml` beyond config fields:

| Property | Purpose |
|----------|---------|
| `lastError` / `_showError` | Daemon error message + red toast visibility |
| `toastMessage` / `_showToast` | Config diff / undo / paste toast |
| `doctorOutput` | Scrollable doctor command output |
| `benchmarkOutput` | Scrollable benchmark results |
| `previewText` | ANSI→StyledText left prompt for the color preview box |
| `previewError` / `previewSsh` / `previewLongCmd` | Preview context toggles |
| `paletteColors` | Theme color map from `palette` IPC response |
| `_undoStack` / `_undoMaxSize` | Config undo circular buffer (max 10) |
| `sessionList` / `activeSessionIndex` | Multi-session socket list |
| `presetPreviews` / `presetCards` | Style-gallery live renders (id-routed `preview` replies) / card fallback strings |
| `omarchyService` | The plugin's `Service` hub instance when the host loaded it (null otherwise) |

## Styling

Color fallbacks when `qs.Commons.Color` is unavailable:

| Role | Hex |
|------|-----|
| accent | `#7aa2f7` |
| background | `#1a1b26` |
| foreground | `#a9b1d6` |
| green | `#9ece6a` |
| red | `#f7768e` |
| muted | `#414868` |

## Installation

The plugin is a git repo with `manifest.json` at its root, so the standard flow works:

```bash
omarchy plugin add <repo-url>          # clones into ~/.config/omarchy/plugins/community.omarchy10k/
omarchy plugin enable community.omarchy10k
```

Enabling records the id in `shell.json` `plugins[]`, which is what makes the host mount `Service.qml` at startup and allows the overlay to be summoned. The bar widget appears in the right section (`barWidget.defaultSection`).

Manual copy also works:

```bash
cp -r quattro/ ~/.config/omarchy/plugins/community.omarchy10k/
omarchy-shell shell rescanPlugins
omarchy plugin enable community.omarchy10k
```

Requires a running Omarchy Quattro desktop with Quickshell-based bar system.

## Known Issues

Recorded by the [Bug Audit](bug-audit.md).

### The fallback config writer destroys `config.toml` (fixed: offline writer removed)

`Panel.qml`'s `_flushSave` used to rebuild the whole file from `Model.parseTOML`'s
flat output whenever `daemonSocket` was disconnected. `parseTOML` keeps only
`section.key = scalar` pairs, so round-tripping stripped every comment and dropped
the nested `[theme.custom]` table entirely.

The offline writer has been removed: config saves now require a connected daemon
and always go through `config_set` (the daemon deep-merges the patch and refuses
to overwrite a file it cannot parse). When no daemon is connected the panel shows
an error toast instead of writing the file. See
[Bug Audit #19](bug-audit.md#19-quattros-fallback-config-writer-destroys-the-config-file).

### Every save writes every mapped key

`Model.collectConfig` returns all of `CONFIG_MAP`, not just the changed keys, so
`_flushSave` sends the panel's full property set on each save. If a save fires
before `config_get` has returned, the panel's QML defaults are written over the
user's real settings.

### Preview escape handling (v0.3.0: fixed)

Preview responses now have `\x01`/`\x02` readline delimiters stripped server-side,
so the panel's `Model.stripAnsi` works correctly on clean ANSI output. Live prompt
strings are now fully readline-safe with all escapes wrapped.

### Segment toggles with limited effect

The Behavior bucket Segments grid toggles `segments.python.enabled`, `segments.toolchain.enabled`
and `segments.nix.enabled`. Those three segments read the *daemon's* environment,
which is frozen at shell startup, so enabling them has no visible effect for a
user who activates a venv, switches mise versions, or enters a nix shell after the
shell started. This is tracked as a [v0.4 design item](bug-audit.md#5-every-environment-derived-segment-is-frozen-at-daemon-start).

The Time segment ABI bug ([#2](bug-audit.md#2-struct-tm-abi-mismatch-corrupts-the-stack-when-the-time-segment-is-enabled))
has been fixed — `struct Tm` now includes all required fields.

## Headless Daemon (Settings With No Terminal Open)

The daemon is per-shell, so with every terminal closed there is nothing for the Control Center to talk to. The panel covers this by spawning a **headless daemon** when its discovery sweep finds no live sessions:

```sh
O10K_SOCK_NAME=headless O10K_PARENT_PID=$(pgrep -x quickshell | head -1) exec omarchy10kd
```

- `O10K_SOCK_NAME` (daemon): binds a fixed socket `omarchy10k-<name>.sock` instead of the parent-pid name. Idempotent — a second spawn refuses to hijack a live daemon (exits) and stale sockets are cleared on bind.
- `O10K_PARENT_PID` = quickshell's pid: the daemon exits cleanly when the desktop shell exits.
- Config writes through the headless daemon land in the same `config.toml`; running shell daemons hot-reload it via their filesystem watcher.
- Discovery (Panel `discoverAllSockets` and `Service.qml` sweep) treats a non-numeric socket pid (e.g. `headless`) as always alive; numeric pids still get the `kill -0` liveness check.

## Panel Self-Recovery

The panel's 5s `reconnectTimer` runs whenever the panel is open and no daemon is connected — including when the service hub is active. The hub sweep can lag a daemon that appeared after the panel opened; the panel's own finder then reconnects without clobbering hub-owned session state. `Panel.qml`'s finder also calls `ensureHeadlessDaemon()` when zero sessions are found.

## Looks + Rail Navigation (v0.5)

The Control Center panel uses a 4-bucket rail (LOOKS · STYLE · BEHAVIOR · SYSTEM) instead of tabs. The daemon's `looks` registry provides named, atomic appearance bundles:

- `[looks.<name>]` tables in `config.toml`: `label`, `palette` ("theme" | "keep" | curated key), `patch` (nested style/glyphs/frame/prompt tables; `glyphs` shortcuts expand to segments keys at apply).
- Control verbs (protocol 0.5): `looks` (list), `looks_apply {name, transient?}` (atomic config merge; `transient` = in-memory only, revert via `reload_config`), `looks_save {name, label}` (snapshot current mapped keys), `palettes` (curated palette table, moved daemon-side from Model.js), `defaults` (factory defaults snapshot powering the panel's modified-ink/reset chips).
- `preview` requests accept `look: "<name>"` for dry-run renders (gallery cards).
- CLI: `omarchy10k look list|apply <name> [--transient]|save <name>`.
- 8 curated Looks ship compiled-in; user entries shadow curated names.
