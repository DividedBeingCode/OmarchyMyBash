import QtQuick
import qs.Commons
import "Fx.js" as Fx

// The Control Center's selectable chip.
//
// This markup was copy-pasted five times — Studio.qml's tab rail,
// StudioTheme's theme and palette rows, StudioWizard's options and
// StudioPrompt's presets — each with slightly different padding, so the same
// control looked subtly different on every tab. Extracted once.
//
// The `swatch` and `swatches` slots are the point: a theme chip can carry the
// colors it will apply, which is what turns the Theme tab from a list of
// words into something you can choose from by looking.
//
// Unbound deliberately: consumers instantiate this from inline
// `Component {}` blocks, which a bound component cannot be used from.
Rectangle {
    id: chip

    property string label: ""
    property bool active: false
    /// Single color dot before the label. Ignored when `swatches` is set.
    property color swatch: "transparent"
    property bool hasSwatch: false
    /// Flat role → hex map; renders a full Swatches strip after the label.
    property var swatches: null
    /// Ordered hex stops; renders the palette's gradient sweep under the row.
    /// A palette's ramp is part of what you are choosing, so it belongs on the
    /// chip you choose it with.
    property var ramp: null
    /// Font for the label — a glyph chip wants the terminal's font, not the UI's.
    property string labelFont: Style.font.family
    property real labelSize: Style.font.bodySmall
    // `enabled` is inherited from Item -- declaring our own would shadow the
    // one MouseArea and the focus chain actually consult.

    signal clicked()

    readonly property bool _showStrip: chip.swatches
        && Object.keys(chip.swatches).length > 0
    readonly property bool _showRamp: chip.ramp && chip.ramp.length >= 2
        && !rampBar.isFlat

    implicitWidth: row.implicitWidth + Style.space(20)
    implicitHeight: Math.max(row.implicitHeight + Style.space(12), Style.space(30))
        + (chip._showRamp ? rampBar.height + Style.space(4) : 0)
    width: implicitWidth
    height: implicitHeight

    radius: Fx.radius(Style.cornerRadius) / 2
    opacity: chip.enabled ? 1.0 : 0.45
    color: chip.active
        ? Color.accent
        : (area.containsMouse ? Style.hoverFill : Style.normalFill)

    Row {
        id: row
        anchors.horizontalCenter: parent.horizontalCenter
        y: Style.space(6)
        spacing: Style.space(7)

        Rectangle {
            visible: chip.hasSwatch && !chip._showStrip
            anchors.verticalCenter: parent.verticalCenter
            width: Style.space(11)
            height: width
            radius: width / 2
            color: chip.swatch
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: chip.label
            // On an accent fill the label must flip to the background color,
            // or an accent-on-foreground pairing goes unreadable.
            color: chip.active ? Color.background : Color.foreground
            font.family: chip.labelFont
            font.pixelSize: chip.labelSize
            font.bold: chip.active
        }

        Swatches {
            visible: chip._showStrip
            anchors.verticalCenter: parent.verticalCenter
            colors: chip.swatches ? chip.swatches : ({})
            dotSize: Style.space(8)
            joined: true
        }
    }

    Ramp {
        id: rampBar
        visible: chip._showRamp
        stops: chip.ramp ? chip.ramp : []
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: Style.space(10)
        anchors.rightMargin: Style.space(10)
        anchors.bottomMargin: Style.space(5)
        barHeight: Style.space(5)
    }

    MouseArea {
        id: area
        anchors.fill: parent
        enabled: chip.enabled
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: chip.clicked()
    }
}
