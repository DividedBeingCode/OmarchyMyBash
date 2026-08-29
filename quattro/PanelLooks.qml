pragma ComponentBehavior: Bound
// LOOKS bucket of the Control Center (B3 decomposition — extracted from
// Panel.qml's looksTab, behavior-identical). State stays in Panel.qml and
// arrives via the injected `panel` property.
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Ui

Column {
    id: looksBucket

    // Injected panel root: config state, looks verbs, service hub.
    property var panel

    spacing: Style.space(12)

    PanelKit.SectionLabel {
        label: "Looks"
        panel: looksBucket.panel
    }

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
                id: lookCard
                required property var modelData
                width: (parent.width - Style.space(8)) / 2
                height: lookLabel.implicitHeight + Style.spacing.panelGap
                radius: Style.cornerRadius
                color: Style.normalFillFor(looksBucket.panel.barForeground, Color.accent, Color.urgent)

                Text {
                    id: lookLabel
                    anchors.centerIn: parent
                    text: lookCard.modelData.label
                    color: looksBucket.panel.barForeground
                    font.family: looksBucket.panel.bar ? looksBucket.panel.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.body
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: looksBucket.panel.applyLook(lookCard.modelData.name)
                }
            }
        }
    }

    TextField {
        id: lookNameField
        width: parent.width
        placeholderText: "Name for the current Look…"
        font.family: looksBucket.panel.bar ? looksBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.bodySmall
        color: looksBucket.panel.barForeground
        background: Rectangle {
            radius: Style.cornerRadius
            color: Style.normalFillFor(looksBucket.panel.barForeground, Color.accent, Color.urgent)
        }
    }

    PanelKit.ActionButton {
        label: "Save current as Look"
        panel: looksBucket.panel
        onClicked: looksBucket.panel.saveLook(lookNameField.text)
    }

    PanelKit.ActionButton {
        label: "Expand gallery"
        panel: looksBucket.panel
        onClicked: {
            if (looksBucket.panel.omarchyService && typeof looksBucket.panel.omarchyService.openGallery === "function")
                looksBucket.panel.omarchyService.openGallery()
            else
                looksBucket.panel.galleryRequested()
        }
    }

    PanelSeparator { foreground: looksBucket.panel.barForeground }

    PanelKit.SectionLabel {
        label: "Identity"
        panel: looksBucket.panel
    }

    Text {
        text: "Palette and theme fine-tuning live under Style."
        color: Color.muted
        font.family: looksBucket.panel.bar ? looksBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
        width: parent.width
    }
}
