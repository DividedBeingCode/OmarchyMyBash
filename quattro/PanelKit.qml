// Shared small pieces for the Control Center buckets (B3 decomposition).
// The root Item is never instantiated; consumers use the qualified inline
// component forms (PanelKit.SectionLabel, PanelKit.ControlRow, …).
// QML ids do not cross file boundaries, so every component receives the
// panel root via the injected `panel` property, bound at the instantiation
// site.
import QtQuick
import qs.Commons

Item {
    // Small-caps style section marker — quieter than a bold body label,
    // consistent across every tab.
    component SectionLabel: Text {
        id: sectionLabel
        property string label
        // Injected panel root (bar-aware font family).
        property var panel

        text: label.toUpperCase()
        color: Color.muted
        font.family: panel && panel.bar ? panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.caption
        font.bold: true
        font.letterSpacing: 1.4
        // Trailing letterSpacing / synthesized-bold overflow is not always
        // covered by implicitWidth; size to the painted text so a Row places
        // the next sibling after the glyphs (DOCTOR header vs raw-toggle
        // chip overlap, observed live).
        width: Math.max(implicitWidth, paintedWidth)
    }

    // Label + option chips + modified-ink/reset affordance for one config key.
    component ControlRow: Row {
        id: controlRow
        property string label
        property string value
        property var options: []
        property string configKey
        // Injected panel root (isModified / resetConfigKey / bar ink).
        property var panel
        signal changed(string val)

        // Modified-vs-default: accent ink bar on the left edge + reset chip
        // after the options, both only when this row's key diverges from the
        // daemon's defaults snapshot.
        readonly property bool modified: configKey.length > 0
            && panel ? panel.isModified(configKey) : false

        width: parent ? parent.width : 200
        spacing: Style.space(8)

        Rectangle {
            width: 3
            height: controlRow.height
            radius: 1
            visible: controlRow.modified
            color: Color.accent
        }

        Text {
            width: controlRow.width * 0.35
            text: controlRow.label
            color: controlRow.panel.barForeground || "#a9b1d6"
            font.family: controlRow.panel.bar ? controlRow.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
            verticalAlignment: Text.AlignVCenter
            height: controlRow.height
        }

        Row {
            spacing: Style.spacing.controlGap
            Repeater {
                model: controlRow.options
                    delegate: Rectangle {
                    id: optChip
                    required property var modelData

                    width: optText.implicitWidth + Style.spacing.controlPaddingX * 2
                    height: Style.spacing.controlHeight
                    radius: Style.cornerRadius
                    color: controlRow.value === optChip.modelData
                        ? (Color.accent)
                        : (Style.normalFillFor(controlRow.panel.barForeground, Color.accent, Color.urgent))

                    Text {
                        id: optText
                        anchors.centerIn: parent
                        text: optChip.modelData
                        color: controlRow.value === optChip.modelData
                            ? (Color.background)
                            : (controlRow.panel.barForeground || "#a9b1d6")
                        font.family: controlRow.panel.bar ? controlRow.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: controlRow.changed(optChip.modelData)
                    }
                }
            }
        }

        Rectangle {
            width: Style.spacing.controlHeight * 0.8
            height: Style.spacing.controlHeight
            radius: Style.cornerRadius
            visible: controlRow.modified
            color: Style.normalFillFor(controlRow.panel.barForeground, Color.accent, Color.urgent)

            Text {
                anchors.centerIn: parent
                text: "\u21ba"
                color: Color.muted
                font.family: controlRow.panel.bar ? controlRow.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.bodySmall
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: controlRow.panel.resetConfigKey(controlRow.configKey)
            }
        }
    }

    // Tool detection row: label on the left, ✓/✗ status text on the right.
    component StatusRow: Row {
        id: statusRow
        property string label
        property string status
        // Injected panel root (bar-aware font family).
        property var panel

        width: parent ? parent.width : 200
        spacing: Style.space(8)

        Text {
            width: statusRow.width * 0.35
            text: statusRow.label
            color: statusRow.panel.barForeground || "#a9b1d6"
            font.family: statusRow.panel.bar ? statusRow.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }
        Text {
            text: statusRow.status
            color: statusRow.status.indexOf("\u2713") >= 0
                ? (Color.accent)
                : (Color.muted)
            font.family: statusRow.panel.bar ? statusRow.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }
    }

    // Full-width clickable action button.
    component ActionButton: Rectangle {
        id: actionButton
        property string label
        property bool dangerous: false
        // Injected panel root (bar ink).
        property var panel
        signal clicked()

        width: parent ? parent.width : 200
        height: Style.spacing.controlHeight
        radius: Style.cornerRadius
        color: mouseArea.containsMouse
            ? (dangerous ? (Color.urgent) : (Color.accent))
            : (Style.normalFillFor(actionButton.panel.barForeground, Color.accent, Color.urgent))

        Text {
            id: btnText
            anchors.centerIn: parent
            text: actionButton.label
            color: mouseArea.containsMouse
                ? (Color.background)
                : (actionButton.panel.barForeground || "#a9b1d6")
            font.family: actionButton.panel.bar ? actionButton.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.body
        }

        MouseArea {
            id: mouseArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: actionButton.clicked()
        }
    }

    // Glyph picker: label column + wrapping flow of selectable chips.
    component GlyphRow: Column {
        id: glyphRow
        property string label
        property string configKey
        property string currentValue
        property var glyphs: []
        property var customHandler: null
        // Injected panel root (setConfigValue / bar ink).
        property var panel

        spacing: Style.space(4)
        width: parent ? parent.width : 200

        Row {
            spacing: Style.space(8)
            width: glyphRow.width

            Text {
                id: glyphLabel
                width: parent.width * 0.26
                text: glyphRow.label
                color: glyphRow.panel.barForeground || "#a9b1d6"
                font.family: glyphRow.panel.bar ? glyphRow.panel.bar.fontFamily : Style.font.family
                font.pixelSize: Style.font.body
                verticalAlignment: Text.AlignVCenter
                height: glyphFlow.height
            }

            Flow {
                id: glyphFlow
                width: parent.width - glyphLabel.width - Style.space(8)
                spacing: Style.spacing.controlGap

                Repeater {
                    model: glyphRow.glyphs
                    delegate: Rectangle {
                        // Size from the leaf Text metrics directly: sizing via
                        // the Column's implicit size while the Column anchors
                        // centerIn the delegate created a polish() loop that
                        // made panel scrolling crawl.
                        id: chip
                        required property var modelData

                        implicitWidth: glyphGlyph.implicitWidth + Style.space(4) + glyphChipLabel.implicitWidth + Style.spacing.controlPaddingX * 2
                        implicitHeight: Style.spacing.controlHeight
                        radius: Style.cornerRadius
                        color: glyphRow.currentValue === chip.modelData.key
                            ? (Color.accent)
                            : (Style.normalFillFor(glyphRow.panel.barForeground, Color.accent, Color.urgent))

                        Row {
                            anchors.centerIn: parent
                            spacing: Style.space(4)

                            Text {
                                id: glyphGlyph
                                text: chip.modelData.glyph
                                color: glyphRow.currentValue === chip.modelData.key
                                    ? (Color.background)
                                    : (Color.foreground)
                                font.family: glyphRow.panel.bar ? glyphRow.panel.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.body
                            }

                            Text {
                                id: glyphChipLabel
                                text: chip.modelData.label
                                color: glyphRow.currentValue === chip.modelData.key
                                    ? Qt.lighter(Color.background, 1.4)
                                    : (Color.muted)
                                font.family: glyphRow.panel.bar ? glyphRow.panel.bar.fontFamily : Style.font.family
                                font.pixelSize: Style.font.caption
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (glyphRow.customHandler) {
                                    glyphRow.customHandler(chip.modelData.key)
                                } else {
                                    glyphRow.panel.setConfigValue(glyphRow.configKey, chip.modelData.key)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
