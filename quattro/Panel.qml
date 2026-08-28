import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

Panel {
    id: root
    moduleName: "community.omarchy10k"
    manageIpc: false

    property var anchorItem: null
    property var hostWidget: null
    signal galleryRequested()
    property string searchQuery: ""
    // Defaults snapshot (daemon `defaults` verb) — powers modified-ink and
    // per-row reset. Empty until the first successful fetch.
    property var defaultFlat: ({})

    function isModified(tomlKey) {
        return root._configFlat[tomlKey] !== undefined
            && root.defaultFlat[tomlKey] !== undefined
            && root._configFlat[tomlKey] !== root.defaultFlat[tomlKey]
    }

    function resetConfigKey(tomlKey) {
        if (root.defaultFlat[tomlKey] === undefined) return
        root.setConfigValue(tomlKey, root.defaultFlat[tomlKey])
        root.toastMessage = "Reset " + tomlKey.split(".").pop()
        root._showToast = true
        toastTimer.restart()
    }

    function fetchDefaults() {
        if (!daemonSocket.connected) return
        daemonSocket.write(JSON.stringify({ type: "control", command: "defaults", id: "defaults-load" }) + "\n")
        daemonSocket.flush()
    }

    // ── Reactive Config State ──────────────────────────────────────────────
    property string cfgLayout: "omarchy"
    property string cfgThemeSource: "omarchy"
    property bool cfgNewline: true
    property bool cfgTransient: true
    property bool cfgBlankLine: true
    property bool cfgRightPrompt: true
    property string cfgStylePreset: "rainbow"
    // Active curated palette: "theme" = follow the Omarchy theme, a
    // CURATED_PALETTES key, or "custom" for hand-set overrides.
    readonly property string cfgPalette: {
        var src = root._configFlat["theme.source"]
        if (!src || src === "omarchy") return "theme"
        var accent = root._configFlat["theme.custom.accent"]
        if (accent) {
            var keys = Object.keys(Model.CURATED_PALETTES)
            for (var i = 0; i < keys.length; i++) {
                if (Model.CURATED_PALETTES[keys[i]].accent.toLowerCase() === String(accent).toLowerCase())
                    return keys[i]
            }
        }
        return "custom"
    }
    property string cfgSepLeft: ""
    property string cfgSepRight: ""
    property bool cfgFrameEnabled: false
    property string cfgFrameGapChar: ""
    property string cfgGitMode: "adaptive"
    property bool cfgGitEnabled: true
    property string cfgGitBranchIcon: "powerline"
    property string cfgOsIcon: "arch"
    property string cfgCharSuccess: "\u276f"
    property string cfgCharError: "\u276f"
    property string cfgCharTransient: "\u276f"
    property bool cfgExitSignalNames: true
    property int cfgCmdDurationMs: 1500
    property string cfgSshShow: "auto"
    property bool cfgContainerEnabled: true
    property bool cfgPythonEnabled: true
    property bool cfgToolchainEnabled: true
    property bool cfgNixEnabled: true
    property bool cfgK8sEnabled: false
    property bool cfgTimeEnabled: false
    property bool cfgLoadEnabled: false
    property string cfgTimeFormat: "%H:%M"
    property bool cfgBatteryEnabled: false
    property int cfgNotifyThresholdMs: 10000
    property bool cfgTitleEnabled: true

    // ── Reactive Daemon State ──────────────────────────────────────────────
    property string daemonStatus: "unknown"
    property string daemonPid: ""
    property string daemonVersion: ""
    property string daemonProtocolVersion: ""
    property string discoveredSocketPath: ""

    // Service-kind hub (v0.4) — mirrors daemon status + session discovery when
    // our Service.qml is loaded by the host. Feature-detected: absent/old
    // hosts keep the panel's own poll/reconnect path.
    readonly property var omarchyService: root.bar && root.bar.shell
        && typeof root.bar.shell.serviceFor === "function"
        ? root.bar.shell.serviceFor("community.omarchy10k") : null

    onOmarchyServiceChanged: _syncFromService()

    Connections {
        target: root.omarchyService
        function onSessionsChanged() { root._syncFromService() }
        function onDaemonStatusChanged() { root._syncFromService() }
    }

    function _syncFromService() {
        if (!root.omarchyService) return
        if (root.omarchyService.daemonStatus) root.daemonStatus = root.omarchyService.daemonStatus
        if (root.omarchyService.sessions && root.omarchyService.sessions.length > 0)
            root.sessionList = root.omarchyService.sessions
    }

    // ── Multi-Session State ─────────────────────────────────────────────────
    property var sessionList: []
    property int activeSessionIndex: 0

    // ── Reactive Tool State ────────────────────────────────────────────────
    property string bleshStatus: "checking..."
    property string atuinStatus: "checking..."
    property string miseStatus: "checking..."
    property string zoxideStatus: "checking..."
    property string fzfStatus: "checking..."

    // ── Error / Toast / Preview State ─────────────────────────────────────
    property string lastError: ""
    property bool _showError: false
    property string toastMessage: ""
    property bool _showToast: false
    property string doctorOutput: ""
    property string previewText: ""
    property bool previewError: false
    property bool previewSsh: false
    property bool previewLongCmd: false
    property int previewJobs: 0
    property var paletteColors: ({})
    property string benchmarkOutput: ""

    // Style gallery cards — static `preview` strings are the offline fallback;
    // live renders are fetched per-preset via the daemon preview message's
    // `style_preset` override (daemon v0.4+; W1 server.rs PreviewRequest).
    property var presetPreviews: ({})
    property var presetCards: [
        { name: "omarchy",    preview: "  ~ \u276f",               desc: "Clean" },
        { name: "powerline",  preview: "  ~ \ue0b0 git",          desc: "Classic" },
        { name: "rainbow",    preview: "  ~ \ue0b0\ue0b0\ue0b0",  desc: "Vibrant" },
        { name: "gradient",   preview: "  ~ \u2584\u2584\u2584",     desc: "Ramp" },
        { name: "framed",     preview: "\u256d\u2500 ~ \u2500\u256e",  desc: "Framed" },
        { name: "classic",    preview: "  ~ \u2502 git",          desc: "Divided" },
        { name: "lean",       preview: "  ~/src",                  desc: "Minimal" },
        { name: "dense",      preview: "  ~ git \u276f",          desc: "Compact" },
        { name: "slanted",    preview: "  ~ \ue0bc git",          desc: "Modern" }
    ]

    // ── Internal ───────────────────────────────────────────────────────────
    property bool _configDirty: false
    property var _configFlat: ({})
    property var _undoStack: []
    property int _undoMaxSize: 10

    readonly property string _configPath: Model.configPath(
        Quickshell.env("XDG_CONFIG_HOME"), Quickshell.env("HOME"))

    // Keys changed in this panel since the last successful save. _flushSave
    // sends ONLY these — a full collectConfig stamp would clobber edits made
    // outside the panel (CLI, another surface) with stale UI state.
    property var _dirtyKeys: ({})

    // ── Panel Lifecycle ────────────────────────────────────────────────────
    function open() {
        root.controller.show()
        discoverAllSockets()
        detectTools()
        loadConfig()
    }

    function close() {
        root.controller.hide()
        daemonSocket.connected = false
    }

    function switchPanel(direction) {
        if (root.bar && typeof root.bar.switchPanelFrom === "function")
            return root.bar.switchPanelFrom(root.hostWidget || root, direction)
        return false
    }

    // ── Config Read (via daemon IPC) ──────────────────────────────────────
    function loadConfig() {
        if (!daemonSocket.connected) {
            configReader.exec(["cat", _configPath])
            return
        }
        daemonSocket.write(Model.buildConfigGet("cfg-load"))
        daemonSocket.flush()
    }

    function _applyParsedConfig(text) {
        root._configFlat = Model.parseTOML(text)
        Model.applyConfig(root._configFlat, root)
    }

    function _applyDaemonConfig(configObj) {
        root._configFlat = Model.flattenConfig(configObj)
        Model.applyConfig(root._configFlat, root)
    }

    // ── Config Write (via daemon IPC) ──────────────────────────────────────
    // Palette cards write straight to the daemon ([theme.custom] + hybrid
    // source) — not through the debounced save path, whose collectConfig
    // would drop the per-role values (no CONFIG_MAP entries).
    function applyPalette(key) {
        if (!daemonSocket.connected) {
            root.lastError = "Changing palette requires a running omarchy10k daemon"
            root._showError = true
            errorTimer.restart()
            return
        }
        var themePatch
        if (key === "theme") {
            themePatch = { source: "omarchy" }
        } else {
            var p = Model.CURATED_PALETTES[key]
            if (!p) return
            themePatch = {
                source: "hybrid",
                custom: {
                    accent: p.accent, foreground: p.foreground, muted: p.muted,
                    background: p.background, red: p.red, green: p.green,
                    yellow: p.yellow, blue: p.blue, magenta: p.magenta,
                    cyan: p.cyan, orange: p.orange
                }
            }
        }
        daemonSocket.write(Model.buildConfigSet({ theme: themePatch }, "palette-set"))
        daemonSocket.flush()
        // Mirror into panel state so a later delta save (or undo snapshot)
        // can't resurrect the pre-palette theme values.
        root._configFlat["theme.source"] = themePatch.source
        if (themePatch.custom) {
            for (var ck in themePatch.custom)
                root._configFlat["theme.custom." + ck] = themePatch.custom[ck]
        }
        root.cfgThemeSource = themePatch.source
        root.toastMessage = "Palette → " + (key === "theme" ? "Omarchy theme" : Model.CURATED_PALETTES[key].label)
        root._showToast = true
        toastTimer.restart()
        Qt.callLater(root.requestPreview)
        Qt.callLater(root.requestPresetPreviews)
        Qt.callLater(root.requestPalette)
    }
    // ── Looks (Wave 2) ─────────────────────────────────────────────────────
    function applyLook(name, transient) {
        if (!daemonSocket.connected) {
            root.lastError = "Applying a Look requires a running omarchy10k daemon"
            root._showError = true
            errorTimer.restart()
            return
        }
        daemonSocket.write(JSON.stringify({
            type: "control", command: "looks_apply",
            name: name, transient: !!transient
        }) + "\n")
        daemonSocket.flush()
        root.toastMessage = "Look → " + name + (transient ? " (try)" : "")
        root._showToast = true
        toastTimer.restart()
        Qt.callLater(root.requestPreview)
        Qt.callLater(root.requestPalette)
        Qt.callLater(root.loadConfig)
    }

    function saveLook(name) {
        var safe = String(name || "").trim()
        if (safe.length === 0) {
            root.lastError = "Enter a name for the Look first"
            root._showError = true
            errorTimer.restart()
            return
        }
        if (!daemonSocket.connected) {
            root.lastError = "Saving a Look requires a running omarchy10k daemon"
            root._showError = true
            errorTimer.restart()
            return
        }
        daemonSocket.write(JSON.stringify({
            type: "control", command: "looks_save", name: safe, label: safe
        }) + "\n")
        daemonSocket.flush()
        root.toastMessage = "Look saved · " + safe
        root._showToast = true
        toastTimer.restart()
    }

    function setConfigValue(tomlKey, value) {
        var prop = Model.CONFIG_MAP[tomlKey]
        var oldVal = prop ? root[prop] : undefined


        if (oldVal !== undefined && oldVal !== value) {
            var snapshot = JSON.parse(JSON.stringify(root._configFlat))
            var stack = root._undoStack.slice()
            stack.push(snapshot)
            if (stack.length > root._undoMaxSize) stack.shift()
            root._undoStack = stack
        }
        if (prop) root[prop] = value
        root._configFlat[tomlKey] = value
        var dirty = root._dirtyKeys
        dirty[tomlKey] = true
        root._dirtyKeys = dirty
        _scheduleSave()

        if (oldVal !== undefined && oldVal !== value) {
            root.toastMessage = "Changed " + tomlKey.split(".").pop() + " → " + value
            root._showToast = true
            toastTimer.restart()
        }
    }

    function undoConfig() {
        if (root._undoStack.length === 0) return
        var stack = root._undoStack.slice()
        var prev = stack.pop()
        root._undoStack = stack
        root._configFlat = prev
        Model.applyConfig(prev, root)
        // The restored snapshot must be persisted wholesale — every mapped
        // key is now "the change".
        var mk = Object.keys(Model.CONFIG_MAP)
        var dirty = root._dirtyKeys
        for (var i = 0; i < mk.length; i++) dirty[mk[i]] = true
        root._dirtyKeys = dirty
        _scheduleSave()
        root.toastMessage = "Undid last change"
        root._showToast = true
        toastTimer.restart()
    }

    function _featureAvailable(minVersion) {
        return Model.protocolAtLeast(root.daemonProtocolVersion, minVersion)
    }

    function _scheduleSave() {
        if (!root._configDirty) {
            root._configDirty = true
            saveTimer.restart()
        }
    }

    function _flushSave() {
        root._configDirty = false

        if (daemonSocket.connected) {
            // Delta save: only keys actually changed here. The panel's UI
            // state is a snapshot from load time — stamping all mapped keys
            // would revert edits made outside the panel since then.
            var full = Model.collectConfig(root)
            var flat = {}
            for (var k in root._dirtyKeys) {
                if (k in full) flat[k] = full[k]
            }
            root._dirtyKeys = {}
            if (Object.keys(flat).length === 0) return
            var patch = Model.unflattenPatch(flat)
            daemonSocket.write(Model.buildConfigSet(patch, "cfg-save"))
            daemonSocket.flush()
        } else {
            console.warn("omarchy10k: config save skipped — no daemon connected")
            root.lastError = "Saving settings requires a running omarchy10k daemon"
            root._showError = true
            errorTimer.restart()
            // Keep the dirty set and re-arm: _onSocketConnected retries the
            // flush, so a daemon restart mid-edit no longer drops changes.
            root._configDirty = true
        }

        Qt.callLater(root.requestPreview)
        Qt.callLater(root.requestPresetPreviews)
    }

    function requestPalette() {
        if (!daemonSocket.connected) return
        daemonSocket.write(Model.buildCommand("palette", "palette-req"))
        daemonSocket.flush()
    }

    function requestPreview() {
        if (!daemonSocket.connected) return
        var ctx = {
            cwd: "~/projects/my-app",
            exit_code: root.previewError ? 1 : 0,
            cmd_duration_ms: root.previewLongCmd ? 5000 : 0,
            cols: 120,
            jobs: root.previewJobs,
            in_ssh: root.previewSsh,
            git_branch: "main",
            git_staged: 2,
            git_unstaged: 1
        }
        daemonSocket.write(Model.buildPreview(ctx, "preview"))
        daemonSocket.flush()
    }

    // Preset gallery live previews: one preview request per card with the
    // daemon-side style.preset override (protocol v0.4 `style_preset` field).
    // Daemons that ignore the field render the current preset for every card;
    // the static fallback strings in `presetCards` cover no-daemon states.
    function requestPresetPreviews() {
        if (!daemonSocket.connected || !root._featureAvailable("0.3")) return
        for (var i = 0; i < root.presetCards.length; i++) {
            var name = root.presetCards[i].name
            var ctx = {
                cwd: "~/projects/my-app",
                exit_code: 0,
                cmd_duration_ms: 0,
                cols: 120,
                jobs: 0,
                in_ssh: false,
                git_branch: "main",
                git_staged: 2,
                git_unstaged: 1,
                style_preset: name
            }
            daemonSocket.write(Model.buildPreview(ctx, "preset-" + name))
        }
        daemonSocket.flush()
    }

    function _applyPresetPreview(name, ansiLeft) {
        var map = root.presetPreviews
        map[name] = Model.ansiToRich(ansiLeft)
        root.presetPreviews = map
    }

    // ── Headless daemon ─────────────────────────────────────────────────────
    // Settings must work with no terminal open. With no shell session alive,
    // spawn a headless omarchy10kd bound to a fixed socket name: it writes
    // the same config.toml (shell daemons hot-reload it via their watcher)
    // and lives until the desktop session ends.
    function ensureHeadlessDaemon() {
        if (daemonSocket.connected || headlessDaemon.running) return
        headlessDaemon.exec(["sh", "-c",
            "O10K_SOCK_NAME=headless O10K_PARENT_PID=$(pgrep -x quickshell | head -1 || true) exec omarchy10kd"])
    }

    function discoverAllSockets() {
        // Only sockets whose owning shell PID is still alive. A dead shell's
        // socket file lingers and otherwise surfaces as a session with no
        // data behind it.
        socketFinder.exec(["sh", "-c",
            "for f in '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock; do " +
            "[[ -e \"$f\" ]] || continue; p=${f##*-}; p=${p%.sock}; " +
            "case \"$p\" in *[!0-9]*) ;; *) kill -0 \"$p\" 2>/dev/null || continue ;; esac; " +
            "timeout 1 socat -u OPEN:/dev/null UNIX-CONNECT:\"$f\" 2>/dev/null && echo \"$f\"; done"])
    }

    function connectToSession(idx) {
        if (idx < 0 || idx >= root.sessionList.length) return
        // Changes belong to the session being edited — save them through the
        // current socket before rebinding. All daemons share one config.toml,
        // so an unflushed delta would otherwise land wherever the timer fires.
        if (root._configDirty) root._flushSave()
        root.activeSessionIndex = idx
        var session = root.sessionList[idx]
        daemonSocket.connected = false
        root.discoveredSocketPath = session.path
        daemonSocket.path = session.path
        daemonSocket.connected = true
    }

    function sendDaemonCommand(name) {
        if (!daemonSocket.connected) return
        daemonSocket.write(Model.buildCommand(name))
        daemonSocket.flush()
    }

    function _handleDaemonMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)

        if (resp.type === "hello") {
            root.daemonProtocolVersion = resp.protocol_version || ""
            root.daemonVersion = resp.server_version || ""
            sendDaemonCommand("status")
            return
        }

        if (resp.type === "control" && resp.config && resp.id === "defaults-load") {
            root.defaultFlat = Model.flattenConfig(resp.config)
            return
        }

        if (resp.type === "config" && resp.config) {
            root._applyDaemonConfig(resp.config)
            return
        }

        if (resp.type === "preview") {
            if (resp.id && resp.id.indexOf("preset-") === 0) {
                root._applyPresetPreview(resp.id.substring("preset-".length), resp.left)
                return
            }
            if (resp.left) {
                // Rendered ANSI → StyledText markup (3.1 live color preview).
                root.previewText = Model.ansiToRich(resp.left)
                return
            }
        }

        if (resp.type === "control" && resp.palette) {
            root.paletteColors = resp.palette
            return
        }

        if (resp.status === "ok" && resp.pid !== undefined) {
            root.daemonStatus = "running"
            root.daemonPid = String(resp.pid)
            root.daemonVersion = resp.version || root.daemonVersion
            root.daemonProtocolVersion = resp.protocol_version || root.daemonProtocolVersion
            if (root.sessionList.length > 0 && root.activeSessionIndex < root.sessionList.length) {
                var updated = root.sessionList.slice()
                updated[root.activeSessionIndex].pid = String(resp.pid)
                updated[root.activeSessionIndex].cwd = resp.cwd || ""
                root.sessionList = updated
            }
            loadConfig()
        } else if (resp.status === "ok") {
            root.daemonStatus = "running"
        } else if (resp.status === "bye") {
            root.daemonStatus = "stopped"
        } else if (resp.error) {
            root.daemonStatus = "error"
            root.lastError = resp.error
            root._showError = true
            errorTimer.restart()
        }
    }

    function _onSocketConnected() {
        daemonSocket.write(Model.buildHello("handshake"))
        daemonSocket.flush()
        // A save that failed while disconnected retries here.
        if (root._configDirty) root._flushSave()
        // Defaults snapshot for modified-ink + per-row reset
        Qt.callLater(root.fetchDefaults)
        Qt.callLater(root.requestPreview)
        Qt.callLater(root.requestPresetPreviews)
        Qt.callLater(root.requestPalette)
    }

    function _onSocketError() {
        console.warn("omarchy10k: socket error on " + (root.discoveredSocketPath || "unknown"))
        daemonSocket.connected = false
        if (root.opened) root.daemonStatus = "not running"
        if (root.discoveredSocketPath.length > 0) {
            root.sessionList = root.sessionList.filter(function (s) {
                return s.path !== root.discoveredSocketPath
            })
        }
        // The first discovered socket can belong to a shell that died between
        // listing and connecting — fall through to the next live session.
        if (root.opened && root.sessionList.length > 0 && !daemonSocket.connected)
            root.connectToSession(0)
    }

    // ── Tool Detection ─────────────────────────────────────────────────────
    function detectTools() {
        root.bleshStatus = "checking..."
        root.atuinStatus = "checking..."
        root.miseStatus = "checking..."
        root.zoxideStatus = "checking..."
        root.fzfStatus = "checking..."
        toolDetector.exec(["sh", "-c",
            "echo blesh=$(command -v blesh 2>/dev/null || echo missing);" +
            "echo atuin=$(command -v atuin 2>/dev/null || echo missing);" +
            "echo mise=$(command -v mise 2>/dev/null || echo missing);" +
            "echo zoxide=$(command -v zoxide 2>/dev/null || echo missing);" +
            "echo fzf=$(command -v fzf 2>/dev/null || echo missing)"
        ])
    }

    function _applyToolDetection(text) {
        var tools = Model.parseToolOutput(text)
        root.bleshStatus  = tools.blesh  ? ("\u2713 " + tools.blesh)  : "\u2717 not found"
        root.atuinStatus  = tools.atuin  ? ("\u2713 " + tools.atuin)  : "\u2717 not found"
        root.miseStatus   = tools.mise   ? ("\u2713 " + tools.mise)   : "\u2717 not found"
        root.zoxideStatus = tools.zoxide ? ("\u2713 " + tools.zoxide) : "\u2717 not found"
        root.fzfStatus    = tools.fzf    ? ("\u2713 " + tools.fzf)    : "\u2717 not found"
    }

    // ── I/O Components ─────────────────────────────────────────────────────

    Process {
        id: configReader
        command: ["cat", root._configPath]
        stdout: StdioCollector {
            onStreamFinished: root._applyParsedConfig(this.text)
        }
    }

    Process {
        id: socketFinder
        stdout: StdioCollector {
            onStreamFinished: {
                var text = this.text.trim()
                if (text.length === 0) {
                    root.daemonStatus = "not running"
                    root.sessionList = []
                    root.ensureHeadlessDaemon()
                    return
                }
                var paths = text.split("\n")
                var sessions = []
                for (var i = 0; i < paths.length; i++) {
                    var p = paths[i].trim()
                    if (p.length === 0) continue
                    var pidMatch = p.match(/omarchy10k-([A-Za-z0-9-]+)\.sock$/)
                    sessions.push({
                        path: p,
                        shellPid: pidMatch ? pidMatch[1] : "?",
                        pid: "",
                        cwd: ""
                    })
                }
                if (root.omarchyService) {
                    // The service hub owns discovery; only ensure a working
                    // connection for this panel's preview/config traffic.
                    if (sessions.length > 0 && !daemonSocket.connected) root.connectToSession(0)
                    return
                }
                root.sessionList = sessions
                if (sessions.length > 0) {
                    root.connectToSession(0)
                }
            }
        }
    }
    Process {
        id: headlessDaemon
    }

    Process {
        id: toolDetector
        stdout: StdioCollector {
            onStreamFinished: root._applyToolDetection(this.text)
        }
    }

    Process {
        id: editorLauncher
    }

    Process {
        id: doctorRunner
        stdout: StdioCollector {
            onStreamFinished: root.doctorOutput = Model.stripAnsi(this.text)
        }
    }

    Process {
        id: floatingTermLauncher
    }

    Process {
        id: installRunner
        onRunningChanged: {
            if (!running) {
                root.detectTools()
                root.toastMessage = "Installation finished — tools refreshed"
                root._showToast = true
                toastTimer.restart()
            }
        }
    }

    Process {
        id: clipboardCopy
    }

    Process {
        id: clipboardPaste
        stdout: StdioCollector {
            onStreamFinished: {
                if (this.text.trim().length > 0) {
                    root._applyParsedConfig(this.text)
                    root._scheduleSave()
                    root.toastMessage = "Config pasted from clipboard"
                    root._showToast = true
                    toastTimer.restart()
                }
            }
        }
    }

    Process {
        id: benchRunner
        stdout: StdioCollector {
            onStreamFinished: root.benchmarkOutput = Model.stripAnsi(this.text)
        }
    }

    Socket {
        id: daemonSocket
        parser: SplitParser {
            onRead: message => root._handleDaemonMessage(message)
        }
        onConnectedChanged: {
            if (connected) root._onSocketConnected()
            else if (root.opened) root.daemonStatus = "not running"
        }
        onError: root._onSocketError()
    }

    Timer {
        id: saveTimer
        interval: 300
        repeat: false
        onTriggered: root._flushSave()
    }


    Timer {
        id: reconnectTimer
        interval: 5000
        repeat: true
        // Runs even when the service hub is active: the hub sweep can lag a
        // daemon that appeared after the panel opened, and the panel's own
        // finder result merges without clobbering hub-owned sessionList.
        running: root.opened && root.daemonStatus !== "running"
        onTriggered: root.discoverAllSockets()
    }

    Timer {
        id: toastTimer
        interval: 2000
        repeat: false
        onTriggered: root._showToast = false
    }

    // ── Panel UI ───────────────────────────────────────────────────────────

    KeyboardPanel {
        id: panel
        anchorItem: root.anchorItem
        owner: root.hostWidget || root
        bar: root.bar
        open: root.opened
        focusTarget: keyCatcher
        contentWidth: panel.fittedContentWidth(Style.space(360))
        contentHeight: panel.fittedContentHeight(content.implicitHeight, Style.space(560))

        PanelKeyCatcher {
            id: keyCatcher
            anchors.fill: parent
            onCloseRequested: root.close()
            onTabRequested: function(direction) { root.switchPanel(direction) }

            // Plain Flickable — NOT a ScrollView. ScrollView carries its own
            // builtin wheel handling that fights a custom WheelHandler (dual
            // owners = the user-visible slowness). A bare Flickable has no
            // builtin wheel path, so the WheelHandler below is the single
            // owner and the boost is authoritative.
            Flickable {
                id: scrollArea
                anchors.fill: parent
                clip: true
                contentWidth: content.implicitWidth
                contentHeight: content.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                // A bare Flickable does not auto-create its attached
                // ScrollBars (ScrollView did) — provide them explicitly.
                ScrollBar.vertical: ScrollBar {
                    policy: content.implicitHeight > scrollArea.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
                }

                // Precision touchpads deliver tiny angleDelta values that
                // scroll painfully slowly. Amplify and drive contentY
                // directly. Tunable: wheelBoost.
                WheelHandler {
                    // NOTE: the boost multiplier must not share its name with
                    // this handler's id — `id: wheelBoost` + `property real
                    // wheelBoost` made every lookup resolve to the handler
                    // object and the scroll step computed NaN (silent dead
                    // scroll). Keep the names distinct.
                    id: panelWheel
                    property real boost: 3.0
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    activeTimeout: 0.5
                    onWheel: (event) => {
                        const pd = event.pixelDelta
                        const ady = event.angleDelta.y
                        if (!(pd && pd.y) && ady === 0) { event.accepted = false; return }
                        const max = Math.max(0, content.implicitHeight - scrollArea.height)
                        // pixelDelta path (touchpads): pixels × boost. Wheel
                        // path: a full notch (120) scrolls 1/4 pane × boost.
                        const step = (pd && pd.y !== 0)
                            ? -pd.y * panelWheel.boost
                            : -(ady / 120) * scrollArea.height * 0.25 * panelWheel.boost
                        scrollArea.contentY = Math.max(0, Math.min(max, scrollArea.contentY + step))
                        event.accepted = true
                    }
                }

                    Column {
                        id: content
                    width: scrollArea.width
                    spacing: Style.space(12)

                   Row {
                       spacing: Style.space(8)

                       Rectangle {
                           width: 8; height: 8; radius: 4
                           anchors.verticalCenter: parent.verticalCenter
                           color: root.daemonStatus === "running"
                               ? (Color.accent)
                               : reconnectTimer.running ? "#e0af68" : (Color.urgent)
                       }

                       Text {
                           text: "Omarchy10k Control Center"
                           color: root.barForeground
                           font.family: root.bar ? root.bar.fontFamily : Style.font.family
                           font.pixelSize: Style.font.subtitle
                           font.bold: true
                       }

                       Rectangle {
                           width: backdropText.implicitWidth + Style.space(8)
                           height: backdropText.implicitHeight + Style.space(4)
                           radius: Style.space(3)
                           color: backdropMa.containsMouse ? (Color.accent) : "transparent"

                           Text {
                               id: backdropText
                               anchors.centerIn: parent
                               text: "◧ bg"
                               color: backdropMa.containsMouse
                                   ? (Color.background)
                                   : (root.barForeground)
                               font.family: root.bar ? root.bar.fontFamily : Style.font.family
                               font.pixelSize: Style.font.caption
                           }

                           MouseArea {
                               id: backdropMa
                               anchors.fill: parent
                               hoverEnabled: true
                               cursorShape: Qt.PointingHandCursor
                               onClicked: parent.parent.backdropMode = (parent.parent.backdropMode + 1) % 2
                           }
                       }

                       Rectangle {
                           width: undoText.implicitWidth + Style.space(8)
                           height: undoText.implicitHeight + Style.space(4)
                           radius: Style.cornerRadius
                           color: undoMa.containsMouse ? (Color.accent) : "transparent"
                           visible: root._undoStack.length > 0

                           Text {
                               id: undoText
                               anchors.centerIn: parent
                               text: "\u21A9 Undo"
                               color: undoMa.containsMouse
                                   ? (Color.background)
                                   : (Color.muted)
                               font.family: root.bar ? root.bar.fontFamily : Style.font.family
                               font.pixelSize: Style.font.caption
                           }

                           MouseArea {
                               id: undoMa
                               anchors.fill: parent
                               hoverEnabled: true
                               cursorShape: Qt.PointingHandCursor
                               onClicked: root.undoConfig()
                           }
                       }
                   }

                   Rectangle {
                       width: parent.width
                       height: previewContent.implicitHeight + Style.space(16)
                       radius: Style.cornerRadius
                       color: Qt.darker(Color.background, 1.5)
                       visible: root.previewText.length > 0 || !root._featureAvailable("0.3")

                       Column {
                           id: previewContent
                        Text {
                            text: root._featureAvailable("0.3")
                                ? root.previewText
                                : (root.daemonStatus === "running"
                                    ? "Live preview requires daemon v0.3+"
                                    : "Daemon not running — open a shell with the Omarchy10k prompt")
                            textFormat: Text.StyledText
                            color: root._featureAvailable("0.3")
                                ? (Color.foreground)
                                : (Color.muted)
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.body
                            font.italic: !root._featureAvailable("0.3")
                            elide: Text.ElideRight
                            width: parent.width
                        }

                           Row {
                               spacing: Style.space(4)

                               Repeater {
                                   model: [
                                       { label: "Error", prop: "previewError" },
                                       { label: "SSH", prop: "previewSsh" },
                                       { label: "Long cmd", prop: "previewLongCmd" }
                                   ]
                                   delegate: Rectangle {
                                       width: toggleLabel.implicitWidth + Style.spacing.controlPaddingX
                                       height: toggleLabel.implicitHeight + Style.space(6)
                                       radius: Style.cornerRadius
                                       color: root[modelData.prop]
                                           ? (Color.accent)
                                           : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

                                       Text {
                                           id: toggleLabel
                                           anchors.centerIn: parent
                                           text: modelData.label
                                           color: root[modelData.prop]
                                               ? (Color.background)
                                               : (Color.muted)
                                           font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                           font.pixelSize: Style.font.bodySmall
                                       }

                                       MouseArea {
                                           anchors.fill: parent
                                           cursorShape: Qt.PointingHandCursor
                                           onClicked: {
                                               root[modelData.prop] = !root[modelData.prop]
                                               root.requestPreview()
                                           }
                                       }
                                   }
                               }
                           }
                       }
                   }

                   Row {
                       id: tabBar
                       spacing: Style.space(4)
                       property int currentTab: 0

                       Repeater {
                            model: ["Looks", "Style", "Behavior", "System"]
                            delegate: Rectangle {
                                width: tabLabel.implicitWidth + Style.space(12)
                                height: tabLabel.implicitHeight + Style.space(8)
                                radius: Style.cornerRadius
                                color: tabBar.currentTab === index
                                    ? (Color.accent)
                                    : "transparent"

                                Text {
                                    id: tabLabel
                                    anchors.centerIn: parent
                                    text: modelData
                                    color: tabBar.currentTab === index
                                        ? (Color.background)
                                        : (root.barForeground || "#a9b1d6")
                                    font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                    font.pixelSize: Style.font.caption
                                    font.bold: tabBar.currentTab === index
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: tabBar.currentTab = index
                                }
                            }
                        }
                    }

                   PanelSeparator {
                       foreground: root.barForeground
                   }

                   Loader {
                       id: tabContent
                       width: parent.width
                       sourceComponent: {
                           switch (tabBar.currentTab) {
                               case 0: return looksTab
                               case 1: return appearanceTab
                               case 2: return behaviorTab
                               case 3: return systemTab
                           }
                       }
                   }

            }
        }
            }
            // Out-of-flow notices: anchored to the card bottom so appearing
            // toast/error text never resizes the panel or shifts the layout.
            Rectangle {
                visible: root._showError
                width: parent.width - Style.space(24)
                height: visible ? errorText.implicitHeight + Style.space(12) : 0
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Style.space(12)
                radius: Style.cornerRadius
                color: Color.urgent
                z: 10

                Text {
                    id: errorText
                    anchors.centerIn: parent
                    text: root.lastError
                    color: Color.background
                    font.family: root.bar ? root.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.caption
                    wrapMode: Text.WordWrap
                    width: parent.width - Style.space(16)
                }
            }

            Rectangle {
                visible: root._showToast
                width: toastText.implicitWidth + Style.space(24)
                height: visible ? toastText.implicitHeight + Style.space(12) : 0
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: Style.space(12)
                radius: Style.cornerRadius
                color: Color.accent
                opacity: root._showToast ? 1 : 0
                z: 10

                Behavior on opacity { NumberAnimation { duration: 300 } }

                Text {
                    id: toastText
                    anchors.centerIn: parent
                    text: root.toastMessage
                    color: Color.background
                    font.family: root.bar ? root.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.caption
                }
            }
    }

    // ── Tab: Appearance ────────────────────────────────────────────────────

    Component {
        id: looksTab
        Column {
            spacing: Style.space(12)

            SectionLabel { label: "Looks" }

            // Curated + user Looks. Card wiring to the daemon `looks` verbs
            // lands with the gallery overlay; the cards render names now.
            Grid {
                columns: 2
                spacing: Style.spacing.controlGap
                width: parent.width

                Repeater {
                    model: [
                        { name: "omnarchy", label: "Omnarchy" },
                        { name: "tokyo-rainbow", label: "Tokyo Rainbow" },
                        { name: "framed-gradient", label: "Framed Gradient" },
                        { name: "lean-pure", label: "Lean Pure" },
                        { name: "slanted-owl", label: "Slanted Owl" },
                        { name: "gruvbox-drift", label: "Gruvbox Drift" },
                        { name: "rose-classic", label: "Rosé Classic" },
                        { name: "polar-lean", label: "Polar Lean" }
                    ]
                    delegate: Rectangle {
                        width: (parent.width - Style.space(8)) / 2
                        height: lookLabel.implicitHeight + Style.spacing.panelGap
                        radius: Style.cornerRadius
                        color: Style.normalFillFor(root.barForeground, Color.accent, Color.urgent)

                        Text {
                            id: lookLabel
                            anchors.centerIn: parent
                            text: modelData.label
                            color: root.barForeground
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.body
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.applyLook(modelData.name)
                        }
                    }
                }
            }

            TextField {
                id: lookNameField
                width: parent.width
                placeholderText: "Name for the current Look…"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
                color: root.barForeground
                background: Rectangle {
                    radius: Style.cornerRadius
                    color: Style.normalFillFor(root.barForeground, Color.accent, Color.urgent)
                }
            }

            ActionButton {
                label: "Save current as Look"
                onClicked: root.saveLook(lookNameField.text)
            }

            ActionButton {
                label: "Expand gallery"
                onClicked: {
                    if (root.omarchyService && typeof root.omarchyService.openGallery === "function")
                        root.omarchyService.openGallery()
                    else
                        root.galleryRequested()
                }
            }

            PanelSeparator { foreground: root.barForeground }

            SectionLabel { label: "Identity" }

            Text {
                text: "Palette and theme fine-tuning live under Style."
                color: Color.muted
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.WordWrap
                width: parent.width
            }
        }
    }

    Component {
        id: appearanceTab
        Column {
            spacing: Style.space(10)

            // ── Style Gallery ──────────────────────────────────────────
            SectionLabel { label: "Style" }

            Grid {
                columns: 4
                spacing: Style.space(6)
                width: parent.width

                Repeater {
                    model: root.presetCards
                    delegate: Rectangle {
                        width: (parent.width - Style.space(18)) / 4
                        height: styleCardCol.implicitHeight + Style.spacing.panelGap
                        radius: Style.cornerRadius
                        color: root.cfgStylePreset === modelData.name
                            ? (Color.accent)
                            : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))
                        border.width: root.cfgStylePreset === modelData.name ? 2 : 0
                        border.color: Color.accent

                        Column {
                            id: styleCardCol
                            anchors.centerIn: parent
                            spacing: Style.space(2)

                            Text {
                                text: root.presetPreviews[modelData.name] || modelData.preview
                                textFormat: Text.StyledText
                                color: root.cfgStylePreset === modelData.name
                                    ? (Color.background)
                                    : (Color.foreground)
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                                elide: Text.ElideRight
                            }

                            Text {
                                text: modelData.name
                                color: root.cfgStylePreset === modelData.name
                                    ? (Color.background)
                                    : (root.barForeground || "#a9b1d6")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.bodySmall
                                font.bold: true
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                            }

                            Text {
                                text: modelData.desc
                                color: root.cfgStylePreset === modelData.name
                                    ? Qt.lighter(Color.background, 1.5)
                                    : (Color.muted)
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.setConfigValue("style.preset", modelData.name)
                                // Preset-controlled granular keys must follow the
                                // preset: _flushSave stamps every CONFIG_MAP key, so
                                // a stale frame/separator toggle from an earlier
                                // preset would silently override the new one.
                                var framed = modelData.name === "framed"
                                root.setConfigValue("style.frame.enabled", framed)
                                root.setConfigValue("style.frame.gap_char", framed ? "\u2500" : "")
                                root.setConfigValue("style.separators.left", "")
                                root.setConfigValue("style.separators.right", "")
                            }
                        }
                    }
                }
            }

            PanelSeparator { foreground: root.barForeground }

            // ── Glyph Pickers ──────────────────────────────────────────
            SectionLabel { label: "Glyphs" }

            

            

            

            

            

            GlyphRow {
                label: "Separator"
                configKey: "style.separators.left"
                currentValue: root.cfgSepLeft || "none"
                customHandler: function(key) {
                    var val = key === "none" ? "" : key
                    root.setConfigValue("style.separators.left", val)
                    root.setConfigValue("style.separators.right", val)
                }
                glyphs: [
                    { key: "none",           glyph: "\u2205",  label: "Default" },
                    { key: "powerline",      glyph: "\ue0b0",  label: "Arrow" },
                    { key: "powerline_thin", glyph: "\ue0b1",  label: "Thin" },
                    { key: "slanted",        glyph: "\ue0bc",  label: "Slant" },
                    { key: "round",          glyph: "\ue0b4",  label: "Round" },
                    { key: "trapezoid",      glyph: "\ue0d2",  label: "Trap" },
                    { key: "trapezoid_rev",  glyph: "\ue0d5",  label: "Trap\u00b7" },
                    { key: "flame",          glyph: "\ue0c0",  label: "Flame" },
                    { key: "dither",         glyph: "\ue0c4",  label: "Dither" },
                    { key: "vertical",       glyph: "\u2502",  label: "Bar" },
                    { key: "dot",            glyph: "\u00b7",  label: "Dot" },
                    { key: "diamond",        glyph: "\u25c6",  label: "Diamond" },
                    { key: "fade",           glyph: "\u2593\u2592\u2591",  label: "Fade" },
                    { key: "fade_rev",       glyph: "\u2591\u2592\u2593",  label: "Fade Rev" }
                ]
            }

            PanelSeparator { foreground: root.barForeground }

            // ── Frame Controls ─────────────────────────────────────────
            Text {
                text: "Frame & Layout"
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
            }

            ControlRow {
                label: "Frame Lines"
                value: root.cfgFrameEnabled ? "On" : "Off"
                options: ["On", "Off"]
                onChanged: function(val) { root.setConfigValue("style.frame.enabled", val === "On") }
            }

            ControlRow {
                visible: root.cfgFrameEnabled
                label: "Gap Fill"
                value: root.cfgFrameGapChar === "\u2500" ? "Line \u2500"
                     : root.cfgFrameGapChar === "\u00b7" ? "Dots \u00b7"
                     : root.cfgFrameGapChar === "\u22ef" ? "Ellipsis \u22ef"
                     : "None"
                options: ["Line \u2500", "Dots \u00b7", "Ellipsis \u22ef", "None"]
                onChanged: function(val) {
                    var ch = val.indexOf("\u2500") >= 0 ? "\u2500"
                           : val.indexOf("\u00b7") >= 0 ? "\u00b7"
                           : val.indexOf("\u22ef") >= 0 ? "\u22ef"
                           : ""
                    root.setConfigValue("style.frame.gap_char", ch)
                }
            }

            

            

            

            PanelSeparator { foreground: root.barForeground }

            // ── Palette ─────────────────────────────────────────────────
            Text {
                text: "Palette"
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
            }

            Grid {
                columns: 4
                spacing: Style.space(6)
                width: parent.width

                Repeater {
                    model: {
                        var cards = [{ key: "theme", label: "Omarchy Theme", p: null }]
                        var keys = Object.keys(Model.CURATED_PALETTES)
                        for (var i = 0; i < keys.length; i++) {
                            cards.push({ key: keys[i], label: Model.CURATED_PALETTES[keys[i]].label, p: Model.CURATED_PALETTES[keys[i]] })
                        }
                        return cards
                    }
                    delegate: Rectangle {
                        id: palCard
                        property string palKey: modelData.key
                        property var pal: modelData.p
                        readonly property bool active: root.cfgPalette === palKey

                        width: (parent.width - Style.space(18)) / 4
                        height: palCardCol.implicitHeight + Style.spacing.panelGap
                        radius: Style.cornerRadius
                        color: active ? (Color.accent) : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))
                        border.width: active ? 2 : 0
                        border.color: Color.accent

                        Column {
                            id: palCardCol
                            anchors.centerIn: parent
                            spacing: Style.space(4)

                            Row {
                                spacing: 2
                                anchors.horizontalCenter: parent.horizontalCenter
                                Repeater {
                                    model: pal ? ["accent", "red", "green", "yellow", "blue"] : []
                                    delegate: Rectangle {
                                        required property string modelData
                                        width: Style.space(11)
                                        height: Style.space(11)
                                        radius: Style.cornerRadius
                                        color: palCard.p ? palCard.p[modelData] : "transparent"
                                    }
                                }
                            }

                            Text {
                                text: modelData.label
                                color: active ? (Color.background) : (root.barForeground || "#a9b1d6")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.bodySmall
                                font.bold: active
                                anchors.horizontalCenter: parent.horizontalCenter
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.applyPalette(palCard.palKey)
                        }
                    }
                }
            }

            PanelSeparator { foreground: root.barForeground }

            // ── Theme ──────────────────────────────────────────────────
            Text {
                text: "Theme"
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
            }

            ControlRow {
                label: "Source"
configKey: "theme.source"
                value: root.cfgThemeSource
                options: ["omarchy", "custom", "hybrid", "terminal"]
                onChanged: function(val) {
                    root.setConfigValue("theme.source", val)
                    root.requestPalette()
                }
            }

            Row {
                spacing: Style.space(3)
                visible: Object.keys(root.paletteColors).length > 0 || !root._featureAvailable("0.3")
                Repeater {
                    model: ["accent", "foreground", "muted", "background", "red", "green", "yellow", "blue"]
                    delegate: Column {
                        spacing: 1
                        Rectangle {
                            width: 20; height: 20; radius: Style.cornerRadius
                            color: root.paletteColors[modelData] || "#333"
                            border.width: 1
                            border.color: Color.muted
                        }
                        Text {
                            text: modelData.charAt(0).toUpperCase()
                            color: Color.muted
                            font.pixelSize: 8
                            horizontalAlignment: Text.AlignHCenter
                            width: 20
                        }
                    }
                }
            }
        }
    }

    // ── Tab: Behavior ──────────────────────────────────────────────────────

    Component {
        id: behaviorTab
        Column {
            spacing: Style.space(12)

            SectionLabel { label: "Prompt" }

            ControlRow {
                            label: "Lines"
                            value: root.cfgNewline ? "Two-line" : "One-line"
                            options: ["Two-line", "One-line"]
                            onChanged: function(val) { root.setConfigValue("prompt.newline", val === "Two-line") }
                        }

            ControlRow {
                            label: "Spacer"
                            value: root.cfgBlankLine ? "On" : "Off"
                            options: ["On", "Off"]
                            onChanged: function(val) { root.setConfigValue("prompt.blank_line", val === "On") }
                        }

            ControlRow {
                            label: "Transient"
                            value: root.cfgTransient ? "On" : "Off"
                            options: ["On", "Off"]
                            onChanged: function(val) { root.setConfigValue("prompt.transient", val === "On") }
                        }

            PanelSeparator { foreground: root.barForeground }

            SectionLabel { label: "Glyphs" }

            GlyphRow {
                            label: "Prompt Char"
                            configKey: "segments.character.success"
                            currentValue: root.cfgCharSuccess
                            customHandler: function(key) {
                                root.setConfigValue("segments.character.success", key)
                                root.setConfigValue("segments.character.error", key)
                                root.setConfigValue("segments.character.transient", key)
                            }
                            glyphs: [
                                { key: "\u276f",  glyph: "\u276f",  label: "Chevron" },
                                { key: "\u279c",  glyph: "\u279c",  label: "Arrow" },
                                { key: "\u03bb",  glyph: "\u03bb",  label: "Lambda" },
                                { key: "$",       glyph: "$",       label: "Dollar" },
                                { key: ">",       glyph: ">",       label: "Angle" },
                                { key: "%",       glyph: "%",       label: "Percent" },
                                { key: "\u25b6",  glyph: "\u25b6",  label: "Triangle" },
                                { key: "#",       glyph: "#",       label: "Hash" }
                            ]
                        }

            GlyphRow {
                            label: "Animals"
                            configKey: "segments.character.success"
                            currentValue: root.cfgCharSuccess
                            customHandler: function(key) {
                                root.setConfigValue("segments.character.success", key)
                                root.setConfigValue("segments.character.error", key)
                                root.setConfigValue("segments.character.transient", key)
                            }
                            glyphs: [
                                { key: "cat",          glyph: "\uf0b58", label: "Cat" },
                                { key: "penguin",      glyph: "\uf0752", label: "Penguin" },
                                { key: "fox",          glyph: "\uf0f86", label: "Fox" },
                                { key: "owl",          glyph: "\uf1041", label: "Owl" },
                                { key: "duck",         glyph: "\uf095f", label: "Duck" },
                                { key: "butterfly",    glyph: "\uf10a9", label: "Fly" },
                                { key: "ladybug",      glyph: "\uf0828", label: "Ladybug" },
                                { key: "bee",          glyph: "\uf0fa1", label: "Bee" },
                                { key: "dog",          glyph: "\uf094c", label: "Dog" },
                                { key: "rabbit",       glyph: "\uf0810", label: "Rabbit" },
                                { key: "turtle",       glyph: "\uf0be0", label: "Turtle" },
                                { key: "paw",          glyph: "\uf02f2", label: "Paw" },
                                { key: "fish",         glyph: "\uf0143", label: "Fish" },
                                { key: "frog",         glyph: "\ued01",  label: "Frog" },
                                { key: "dragon",       glyph: "\uee01",  label: "Dragon" },
                                { key: "panda",        glyph: "\uf02e3", label: "Panda" },
                                { key: "koala",        glyph: "\uf1648", label: "Koala" },
                                { key: "unicorn",      glyph: "\uf14cb", label: "Unicorn" },
                                { key: "teddy",        glyph: "\uf1804", label: "Teddy" },
                                { key: "cow",          glyph: "\uf01e4", label: "Cow" },
                                { key: "horse",        glyph: "\uf0f12", label: "Horse" },
                                { key: "pig",          glyph: "\uf1045", label: "Pig" },
                                { key: "sheep",        glyph: "\uf1077", label: "Sheep" }
                            ]
                        }

            GlyphRow {
                            label: "Kaomoji"
                            configKey: "segments.character.success"
                            currentValue: root.cfgCharSuccess
                            customHandler: function(key) {
                                root.setConfigValue("segments.character.success", key)
                                root.setConfigValue("segments.character.error", key)
                                root.setConfigValue("segments.character.transient", key)
                            }
                            glyphs: [
                                { key: "kaomoji_bear",       glyph: "ʕ•ᴥ•ʔ",   label: "Bear" },
                                { key: "kaomoji_smile",      glyph: "(◕‿◕)",   label: "Smile" },
                                { key: "kaomoji_rage",       glyph: "(╯°□°)╯", label: "Rage" },
                                { key: "kaomoji_relaxed",    glyph: "ヽ(´ー`)ノ", label: "Relaxed" },
                                { key: "kaomoji_smirk",      glyph: "(¬‿¬)",   label: "Smirk" },
                                { key: "kaomoji_disapprove", glyph: "ಠ_ಠ",     label: "No" }
                            ]
                        }

            GlyphRow {
                            label: "OS Icon"
                            configKey: "segments.os.icon"
                            currentValue: root.cfgOsIcon
                            glyphs: [
                                { key: "arch",    glyph: "\uf303",  label: "Arch" },
                                { key: "ubuntu",  glyph: "\uf31b",  label: "Ubuntu" },
                                { key: "debian",  glyph: "\uf306",  label: "Debian" },
                                { key: "fedora",  glyph: "\uf30a",  label: "Fedora" },
                                { key: "nixos",   glyph: "\uf313",  label: "NixOS" },
                                { key: "macos",   glyph: "\uf179",  label: "macOS" },
                                { key: "windows", glyph: "\uf17a",  label: "Win" },
                                { key: "linux",   glyph: "\uf17c",  label: "Linux" },
                                { key: "omarchy", glyph: "\uf312",  label: "Omarchy" },
                                { key: "alpine",  glyph: "\uf300",  label: "Alpine" },
                                { key: "void",    glyph: "\uf32e",  label: "Void" },
                                { key: "gentoo",  glyph: "\uf30d",  label: "Gentoo" },
                                { key: "none",    glyph: "\u2205",  label: "None" }
                            ]
                        }

            GlyphRow {
                            label: "Git Icon"
                            configKey: "git.branch_icon"
                            currentValue: root.cfgGitBranchIcon
                            glyphs: [
                                { key: "powerline", glyph: "\ue0a0",  label: "Powerline" },
                                { key: "octicon",   glyph: "\uf418",  label: "Octicon" },
                                { key: "nerd",      glyph: "\uf126",  label: "Nerd" },
                                { key: "text",      glyph: "git:",    label: "Text" },
                                { key: "none",      glyph: "\u2205",  label: "None" }
                            ]
                        }

            PanelSeparator { foreground: root.barForeground }

            SectionLabel { label: "Context" }

            ControlRow {
                            label: "Git"
                            value: root.cfgGitMode
                            options: ["adaptive", "compact", "expanded", "hidden"]
                            onChanged: function(val) { root.setConfigValue("git.mode", val) }
                        }

            ControlRow {
                            label: "Duration"
                            value: root.cfgCmdDurationMs + "ms"
                            options: ["500ms", "1000ms", "1500ms", "3000ms", "5000ms"]
                            onChanged: function(val) {
                                var ms = parseInt(val)
                                root.setConfigValue("segments.command_duration.show_above_ms", ms)
                            }
                        }

            ControlRow {
                            label: "SSH"
                            value: root.cfgSshShow
                            options: ["auto", "always", "never"]
                            onChanged: function(val) { root.setConfigValue("segments.ssh.show", val) }
                        }

            ControlRow {
                            label: "Exit Status"
                            value: root.cfgExitSignalNames ? "Signal names" : "Codes only"
                            options: ["Signal names", "Codes only"]
                            onChanged: function(val) {
                                root.setConfigValue("segments.exit_status.show_signal_name", val === "Signal names")
                            }

                        Text {
                            text: "Toggle prompt segments on or off."
                            color: Color.muted
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            wrapMode: Text.WordWrap
                            width: parent.width
                        }

                        Grid {
                            columns: 2
                            spacing: Style.space(6)
                            width: parent.width

                            Repeater {
                                model: [
                                    { label: "Container", key: "segments.container.enabled", prop: "cfgContainerEnabled" },
                                    { label: "Python", key: "segments.python.enabled", prop: "cfgPythonEnabled" },
                                    { label: "Toolchain", key: "segments.toolchain.enabled", prop: "cfgToolchainEnabled" },
                                    { label: "Nix", key: "segments.nix.enabled", prop: "cfgNixEnabled" },
                                    { label: "Kubernetes", key: "segments.k8s.enabled", prop: "cfgK8sEnabled" },
                                    { label: "Time", key: "segments.time.enabled", prop: "cfgTimeEnabled" },
                                    { label: "Load", key: "segments.load.enabled", prop: "cfgLoadEnabled" },
                                    { label: "Battery", key: "segments.battery.enabled", prop: "cfgBatteryEnabled" },
                                    { label: "Terminal Title", key: "terminal.title.enabled", prop: "cfgTitleEnabled" }
                                ]
                                delegate: Rectangle {
                                    width: (parent.width - Style.space(6)) / 2
                                    height: segLabel.implicitHeight + Style.spacing.panelGap
                                    visible: root.searchQuery.length === 0
                                        || modelData.label.toLowerCase().indexOf(root.searchQuery.toLowerCase()) >= 0
                                    radius: Style.cornerRadius
                                    color: root[modelData.prop]
                                        ? (Color.accent)
                                        : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

                                    Text {
                                        id: segLabel
                                        anchors.centerIn: parent
                                        text: modelData.label
                                        color: root[modelData.prop]
                                            ? (Color.background)
                                            : (root.barForeground || "#a9b1d6")
                                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                        font.pixelSize: Style.font.caption
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: root.setConfigValue(modelData.key, !root[modelData.prop])
                                    }
                                }
                            }
                        }

                        ControlRow {
                            label: "Time Format"
configKey: "segments.time.format"
                            visible: root.cfgTimeEnabled
                            value: root.cfgTimeFormat === "%H:%M" ? "HH:MM"
                                 : root.cfgTimeFormat === "%H:%M:%S" ? "HH:MM:SS"
                                 : root.cfgTimeFormat === "%I:%M %p" ? "hh:mm AM/PM"
                                 : "HH:MM"
                            options: ["HH:MM", "HH:MM:SS", "hh:mm AM/PM"]
                            onChanged: function(val) {
                                var fmt = val === "HH:MM:SS" ? "%H:%M:%S"
                                        : val === "hh:mm AM/PM" ? "%I:%M %p"
                                        : "%H:%M"
                                root.setConfigValue("segments.time.format", fmt)
                            }
                        }
                    }

            PanelSeparator { foreground: root.barForeground }

            SectionLabel { label: "Segments" }

            Grid {
                            columns: 2
                            spacing: Style.spacing.controlGap
                            width: parent.width

                            Repeater {
                                model: [
                                    { name: "omnarchy", label: "Omnarchy" },
                                    { name: "tokyo-rainbow", label: "Tokyo Rainbow" },
                                    { name: "framed-gradient", label: "Framed Gradient" },
                                    { name: "lean-pure", label: "Lean Pure" },
                                    { name: "slanted-owl", label: "Slanted Owl" },
                                    { name: "gruvbox-drift", label: "Gruvbox Drift" },
                                    { name: "rose-classic", label: "Rosé Classic" },
                                    { name: "polar-lean", label: "Polar Lean" }
                                ]
                                delegate: Rectangle {
                                    width: (parent.width - Style.space(8)) / 2
                                    height: lookLabel.implicitHeight + Style.spacing.panelGap
                                    radius: Style.cornerRadius
                                    color: Style.normalFillFor(root.barForeground, Color.accent, Color.urgent)

                                    Text {
                                        id: lookLabel
                                        anchors.centerIn: parent
                                        text: modelData.label
                                        color: root.barForeground
                                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                        font.pixelSize: Style.font.body
                                    }

                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: root.setConfigValue("style.preset", modelData.name)
                                    }
                                }
                            }
                        }

            ControlRow {
                            label: "Time Format"
                            visible: root.cfgTimeEnabled
                            value: root.cfgTimeFormat === "%H:%M" ? "HH:MM"
                                 : root.cfgTimeFormat === "%H:%M:%S" ? "HH:MM:SS"
                                 : root.cfgTimeFormat === "%I:%M %p" ? "hh:mm AM/PM"
                                 : "HH:MM"
                            options: ["HH:MM", "HH:MM:SS", "hh:mm AM/PM"]
                            onChanged: function(val) {
                                var fmt = val === "HH:MM:SS" ? "%H:%M:%S"
                                        : val === "hh:mm AM/PM" ? "%I:%M %p"
                                        : "%H:%M"
                                root.setConfigValue("segments.time.format", fmt)
                            }
                        }

            PanelSeparator { foreground: root.barForeground }

            SectionLabel { label: "Notifications" }

            ControlRow {
                            label: "Notify After"
                            value: root.cfgNotifyThresholdMs === 5000 ? "5s"
                                 : root.cfgNotifyThresholdMs === 10000 ? "10s"
                                 : root.cfgNotifyThresholdMs === 30000 ? "30s"
                                 : root.cfgNotifyThresholdMs + "ms"
                            options: ["5s", "10s", "30s"]
                            onChanged: function(val) {
                                var ms = val === "5s" ? 5000 : val === "30s" ? 30000 : 10000
                                root.setConfigValue("segments.notification.threshold_ms", ms)
                            }
                        }
        }
    }

    // ── Tab: System ────────────────────────────────────────────────────────

    Component {
        id: systemTab
        Column {
            spacing: Style.space(12)

Text {
                text: "Shell integrations are configured through their own tools.\nOmarchy10k coordinates their lifecycle via the hook broker."
                color: Color.muted
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.WordWrap
                width: parent.width
            }

            StatusRow { label: "ble.sh"; status: root.bleshStatus }
            StatusRow { label: "Atuin"; status: root.atuinStatus }

            ActionButton {
                label: "Install Atuin"
                visible: root.atuinStatus.indexOf("\u2717") >= 0
                onClicked: {
                    installRunner.exec(["sh", "-c", "curl --proto '=https' --tlsv1.2 -sSf https://setup.atuin.sh | bash"])
                }
            }

            StatusRow { label: "Mise"; status: root.miseStatus }

            ActionButton {
                label: "Install Mise"
                visible: root.miseStatus.indexOf("\u2717") >= 0
                onClicked: {
                    installRunner.exec(["sh", "-c", "curl https://mise.run | sh"])
                }
            }

            StatusRow { label: "Zoxide"; status: root.zoxideStatus }
            StatusRow { label: "fzf"; status: root.fzfStatus }

            PanelSeparator { foreground: root.barForeground }


            Rectangle {
                width: parent.width
                height: daemonInfo.implicitHeight + Style.space(12)
                radius: Style.cornerRadius
                color: Qt.darker(Color.background, 1.3)

                Column {
                    id: daemonInfo
                    anchors.fill: parent
                    anchors.margins: Style.space(8)
                    spacing: Style.space(4)

                    Text {
                        text: "Daemon: " + root.daemonStatus
                        color: root.daemonStatus === "running"
                            ? (Color.accent)
                            : (Color.urgent)
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "PID: " + (root.daemonPid || "\u2014")
                        color: Color.muted
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "Version: " + (root.daemonVersion || "\u2014") + " (protocol " + (root.daemonProtocolVersion || "\u2014") + ")"
                        color: Color.muted
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: root.daemonProtocolVersion
                            ? ("Protocol status: " + (root._featureAvailable("0.3") ? "full (v0.3+)" : "degraded (upgrade daemon)"))
                            : "Protocol status: unknown"
                        color: root._featureAvailable("0.3")
                            ? (Color.accent)
                            : (Color.muted)
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "Sessions: " + root.sessionList.length
                        color: Color.muted
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }
            }

            Repeater {
                model: root.sessionList
                delegate: Rectangle {
                    width: parent.width
                    height: Style.spacing.controlHeight
                    radius: Style.cornerRadius
                    color: index === root.activeSessionIndex
                        ? (Color.accent)
                        : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.connectToSession(index)
                    }

                    Row {
                        id: sessionRow
                        anchors.fill: parent
                        anchors.margins: Style.space(4)
                        spacing: Style.space(8)

                        Text {
                            id: sessionPidText
                            text: "Shell " + modelData.shellPid
                            color: index === root.activeSessionIndex
                                ? (Color.background)
                                : (root.barForeground || "#a9b1d6")
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            font.bold: index === root.activeSessionIndex
                        }
                        Text {
                            text: modelData.cwd || ""
                            color: Color.muted
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            elide: Text.ElideMiddle
                            // Stop short of the floating terminal button on the right.
                            width: parent.width - sessionPidText.implicitWidth - Style.space(40)
                        }
                    }

                    Rectangle {
                        width: 24; height: 24; radius: Style.cornerRadius
                        anchors.right: parent.right
                        anchors.rightMargin: Style.space(4)
                        anchors.verticalCenter: parent.verticalCenter
                        z: 2
                        color: termMa.containsMouse ? (Color.accent) : "transparent"
                        visible: modelData.cwd.length > 0

                        Text {
                            anchors.centerIn: parent
                            text: "\uf120"
                            color: termMa.containsMouse
                                ? (Color.background)
                                : (Color.muted)
                            font.pixelSize: 12
                        }

                        MouseArea {
                            id: termMa
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                var safeCwd = modelData.cwd.replace(/'/g, "'\\''")
                                floatingTermLauncher.command = ["sh", "-c",
                                    "cd '" + safeCwd + "' && exec ${SHELL:-bash}"]
                                floatingTermLauncher.startDetached()
                            }
                        }
                    }
                }
            }


            ActionButton {
                label: "Open Config File"
                onClicked: {
                    editorLauncher.command = ["sh", "-c",
                        "${TERMINAL:-foot} -e sh -c '${EDITOR:-nano} \"" + root._configPath + "\"'"]
                    editorLauncher.startDetached()
                }
            }

            ActionButton {
                label: "Run Doctor"
                onClicked: doctorRunner.exec(["omarchy10k", "doctor"])
            }

            Row {
                spacing: Style.space(6)
                width: parent.width

                ActionButton {
                    label: "Copy Config"
                    width: (parent.width - Style.space(6)) / 2
                    onClicked: {
                        var toml = Model.buildTOML(Model.collectConfig(root))
                        clipboardCopy.exec(["sh", "-c",
                            "echo '" + toml.replace(/'/g, "'\\''") + "' | xclip -selection clipboard 2>/dev/null || wl-copy 2>/dev/null"])
                        root.toastMessage = "Config copied to clipboard"
                        root._showToast = true
                        toastTimer.restart()
                    }
                }

                ActionButton {
                    label: "Paste Config"
                    width: (parent.width - Style.space(6)) / 2
                    onClicked: {
                        clipboardPaste.exec(["sh", "-c", "xclip -selection clipboard -o 2>/dev/null || wl-paste 2>/dev/null"])
                    }
                }
            }

            Rectangle {
                visible: root.doctorOutput.length > 0
                width: parent.width
                height: Math.min(doctorText.implicitHeight + Style.space(12), 200)
                radius: Style.cornerRadius
                color: Qt.darker(Color.background, 1.3)
                clip: true

                Flickable {
                    anchors.fill: parent
                    anchors.margins: Style.space(6)
                    contentHeight: doctorText.implicitHeight
                    flickableDirection: Flickable.VerticalFlick

                    TextEdit {
                        id: doctorText
                        width: parent.width
                        text: root.doctorOutput
                        color: Color.foreground
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                    }
                }
            }

            ActionButton {
                label: "Reload Config"
                onClicked: {
                    if (daemonSocket.connected) {
                        root.sendDaemonCommand("reload_config")
                        Qt.callLater(root.loadConfig)
                    } else {
                        root.loadConfig()
                    }
                }
            }

            ActionButton {
                label: "Run Benchmark"
                onClicked: {
                    root.benchmarkOutput = "Running..."
                    benchRunner.exec(["omarchy10k", "benchmark", "--iterations", "50"])
                }
            }

            Rectangle {
                visible: root.benchmarkOutput.length > 0
                width: parent.width
                height: Math.min(benchText.implicitHeight + Style.space(12), 150)
                radius: Style.cornerRadius
                color: Qt.darker(Color.background, 1.3)
                clip: true

                Flickable {
                    anchors.fill: parent
                    anchors.margins: Style.space(6)
                    contentHeight: benchText.implicitHeight
                    flickableDirection: Flickable.VerticalFlick

                    TextEdit {
                        id: benchText
                        width: parent.width
                        text: root.benchmarkOutput
                        color: Color.foreground
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                    }
                }
            }

            ActionButton {
                label: "Reset to Defaults"
                dangerous: true
                                onClicked: {
                    // A pending delta would resurrect the pre-reset keys the
                    // moment saveTimer fires after the file is deleted.
                    root._configDirty = false
                    root._dirtyKeys = {}
                    resetProc.exec(["sh", "-c",
                        "cp '" + root._configPath + "' '" + root._configPath + ".bak' 2>/dev/null; " +
                        "rm -f '" + root._configPath + "'"])
                }
            }
        }
    }

    // ── Reset Process ──────────────────────────────────────────────────────

    Process {
        id: resetProc
        onRunningChanged: {
            if (!running) {
                if (daemonSocket.connected) {
                    root.sendDaemonCommand("reload_config")
                    Qt.callLater(root.loadConfig)
                } else {
                    root.loadConfig()
                }
                root._undoStack = []
            }
        }

    // ── Reusable Components ────────────────────────────────────────────────

    component ControlRow: Row {
        property string label
        property string value
        property var options: []
        property string configKey
        signal changed(string val)

        // Modified-vs-default: accent ink bar on the left edge + reset chip
        // after the options, both only when this row's key diverges from the
        // daemon's defaults snapshot.
        readonly property bool modified: configKey.length > 0 && root.isModified(configKey)

        width: parent ? parent.width : 200
        spacing: Style.space(8)

        Rectangle {
            width: 3
            height: parent.height
            radius: 1
            visible: parent.modified
            color: Color.accent
        }

        Text {
            width: parent.width * 0.35
            text: label
            color: root.barForeground || "#a9b1d6"
            font.family: root.bar ? root.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
            verticalAlignment: Text.AlignVCenter
            height: parent.height
        }

        Row {
            spacing: Style.spacing.controlGap
            Repeater {
                model: options
                delegate: Rectangle {
                    width: optText.implicitWidth + Style.spacing.controlPaddingX * 2
                    height: Style.spacing.controlHeight
                    radius: Style.cornerRadius
                    color: value === modelData
                        ? (Color.accent)
                        : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

                    Text {
                        id: optText
                        anchors.centerIn: parent
                        text: modelData
                        color: value === modelData
                            ? (Color.background)
                            : (root.barForeground || "#a9b1d6")
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: changed(modelData)
                    }
                }
            }
        }

        Rectangle {
            width: Style.spacing.controlHeight * 0.8
            height: Style.spacing.controlHeight
            radius: Style.cornerRadius
            visible: parent.modified
            color: Style.normalFillFor(root.barForeground, Color.accent, Color.urgent)

            Text {
                anchors.centerIn: parent
                text: "\u21ba"
                color: Color.muted
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.resetConfigKey(parent.parent.configKey)
            }
        }
    }

    component StatusRow: Row {
        property string label
        property string status

        width: parent ? parent.width : 200
        spacing: Style.space(8)

        Text {
            width: parent.width * 0.35
            text: label
            color: root.barForeground || "#a9b1d6"
            font.family: root.bar ? root.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }
        Text {
            text: status
            color: status.indexOf("\u2713") >= 0
                ? (Color.accent)
                : (Color.muted)
            font.family: root.bar ? root.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }
    }

    component ActionButton: Rectangle {
        property string label
        property bool dangerous: false
        signal clicked()

        width: parent ? parent.width : 200
        height: Style.spacing.controlHeight
        radius: Style.cornerRadius
        color: mouseArea.containsMouse
            ? (dangerous ? (Color.urgent) : (Color.accent))
            : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

        Text {
            id: btnText
            anchors.centerIn: parent
            text: label
            color: mouseArea.containsMouse
                ? (Color.background)
                : (root.barForeground || "#a9b1d6")
            font.family: root.bar ? root.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }

        MouseArea {
            id: mouseArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Small-caps style section marker — quieter than a bold body label,
    // consistent across every tab.
    component SectionLabel: Text {
        property string label
        text: label.toUpperCase()
        color: Color.muted
        font.family: root.bar ? root.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.caption
        font.bold: true
        font.letterSpacing: 1.4
    }

    component GlyphRow: Column {
        property string label
        property string configKey
        property string currentValue
        property var glyphs: []
        property var customHandler: null

        spacing: Style.space(4)
        width: parent ? parent.width : 200

        Row {
            spacing: Style.space(8)
            width: parent.width

            Text {
                id: glyphLabel
                width: parent.width * 0.26
                text: label
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                verticalAlignment: Text.AlignVCenter
                height: glyphFlow.height
            }

            Flow {
                id: glyphFlow
                width: parent.width - glyphLabel.width - Style.space(8)
                spacing: Style.spacing.controlGap

                Repeater {
                    model: glyphs
                    delegate: Rectangle {
                        // Size from the leaf Text metrics directly: sizing via
                        // the Column's implicit size while the Column anchors
                        // centerIn the delegate created a polish() loop that
                        // made panel scrolling crawl.
                        id: chip
                        implicitWidth: glyphGlyph.implicitWidth + Style.space(4) + glyphChipLabel.implicitWidth + Style.spacing.controlPaddingX * 2
                        implicitHeight: Style.spacing.controlHeight
                        radius: Style.cornerRadius
                        color: currentValue === modelData.key
                            ? (Color.accent)
                            : (Style.normalFillFor(root.barForeground, Color.accent, Color.urgent))

                        Row {
                            anchors.centerIn: parent
                            spacing: Style.space(4)

                            Text {
                                id: glyphGlyph
                                text: modelData.glyph
                                color: currentValue === modelData.key
                                    ? (Color.background)
                                    : (Color.foreground)
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.body
                            }

                            Text {
                                id: glyphChipLabel
                                text: modelData.label
                                color: currentValue === modelData.key
                                    ? Qt.lighter(Color.background, 1.4)
                                    : (Color.muted)
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (customHandler) {
                                    customHandler(modelData.key)
                                } else {
                                    root.setConfigValue(configKey, modelData.key)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
}
