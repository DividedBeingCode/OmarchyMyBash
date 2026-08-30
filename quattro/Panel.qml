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
    // Collapsed/raw surface toggles for the Doctor and Benchmark cards.
    property bool _doctorRaw: false
    property bool _benchRaw: false
    // Parsed views: doctor → one row per subsystem line; bench → numeric ms
    // values found in the benchmark text (null while running/absent).
    readonly property var doctorCards: doctorOutput.length > 0 ? _parseDoctorCards(doctorOutput) : []
    readonly property var benchStats: benchmarkOutput.length > 0 ? _parseBenchStats(benchmarkOutput) : null
    // Not-installed toolDetector tools that get a remediation card.
    readonly property var missingTools: _collectMissingTools()

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

    // ── Doctor Card Parsing ────────────────────────────────────────────────
    // Doctor output is one subsystem per line: 2-space indent, padded name
    // column, then a status column with a ✓/✘/⚠/?/- marker. Lines that don't
    // match the shape are skipped here but stay reachable via the raw toggle.
    function _parseDoctorCards(text) {
        var cards = []
        var lines = String(text).split("\n")
        for (var i = 0; i < lines.length; i++) {
            var line = lines[i].replace(/ +$/, "")
            if (!/^ {2,}\S/.test(line)) continue
            var head = line.replace(/^ +/, "")
            var split = head.match(/^(.*?) {2,}(.*)$/)
            var name = split ? split[1] : head
            var rest = split ? split[2] : ""
            var status = "skip"
            var glyph = "-"
            if (rest.indexOf("\u2713") >= 0) {
                status = "ok"
                glyph = "\u2713"
            } else if (rest.indexOf("\u2718") >= 0 || rest.indexOf("\u2717") >= 0) {
                status = "bad"
                glyph = "\u2718"
            } else if (rest.indexOf("\u26a0") >= 0) {
                status = "bad"
                glyph = "\u26a0"
            } else if (rest.indexOf("?") >= 0) {
                glyph = "?"
            }
            var detail = rest
                .replace(/[\u2713\u2717\u2718\u26a0]/g, "")
                .replace(/(^|\s)\?(?=\s|\()/g, "$1")
                .replace(/(^|\s)-(?=\s|$)/g, "$1")
                .replace(/ {2,}/g, " ")
                .trim()
            // print_tool_check prints a bare trailing "-" for a missing tool.
            if (detail.length === 0 || detail === "-")
                detail = status === "skip" && glyph === "-" ? "not installed" : "\u2014"
            cards.push({ name: name, detail: detail, status: status, glyph: glyph })
        }
        return cards
    }

    // ── Benchmark Parsing ──────────────────────────────────────────────────
    // Collect every "N.NNms" value in the benchmark text (summary stats and
    // any per-iteration lines) and derive a compact braille sparkline plus
    // best/median/worst. Returns null when nothing numeric has arrived yet.
    function _parseBenchStats(text) {
        var vals = []
        var re = /([0-9]+\.[0-9]+)\s*ms/g
        var m
        var s = String(text)
        while ((m = re.exec(s)) !== null) {
            var v = parseFloat(m[1])
            if (isFinite(v)) vals.push(v)
        }
        if (vals.length === 0) return null
        var sorted = vals.slice().sort(function (a, b) { return a - b })
        var best = sorted[0]
        var worst = sorted[sorted.length - 1]
        var median = sorted[Math.floor((sorted.length - 1) / 2)]
        // One braille cell per value; the dot column rises with the value so
        // the sparkline reads left-to-right like the recorded sequence.
        var levels = ["\u2801", "\u2803", "\u2807", "\u2847"]
        var span = worst - best
        var spark = ""
        for (var i = 0; i < vals.length; i++) {
            var level = span > 0
                ? Math.min(4, Math.max(1, 1 + Math.round((vals[i] - best) / span * 3)))
                : 2
            spark += levels[level - 1]
        }
        return { spark: spark, best: best, median: median, worst: worst, n: vals.length }
    }

    function _fmtMs(v) {
        return Number(v).toFixed(2) + "ms"
    }

    // ── Remediation Cards ──────────────────────────────────────────────────
    // One card per toolDetector-tracked tool that detection reported missing.
    // Cards only appear once detection answered (✗ marker) — never while a
    // check is still "checking...". Nothing is auto-installed; COPY hands the
    // exact command to the clipboard.
    function _collectMissingTools() {
        var defs = [
            { name: "ble.sh", why: "Optional — enables enhanced line editing",
              cmd: "git clone --depth=1 https://github.com/akinomyoga/ble.sh.git && make -C ble.sh install PREFIX=~/.local",
              status: root.bleshStatus },
            { name: "Atuin", why: "Synced, searchable shell history",
              cmd: "curl --proto '=https' --tlsv1.2 -sSf https://setup.atuin.sh | bash",
              status: root.atuinStatus },
            { name: "Mise", why: "Per-project dev-tool version manager",
              cmd: "curl https://mise.run | sh",
              status: root.miseStatus },
            { name: "Zoxide", why: "Faster directory jumping (z command)",
              cmd: "sudo pacman -S --noconfirm zoxide",
              status: root.zoxideStatus },
            { name: "fzf", why: "Fuzzy finder for history and file search",
              cmd: "sudo pacman -S --noconfirm fzf",
              status: root.fzfStatus }
        ]
        var out = []
        for (var i = 0; i < defs.length; i++) {
            if (defs[i].status.indexOf("\u2717") >= 0)
                out.push({ name: defs[i].name, why: defs[i].why, cmd: defs[i].cmd })
        }
        return out
    }

    function copyInstallCommand(toolName, cmd) {
        clipboardCopy.exec(["sh", "-c",
            "echo '" + String(cmd).replace(/'/g, "'\\''") + "' | xclip -selection clipboard 2>/dev/null || wl-copy 2>/dev/null"])
        root.toastMessage = "Install command copied — " + toolName
        root._showToast = true
        toastTimer.restart()
    }

    // ── Bucket Actions (B3 decomposition) ──────────────────────────────────
    // The System bucket (PanelSystem.qml) triggers daemon/OS actions through
    // these wrappers so the panel's Process and Socket ids never leak into
    // the extracted bucket files. Bodies are verbatim from the pre-split
    // inline handlers.
    function openFloatingTerminal(cwd) {
        var safeCwd = cwd.replace(/'/g, "'\\''")
        floatingTermLauncher.command = ["sh", "-c",
            "cd '" + safeCwd + "' && exec ${SHELL:-bash}"]
        floatingTermLauncher.startDetached()
    }

    function openConfigInEditor() {
        editorLauncher.command = ["sh", "-c",
            "${TERMINAL:-foot} -e sh -c '${EDITOR:-nano} \"" + root._configPath + "\"'"]
        editorLauncher.startDetached()
    }

    function runDoctor() {
        doctorRunner.exec(["omarchy10k", "doctor"])
    }

    function runBenchmark() {
        root.benchmarkOutput = "Running..."
        benchRunner.exec(["omarchy10k", "benchmark", "--iterations", "50"])
    }

    function copyConfigToClipboard() {
        var toml = Model.buildTOML(Model.collectConfig(root))
        clipboardCopy.exec(["sh", "-c",
            "echo '" + toml.replace(/'/g, "'\\''") + "' | xclip -selection clipboard 2>/dev/null || wl-copy 2>/dev/null"])
        root.toastMessage = "Config copied to clipboard"
        root._showToast = true
        toastTimer.restart()
    }

    function pasteConfigFromClipboard() {
        clipboardPaste.exec(["sh", "-c", "xclip -selection clipboard -o 2>/dev/null || wl-paste 2>/dev/null"])
    }

    function reloadConfig() {
        if (daemonSocket.connected) {
            root.sendDaemonCommand("reload_config")
            Qt.callLater(root.loadConfig)
        } else {
            root.loadConfig()
        }
    }

    function resetToDefaults() {
        // A pending delta would resurrect the pre-reset keys the
        // moment saveTimer fires after the file is deleted.
        root._configDirty = false
        root._dirtyKeys = {}
        resetProc.exec(["sh", "-c",
            "cp '" + root._configPath + "' '" + root._configPath + ".bak' 2>/dev/null; " +
            "rm -f '" + root._configPath + "'"])
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

                   // Open Studio — the primary action of this popout.
                   //
                   // It used to sit at the bottom of the Looks bucket, three
                   // scrolls down and labelled "Expand gallery", which buried
                   // the surface where nearly everything actually happens.
                   // The popout is for quick changes; the Studio is where you
                   // browse 28 presets and 53 palettes with a live preview.
                   Rectangle {
                       width: parent.width
                       height: studioLabel.implicitHeight + Style.space(16)
                       radius: Style.cornerRadius
                       // Accent-filled rather than a bordered chip: this is
                       // the one thing on the panel worth making obvious.
                       color: studioMa.containsMouse
                           ? Qt.lighter(Color.accent, 1.15) : Color.accent

                       Row {
                           anchors.centerIn: parent
                           spacing: Style.space(8)

                           Text {
                               id: studioLabel
                               anchors.verticalCenter: parent.verticalCenter
                               text: "Open Studio"
                               color: Color.background
                               font.family: root.bar ? root.bar.fontFamily : Style.font.family
                               font.pixelSize: Style.font.body
                               font.bold: true
                           }

                           Text {
                               anchors.verticalCenter: parent.verticalCenter
                               text: "\u2197"
                               color: Color.background
                               font.family: root.bar ? root.bar.fontFamily : Style.font.family
                               font.pixelSize: Style.font.body
                           }
                       }

                       MouseArea {
                           id: studioMa
                           anchors.fill: parent
                           hoverEnabled: true
                           cursorShape: Qt.PointingHandCursor
                           onClicked: {
                               if (root.omarchyService
                                       && typeof root.omarchyService.openGallery === "function")
                                   root.omarchyService.openGallery()
                               else
                                   root.galleryRequested()
                               // Close the popout behind it: two stacked
                               // surfaces showing the same settings is
                               // confusing, and the Studio takes the screen.
                               root.close()
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

    // ── Tab Buckets (B3 decomposition) ─────────────────────────────────────
    // Each rail bucket lives in its own file; state stays here and is
    // injected via the `panel` property at instantiation. The Loader keeps
    // the lazy one-active-tab behavior of the previous inline Components.

    Component { id: looksTab; PanelLooks { panel: root } }

    Component { id: appearanceTab; PanelStyle { panel: root } }

    Component { id: behaviorTab; PanelBehavior { panel: root } }

    Component { id: systemTab; PanelSystem { panel: root } }

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
    }
}
