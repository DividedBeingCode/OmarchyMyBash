# Quattro Plugin Reference

[← Index](INDEX.md) | [Protocol](protocol.md) | [Configuration](config.md)

The Quattro plugin provides a desktop Control Center for Omarchy10k, surfaced as a bar widget in the Omarchy Quattro panel. It reads and writes config files, communicates with running daemon instances over Unix sockets, detects installed shell tools, and previews prompt output in real time.

**Protocol version:** 0.3 hello handshake (feature gating); v0.4 daemon adds `style_preset` on `preview` requests.

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
├── serviceSocketFinder Process → discovers every omarchy10k-*.sock (10s timer)
├── Instantiator → one persistent Socket per session (hello → status)
├── controlSocket Socket (config_get/config_set/invalidate_git for IPC)
└── IpcHandler target "community.omarchy10k"

BarWidget (qs.Ui.BarWidget)
├── omarchyService (shell.serviceFor("community.omarchy10k"), feature-detected)
│   └── present → mirrors daemonStatus, poll timer off
│   └── absent  → barSocketFinder + barStatusSocket + barPollTimer (5s), unchanged
├── Loader → Panel.qml
└── WidgetButton (❯ glyph, ✓/✗ tooltip)

Panel (qs.Ui.Panel, manageIpc: false)
├── Header: connection dot + title + ↩ Undo button
├── Live prompt preview (ANSI → StyledText colors) + Error/SSH/Long cmd toggles
├── Tab bar: Appearance, Context, Segments, Shell, Advanced
├── omarchyService mirror (daemonStatus + sessionList, feature-detected)
├── 5× Component tabs (Loader-switched)
└── Processes/Sockets/Timers as in v0.3 (own daemonSocket still drives preview/config)

SessionPicker (Item, overlay kind — summoned on demand)
├── open(payloadJson) / close() entry-point contract
├── PanelWindow (WlrLayer.Overlay, exclusive keyboard) + scrim + Escape/dismiss
├── ListView of live sessions (CWD, branch, dirty, last duration, age)
├── focuswindow pid:<shellPid> via hyprctl → fallback omarchy-launch-floating-terminal
└── graceful empty state (no sessions / service not loaded / non-Hyprland)
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

## UX Features (v0.3)

### P0 — Core QoL

| Feature | Implementation |
|---------|----------------|
| Connection status indicator | Green/yellow/red dot in panel header; green = running, yellow = reconnecting, red = not running |
| Error feedback on save | Red toast with daemon error message; auto-dismiss after 5s (`errorTimer`) |
| Doctor output in panel | Scrollable monospace `TextEdit` in Advanced tab after "Run Doctor" |
| Live prompt preview | Preview box above tabs; `preview` IPC with simulated context; auto-updates on config save |
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
| Segment toggle grid | New Segments tab with 2-column grid for 8 segment flags |
| One-click tool setup | "Install Atuin" / "Install Mise" buttons in Shell tab when tools missing |
| Config undo | Circular buffer of last 10 config states; "↩ Undo" button in header |
| Config import/export | "Copy Config" / "Paste Config" in Advanced tab via xclip/wl-copy |
| Degradation labels | Per-feature: preview shows "Live preview requires daemon v0.3+" and palette shows "Palette preview requires daemon v0.3+" when protocol < 0.3; Advanced tab shows "full (v0.3+)" / "degraded (upgrade daemon)" |
| Benchmark display | "Run Benchmark" button with scrollable results (`omarchy10k benchmark --iterations 50`) |

## Live Prompt Preview (ANSI-colored, v0.4)

Above the tab bar, a preview box shows the rendered left prompt with simulated context. On socket connect and after each debounced save, the panel sends a `preview` message:

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

The Appearance tab's 8 style cards render **live daemon previews** instead of hardcoded strings: `requestPresetPreviews()` sends one `preview` request per preset with the id `preset-<name>` and the v0.4 `style_preset` override field (daemon renders one-shot with that preset; no config mutation — see [protocol.md](protocol.md)). Responses route by id into `presetPreviews[name]`; the card shows the rich preview (`textFormat: Text.StyledText`), falling back to the static glyph string when the daemon is unreachable or older than the override. Requires daemon ≥ 0.3 for previews, ≥ 0.4 for per-card distinct presets.

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
```

| Method | Returns | Behavior |
|--------|---------|----------|
| `status()` | JSON object | `ok`, `daemon`, `sessions`, plus the primary session's enriched `status` fields (pid, version, protocol_version, cwd, git, last_cmd_duration_ms, last_exit_code, session_age_secs, battery). `ok:false` when no daemon is running. |
| `sessions()` | JSON array | One object per live socket: `shell_pid`, `pid`, `cwd`, `branch`, `dirty`, `last_cmd_duration_ms`, `session_age_secs`. |
| `setLayout(preset)` | JSON object | Queues `config_set {"style":{"preset":…}}` on the hub's control socket. |
| `toggleTransient()` | JSON object | Reads cached `prompt.transient` (from `config_get` at connect), queues the flipped value via `config_set`. |
| `picker()` | `"ok"` or JSON error | `shell.summon("community.omarchy10k", "{}")` — opens the overlay. |
| `invalidateGit()` | JSON object | Queues the `invalidate_git` control command on the primary socket. |

Notes:

- Method names are the QML function names (camelCase, per upstream's `IpcHandler` dispatch). The intel doc's kebab names (`set-layout`, `toggle-transient`, `invalidate-git`) map onto `setLayout` / `toggleTransient` / `invalidateGit` — QML function names cannot contain dashes.
- **Config methods never throw.** No-daemon / write-failure returns `{"ok":false,"error":"…"}`; a successfully queued write returns `{"ok":true,"queued":true}` (the daemon's ok/error reply is consumed asynchronously; persistent failures surface on the next status poll and via the `eventReceived` signal bus).
- `config_set` writes go through `Model.buildConfigSet` over the hub's own persistent control socket — the panel does not need to be open. (Deviation from the original task wording, which assumed the *panel's* `daemonSocket`: the panel disconnects on close, so an always-loaded hub socket is the only path that keeps the target answering when no panel is open.)
- install.sh keybind hints are W2's scope, not documented here.

## Service Plugin: `Service.qml` (2.2)

One persistent connection hub replacing the three duplicated poll/reconnect lifecycles:

- **Discovery:** `ls $XDG_RUNTIME_DIR/omarchy10k-*.sock` on startup + every 10s; the session socket set is only rebuilt when the discovered path list actually changes (no churn).
- **Per-session sockets:** a `Quickshell` `Instantiator` holds one persistent `Socket` per path; each handshakes (`hello`) and issues `status`. Responses update `sessions[i]` (`pid`, `cwd`, `branch`, `dirty`, `lastCmdMs`, `ageSecs`) and, for the primary session, `lastStatus` + `daemonStatus = "running"`.
- **Control socket:** a dedicated connection to the first socket for `config_get` (seeds the transient cache) / `config_set` / control commands used by the IPC target.
- **State surface:** `daemonStatus` (`"running"`/`"not running"`), `sessions`, `lastStatus`, and the `eventReceived(var)` signal bus (reserved for daemon push events: `long_command`, `git_stale`, `battery_low`).
- **Consumers:** `BarWidget` mirrors `daemonStatus` (poll timer off); `Panel` mirrors `daemonStatus` + `sessionList` on the hub's change signals; `SessionPicker` reads `sessions` via host injection.
- **Graceful fallback:** every consumer feature-detects the hub (`typeof shell.serviceFor === "function"` + null check). Without it, BarWidget polls as in v0.3, the panel re-discovers on its 5s `reconnectTimer`, and the overlay shows its "service not loaded" empty state. On old hosts that ignore the `service` kind, `Service.qml` is simply never instantiated.

## Session Picker Overlay: `SessionPicker.qml` (2.3)

Fullscreen overlay (summoned by the `picker` IPC method, a Hyprland keybind, or `omarchy-shell shell summon community.omarchy10k '{}'`):

- Lists every live session with **CWD, git branch (+ dirty dot), last command duration, session age**, and pid — data read from the plugin's own `Service` (`property var service`, host-injected).
- **Enter / click** activates: under Hyprland (`HYPRLAND_INSTANCE_SIGNATURE` set) it runs `hyprctl dispatch focuswindow pid:<shellPid>`; on nonzero exit (or non-Hyprland) it falls back to `omarchy-launch-floating-terminal` from the session's CWD, then closes.
  - *Assumption:* the socket-name PID is the shell session PID; `focuswindow pid:` matches client windows exactly, so the pid→window hit is best-effort with the terminal fallback as guarantee.
- **Escape / scrim click** closes (`shell.hide(<id>)` when the host is available, plain hide otherwise).
- **Empty state** when no sessions are live or the service isn't loaded (with a hint); non-Hyprland adjusts the help line to the terminal-fallback behavior.
- Rendering follows the first-party overlay pattern: `PanelWindow` anchored to all edges, transparent, `WlrLayer.Overlay`, exclusive keyboard focus, scrim + centered card, with `Color.menu.*` theme tokens falling back to `Color.*` then hard-coded values.

## Config Undo

`setConfigValue()` pushes a JSON snapshot of `_configFlat` onto `_undoStack` before each change. The stack holds at most 10 entries (FIFO eviction). The "↩ Undo" button in the header is visible when the stack is non-empty; clicking pops the last snapshot, re-applies properties, and triggers a debounced save.

## Socket Discovery and Daemon IPC

### Discovery

```javascript
socketFinder.exec(["sh", "-c",
    "ls '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock 2>/dev/null"])
```

Enumerates **all** `omarchy10k-*.sock` files in `$XDG_RUNTIME_DIR` (or `/tmp`). Each discovered socket is parsed to extract shell PID and added to the `sessionList` model. The user can select between sessions in the Advanced tab.

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

`Model.protocolAtLeast(current, min)` compares dotted version strings. The Advanced tab daemon info block shows:

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

Maps TOML keys to QML property names (31 keys):

| TOML Key | QML Property |
|----------|-------------|
| `prompt.layout` | `cfgLayout` |
| `prompt.transient` | `cfgTransient` |
| `prompt.newline` | `cfgNewline` |
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
| `segments.time.format` | `cfgTimeFormat` |
| `segments.battery.enabled` | `cfgBatteryEnabled` |
| `segments.notification.threshold_ms` | `cfgNotifyThresholdMs` |
| `terminal.title.enabled` | `cfgTitleEnabled` |

### Import / Export

**Copy Config** serializes current QML properties via `Model.collectConfig()` → `Model.buildTOML()` and pipes to clipboard (`xclip` or `wl-copy`).

**Paste Config** reads clipboard (`xclip -o` or `wl-paste`), parses TOML, applies properties, and triggers a debounced save.

## UI Tabs

Five tabs: `["Appearance", "Context", "Segments", "Shell", "Advanced"]`

### Appearance Tab

Redesigned in v0.3 with a visual style gallery, glyph pickers, and frame controls.

#### Style Gallery

An 8-card grid replaces the old Preset dropdown. Each card shows a visual preview of the style, its name, and a short description. Clicking a card sets `style.preset`:

Each card's preview line renders the **live daemon preview** for that preset (see [Preset Gallery Live Previews](#preset-gallery-live-previews)); the static glyphs below are the offline fallback. Clicking a card still sets `style.preset`.

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

#### Glyph Pickers

Four scrollable glyph rows below the gallery, each showing clickable icon buttons:

| Picker | TOML Key(s) | Options |
|--------|------------|---------|
| OS Icon | `segments.os.icon` | 13 distro icons (Arch, Ubuntu, Debian, Fedora, NixOS, macOS, Win, Linux, Omarchy, Alpine, Void, Gentoo) + None |
| Prompt Char | `segments.character.success` + `.error` + `.transient` | ❯, ➜, λ, $, >, %, ▶, # |
| Git Icon | `git.branch_icon` | Powerline, Octicon, Nerd, git:, None |
| Separator | `style.separators.left` + `.right` | Default, Arrow, Thin, Slant, Round, Bar, Dot, Diamond |

The Prompt Char and Separator pickers use custom handlers to set multiple config keys at once (e.g. prompt char sets success, error, and transient simultaneously).

#### Frame Controls

| Control | TOML Key | Options |
|---------|----------|---------|
| Frame Lines | `style.frame.enabled` | On / Off |
| Gap Fill (visible when Frame is On) | `style.frame.gap_char` | Line ─, Dots ·, Ellipsis ⋯, None |

#### Layout Controls

| Control | TOML Key | Options |
|---------|----------|---------|
| Lines | `prompt.newline` | Two-line / One-line |
| Transient | `prompt.transient` | On / Off |

#### Theme Section

| Control | TOML Key | Options |
|---------|----------|---------|
| Source | `theme.source` | omarchy, custom, hybrid, terminal |
| Theme swatches | (from `palette` IPC) | accent, foreground, muted, background, red, green, yellow, blue |

Changing theme source triggers `requestPalette()` to refresh the color swatch row. Swatches appear when `paletteColors` is populated from the daemon response.

### Context Tab

| Control | TOML Key | Options |
|---------|----------|---------|
| Git | `git.mode` | adaptive, compact, expanded, hidden |

| Duration | `segments.command_duration.show_above_ms` | 500, 1000, 1500, 3000, 5000 ms |
| SSH | `segments.ssh.show` | auto, always, never |
| Exit Status | `segments.exit_status.show_signal_name` | Signal names / Codes only |

### Segments Tab

Two-column toggle grid for eight segment/feature flags. Clicking a pill toggles the boolean config value via `setConfigValue()`:

| Label | TOML Key | QML Property |
|-------|----------|-------------|
| Container | `segments.container.enabled` | `cfgContainerEnabled` |
| Python | `segments.python.enabled` | `cfgPythonEnabled` |
| Toolchain | `segments.toolchain.enabled` | `cfgToolchainEnabled` |
| Nix | `segments.nix.enabled` | `cfgNixEnabled` |
| Kubernetes | `segments.k8s.enabled` | `cfgK8sEnabled` |
| Time | `segments.time.enabled` | `cfgTimeEnabled` |
| Battery | `segments.battery.enabled` | `cfgBatteryEnabled` |
| Terminal Title | `terminal.title.enabled` | `cfgTitleEnabled` |

Enabled segments render with accent background; disabled segments use muted styling.

**Time Format selector** — visible when Time is enabled. Three options: `HH:MM` (`%H:%M`), `HH:MM:SS` (`%H:%M:%S`), `hh:mm AM/PM` (`%I:%M %p`).

### Shell Tab

Displays detection results for five tools with conditional install actions:

| Tool | Detection | Install Action |
|------|-----------|----------------|
| ble.sh | `command -v blesh` | — |
| Atuin | `command -v atuin` | "Install Atuin" → `curl setup.atuin.sh` |
| Mise | `command -v mise` | "Install Mise" → `curl mise.run` |
| Zoxide | `command -v zoxide` | — |
| fzf | `command -v fzf` | — |

Install buttons appear only when the tool status contains `✗ not found`. After any install runner completes, `detectTools()` is called automatically and a success toast is shown.

**Notification Threshold** — `ControlRow` selector below the tool list with `5s`, `10s`, `30s` options. Maps to `segments.notification.threshold_ms` (5000/10000/30000). The daemon includes `notify_threshold_ms` in prompt responses; the bash adapter updates its threshold from this field.

### Advanced Tab

| Action | Behavior |
|--------|----------|
| Open Config File | Opens `$TERMINAL` (default: `foot`) running `$EDITOR` (default: `nano`) via `Process.startDetached()` |
| Run Doctor | `omarchy10k doctor`; output shown in scrollable monospace area below |
| Copy Config | Serialize config to clipboard |
| Paste Config | Parse clipboard TOML, apply, save |
| Reload Config | Re-fetch config via `config_get` + `reload_config` |
| Run Benchmark | `omarchy10k benchmark --iterations 50`; results in scrollable area |
| Reset to Defaults | Backup to `.bak`, delete config, reload |
| Daemon info | Status, PID, version, protocol version, protocol status label |
| Session list | All discovered sockets with shell PID, CWD, floating-terminal button |

### Multi-Session

When multiple shells are running, each has its own daemon socket. The Advanced tab provides a session selector:

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
| `hyprctlFocus` (in `SessionPicker.qml`) | `hyprctl dispatch focuswindow pid:<shellPid>` | Session picker activation |
| `focusLauncher` (in `SessionPicker.qml`) | `cd '<cwd>' && omarchy-launch-floating-terminal` | Picker fallback focus |
| `configReader` | `cat config.toml` | Panel open, reload, reset |
| `socketFinder` | `ls $XDG_RUNTIME_DIR/omarchy10k-*.sock` | Panel open, reconnect |
| `barSocketFinder` | `ls … \| head -1` | BarWidget init, bar poll |
| `toolDetector` | 5× `command -v` | Panel open |
| `editorLauncher` | `$EDITOR config.toml` | Advanced tab button |
| `doctorRunner` | `omarchy10k doctor` | Advanced tab button |
| `benchRunner` | `omarchy10k benchmark --iterations 50` | Advanced tab button |
| `installRunner` | curl install scripts | Shell tab install buttons |
| `floatingTermLauncher` | `cd '$cwd' && exec $SHELL` | Session row terminal icon |
| `clipboardCopy` | xclip / wl-copy | Copy Config |
| `clipboardPaste` | xclip -o / wl-paste | Paste Config |
| `resetProc` | Backup + rm config.toml | Advanced tab button |

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
| `CONFIG_MAP` | TOML key ↔ QML property mapping (31 keys) |
| `applyConfig(flat, target)` | Load parsed config into QML properties; skips undefined/null values |
| `collectConfig(source)` | Export QML properties to flat object |
| `parseToolOutput(text)` | Parse `name=path\|missing` format |

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

The Segments tab toggles `segments.python.enabled`, `segments.toolchain.enabled`
and `segments.nix.enabled`. Those three segments read the *daemon's* environment,
which is frozen at shell startup, so enabling them has no visible effect for a
user who activates a venv, switches mise versions, or enters a nix shell after the
shell started. This is tracked as a [v0.4 design item](bug-audit.md#5-every-environment-derived-segment-is-frozen-at-daemon-start).

The Time segment ABI bug ([#2](bug-audit.md#2-struct-tm-abi-mismatch-corrupts-the-stack-when-the-time-segment-is-enabled))
has been fixed — `struct Tm` now includes all required fields.
