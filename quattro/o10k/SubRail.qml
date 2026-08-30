pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Second-level tab rail, inside a Studio tab.
//
// Three tabs stacked more than a screen: Prompt ran STYLE PRESET → SEPARATOR →
// PROMPT CHARACTER → ALL GLYPHS → BEHAVIOR, so the per-segment toggles — the
// controls changed most often — sat below a 76-tile glyph wall.
//
// Deliberately lighter than Studio.qml's top rail: underline rather than an
// accent fill, so the two levels read as a hierarchy instead of competing.
Item {
    id: rail

    /// Sub-tab labels, in order.
    property var tabs: []
    /// Selected index. Always clamped into range — a tab body keyed on this
    /// would render nothing at all for an out-of-range value, which looks
    /// like a blank tab with no error anywhere.
    property int current: 0

    signal switched(int index)

    onTabsChanged: rail.current = rail._clamp(rail.current)
    onCurrentChanged: {
        var c = rail._clamp(rail.current)
        if (c !== rail.current) rail.current = c
    }

    function _clamp(i) {
        if (!rail.tabs || rail.tabs.length === 0) return 0
        return Math.max(0, Math.min(rail.tabs.length - 1, i))
    }

    /// Select a sub-tab and notify. Setting `current` directly does not emit.
    function select(index) {
        var c = rail._clamp(index)
        rail.current = c
        rail.switched(c)
    }

    visible: rail.tabs && rail.tabs.length > 1
    implicitHeight: visible ? row.implicitHeight : 0
    height: implicitHeight

    Row {
        id: row
        spacing: Style.space(4)

        Repeater {
            model: rail.tabs

            delegate: Item {
                id: item
                required property string modelData
                required property int index

                implicitWidth: text.implicitWidth + Style.space(18)
                implicitHeight: text.implicitHeight + Style.space(14)

                readonly property bool isCurrent: rail.current === item.index

                Text {
                    id: text
                    anchors.centerIn: parent
                    text: item.modelData
                    color: item.isCurrent ? Color.foreground : Color.muted
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                    font.bold: item.isCurrent
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: Style.space(6)
                    anchors.rightMargin: Style.space(6)
                    height: 2
                    radius: 1
                    visible: item.isCurrent
                    color: Color.accent
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: rail.select(item.index)
                }
            }
        }
    }
}
