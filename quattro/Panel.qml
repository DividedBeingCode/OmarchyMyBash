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
    property string cfgGitMode: "adaptive"
    property bool cfgGitEnabled: true
    property string cfgOsIcon: "arch"
    property bool cfgExitSignalNames: true
    property int cfgCmdDurationMs: 1500
    property string cfgSshShow: "auto"

    // ── Reactive Daemon State ──────────────────────────────────────────────
    property string daemonStatus: "unknown"
    property string daemonPid: ""
    property string daemonVersion: ""
    property string discoveredSocketPath: ""

    // ── Reactive Tool State ────────────────────────────────────────────────
    property string bleshStatus: "checking..."
    property string atuinStatus: "checking..."
    property string miseStatus: "checking..."
    property string zoxideStatus: "checking..."
    property string fzfStatus: "checking..."

    // ── Internal ───────────────────────────────────────────────────────────
    property bool _configDirty: false
    property var _configFlat: ({})

    readonly property string _configPath: Model.configPath()

    // ── Panel Lifecycle ────────────────────────────────────────────────────
    function open() {
        root.controller.show()
        loadConfig()
        discoverSocket()
        detectTools()
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

    // ── Config Read ────────────────────────────────────────────────────────
    function loadConfig() {
        configReader.exec(["cat", _configPath])
    }

    function _applyParsedConfig(text) {
        root._configFlat = Model.parseTOML(text)
        Model.applyConfig(root._configFlat, root)
    }

    // ── Config Write ───────────────────────────────────────────────────────
    function setConfigValue(tomlKey, value) {
        var prop = Model.CONFIG_MAP[tomlKey]
        if (prop) root[prop] = value

        root._configFlat[tomlKey] = value
        _scheduleSave()
    }

    function _scheduleSave() {
        if (!root._configDirty) {
            root._configDirty = true
            saveTimer.restart()
        }
    }

    function _flushSave() {
        root._configDirty = false
        var toml = Model.buildTOML(root._configFlat)
        configWriter.exec({
            command: ["sh", "-c",
                "mkdir -p '" + Model.configDir() + "' && cat > '" + _configPath + "'"]
        })
        configWriter.write(toml)
        configWriter.write("")
        configWriter.running = false
    }

    // ── Daemon IPC ─────────────────────────────────────────────────────────
    function discoverSocket() {
        socketFinder.exec(["sh", "-c",
            "ls " + Model.runtimeDir() + "/omarchy10k-*.sock 2>/dev/null | head -1"])
    }

    function sendDaemonCommand(name) {
        if (!daemonSocket.connected) return
        daemonSocket.write(Model.buildCommand(name))
        daemonSocket.flush()
    }

    function _handleDaemonMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)
        if (resp.status === "ok" && resp.pid !== undefined) {
            root.daemonStatus = "running"
            root.daemonPid = String(resp.pid)
            root.daemonVersion = resp.version || ""
        } else if (resp.status === "ok") {
            root.daemonStatus = "running"
        } else if (resp.status === "bye") {
            root.daemonStatus = "stopped"
        } else if (resp.error) {
            root.daemonStatus = "error"
        }
    }

    function _onSocketConnected() {
        sendDaemonCommand("status")
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
                var path = this.text.trim()
                if (path.length > 0) {
                    root.discoveredSocketPath = path
                    daemonSocket.path = path
                    daemonSocket.connected = true
                } else {
                    root.daemonStatus = "not running"
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
            onStreamFinished: console.log("omarchy10k doctor:", this.text)
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
        onTriggered: root.discoverSocket()
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

                Text {
                    text: "Omarchy10k Control Center"
                    color: root.barForeground
                    font.family: root.bar ? root.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.subtitle
                    font.bold: true
                }

                Row {
                    id: tabBar
                    spacing: Style.space(4)
                    property int currentTab: 0

                    Repeater {
                        model: ["Appearance", "Context", "Shell", "Advanced"]
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
                            case 2: return shellTab
                            case 3: return advancedTab
                        }
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

            ControlRow {
                label: "Preset"
                value: root.cfgLayout
                options: ["omarchy", "minimal", "powerline", "classic", "pure", "dense"]
                onChanged: function(val) { root.setConfigValue("prompt.layout", val) }
            }

            ControlRow {
                label: "Theme"
                value: root.cfgThemeSource
                options: ["omarchy", "custom", "hybrid", "terminal"]
                onChanged: function(val) { root.setConfigValue("theme.source", val) }
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

            ControlRow {
                label: "OS Icon"
                value: root.cfgOsIcon
                options: ["arch", "linux", "omarchy", "none"]
                onChanged: function(val) { root.setConfigValue("segments.os.icon", val) }
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
            StatusRow { label: "Mise"; status: root.miseStatus }
            StatusRow { label: "Zoxide"; status: root.zoxideStatus }
            StatusRow { label: "fzf"; status: root.fzfStatus }
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
                        "${EDITOR:-nano} '" + root._configPath + "'"]
                    editorLauncher.startDetached()
                }
            }

            ActionButton {
                label: "Run Doctor"
                onClicked: doctorRunner.exec(["omarchy10k", "doctor"])
            }

            ActionButton {
                label: "Reload Config"
                onClicked: {
                    root.loadConfig()
                    root.sendDaemonCommand("reload_config")
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
                        text: "Version: " + (root.daemonVersion || "\u2014")
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
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
                root.loadConfig()
                root.sendDaemonCommand("reload_config")
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
}
