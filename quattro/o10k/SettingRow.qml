import QtQuick
import qs.Commons
import "Fx.js" as Fx

// One settings row: label, control slot, modified-vs-default ink, and a
// per-row reset affordance.
//
// Panel.qml already had isModified()/resetConfigKey(), but applied by hand
// per row and inconsistently. Making it a component is what stops the
// Studio's larger surface area from multiplying that inconsistency.
//
// Unbound by design (see Card.qml).
Item {
    id: row

    property string label: ""
    property var value: undefined
    property var defaultValue: undefined

    // Modified ink requires BOTH sides to be known. A key with no recorded
    // default is not "modified" — it is unknown, and claiming otherwise
    // would offer a reset with no target.
    readonly property bool modified:
        row.value !== undefined
        && row.defaultValue !== undefined
        && row.value !== row.defaultValue

    signal resetRequested()

    function requestReset() {
        row.resetRequested()
    }

    default property alias control: controlSlot.data

    implicitHeight: Math.max(labelText.implicitHeight, controlSlot.childrenRect.height)
                    + Style.space(8)
    implicitWidth: parent ? parent.width : 320

    // Modified ink: a 3px accent bar on the leading edge.
    Rectangle {
        id: ink
        width: 3
        height: parent.height
        radius: 1
        color: Color.accent
        visible: row.modified
        anchors.left: parent.left
    }

    Text {
        id: labelText
        anchors.left: ink.right
        anchors.leftMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        text: row.label
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
    }

    Item {
        id: controlSlot
        anchors.right: resetChip.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        width: childrenRect.width
        height: childrenRect.height
    }

    Rectangle {
        id: resetChip
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: row.modified ? resetLabel.implicitWidth + Style.space(10) : 0
        height: row.modified ? resetLabel.implicitHeight + Style.space(4) : 0
        radius: Fx.radius(Style.cornerRadius) / 2
        color: resetArea.containsMouse ? Style.hoverFill : Style.normalFill
        visible: row.modified

        Text {
            id: resetLabel
            anchors.centerIn: parent
            text: "↺"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        MouseArea {
            id: resetArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: row.requestReset()
        }
    }
}
