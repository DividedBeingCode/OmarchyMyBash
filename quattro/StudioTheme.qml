pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx
import "Model.js" as Model

// Studio → Theme tab: the bind state machine and the two things that drive
// it — Omarchy's own themes, and the terminal-only palette override.
//
// Applying an Omarchy theme is DESKTOP-WIDE and shells out to
// `omarchy theme set`; this surface never writes theme files, matching the
// rule both sibling projects hold.
Flickable {
    id: themeTab

    property var service: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    property var themes: []
    property string current: ""

    readonly property var cfg: themeTab.service ? themeTab.service.cfgFlat : ({})
    readonly property var palettes:
        (themeTab.service && themeTab.service.palettes
         && Object.keys(themeTab.service.palettes).length > 0)
            ? themeTab.service.palettes : Model.CURATED_PALETTES

    Component.onCompleted: themeTab.refresh()

    function refresh() {
        themeLister.running = true
        currentProbe.running = true
    }

    Process {
        id: themeLister
        command: ["omarchy", "theme", "list"]
        stdout: StdioCollector {
            onStreamFinished: {
                var out = []
                var lines = String(this.text).split("\n")
                for (var i = 0; i < lines.length; i++) {
                    var t = lines[i].trim()
                    if (t.length > 0) out.push(t)
                }
                themeTab.themes = out
            }
        }
    }

    Process {
        id: currentProbe
        command: ["omarchy", "theme", "current"]
        stdout: StdioCollector {
            onStreamFinished: themeTab.current = String(this.text).trim()
        }
    }

    Process { id: themeSetter }

    function applyTheme(name) {
        themeSetter.command = ["omarchy", "theme", "set", name]
        themeSetter.running = true
        recheckTimer.restart()
    }

    Timer {
        id: recheckTimer
        interval: 1200
        onTriggered: themeTab.refresh()
    }

    Column {
        id: body
        width: themeTab.width
        spacing: Style.space(14)

        ThemeBindRow {
            width: parent.width
            cfgFlat: themeTab.cfg
            palettes: themeTab.palettes
            desktopTheme: themeTab.service ? themeTab.service.desktopTheme : themeTab.current
            onSyncRequested: {
                if (themeTab.service && themeTab.service.applyPaletteTheme)
                    themeTab.service.applyPaletteTheme()
            }
        }

        // ── Omarchy themes (desktop-wide) ──────────────────────────────────
        Text {
            text: "OMARCHY THEMES"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Applies desktop-wide — every themed app follows, not just the terminal."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: themeTab.themes
                delegate: Rectangle {
                    id: themeChip
                    required property string modelData
                    readonly property bool active: themeTab.current === themeChip.modelData
                    width: themeText.implicitWidth + Style.space(20)
                    height: themeText.implicitHeight + Style.space(12)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: themeChip.active ? Color.accent
                        : (themeArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        id: themeText
                        anchors.centerIn: parent
                        text: themeChip.modelData
                        color: themeChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        id: themeArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: themeTab.applyTheme(themeChip.modelData)
                    }
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Terminal-only palette override ─────────────────────────────────
        Text {
            text: "PIN TERMINAL COLORS"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Overrides the prompt palette only, leaving the desktop theme alone. "
                  + "This is what unbinds the two — use Sync above to rebind."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: Object.keys(themeTab.palettes)
                delegate: Rectangle {
                    id: palChip
                    required property string modelData
                    readonly property var pal: themeTab.palettes[palChip.modelData]
                    readonly property bool active:
                        String(themeTab.cfg["theme.custom.accent"] || "").toLowerCase()
                        === String(palChip.pal ? palChip.pal.accent : "").toLowerCase()
                    width: palRow.implicitWidth + Style.space(18)
                    height: Style.space(34)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: palChip.active ? Color.accent
                        : (palArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Row {
                        id: palRow
                        anchors.centerIn: parent
                        spacing: Style.space(6)

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Style.space(12)
                            height: Style.space(12)
                            radius: width / 2
                            color: palChip.pal && palChip.pal.accent
                                ? palChip.pal.accent : Color.muted
                        }

                        Text {
                            text: palChip.pal && palChip.pal.label
                                ? palChip.pal.label : palChip.modelData
                            color: palChip.active ? Color.background : Color.foreground
                            font.family: Style.font.family
                            font.pixelSize: Style.font.bodySmall
                        }
                    }

                    MouseArea {
                        id: palArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (themeTab.service && themeTab.service.applyPalette)
                                themeTab.service.applyPalette(palChip.modelData)
                        }
                    }
                }
            }
        }
    }
}
