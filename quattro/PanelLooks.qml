pragma ComponentBehavior: Bound
// LOOKS bucket of the Control Center (B3 decomposition — extracted from
// Panel.qml's looksTab, behavior-identical). State stays in Panel.qml and
// arrives via the injected `panel` property.
import QtQuick
import QtQuick.Controls
import qs.Commons
import qs.Ui
import "o10k"

Column {
    id: looksBucket

    // Injected panel root: config state, looks verbs, service hub.
    property var panel

    readonly property bool hasScripts: looksBucket.panel.omarchyService
        && looksBucket.panel.omarchyService.scripts
        && looksBucket.panel.omarchyService.scripts.length > 0

    // The daemon enforces its own 30s timeout and the traversal guard; the
    // panel only reports the outcome.
    function runScript(name) {
        if (!looksBucket.panel.omarchyService) return
        looksBucket.panel.toastMessage = "Running " + name + "\u2026"
        looksBucket.panel._showToast = true
        looksBucket.panel.omarchyService.runScript(name, function (resp) {
            looksBucket.panel.toastMessage = (resp && resp.status === "ok")
                ? (name + " \u2713")
                : (name + " failed: " + ((resp && resp.error) || "no daemon"))
            looksBucket.panel._showToast = true
        })
    }

    spacing: Style.space(12)

    ThemeBindRow {
        width: parent.width
        cfgFlat: looksBucket.panel._configFlat
        palettes: looksBucket.panel.omarchyService
            ? looksBucket.panel.omarchyService.palettes : ({})
        desktopTheme: looksBucket.panel.omarchyService
            ? looksBucket.panel.omarchyService.desktopTheme : ""
        // Returning to the desktop theme is a config write like any other,
        // so it goes through the panel's normal daemon path.
        onSyncRequested: looksBucket.panel.applyPalette("theme")
    }

    PanelKit.SectionLabel {
        label: "Looks"
        panel: looksBucket.panel
    }

    // Curated + user Looks, from the daemon's `looks` verb via the service.
    //
    // This list used to be a hardcoded array of the 8 curated names, so a
    // Look the user saved never appeared here even though the Gallery
    // listed it correctly — the drift that motivated one owner for this
    // state. The hardcoded set survives only as an offline fallback for
    // when no daemon is reachable.
    Grid {
        columns: 2
        spacing: Style.spacing.controlGap
        width: parent.width

        Repeater {
            model: (looksBucket.panel.omarchyService
                    && looksBucket.panel.omarchyService.looks
                    && looksBucket.panel.omarchyService.looks.length > 0)
                ? looksBucket.panel.omarchyService.looks
                : [
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
                    text: lookCard.modelData.label && lookCard.modelData.label.length > 0
                        ? lookCard.modelData.label
                        : lookCard.modelData.name
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

    // Quick actions: the daemon's script_list registry, which had no UI at
    // all before this — script_run was reachable only from the CLI.
    PanelKit.SectionLabel {
        label: "Quick Actions"
        panel: looksBucket.panel
        visible: looksBucket.hasScripts
    }

    Grid {
        columns: 2
        spacing: Style.spacing.controlGap
        width: parent.width
        visible: looksBucket.hasScripts

        Repeater {
            model: looksBucket.hasScripts
                ? looksBucket.panel.omarchyService.scripts : []
            delegate: Rectangle {
                id: actionCard
                required property var modelData
                width: (parent.width - Style.space(8)) / 2
                height: actionLabel.implicitHeight + Style.spacing.panelGap
                radius: Style.cornerRadius
                color: actionArea.containsMouse
                    ? Style.hoverFill
                    : Style.normalFillFor(looksBucket.panel.barForeground, Color.accent, Color.urgent)

                Text {
                    id: actionLabel
                    anchors.centerIn: parent
                    width: parent.width - Style.space(12)
                    horizontalAlignment: Text.AlignHCenter
                    // Scripts are named like files; the extension is noise.
                    text: "\u26a1 " + String(actionCard.modelData.name).replace(/\.[^.]+$/, "")
                    color: looksBucket.panel.barForeground
                    font.family: looksBucket.panel.bar ? looksBucket.panel.bar.fontFamily : Style.font.family
                    font.pixelSize: Style.font.bodySmall
                    elide: Text.ElideRight
                }

                MouseArea {
                    id: actionArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: looksBucket.runScript(actionCard.modelData.name)
                }
            }
        }
    }

    PanelSeparator {
        foreground: looksBucket.panel.barForeground
        visible: looksBucket.hasScripts
    }

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
