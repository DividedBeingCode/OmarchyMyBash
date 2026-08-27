# Quattro Plugin Reference

[← Index](INDEX.md) | [Protocol](protocol.md) | [Configuration](config.md)

The Quattro plugin provides a desktop Control Center for Omarchy10k, surfaced as a bar widget in the Omarchy Quattro panel. It reads and writes config files, communicates with running daemon instances over Unix sockets, and detects installed shell tools.

## Manifest (`quattro/manifest.json`)

```json
{
  "schemaVersion": 1,
  "id": "community.omarchy10k",
  "name": "Omarchy10k",
  "version": "0.1.0",
  "kinds": ["bar-widget"],
  "entryPoints": { "barWidget": "BarWidget.qml" },
  "barWidget": {
    "displayName": "Omarchy10k",
    "category": "Shell",
    "allowMultiple": false,
    "defaultSection": "right"
  }
}
```

- `allowMultiple: false` — only one instance on the bar
- `defaultSection: "right"` — appears in right bar section by default
- Panel path is not declared in manifest — `BarWidget.qml` loads `Panel.qml` via `Loader`

## Component Hierarchy

```
BarWidget (qs.Ui.BarWidget)
├── Loader → Panel.qml
├── WidgetButton
│   ├── text: "❯"
│   ├── tooltip: "Omarchy10k"
│   └── onClicked: toggle()
└── panel ↔ bar wiring (injectPanel on load/bar change)

Panel (qs.Ui.Panel, manageIpc: false)
├── KeyboardPanel → PanelKeyCatcher (Escape, Tab)
├── Tab bar (Appearance, Context, Shell, Advanced)
├── 4× Component tabs (Loader-switched)
├── Inline components: ControlRow, StatusRow, ActionButton
├── 6× Process (configReader, configWriter, socketFinder, toolDetector, editorLauncher, doctorRunner, resetProc)
├── 1× Socket (daemonSocket + SplitParser)
└── 2× Timer (saveTimer 300ms, reconnectTimer 5000ms)
```

## Quickshell Imports

| Import | Components Used |
|--------|----------------|
| `QtQuick` | Core QML types |
| `Quickshell` | Base types |
| `Quickshell.Io` | `Process`, `Socket`, `StdioCollector`, `SplitParser` |
| `qs.Commons` | `Color` (accent, background, muted, green, red) |
| `qs.Ui` | `BarWidget`, `Panel`, `WidgetButton`, `KeyboardPanel`, `PanelKeyCatcher`, `Style` |
| `Model.js` | `parseTOML`, `buildTOML`, `buildCommand`, `configPath`, `runtimeDir`, etc. |

## Panel Lifecycle

| Event | Actions |
|-------|---------|
| User clicks bar glyph | `toggle()` → `controller.show()` |
| Panel opens | Load config, discover socket, detect tools |
| Config change | Update property → debounce 300ms → write TOML → `reload_config` |
| Panel closes | `controller.hide()`, disconnect socket |
| Tab switch | `Loader` swaps `appearanceTab` / `contextTab` / `shellTab` / `advancedTab` |

## Socket Discovery and Daemon IPC

### Discovery

```javascript
socketFinder.exec(["sh", "-c",
    "ls " + Model.runtimeDir() + "/omarchy10k-*.sock 2>/dev/null | head -1"])
```

Globs all `omarchy10k-*.sock` files in `$XDG_RUNTIME_DIR` (or `/tmp`). Takes the first match. This works for single-session use; with multiple shells, it connects to whichever socket is found first.

### Connection Flow

```
socketFinder returns path
  → daemonSocket.path = path
  → daemonSocket.connected = true
    → onConnectedChanged → sendDaemonCommand("status")
      → response parsed → daemonStatus = "running", pid, version set
```

### Reconnection

`reconnectTimer` fires every 5 seconds when panel is open and daemon is not running. Re-runs socket discovery.

On panel close, socket is explicitly disconnected.

### Protocol

Outbound: `Model.buildCommand(name)` → `'{"command":"name"}\n'`
Inbound: `Model.parseDaemonResponse(json)` → parsed JSON object

Commands used by the panel:

| Command | When |
|---------|------|
| `status` | On socket connect |
| `reload_config` | After config write, manual reload, after reset |

Commands available in daemon but not used by panel: `reload_theme`, `invalidate_git`, `shutdown`.

## Config Management

### Read Flow

```
open() → configReader.exec(["cat", configPath])
  → StdioCollector captures stdout
  → onRunningChanged (when stopped): parse TOML
  → Model.parseTOML(text) → flat {key: value} object
  → _applyParsedConfig(flat) → set QML properties
  → _configFlat = flat (master copy)
```

### Write Flow

```
setConfigValue(key, value)
  → _configFlat[key] = value
  → QML property updated
  → saveTimer.restart() (300ms debounce)
    → _flushSave()
      → Model.buildTOML(_configFlat) → TOML string
      → configWriter.exec(["sh", "-c", "mkdir -p dir && cat > file"])
      → configWriter.write(toml)
      → onRunningChanged (when stopped): sendDaemonCommand("reload_config")
```

### CONFIG_MAP

Maps TOML keys to QML property names:

| TOML Key | QML Property |
|----------|-------------|
| `prompt.layout` | `cfgLayout` |
| `prompt.transient` | `cfgTransient` |
| `prompt.newline` | `cfgNewline` |
| `prompt.right_prompt` | `cfgRightPrompt` |
| `theme.source` | `cfgThemeSource` |
| `git.mode` | `cfgGitMode` |
| `git.enabled` | `cfgGitEnabled` |
| `segments.os.icon` | `cfgOsIcon` |
| `segments.exit_status.show_signal_name` | `cfgExitSignalNames` |
| `segments.command_duration.show_above_ms` | `cfgCmdDurationMs` |
| `segments.ssh.show` | `cfgSshShow` |

## UI Tabs

### Appearance Tab

| Control | TOML Key | Options |
|---------|----------|---------|
| Preset | `prompt.layout` | omarchy, minimal, powerline, classic, pure, dense |
| Theme | `theme.source` | omarchy, custom, hybrid, terminal |
| Lines | `prompt.newline` | Two-line / One-line |
| Transient | `prompt.transient` | On / Off |
| OS Icon | `segments.os.icon` | arch, linux, omarchy, none |

### Context Tab

| Control | TOML Key | Options |
|---------|----------|---------|
| Git | `git.mode` | adaptive, compact, expanded, hidden |
| Duration | `segments.command_duration.show_above_ms` | 500, 1000, 1500, 3000, 5000 ms |
| SSH | `segments.ssh.show` | auto, always, never |
| Exit Status | `segments.exit_status.show_signal_name` | Signal names / Codes only |

### Shell Tab (read-only)

Displays detection results for five tools:

| Tool | Detection |
|------|-----------|
| ble.sh | `command -v ble.sh` or check `~/.local/share/blesh/ble.sh` |
| Atuin | `command -v atuin` |
| Mise | `command -v mise` |
| Zoxide | `command -v zoxide` |
| fzf | `command -v fzf` |

Shows path if installed, "not found" otherwise.

### Advanced Tab

| Action | Behavior |
|--------|----------|
| Open Config File | `$EDITOR` or `nano` via `Process.startDetached()` |
| Run Doctor | `omarchy10k doctor`, output logged |
| Reload Config | Re-read TOML + `reload_config` to daemon |
| Reset to Defaults | Backup to `.bak`, delete config, reload |
| Daemon info | Status, PID, version from `status` command |

## Process Components

| ID | Command | Trigger |
|----|---------|---------|
| `configReader` | `cat config.toml` | Panel open, reload, reset |
| `configWriter` | `sh -c "mkdir -p && cat > file"` | Debounced save |
| `socketFinder` | `ls $XDG_RUNTIME_DIR/omarchy10k-*.sock \| head -1` | Panel open, reconnect |
| `toolDetector` | 5× `command -v` | Panel open |
| `editorLauncher` | `$EDITOR config.toml` | Advanced tab button |
| `doctorRunner` | `omarchy10k doctor` | Advanced tab button |
| `resetProc` | Backup + rm config.toml | Advanced tab button |

## Model.js

Stateless helper library (`.pragma library`):

| Function | Purpose |
|----------|---------|
| `configDir()` | `$HOME/.config/omarchy10k` |
| `configPath()` | `configDir()/config.toml` |
| `runtimeDir()` | `$XDG_RUNTIME_DIR` or `/tmp` |
| `buildCommand(name)` | JSON command string with newline |
| `parseDaemonResponse(json)` | Safe JSON parse with error wrapping |
| `parseTOML(text)` | Subset TOML parser → flat key-value object |
| `buildTOML(flat)` | Flat object → sectioned TOML string |
| `CONFIG_MAP` | TOML key ↔ QML property mapping |
| `applyConfig(flat, target)` | Load parsed TOML into QML properties |
| `collectConfig(source)` | Export QML properties to flat object |
| `parseToolOutput(text)` | Parse `name=path\|missing` format |

### TOML Parser Limitations

The `parseTOML()` implementation supports:
- `[section]` headers
- `key = "string"`, `key = true/false`, `key = integer`
- `#` comments

It does not support: nested tables, arrays, inline tables, multi-line strings, dotted keys.

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

```bash
cp -r quattro/ ~/.config/omarchy/plugins/community.omarchy10k/
```

Requires a running Omarchy Quattro desktop with Quickshell-based bar system.
