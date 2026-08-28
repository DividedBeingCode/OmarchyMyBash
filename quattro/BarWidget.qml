import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import "Model.js" as Model

BarWidget {
    id: root
    moduleName: "community.omarchy10k"

    property string barDaemonStatus: "unknown"
    property string barSocketPath: ""

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
            "ls '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock 2>/dev/null | head -1"])
    }

    function _handleBarStatusMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)
        if (resp.type === "hello") {
            barStatusSocket.write(Model.buildCommand("status", "bar-poll"))
            barStatusSocket.flush()
            return
        }
        if (resp.status === "ok" && resp.pid !== undefined) {
            root.barDaemonStatus = "running"
        } else if (resp.status === "ok") {
            root.barDaemonStatus = "running"
        } else if (resp.status === "bye") {
            root.barDaemonStatus = "stopped"
        } else if (resp.error) {
            root.barDaemonStatus = "error"
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
            root.barSocketPath = ""
        }
    }

    Timer {
        id: barPollTimer
        interval: 5000
        repeat: true
        running: !root.opened
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
        tooltipText: "Omarchy10k" + (barDaemonStatus === "running" ? " ✓" : " ✗")
        onPressed: function(buttonCode) {
            if (buttonCode === Qt.LeftButton) root.toggle()
        }
    }
}
