import QtQuick
import qs.Commons
import "Fx.js" as Fx
import "Store.js" as Store

// Theme bind indicator: says whether the terminal's colors follow the
// Omarchy desktop theme or are pinned away from it, and offers the way back.
//
// This exists because applying a curated palette writes theme.source =
// "hybrid" and silently desyncs the terminal from the desktop — with no
// indicator and no road back. The daemon already models the three states
// through the Look schema's palette directive; this surfaces them.
//
// Unbound by design (see Card.qml).
Item {
    id: bindRow

    // Flattened config ("theme.source", "theme.custom.accent", ...).
    property var cfgFlat: ({})
    // Curated palettes, keyed as in the daemon's `palettes` verb.
    property var palettes: ({})
    // Active Omarchy theme name, for naming what we diverge from.
    property string desktopTheme: ""

    readonly property var _bind: Store.themeBindState(bindRow.cfgFlat,
                                                      bindRow.palettes,
                                                      bindRow.desktopTheme)

    // `state` is taken by QtQuick.Item, hence the trailing underscore.
    readonly property string state_: bindRow._bind.state
    readonly property var syncPatch: bindRow._bind.syncPatch
    readonly property bool canSync: bindRow.state_ !== "bound"

    readonly property string glyph: bindRow.state_ === "bound" ? "\u{1F517}"
                                  : bindRow.state_ === "index" ? "▦"
                                  : "\u{1F4CC}"

    readonly property string summary: {
        var theme = bindRow.desktopTheme.length > 0
            ? bindRow.desktopTheme : "the desktop theme"
        if (bindRow.state_ === "bound")
            return "Colors follow " + theme
        if (bindRow.state_ === "index")
            return "Terminal palette · desktop is " + theme
        var label = bindRow._bind.paletteLabel.length > 0
            ? bindRow._bind.paletteLabel : "custom colors"
        return "Pinned to " + label + " · desktop is " + theme
    }

    signal syncRequested()

    function requestSync() {
        bindRow.syncRequested()
    }

    implicitHeight: label.implicitHeight + Style.space(12)

    Text {
        id: glyphText
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: bindRow.glyph
        font.pixelSize: Style.font.body
    }

    Text {
        id: label
        anchors.left: glyphText.right
        anchors.leftMargin: Style.space(6)
        anchors.right: syncChip.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        text: bindRow.summary
        // Pinned is the state worth noticing; bound is ambient.
        color: bindRow.state_ === "bound" ? Color.muted : Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
    }

    Rectangle {
        id: syncChip
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: bindRow.canSync ? syncLabel.implicitWidth + Style.space(14) : 0
        height: bindRow.canSync ? syncLabel.implicitHeight + Style.space(6) : 0
        radius: Fx.radius(Style.cornerRadius) / 2
        color: syncArea.containsMouse ? Style.hoverFill : Style.normalFill
        visible: bindRow.canSync

        Text {
            id: syncLabel
            anchors.centerIn: parent
            text: "Sync ↻"
            color: Color.accent
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        MouseArea {
            id: syncArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: bindRow.requestSync()
        }
    }
}
