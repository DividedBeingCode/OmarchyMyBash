import QtQuick
import qs.Commons
import qs.Ui
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
    /// Whether colors are LOCKED to the desktop theme, as opposed to merely
    /// happening to match it right now. A lock survives applying a Look.
    property bool locked: false

    signal lockToggled(bool on)

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
        anchors.right: lockChip.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        text: bindRow.summary
        // Pinned is the state worth noticing; bound is ambient.
        color: bindRow.state_ === "bound" ? Color.muted : Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
    }

    // The lock. Distinct from the bound/pinned STATE above: that says what
    // your colors happen to be right now, this says they must stay that way
    // through every Look you apply.
    Rectangle {
        id: lockChip
        anchors.right: syncChip.visible ? syncChip.left : parent.right
        anchors.rightMargin: syncChip.visible ? Style.space(6) : 0
        anchors.verticalCenter: parent.verticalCenter
        width: lockLabel.implicitWidth + Style.space(14)
        height: lockLabel.implicitHeight + Style.space(6)
        radius: Fx.radius(Style.cornerRadius) / 2
        color: bindRow.locked
            ? Color.accent
            : (lockArea.containsMouse ? Style.hoverFill : Style.normalFill)

        Text {
            id: lockLabel
            anchors.centerIn: parent
            text: bindRow.locked ? "Locked to desktop \u{1F512}" : "Lock to desktop"
            color: bindRow.locked ? Color.background : Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        MouseArea {
            id: lockArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: bindRow.lockToggled(!bindRow.locked)
        }

        PanelToolTip {
            visible: lockArea.containsMouse
            text: bindRow.locked
                ? "Applying a Look will change its shape but keep these colors."
                : "Keep these colors through every Look you apply."
        }
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
