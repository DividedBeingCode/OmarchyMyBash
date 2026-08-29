import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

BarWidget {
    id: root
    moduleName: "community.omarchy10k"

    property string barDaemonStatus: "unknown"
    property string barSocketPath: ""

    // ── Bar intelligence (bound over the existing status flow — no new
    // timers/sockets; the data rides on the poll path or the Service mirror).

    // Fallback long-cmd threshold when no service config is reachable
    // (matches the daemon default in [notifications].threshold_ms).
    readonly property int barLongCmdFallbackMs: 10000

    // Full last `status` payload: Service.lastStatus in service mode, the
    // already-arriving poll response otherwise. Empty ({}) while down.
    property var barPollStatus: ({})
    readonly property var barStatusData: omarchyService
        ? (omarchyService.lastStatus || {}) : barPollStatus

    readonly property bool barDaemonRunning: omarchyService
        ? omarchyService.daemonStatus === "running"
        : barDaemonStatus === "running"

    // 1) Glyph health color: accent while the daemon is reachable and
    // running, urgent when disconnected / not running.
    readonly property color barGlyphColor: barDaemonRunning
        ? Color.accent : Color.urgent

    // 2) Git mini-badge inputs. Fields come from status.git
    // {branch, dirty, staged, unstaged}; the dot hides entirely when git is
    // absent or fully clean.
    readonly property var barGit: barStatusData && barStatusData.git
        ? barStatusData.git : null
    readonly property bool barGitHasActivity: !!barGit
        && (!!barGit.dirty || Number(barGit.staged) > 0
            || Number(barGit.unstaged) > 0)
    // "Hot" = working tree touched (dirty or unstaged); staged-only is calm.
    readonly property bool barGitHot: !!barGit
        && (!!barGit.dirty || Number(barGit.unstaged) > 0)

    // 3) Agent mini-badge inputs. status.agent ("claude" | "codex") is set
    // when the last prompt render saw an agent env key (mirrors
    // segments/ai.rs detection); null otherwise. Glyph: Nerd Font robot,
    // falling back to the diamond star when the bar font is not a Nerd Font.
    readonly property var barAgent: barStatusData && barStatusData.agent
        ? barStatusData.agent : null
    readonly property string barAgentGlyph: button.fontFamily
        && button.fontFamily.toLowerCase().indexOf("nerd") >= 0
        ? "\uF086" : "\u2726"

    // 4) Long-command badge inputs. Threshold from the daemon config when
    // the service is alive (notifyThresholdMs; 0 = notifications off), else
    // the fallback constant.
    readonly property bool barLongCmdEnabled: omarchyService
        ? omarchyService.notifyThresholdMs > 0 : true
    readonly property int barLongCmdThresholdMs: omarchyService
        ? (omarchyService.notifyThresholdMs > 0
            ? omarchyService.notifyThresholdMs : barLongCmdFallbackMs)
        : barLongCmdFallbackMs
    readonly property int barLastCmdMs: Number(
        barStatusData && barStatusData.last_cmd_duration_ms) || 0
    readonly property bool barLongCmdActive: barLongCmdEnabled
        && barLastCmdMs > barLongCmdThresholdMs

    function _fmtDurationMs(ms) {
        var s = Math.round(ms / 1000)
        if (s < 60) return s + "s"
        return Math.floor(s / 60) + "m"
            + ("0" + (s % 60)).slice(-2) + "s"
    }

    // Service-kind hub (v0.4): when the host loaded our Service.qml (plugin
    // enabled in shell.json plugins[]), the widget mirrors its state instead
    // of polling. Feature-detected — absent/old hosts keep today's poll path.
    readonly property var omarchyService: root.bar && root.bar.shell
        && typeof root.bar.shell.serviceFor === "function"
        ? root.bar.shell.serviceFor("community.omarchy10k") : null

    onOmarchyServiceChanged: {
        if (omarchyService) {
            barDaemonStatus = omarchyService.daemonStatus
            // Retire the duplicate poll connection while the service lives.
            barStatusSocket.connected = false
        } else {
            discoverBarSocket()
        }
    }

    Connections {
        target: root.omarchyService
        function onDaemonStatusChanged() {
            if (root.omarchyService) root.barDaemonStatus = root.omarchyService.daemonStatus
        }
    }

    readonly property bool opened: panelLoader.item
        ? panelLoader.item.opened === true
        : false
    readonly property bool popoutSwitchClosing: panelLoader.item
        ? panelLoader.item.popoutSwitchClosing === true
        : false

    function open() {
        if (panelLoader.item) panelLoader.item.open()
    }

    function close() {
        if (panelLoader.item) panelLoader.item.close()
    }

    function toggle() {
        if (panelLoader.item) panelLoader.item.toggle()
    }

    function closeForPopoutSwitch() {
        if (panelLoader.item) panelLoader.item.closeForPopoutSwitch()
    }

    function injectPanel() {
        if (!panelLoader.item) return
        panelLoader.item.bar = root.bar
        panelLoader.item.anchorItem = button
        panelLoader.item.hostWidget = root
    }

    function discoverBarSocket() {
        barSocketFinder.exec(["sh", "-c",
            "for f in '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock; do " +
            "[[ -e \"$f\" ]] || continue; p=${f##*-}; p=${p%.sock}; " +
            "kill -0 \"$p\" 2>/dev/null && timeout 1 socat -u OPEN:/dev/null UNIX-CONNECT:\"$f\" 2>/dev/null && echo \"$f\"; done | head -1"])
    }

    function _handleBarStatusMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)
        if (resp.type === "hello") {
            barStatusSocket.write(Model.buildCommand("status", "bar-poll"))
            barStatusSocket.flush()
            return
        }
        if (resp.status === "ok") {
            root.barDaemonStatus = "running"
            root.barPollStatus = resp
        } else if (resp.status === "bye") {
            root.barDaemonStatus = "stopped"
            root.barPollStatus = ({})
        } else if (resp.error) {
            root.barDaemonStatus = "error"
            root.barPollStatus = ({})
        }
    }

    implicitWidth: button.implicitWidth
    implicitHeight: button.implicitHeight

    onBarChanged: injectPanel()

    Component.onCompleted: discoverBarSocket()

    Process {
        id: barSocketFinder
        stdout: StdioCollector {
            onStreamFinished: {
                var text = this.text.trim()
                if (text.length === 0) {
                    root.barDaemonStatus = "not running"
                    root.barPollStatus = ({})
                    root.barSocketPath = ""
                    barStatusSocket.connected = false
                    return
                }
                root.barSocketPath = text.split("\n")[0].trim()
                barStatusSocket.path = root.barSocketPath
                barStatusSocket.connected = true
            }
        }
    }

    Socket {
        id: barStatusSocket
        parser: SplitParser {
            onRead: message => root._handleBarStatusMessage(message)
        }
        onConnectedChanged: {
            if (connected) {
                barStatusSocket.write(Model.buildHello("bar-handshake"))
                barStatusSocket.flush()
            }
        }
        onError: {
            barStatusSocket.connected = false
            root.barDaemonStatus = "not running"
            root.barPollStatus = ({})
            root.barSocketPath = ""
        }
    }

    Timer {
        id: barPollTimer
        interval: 5000
        repeat: true
        running: !root.opened && !root.omarchyService
        onTriggered: {
            if (barSocketPath.length > 0 && barStatusSocket.connected) {
                barStatusSocket.write(Model.buildCommand("status", "bar-poll") + "")
                barStatusSocket.flush()
            } else {
                discoverBarSocket()
            }
        }
    }

    Loader {
        id: panelLoader
        active: true
        source: Qt.resolvedUrl("Panel.qml")
        visible: false
        onLoaded: {
            root.injectPanel()
            Qt.callLater(root.injectPanel)
        }
    }

    WidgetButton {
        id: button
        anchors.fill: parent
        bar: root.bar
        text: "\u276f"
        // Health-colored glyph: accent while the daemon answers, urgent
        // otherwise (default bar foreground is replaced by the binding).
        foreground: root.barGlyphColor
        tooltipText: "Omarchy10k" + (barDaemonStatus === "running" ? " ✓" : " ✗")
            + (root.barAgent ? " · Agent: " + root.barAgent : "")
        onPressed: function(buttonCode) {
            if (buttonCode === Qt.LeftButton) root.toggle()
        }
    }

    // Badges anchored just right of the ❯ glyph (the label is centered in
    // the button, so its right edge is horizontalCenter + labelWidth/2).
    Row {
        id: badges
        anchors.verticalCenter: button.verticalCenter
        anchors.left: button.horizontalCenter
        anchors.leftMargin: Math.ceil(button.labelWidth / 2) + 3
        spacing: 3

        // 2) Git mini-badge: accent dot while staged-only, urgent once the
        // tree is dirty/unstaged. Hidden entirely when git is null/clean or
        // the daemon is down (status data goes stale → bindings collapse).
        Rectangle {
            width: 6
            height: 6
            radius: 3
            visible: root.barDaemonRunning && root.barGitHasActivity
            color: root.barGitHot ? Color.urgent : Color.accent
        }

        // 3) Agent mini-badge: robot glyph (Nerd Font; diamond star fallback)
        // in accent color while an AI agent was detected at the last prompt
        // render. Gated exactly like the git dot: hidden when the daemon is
        // down (status data stale) or status.agent is null.
        Text {
            visible: root.barDaemonRunning && !!root.barAgent
            anchors.verticalCenter: parent.verticalCenter
            text: root.barAgentGlyph
            color: Color.accent
            font.family: button.fontFamily
            font.pixelSize: Math.max(9, Math.round(button.fontSize * 0.75))
        }

        // 4) Long-command chip: ⏱ + duration while the last command
        // outlived the notification threshold; the next status reporting a
        // fresh (0 or below-threshold) duration clears it by rebinding.
        Text {
            visible: root.barDaemonRunning && root.barLongCmdActive
            anchors.verticalCenter: parent.verticalCenter
            text: "\u23F1 " + root._fmtDurationMs(root.barLastCmdMs)
            color: Color.urgent
            font.family: button.fontFamily
            font.pixelSize: Math.max(9, Math.round(button.fontSize * 0.65))
        }
    }

    // IPC: lets scripts and other surfaces open the Control Center without a
    // pointer click, mirroring first-party widgets that register one target
    // each (omarchy-shell call community.omarchy10k.panel toggle).
    IpcHandler {
        target: "community.omarchy10k.panel"

        function toggle(): string {
            root.toggle()
            return "ok"
        }

        function open(): string {
            root.open()
            return "ok"
        }

        function close(): string {
            root.close()
            return "ok"
        }
    }
}
