import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons

// Omarchy10k Session Picker — overlay-kind plugin entry point.
//
// Summoned via `omarchy-shell call community.omarchy10k picker` (the service's
// IPC target calls shell.summon) or `omarchy-shell shell summon
// community.omarchy10k '<json>'`. Lists every live omarchy10k shell session
// (CWD, git branch, dirty state, last command duration, session age) and
// focuses its terminal window on selection:
//   1. `hyprctl dispatch focuswindow pid:<shellPid>` under Hyprland
//      (best-effort — the shell PID must map to a client window)
//   2. fallback: omarchy-launch-floating-terminal at the session's CWD
// Escape (or clicking the scrim) closes.
//
// The host injects omarchyPath/shell/manifest/pluginRegistry and — when this
// plugin's service is loaded — `service`. All reads are feature-detected so
// the overlay still opens (with an empty state) if the service is missing.

Item {
    id: root

    // Injected by the host panel/overlay loader (feature-detected).
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var shell: null
    property var manifest: null
    property var service: null

    readonly property string pluginId: manifest && manifest.id ? String(manifest.id) : "community.omarchy10k"
    readonly property bool hyprland: Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE") !== ""

    property bool opened: false
    property var rows: []
    property int selectedIndex: 0

    // Theme tokens with graceful fallbacks when Color.menu roles are absent.
    function _c(path, fallback) {
        try {
            var cur = Color
            var parts = path.split(".")
            for (var i = 0; i < parts.length; i++) {
                if (cur === undefined || cur === null) return fallback
                cur = cur[parts[i]]
            }
            return (cur === undefined || cur === null) ? fallback : cur
        } catch (e) { return fallback }
    }
    readonly property color surfaceColor: _c("menu.background", _c("background", "#1a1b26"))
    readonly property color surfaceText: _c("menu.text", _c("foreground", "#a9b1d6"))
    readonly property color scrimColor: _c("menu.scrim", "#000000")
    readonly property color selectedBg: _c("menu.selectedBackground", _c("accent", "#7aa2f7"))
    readonly property color selectedText: _c("menu.selectedText", _c("background", "#1a1b26"))
    readonly property color mutedColor: _c("muted", "#414868")
    readonly property int surfaceRadius: typeof Style !== "undefined" && Style.cornerRadius !== undefined ? Style.cornerRadius : 8

    // ── Lifecycle (overlay entry point contract: open/close) ──────────────
    function open(payloadJson) {
        root.opened = true
        root.selectedIndex = 0
        root.refreshRows()
        Qt.callLater(function () { keyCatcher.forceActiveFocus() })
    }

    function close() {
        root.opened = false
    }

    function dismiss() {
        root.opened = false
        if (root.shell && typeof root.shell.hide === "function")
            root.shell.hide(root.pluginId)
    }

    function refreshRows() {
        var src = root.service && root.service.sessions ? root.service.sessions : []
        var list = []
        for (var i = 0; i < src.length; i++) {
            var s = src[i]
            list.push({
                shellPid: String(s.shellPid || "?"),
                pid: s.pid !== undefined && s.pid !== null && String(s.pid) !== "" ? String(s.pid) : "",
                cwd: s.cwd || "",
                branch: s.branch || "",
                dirty: !!s.dirty,
                lastCmdMs: s.lastCmdMs || 0,
                ageSecs: s.ageSecs || 0
            })
        }
        root.rows = list
        if (root.selectedIndex >= list.length) root.selectedIndex = list.length > 0 ? list.length - 1 : 0
    }

    onOpenedChanged: if (opened) refreshRows()
    onServiceChanged: refreshRows()

    // ── Activation ─────────────────────────────────────────────────────────
    function _formatDuration(ms) {
        if (!ms || ms <= 0) return ""
        if (ms < 1000) return ms + "ms"
        if (ms < 60000) return (ms / 1000).toFixed(1) + "s"
        var mins = Math.floor(ms / 60000)
        var secs = Math.round((ms % 60000) / 1000)
        return mins + "m" + (secs < 10 ? "0" : "") + secs
    }

    function _formatAge(secs) {
        if (!secs || secs <= 0) return ""
        if (secs < 60) return secs + "s old"
        if (secs < 3600) return Math.floor(secs / 60) + "m old"
        return Math.floor(secs / 3600) + "h old"
    }

    function _openTerminalAt(cwd) {
        if (!cwd) return
        var safeCwd = String(cwd).replace(/'/g, "'\\''")
        focusLauncher.exec(["sh", "-c",
            "cd '" + safeCwd + "' && exec omarchy-launch-floating-terminal"])
    }

    function activateIndex(index) {
        if (index < 0 || index >= root.rows.length) return
        var s = root.rows[index]
        root.dismiss()
        if (root.hyprland && s.shellPid && s.shellPid !== "?") {
            // Best-effort: focus the client window owning this shell session.
            // If hyprctl cannot map the pid to a window (nonzero exit), fall
            // back to opening a floating terminal in the session's CWD.
            pendingFocusCwd = s.cwd
            hyprctlFocus.exec(["sh", "-c",
                "hyprctl dispatch focuswindow pid:" + s.shellPid + " >/dev/null 2>&1"])
        } else {
            root._openTerminalAt(s.cwd)
        }
    }

    property string pendingFocusCwd: ""

    Process {
        id: hyprctlFocus
        onExited: function (exitCode) {
            if (exitCode !== 0) root._openTerminalAt(root.pendingFocusCwd)
        }
    }

    Process {
        id: focusLauncher
    }

    // ── Overlay surface ────────────────────────────────────────────────────
    PanelWindow {
        id: overlay
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        WlrLayershell.namespace: "omarchy10k-session-picker"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        exclusionMode: ExclusionMode.Ignore

        Rectangle {
            anchors.fill: parent
            color: root.scrimColor
            opacity: 0.55
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismiss()
        }

        Rectangle {
            id: card
            width: Math.min(520, parent.width - 32)
            height: Math.min(420, parent.height - 32)
            radius: root.surfaceRadius
            anchors.centerIn: parent
            color: root.surfaceColor
            border.width: 1
            border.color: root.mutedColor

            MouseArea { anchors.fill: parent; onClicked: { } }

            Item {
                id: keyCatcher
                anchors.fill: parent
                focus: true

                Keys.priority: Keys.BeforeItem
                Keys.onPressed: function (event) {
                    if (event.key === Qt.Key_Escape) {
                        root.dismiss()
                        event.accepted = true
                    } else if (event.key === Qt.Key_Up || event.key === Qt.Key_K) {
                        if (root.selectedIndex > 0) root.selectedIndex--
                        event.accepted = true
                    } else if (event.key === Qt.Key_Down || event.key === Qt.Key_J) {
                        if (root.selectedIndex < root.rows.length - 1) root.selectedIndex++
                        event.accepted = true
                    } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        root.activateIndex(root.selectedIndex)
                        event.accepted = true
                    }
                }

                Column {
                    anchors.fill: parent
                    anchors.margins: Style.space(16)
                    spacing: Style.space(12)

                    Text {
                        text: "Omarchy10k Sessions"
                        color: root.surfaceText
                        font.family: typeof Style !== "undefined" && Style.font ? Style.font.family : "monospace"
                        font.pixelSize: typeof Style !== "undefined" && Style.font ? Style.font.heading : 16
                        font.bold: true
                    }

                    Text {
                        id: sessionHint
                        width: parent.width
                        text: root.hyprland
                            ? "Enter focuses the session's terminal · Esc closes"
                            : "Enter opens a floating terminal in the session's CWD · Esc closes"
                        color: root.mutedColor
                        font.pixelSize: 11
                    }

                    ListView {
                        id: sessionList
                        width: parent.width
                        height: parent.height - sessionHint.y - sessionHint.height - Style.space(8)
                        model: root.rows
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        spacing: 4

                        delegate: Rectangle {
                            required property var modelData
                            required property int index

                            width: sessionList.width
                            height: rowCol.implicitHeight + Style.space(14)
                            radius: Style.space(4)
                            color: index === root.selectedIndex ? root.selectedBg : Qt.lighter(root.surfaceColor, 1.15)

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onContainsMouseChanged: if (containsMouse) root.selectedIndex = index
                                onClicked: root.activateIndex(index)
                            }

                            Column {
                                id: rowCol
                                anchors.fill: parent
                                anchors.margins: 7
                                spacing: 2

                                Text {
                                    width: parent.width
                                    text: modelData.cwd || ("Shell " + modelData.shellPid)
                                    color: index === root.selectedIndex
                                        ? root.selectedText
                                        : (modelData.cwd ? root.surfaceText : root.mutedColor)
                                    font.family: "monospace"
                                    font.pixelSize: 12
                                    elide: Text.ElideMiddle
                                }

                                Text {
                                    width: parent.width
                                    visible: text.length > 0
                                    text: {
                                        var bits = []
                                        if (modelData.branch) bits.push(modelData.branch + (modelData.dirty ? " ●" : ""))
                                        var dur = root._formatDuration(modelData.lastCmdMs)
                                        if (dur) bits.push(dur)
                                        var age = root._formatAge(modelData.ageSecs)
                                        if (age) bits.push(age)
                                        if (modelData.pid) bits.push("pid " + modelData.pid)
                                        return bits.join("  ·  ")
                                    }
                                    color: index === root.selectedIndex ? root.selectedText : root.mutedColor
                                    font.family: "monospace"
                                    font.pixelSize: 10
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }

                    // Graceful empty state: no sessions, or service not loaded.
                    Column {
                        width: parent.width
                        spacing: 6
                        visible: root.rows.length === 0

                        Text {
                            width: parent.width
                            text: root.service ? "No live Omarchy10k sessions"
                                               : "Omarchy10k service not loaded"
                            color: root.surfaceText
                            font.pixelSize: 13
                            font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Text {
                            width: parent.width
                            text: root.service
                                ? "Start a shell with the omarchy10k bash adapter enabled."
                                : "The plugin's service kind needs an Omarchy host that\nsupports service plugins (and the plugin enabled in shell.json)."
                            color: root.mutedColor
                            font.pixelSize: 11
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }
                }
            }
        }
    }
}
