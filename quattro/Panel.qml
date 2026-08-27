import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import qs.Commons
import qs.Ui

Panel {
    id: root
    moduleName: "community.omarchy10k"
    manageIpc: false

    property var anchorItem: null
    property var hostWidget: null

    function open() {
        root.controller.show()
        Model.loadConfig()
        Model.queryDaemon()
    }

    function close() {
        root.controller.hide()
    }

    function switchPanel(direction) {
        if (root.bar && typeof root.bar.switchPanelFrom === "function")
            return root.bar.switchPanelFrom(root.hostWidget || root, direction)
        return false
    }

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

                // Header
                Text {
                    text: "Omarchy10k Control Center"
                    color: root.barForeground
                    font.family: root.bar ? root.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.subtitle
                    font.bold: true
                }

                // Tab bar
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

                // Separator
                Rectangle {
                    width: parent.width - Style.space(32)
                    height: 1
                    color: Color.muted || "#414868"
                    x: Style.space(16)
                }

                // Tab content
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

    // ── Tab Components ────────────────────────────────────────────────────

    Component {
        id: appearanceTab
        Column {
            spacing: Style.space(10)

            ControlRow {
                label: "Preset"
                value: Model.config.prompt_layout || "omarchy"
                options: ["omarchy", "minimal", "powerline", "classic", "pure", "dense"]
                onChanged: function(val) { Model.setConfig("prompt.layout", val) }
            }

            ControlRow {
                label: "Theme"
                value: Model.config.theme_source || "omarchy"
                options: ["omarchy", "custom", "hybrid", "terminal"]
                onChanged: function(val) { Model.setConfig("theme.source", val) }
            }

            ControlRow {
                label: "Lines"
                value: Model.config.prompt_newline ? "Two-line" : "One-line"
                options: ["Two-line", "One-line"]
                onChanged: function(val) { Model.setConfig("prompt.newline", val === "Two-line") }
            }

            ControlRow {
                label: "Transient"
                value: Model.config.prompt_transient ? "On" : "Off"
                options: ["On", "Off"]
                onChanged: function(val) { Model.setConfig("prompt.transient", val === "On") }
            }

            ControlRow {
                label: "OS Icon"
                value: Model.config.os_icon || "arch"
                options: ["arch", "linux", "omarchy", "none"]
                onChanged: function(val) { Model.setConfig("segments.os.icon", val) }
            }
        }
    }

    Component {
        id: contextTab
        Column {
            spacing: Style.space(10)

            ControlRow {
                label: "Git"
                value: Model.config.git_mode || "adaptive"
                options: ["adaptive", "compact", "expanded", "hidden"]
                onChanged: function(val) { Model.setConfig("git.mode", val) }
            }

            ControlRow {
                label: "Duration"
                value: (Model.config.cmd_duration_ms || 1500) + "ms"
                options: ["500ms", "1000ms", "1500ms", "3000ms", "5000ms"]
                onChanged: function(val) {
                    var ms = parseInt(val)
                    Model.setConfig("segments.command_duration.show_above_ms", ms)
                }
            }

            ControlRow {
                label: "SSH"
                value: Model.config.ssh_show || "auto"
                options: ["auto", "always", "never"]
                onChanged: function(val) { Model.setConfig("segments.ssh.show", val) }
            }

            ControlRow {
                label: "Exit Status"
                value: Model.config.exit_signal_names ? "Signal names" : "Codes only"
                options: ["Signal names", "Codes only"]
                onChanged: function(val) {
                    Model.setConfig("segments.exit_status.show_signal_name", val === "Signal names")
                }
            }
        }
    }

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

            StatusRow { label: "ble.sh"; status: Model.daemon.blesh_status || "checking..." }
            StatusRow { label: "Atuin"; status: Model.daemon.atuin_status || "checking..." }
            StatusRow { label: "Mise"; status: Model.daemon.mise_status || "checking..." }
            StatusRow { label: "Zoxide"; status: Model.daemon.zoxide_status || "checking..." }
            StatusRow { label: "fzf"; status: Model.daemon.fzf_status || "checking..." }
        }
    }

    Component {
        id: advancedTab
        Column {
            spacing: Style.space(10)

            ActionButton {
                label: "Open Config File"
                onClicked: {
                    var configPath = Model.configPath()
                    root.bar.run("${EDITOR:-nano} " + configPath)
                }
            }

            ActionButton {
                label: "Run Doctor"
                onClicked: root.bar.run("omarchy10k doctor")
            }

            ActionButton {
                label: "Reload Config"
                onClicked: Model.reloadDaemon()
            }

            ActionButton {
                label: "Reset to Defaults"
                dangerous: true
                onClicked: Model.resetConfig()
            }

            // Daemon status
            Rectangle {
                width: parent.width
                height: daemonInfo.implicitHeight + Style.space(12)
                radius: Style.space(4)
                color: Color.darker_background || "#0e0e14"

                Column {
                    id: daemonInfo
                    anchors.fill: parent
                    anchors.margins: Style.space(8)
                    spacing: Style.space(4)

                    Text {
                        text: "Daemon: " + (Model.daemon.status || "unknown")
                        color: Color.green || "#9ece6a"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "PID: " + (Model.daemon.pid || "—")
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                    Text {
                        text: "Version: " + (Model.daemon.version || "—")
                        color: Color.muted || "#414868"
                        font.family: root.bar ? root.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }
            }
        }
    }

    // ── Reusable Components ──────────────────────────────────────────────

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
            color: status.startsWith("✓")
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
