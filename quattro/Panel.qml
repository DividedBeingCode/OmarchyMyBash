import QtQuick
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

    // ── Reactive Config State ──────────────────────────────────────────────
    property string cfgLayout: "omarchy"
    property string cfgThemeSource: "omarchy"
    property bool cfgNewline: true
    property bool cfgTransient: true
    property bool cfgRightPrompt: true
    property string cfgStylePreset: "omarchy"
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

    // ── Internal ───────────────────────────────────────────────────────────
    property bool _configDirty: false
    property var _configFlat: ({})
    property var _undoStack: []
    property int _undoMaxSize: 10

    readonly property string _configPath: Model.configPath()

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
            var patch = Model.unflattenPatch(Model.collectConfig(root))
            daemonSocket.write(Model.buildConfigSet(patch, "cfg-save"))
            daemonSocket.flush()
        } else {
            var toml = Model.buildTOML(root._configFlat)
            var escaped = toml.replace(/'/g, "'\\''")
            configWriter.exec(["sh", "-c",
                "mkdir -p '" + Model.configDir() + "' && printf '%s\\n' '" + escaped + "' > '" + _configPath + "'"])
        }

        Qt.callLater(root.requestPreview)
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

    // ── Daemon IPC ─────────────────────────────────────────────────────────
    function discoverAllSockets() {
        socketFinder.exec(["sh", "-c",
            "ls " + Model.runtimeDir() + "/omarchy10k-*.sock 2>/dev/null"])
    }

    function connectToSession(idx) {
        if (idx < 0 || idx >= root.sessionList.length) return
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

        if (resp.type === "config" && resp.config) {
            root._applyDaemonConfig(resp.config)
            return
        }

        if (resp.type === "preview" && resp.left) {
            root.previewText = Model.stripAnsi(resp.left)
            return
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
        Qt.callLater(root.requestPreview)
        Qt.callLater(root.requestPalette)
    }

    function _onSocketDisconnected() {
        if (root.opened)
            root.daemonStatus = "not running"
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
        id: configWriter
        onRunningChanged: {
            if (!running && root._configDirty === false)
                root.sendDaemonCommand("reload_config")
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
                    return
                }
                var paths = text.split("\n")
                var sessions = []
                for (var i = 0; i < paths.length; i++) {
                    var p = paths[i].trim()
                    if (p.length === 0) continue
                    var pidMatch = p.match(/omarchy10k-(\d+)\.sock$/)
                    sessions.push({
                        path: p,
                        shellPid: pidMatch ? pidMatch[1] : "?",
                        pid: "",
                        cwd: ""
                    })
                }
                root.sessionList = sessions
                if (sessions.length > 0) {
                    root.connectToSession(0)
                }
            }
        }
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
            onStreamFinished: root.doctorOutput = this.text
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
            onStreamFinished: root.benchmarkOutput = this.text
        }
    }

    Socket {
        id: daemonSocket
        parser: SplitParser {
            onRead: message => root._handleDaemonMessage(message)
        }
        onConnectedChanged: {
            if (connected) root._onSocketConnected()
            else root._onSocketDisconnected()
        }
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
        running: root.opened && root.daemonStatus !== "running"
        onTriggered: root.discoverAllSockets()
    }

    Timer {
        id: errorTimer
        interval: 5000
        repeat: false
        onTriggered: { root._showError = false; root.lastError = "" }
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
        contentHeight: panel.fittedContentHeight(content.implicitHeight)

        PanelKeyCatcher {
            id: keyCatcher
            anchors.fill: parent
            onCloseRequested: root.close()
            onTabRequested: function(direction) { root.switchPanel(direction) }

            Column {
                id: content
                width: parent.width
                spacing: Style.space(12)
                padding: Style.space(16)

                Row {
                    spacing: Style.space(8)

                    Rectangle {
                        width: 8; height: 8; radius: 4
                        anchors.verticalCenter: parent.verticalCenter
                        color: root.daemonStatus === "running"
                            ? (Color.green || "#9ece6a")
                            : reconnectTimer.running ? "#e0af68" : (Color.red || "#f7768e")
                    }

                    Text {
                        text: "Omarchy10k Control Center"
                        color: root.barForeground
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.subtitle
                        font.bold: true
                    }

                    Rectangle {
                        width: undoText.implicitWidth + Style.space(8)
                        height: undoText.implicitHeight + Style.space(4)
                        radius: Style.space(3)
                        color: undoMa.containsMouse ? (Color.accent || "#7aa2f7") : "transparent"
                        visible: root._undoStack.length > 0

                        Text {
                            id: undoText
                            anchors.centerIn: parent
                            text: "\u21A9 Undo"
                            color: undoMa.containsMouse
                                ? (Color.background || "#1a1b26")
                                : (Color.muted || "#414868")
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
                    width: parent.width - Style.space(32)
                    height: previewContent.implicitHeight + Style.space(16)
                    x: Style.space(16)
                    radius: Style.space(4)
                    color: Qt.darker(Color.background || "#1a1b26", 1.5)
                    visible: root.previewText.length > 0 || !root._featureAvailable("0.3")

                    Column {
                        id: previewContent
                        anchors.fill: parent
                        anchors.margins: Style.space(8)
                        spacing: Style.space(4)

                        Text {
                            text: root._featureAvailable("0.3")
                                ? root.previewText
                                : "Live preview requires daemon v0.3+"
                            color: root._featureAvailable("0.3")
                                ? (Color.foreground || "#a9b1d6")
                                : (Color.muted || "#414868")
                            font.family: "monospace"
                            font.pixelSize: Style.font.body
                            font.italic: !root._featureAvailable("0.3")
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
                                    width: toggleLabel.implicitWidth + Style.space(12)
                                    height: toggleLabel.implicitHeight + Style.space(4)
                                    radius: Style.space(3)
                                    color: root[modelData.prop]
                                        ? (Color.accent || "#7aa2f7")
                                        : (Color.lighter_background || "#24283b")

                                    Text {
                                        id: toggleLabel
                                        anchors.centerIn: parent
                                        text: modelData.label
                                        color: root[modelData.prop]
                                            ? (Color.background || "#1a1b26")
                                            : (Color.muted || "#414868")
                                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                        font.pixelSize: Style.font.caption - 1
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
                        model: ["Appearance", "Context", "Segments", "Shell", "Advanced"]
                        delegate: Rectangle {
                            width: tabLabel.implicitWidth + Style.space(16)
                            height: tabLabel.implicitHeight + Style.space(8)
                            radius: Style.space(4)
                            color: tabBar.currentTab === index
                                ? (Color.accent || "#7aa2f7")
                                : "transparent"

                            Text {
                                id: tabLabel
                                anchors.centerIn: parent
                                text: modelData
                                color: tabBar.currentTab === index
                                    ? (Color.background || "#1a1b26")
                                    : (root.barForeground || "#a9b1d6")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.body
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

                Rectangle {
                    width: parent.width - Style.space(32)
                    height: 1
                    color: Color.muted || "#414868"
                    x: Style.space(16)
                }

                Loader {
                    id: tabContent
                    width: parent.width - Style.space(32)
                    x: Style.space(16)
                    sourceComponent: {
                        switch (tabBar.currentTab) {
                            case 0: return appearanceTab
                            case 1: return contextTab
                            case 2: return segmentsTab
                            case 3: return shellTab
                            case 4: return advancedTab
                        }
                    }
                }

                Rectangle {
                    visible: root._showError
                    width: parent.width - Style.space(32)
                    height: visible ? errorText.implicitHeight + Style.space(12) : 0
                    x: Style.space(16)
                    radius: Style.space(4)
                    color: Color.red || "#f7768e"

                    Text {
                        id: errorText
                        anchors.centerIn: parent
                        text: root.lastError
                        color: Color.background || "#1a1b26"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                        wrapMode: Text.WordWrap
                        width: parent.width - Style.space(16)
                    }
                }

                Rectangle {
                    visible: root._showToast
                    width: parent.width - Style.space(32)
                    height: visible ? toastText.implicitHeight + Style.space(10) : 0
                    x: Style.space(16)
                    radius: Style.space(4)
                    color: Color.accent || "#7aa2f7"
                    opacity: root._showToast ? 1 : 0

                    Behavior on opacity { NumberAnimation { duration: 300 } }

                    Text {
                        id: toastText
                        anchors.centerIn: parent
                        text: root.toastMessage
                        color: Color.background || "#1a1b26"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }
            }
        }
    }

    // ── Tab: Appearance ────────────────────────────────────────────────────

    Component {
        id: appearanceTab
        Column {
            spacing: Style.space(10)

            // ── Style Gallery ──────────────────────────────────────────
            Text {
                text: "Style"
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
                    model: [
                        { name: "omarchy",    preview: "  ~ \u276f",               desc: "Clean" },
                        { name: "powerline",  preview: "  ~ \ue0b0 git",          desc: "Classic" },
                        { name: "rainbow",    preview: "  ~ \ue0b0\ue0b0\ue0b0",  desc: "Vibrant" },
                        { name: "framed",     preview: "\u256d\u2500 ~ \u2500\u256e",  desc: "Framed" },
                        { name: "classic",    preview: "  ~ \u2502 git",          desc: "Divided" },
                        { name: "lean",       preview: "  ~/src",                  desc: "Minimal" },
                        { name: "dense",      preview: "  ~ git \u276f",          desc: "Compact" },
                        { name: "slanted",    preview: "  ~ \ue0bc git",          desc: "Modern" }
                    ]
                    delegate: Rectangle {
                        width: (parent.width - Style.space(18)) / 4
                        height: styleCardCol.implicitHeight + Style.space(12)
                        radius: Style.space(4)
                        color: root.cfgStylePreset === modelData.name
                            ? (Color.accent || "#7aa2f7")
                            : (Color.lighter_background || "#24283b")
                        border.width: root.cfgStylePreset === modelData.name ? 2 : 0
                        border.color: Color.accent || "#7aa2f7"

                        Column {
                            id: styleCardCol
                            anchors.centerIn: parent
                            spacing: Style.space(2)

                            Text {
                                text: modelData.preview
                                color: root.cfgStylePreset === modelData.name
                                    ? (Color.background || "#1a1b26")
                                    : (Color.foreground || "#a9b1d6")
                                font.family: "monospace"
                                font.pixelSize: Style.font.caption - 2
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                                elide: Text.ElideRight
                            }

                            Text {
                                text: modelData.name
                                color: root.cfgStylePreset === modelData.name
                                    ? (Color.background || "#1a1b26")
                                    : (root.barForeground || "#a9b1d6")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption - 1
                                font.bold: true
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                            }

                            Text {
                                text: modelData.desc
                                color: root.cfgStylePreset === modelData.name
                                    ? Qt.lighter(Color.background || "#1a1b26", 1.5)
                                    : (Color.muted || "#414868")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption - 2
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(8)
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.setConfigValue("style.preset", modelData.name)
                        }
                    }
                }
            }

            Rectangle { width: parent.width - Style.space(32); height: 1; color: Color.muted || "#414868"; x: Style.space(16) }

            // ── Glyph Pickers ──────────────────────────────────────────
            Text {
                text: "Glyphs"
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
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
                    { key: "vertical",       glyph: "\u2502",  label: "Bar" },
                    { key: "dot",            glyph: "\u00b7",  label: "Dot" },
                    { key: "diamond",        glyph: "\u25c6",  label: "Diamond" }
                ]
            }

            Rectangle { width: parent.width - Style.space(32); height: 1; color: Color.muted || "#414868"; x: Style.space(16) }

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

            ControlRow {
                label: "Lines"
                value: root.cfgNewline ? "Two-line" : "One-line"
                options: ["Two-line", "One-line"]
                onChanged: function(val) { root.setConfigValue("prompt.newline", val === "Two-line") }
            }

            ControlRow {
                label: "Transient"
                value: root.cfgTransient ? "On" : "Off"
                options: ["On", "Off"]
                onChanged: function(val) { root.setConfigValue("prompt.transient", val === "On") }
            }

            Rectangle { width: parent.width - Style.space(32); height: 1; color: Color.muted || "#414868"; x: Style.space(16) }

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
                            width: 20; height: 20; radius: 3
                            color: root.paletteColors[modelData] || "#333"
                            border.width: 1
                            border.color: Color.muted || "#414868"
                        }
                        Text {
                            text: modelData.charAt(0).toUpperCase()
                            color: Color.muted || "#414868"
                            font.pixelSize: 8
                            horizontalAlignment: Text.AlignHCenter
                            width: 20
                        }
                    }
                }
            }
        }
    }

    // ── Tab: Context ───────────────────────────────────────────────────────

    Component {
        id: contextTab
        Column {
            spacing: Style.space(10)

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
            }
        }
    }

    // ── Tab: Segments ──────────────────────────────────────────────────────

    Component {
        id: segmentsTab
        Column {
            spacing: Style.space(8)

            Text {
                text: "Toggle prompt segments on or off."
                color: Color.muted || "#414868"
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
                        { label: "Battery", key: "segments.battery.enabled", prop: "cfgBatteryEnabled" },
                        { label: "Terminal Title", key: "terminal.title.enabled", prop: "cfgTitleEnabled" }
                    ]
                    delegate: Rectangle {
                        width: (parent.width - Style.space(6)) / 2
                        height: segLabel.implicitHeight + Style.space(12)
                        radius: Style.space(4)
                        color: root[modelData.prop]
                            ? (Color.accent || "#7aa2f7")
                            : (Color.lighter_background || "#24283b")

                        Text {
                            id: segLabel
                            anchors.centerIn: parent
                            text: modelData.label
                            color: root[modelData.prop]
                                ? (Color.background || "#1a1b26")
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
    }

    // ── Tab: Shell ─────────────────────────────────────────────────────────

    Component {
        id: shellTab
        Column {
            spacing: Style.space(10)

            Text {
                text: "Shell integrations are configured through their own tools.\nOmarchy10k coordinates their lifecycle via the hook broker."
                color: Color.muted || "#414868"
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

            Rectangle {
                width: parent.width
                height: 1
                color: Color.muted || "#414868"
            }

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

    // ── Tab: Advanced ──────────────────────────────────────────────────────

    Component {
        id: advancedTab
        Column {
            spacing: Style.space(10)

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
                radius: Style.space(4)
                color: Qt.darker(Color.background || "#1a1b26", 1.3)
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
                        color: Color.foreground || "#a9b1d6"
                        font.family: "monospace"
                        font.pixelSize: Style.font.caption - 1
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
                radius: Style.space(4)
                color: Qt.darker(Color.background || "#1a1b26", 1.3)
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
                        color: Color.foreground || "#a9b1d6"
                        font.family: "monospace"
                        font.pixelSize: Style.font.caption - 1
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
                    resetProc.exec(["sh", "-c",
                        "cp '" + root._configPath + "' '" + root._configPath + ".bak' 2>/dev/null; " +
                        "rm -f '" + root._configPath + "'"])
                }
            }

            Rectangle {
                width: parent.width
                height: daemonInfo.implicitHeight + Style.space(12)
                radius: Style.space(4)
                color: Qt.darker(Color.background || "#1a1b26", 1.3)

                Column {
                    id: daemonInfo
                    anchors.fill: parent
                    anchors.margins: Style.space(8)
                    spacing: Style.space(4)

                    Text {
                        text: "Daemon: " + root.daemonStatus
                        color: root.daemonStatus === "running"
                            ? (Color.green || "#9ece6a")
                            : (Color.red || "#f7768e")
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "PID: " + (root.daemonPid || "\u2014")
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "Version: " + (root.daemonVersion || "\u2014") + " (protocol " + (root.daemonProtocolVersion || "\u2014") + ")"
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: root.daemonProtocolVersion
                            ? ("Protocol status: " + (root._featureAvailable("0.3") ? "full (v0.3+)" : "degraded (upgrade daemon)"))
                            : "Protocol status: unknown"
                        color: root._featureAvailable("0.3")
                            ? (Color.green || "#9ece6a")
                            : (Color.muted || "#414868")
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "Sessions: " + root.sessionList.length
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }
            }

            Repeater {
                model: root.sessionList
                delegate: Rectangle {
                    width: parent.width
                    height: sessionRow.implicitHeight + Style.space(8)
                    radius: Style.space(3)
                    color: index === root.activeSessionIndex
                        ? (Color.accent || "#7aa2f7")
                        : (Color.lighter_background || "#24283b")

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
                            text: "Shell " + modelData.shellPid
                            color: index === root.activeSessionIndex
                                ? (Color.background || "#1a1b26")
                                : (root.barForeground || "#a9b1d6")
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            font.bold: index === root.activeSessionIndex
                        }
                        Text {
                            text: modelData.cwd || ""
                            color: Color.muted || "#414868"
                            font.family: root.bar ? root.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            elide: Text.ElideMiddle
                            width: parent.width * 0.5
                        }
                    }

                    Rectangle {
                        width: 24; height: 24; radius: 4
                        anchors.right: parent.right
                        anchors.rightMargin: Style.space(4)
                        anchors.verticalCenter: parent.verticalCenter
                        z: 2
                        color: termMa.containsMouse ? (Color.accent || "#7aa2f7") : "transparent"
                        visible: modelData.cwd.length > 0

                        Text {
                            anchors.centerIn: parent
                            text: "\uf120"
                            color: termMa.containsMouse
                                ? (Color.background || "#1a1b26")
                                : (Color.muted || "#414868")
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
    }

    // ── Reusable Components ────────────────────────────────────────────────

    component ControlRow: Row {
        property string label
        property string value
        property var options: []
        signal changed(string val)

        width: parent ? parent.width : 200
        spacing: Style.space(8)

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
            spacing: Style.space(4)
            Repeater {
                model: options
                delegate: Rectangle {
                    width: optText.implicitWidth + Style.space(12)
                    height: optText.implicitHeight + Style.space(6)
                    radius: Style.space(3)
                    color: value === modelData
                        ? (Color.accent || "#7aa2f7")
                        : (Color.lighter_background || "#24283b")

                    Text {
                        id: optText
                        anchors.centerIn: parent
                        text: modelData
                        color: value === modelData
                            ? (Color.background || "#1a1b26")
                            : (root.barForeground || "#a9b1d6")
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: changed(modelData)
                    }
                }
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
                ? (Color.green || "#9ece6a")
                : (Color.muted || "#414868")
            font.family: root.bar ? root.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }
    }

    component ActionButton: Rectangle {
        property string label
        property bool dangerous: false
        signal clicked()

        width: parent ? parent.width : 200
        height: btnText.implicitHeight + Style.space(12)
        radius: Style.space(4)
        color: mouseArea.containsMouse
            ? (dangerous ? (Color.red || "#f7768e") : (Color.accent || "#7aa2f7"))
            : (Color.lighter_background || "#24283b")

        Text {
            id: btnText
            anchors.centerIn: parent
            text: label
            color: mouseArea.containsMouse
                ? (Color.background || "#1a1b26")
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
                width: parent.width * 0.22
                text: label
                color: root.barForeground || "#a9b1d6"
                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                verticalAlignment: Text.AlignVCenter
                height: glyphFlow.height
            }

            Flow {
                id: glyphFlow
                width: parent.width * 0.75
                spacing: Style.space(3)

                Repeater {
                    model: glyphs
                    delegate: Rectangle {
                        width: glyphCol.implicitWidth + Style.space(10)
                        height: glyphCol.implicitHeight + Style.space(6)
                        radius: Style.space(3)
                        color: currentValue === modelData.key
                            ? (Color.accent || "#7aa2f7")
                            : (Color.lighter_background || "#24283b")

                        Column {
                            id: glyphCol
                            anchors.centerIn: parent
                            spacing: 1

                            Text {
                                text: modelData.glyph
                                color: currentValue === modelData.key
                                    ? (Color.background || "#1a1b26")
                                    : (Color.foreground || "#a9b1d6")
                                font.family: "monospace"
                                font.pixelSize: Style.font.body
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(6)
                            }

                            Text {
                                text: modelData.label
                                color: currentValue === modelData.key
                                    ? Qt.lighter(Color.background || "#1a1b26", 1.4)
                                    : (Color.muted || "#414868")
                                font.family: root.bar ? root.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption - 2
                                horizontalAlignment: Text.AlignHCenter
                                width: parent.parent.width - Style.space(6)
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
