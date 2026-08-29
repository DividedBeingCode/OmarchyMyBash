pragma ComponentBehavior: Bound
// SYSTEM bucket of the Control Center (B3 decomposition — extracted from
// Panel.qml's systemTab, behavior-identical). State stays in Panel.qml and
// arrives via the injected `panel` property; daemon-facing actions go
// through panel functions (runDoctor, copyConfigToClipboard, …) so the
// panel's Process/Socket ids never leak into this file.
import QtQuick
import qs.Commons
import qs.Ui

Column {
    id: systemBucket

    // Injected panel root: tool status, daemon status, sessions, actions.
    property var panel

    // Collapsed/raw toggle chip for the Doctor and Benchmark cards — the
    // parsed summary is the default surface; raw daemon output stays one
    // click away so nothing is lost.
    component RawToggle: Rectangle {
        id: rawToggle
        property bool active: false
        // Injected panel root (bar ink).
        property var panel
        signal toggled()

        width: rawToggleText.implicitWidth + Style.spacing.controlPaddingX * 2
        height: Style.spacing.controlHeight
        radius: Style.cornerRadius
        color: rawToggle.active
            ? (Color.accent)
            : (Style.normalFillFor(rawToggle.panel.barForeground, Color.accent, Color.urgent))

        Text {
            id: rawToggleText
            anchors.centerIn: parent
            text: rawToggle.active ? "raw \u25be" : "raw \u25b8"
            color: rawToggle.active ? (Color.background) : (rawToggle.panel.barForeground || "#a9b1d6")
            font.family: rawToggle.panel.bar ? rawToggle.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.caption
        }

        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: rawToggle.toggled()
        }
    }

    spacing: Style.space(12)

    Text {
        text: "Shell integrations are configured through their own tools.\nOmarchy10k coordinates their lifecycle via the hook broker."
        color: Color.muted
        font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
        width: parent.width
    }

    PanelKit.StatusRow { label: "ble.sh"; status: systemBucket.panel.bleshStatus; panel: systemBucket.panel }
    PanelKit.StatusRow { label: "Atuin"; status: systemBucket.panel.atuinStatus; panel: systemBucket.panel }


    PanelKit.StatusRow { label: "Mise"; status: systemBucket.panel.miseStatus; panel: systemBucket.panel }


    PanelKit.StatusRow { label: "Zoxide"; status: systemBucket.panel.zoxideStatus; panel: systemBucket.panel }
    PanelKit.StatusRow { label: "fzf"; status: systemBucket.panel.fzfStatus; panel: systemBucket.panel }
    PanelKit.SectionLabel { label: "Remediations"; visible: systemBucket.panel.missingTools.length > 0; panel: systemBucket.panel }

    Repeater {
        model: systemBucket.panel.missingTools
        delegate: Rectangle {
            id: remediationCard
            required property var modelData
            width: parent.width
            height: remediationBody.implicitHeight + Style.space(12)
            radius: Style.cornerRadius
            color: Qt.darker(Color.background, 1.3)

            Column {
                id: remediationBody
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: Style.space(6)
                spacing: Style.space(4)

                Row {
                    spacing: Style.space(8)

                    Text {
                        text: remediationCard.modelData.name
                        color: Color.accent
                        font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.body
                        font.bold: true
                    }

                    Rectangle {
                        width: remediationCopyText.implicitWidth + Style.space(12)
                        height: Style.spacing.controlHeight
                        radius: Style.cornerRadius
                        color: remediationCopyArea.containsMouse
                            ? (Color.accent)
                            : (Style.normalFillFor(systemBucket.panel.barForeground, Color.accent, Color.urgent))

                        Text {
                            id: remediationCopyText
                            anchors.centerIn: parent
                            text: "COPY"
                            color: remediationCopyArea.containsMouse
                                ? (Color.background)
                                : (systemBucket.panel.barForeground || "#a9b1d6")
                            font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.caption
                            font.bold: true
                        }

                        MouseArea {
                            id: remediationCopyArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: systemBucket.panel.copyInstallCommand(remediationCard.modelData.name, remediationCard.modelData.cmd)
                        }
                    }
                }

                Text {
                    width: parent.width
                    text: remediationCard.modelData.why
                    color: Color.muted
                    font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.caption
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    PanelSeparator { foreground: systemBucket.panel.barForeground }


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
                text: "Daemon: " + systemBucket.panel.daemonStatus
                color: systemBucket.panel.daemonStatus === "running"
                    ? (Color.accent)
                    : (Color.urgent)
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
            Text {
                text: "PID: " + (systemBucket.panel.daemonPid || "\u2014")
                color: Color.muted
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
            Text {
                text: "Version: " + (systemBucket.panel.daemonVersion || "\u2014") + " (protocol " + (systemBucket.panel.daemonProtocolVersion || "\u2014") + ")"
                color: Color.muted
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
            Text {
                text: systemBucket.panel.daemonProtocolVersion
                    ? ("Protocol status: " + (systemBucket.panel._featureAvailable("0.3") ? "full (v0.3+)" : "degraded (upgrade daemon)"))
                    : "Protocol status: unknown"
                color: systemBucket.panel._featureAvailable("0.3")
                    ? (Color.accent)
                    : (Color.muted)
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
            Text {
                text: "Sessions: " + systemBucket.panel.sessionList.length
                color: Color.muted
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
        }
    }

    Repeater {
        model: systemBucket.panel.sessionList
        delegate: Rectangle {
            id: sessionCard
            required property var modelData
            required property int index
            width: parent.width
            height: Style.spacing.controlHeight
            radius: Style.cornerRadius
            color: sessionCard.index === systemBucket.panel.activeSessionIndex
                ? (Color.accent)
                : (Style.normalFillFor(systemBucket.panel.barForeground, Color.accent, Color.urgent))

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: systemBucket.panel.connectToSession(sessionCard.index)
            }

            Row {
                id: sessionRow
                anchors.fill: parent
                anchors.margins: Style.space(4)
                spacing: Style.space(8)

                Text {
                    id: sessionPidText
                    text: "Shell " + sessionCard.modelData.shellPid
                    color: sessionCard.index === systemBucket.panel.activeSessionIndex
                        ? (Color.background)
                        : (systemBucket.panel.barForeground || "#a9b1d6")
                    font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.caption
                    font.bold: sessionCard.index === systemBucket.panel.activeSessionIndex
                }
                Text {
                    text: sessionCard.modelData.cwd || ""
                    color: Color.muted
                    font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
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
                visible: sessionCard.modelData.cwd.length > 0

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
                    onClicked: systemBucket.panel.openFloatingTerminal(sessionCard.modelData.cwd)
                }
            }
        }
    }


    PanelKit.ActionButton {
        label: "Open Config File"
        panel: systemBucket.panel
        onClicked: systemBucket.panel.openConfigInEditor()
    }

    PanelKit.ActionButton {
        label: "Run Doctor"
        panel: systemBucket.panel
        onClicked: systemBucket.panel.runDoctor()
    }

    Row {
        spacing: Style.space(6)
        width: parent.width

        PanelKit.ActionButton {
            label: "Copy Config"
            panel: systemBucket.panel
            width: (parent.width - Style.space(6)) / 2
            onClicked: systemBucket.panel.copyConfigToClipboard()
        }

        PanelKit.ActionButton {
            label: "Paste Config"
            panel: systemBucket.panel
            width: (parent.width - Style.space(6)) / 2
            onClicked: systemBucket.panel.pasteConfigFromClipboard()
        }
    }

    Row {
        spacing: Style.space(8)

        PanelKit.SectionLabel {
            label: "Doctor"
            panel: systemBucket.panel
            anchors.verticalCenter: parent.verticalCenter
        }

        RawToggle {
            active: systemBucket.panel._doctorRaw
            onToggled: systemBucket.panel._doctorRaw = !systemBucket.panel._doctorRaw
            panel: systemBucket.panel
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Column {
        visible: !systemBucket.panel._doctorRaw && systemBucket.panel.doctorCards.length > 0
        width: parent.width
        spacing: Style.space(6)

        Repeater {
            model: systemBucket.panel.doctorCards
            delegate: Rectangle {
                id: doctorCard
                required property var modelData
                width: parent.width
                height: doctorCardBody.implicitHeight + Style.space(12)
                radius: Style.cornerRadius
                color: Qt.darker(Color.background, 1.3)

                Column {
                    id: doctorCardBody
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: Style.space(6)
                    spacing: Style.space(2)

                    Row {
                        spacing: Style.space(6)

                        Text {
                            text: doctorCard.modelData.glyph
                            color: doctorCard.modelData.status === "ok"
                                ? (Color.accent)
                                : (doctorCard.modelData.status === "bad" ? (Color.urgent) : (Color.muted))
                            font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.body
                            font.bold: true
                        }

                        Text {
                            text: doctorCard.modelData.name
                            color: systemBucket.panel.barForeground || "#a9b1d6"
                            font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                            font.pixelSize: Style.font.body
                        }

                        Rectangle {
                            width: doctorChipText.implicitWidth + Style.space(10)
                            height: Style.spacing.controlHeight
                            radius: Style.cornerRadius
                            color: doctorCard.modelData.status === "ok"
                                ? (Color.accent)
                                : (doctorCard.modelData.status === "bad" ? (Color.urgent) : ("transparent"))
                            border.width: doctorCard.modelData.status === "skip" ? 1 : 0
                            border.color: Color.muted

                            Text {
                                id: doctorChipText
                                anchors.centerIn: parent
                                text: doctorCard.modelData.status === "ok"
                                    ? "OK"
                                    : (doctorCard.modelData.status === "bad" ? "FAIL" : "SKIP")
                                color: doctorCard.modelData.status === "skip" ? (Color.muted) : (Color.background)
                                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption
                                font.bold: true
                            }
                        }
                    }

                    Text {
                        width: parent.width
                        text: doctorCard.modelData.detail
                        visible: doctorCard.modelData.detail.length > 0
                        color: Color.muted
                        font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }

    Rectangle {
        visible: systemBucket.panel.doctorOutput.length > 0 && (systemBucket.panel._doctorRaw || systemBucket.panel.doctorCards.length === 0)
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
                text: systemBucket.panel.doctorOutput
                color: Color.foreground
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
            }
        }
    }

    PanelKit.ActionButton {
        label: "Reload Config"
        panel: systemBucket.panel
        onClicked: systemBucket.panel.reloadConfig()
    }

    PanelKit.ActionButton {
        label: "Run Benchmark"
        panel: systemBucket.panel
        onClicked: systemBucket.panel.runBenchmark()
    }

    Row {
        spacing: Style.space(8)

        PanelKit.SectionLabel {
            label: "Benchmark"
            panel: systemBucket.panel
            anchors.verticalCenter: parent.verticalCenter
        }

        RawToggle {
            visible: systemBucket.panel.benchStats !== null
            active: systemBucket.panel._benchRaw
            onToggled: systemBucket.panel._benchRaw = !systemBucket.panel._benchRaw
            panel: systemBucket.panel
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Rectangle {
        visible: systemBucket.panel.benchStats !== null && !systemBucket.panel._benchRaw
        width: parent.width
        height: benchCardBody.implicitHeight + Style.space(12)
        radius: Style.cornerRadius
        color: Qt.darker(Color.background, 1.3)

        Column {
            id: benchCardBody
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.space(6)
            spacing: Style.space(4)

            Text {
                text: systemBucket.panel.benchStats ? systemBucket.panel.benchStats.spark : ""
                color: Color.accent
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
            }

            Text {
                width: parent.width
                text: systemBucket.panel.benchStats
                    ? ("best " + systemBucket.panel._fmtMs(systemBucket.panel.benchStats.best)
                       + "  \u00b7  median " + systemBucket.panel._fmtMs(systemBucket.panel.benchStats.median)
                       + "  \u00b7  worst " + systemBucket.panel._fmtMs(systemBucket.panel.benchStats.worst)
                       + "  (" + systemBucket.panel.benchStats.n + " values)")
                    : ""
                color: Color.muted
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.caption
            }
        }
    }

    Rectangle {
        visible: systemBucket.panel.benchmarkOutput.length > 0 && (systemBucket.panel._benchRaw || systemBucket.panel.benchStats === null)
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
                text: systemBucket.panel.benchmarkOutput
                color: Color.foreground
                font.family: systemBucket.panel.bar ? systemBucket.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
            }
        }
    }

    PanelKit.ActionButton {
        label: "Reset to Defaults"
        dangerous: true
        panel: systemBucket.panel
        onClicked: systemBucket.panel.resetToDefaults()
    }
}
