pragma Singleton
import QtQuick

// Test stub for the omarchy-shell Style singleton. Mirrors the real
// singleton's SHAPE (/usr/share/omarchy/shell/Commons/Style.qml) for the
// members the o10k kit touches, with stock-Omarchy defaults.
QtObject {
    id: stub

    // Mirrors Hyprland decoration:rounding. Omarchy ships 0.
    property int cornerRadius: 0
    property int gapsOut: 5

    property real normalFillAlpha: 0.04
    property real hoverFillAlpha: 0.08
    property real selectedFillAlpha: 0.18
    property real pressedFillAlpha: 0.22
    property real focusFillAlpha: 0.08

    property color normalFill: Qt.rgba(0.75, 0.79, 0.96, stub.normalFillAlpha)
    property color hoverFill: Qt.rgba(0.75, 0.79, 0.96, stub.hoverFillAlpha)
    property color selectedFill: Qt.rgba(0.48, 0.64, 0.97, stub.selectedFillAlpha)
    property color pressedFill: Qt.rgba(0.48, 0.64, 0.97, stub.pressedFillAlpha)
    property color focusFill: Qt.rgba(0.75, 0.79, 0.96, stub.focusFillAlpha)

    property var font: ({ family: "monospace", body: 12, bodySmall: 11,
                          caption: 10, subtitle: 14 })
    property var spacing: ({ controlGap: 6, controlHeight: 28,
                             controlPaddingX: 10, panelGap: 12 })

    function space(n) { return n }
}
