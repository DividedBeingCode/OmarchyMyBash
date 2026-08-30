pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Second-level tab rail, inside a Studio tab.
//
// Three tabs stacked more than a screen: Prompt ran STYLE PRESET → SEPARATOR →
// PROMPT CHARACTER → ALL GLYPHS → BEHAVIOR, so the per-segment toggles — the
// controls changed most often — sat below a 78-tile glyph wall.
//
// Deliberately lighter than Studio.qml's top rail: underline rather than an
// accent fill, so the two levels read as a hierarchy instead of competing.
Item {
    id: rail

    /// Sub-tab labels, in order.
    property var tabs: []
    /// Selected index — an INPUT, owned by the call site.
    ///
    /// Every call site binds this (`current: promptTab.subTab`) and owns the
    /// value through `onSwitched`. Nothing in here ever assigns it: a single
    /// imperative write destroys that binding permanently, and the rail would
    /// then quietly stop following its tab's own state. Clamping happens in
    /// `clamped` instead, which is what the rail actually renders from.
    property int current: 0

    /// `current` pulled into range, for rendering.
    ///
    /// A tab body keyed on an out-of-range index renders nothing at all —
    /// a blank tab with no error anywhere — so the rail must never point at
    /// one, even while `tabs` is mid-swap and `current` still holds the old
    /// tab's index.
    readonly property int clamped: rail._clamp(rail.current)

    signal switched(int index)

    function _clamp(i) {
        if (!rail.tabs || rail.tabs.length === 0) return 0
        return Math.max(0, Math.min(rail.tabs.length - 1, i))
    }

    /// Notify that a sub-tab was chosen. The call site moves `current`; the
    /// rail does not move it itself, or the binding would be gone.
    function select(index) {
        rail.switched(rail._clamp(index))
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

                readonly property bool isCurrent: rail.clamped === item.index

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
