import QtQuick
import QtQuick.Effects
import qs.Commons
import "Fx.js" as Fx

// Elevated surface primitive for the Control Center.
//
// Deliberately UNBOUND (no pragma ComponentBehavior: Bound): bound inline
// components cannot be instantiated cross-file, which was the key finding
// of the C4 Panel decomposition. Consumers may be bound; this must not be.
Rectangle {
    id: card

    // "flat" | "rest" | "raised"
    property string elevation: "rest"
    // Accessibility escape hatch — shadows off costs nothing to draw.
    property bool shadowsEnabled: true

    // Resolved shadow parameters, exposed so tests and consumers can read
    // them without reaching into the RectangularShadow.
    readonly property var _elev: Fx.elevation(card.elevation, card.shadowsEnabled)
    readonly property real shadowOpacity: card._elev.opacity

    default property alias content: inner.data

    radius: Fx.radius(Style.cornerRadius)
    color: Style.normalFill

    // RectangularShadow computes the falloff analytically in one quad. A
    // MultiEffect/DropShadow here would require layer.enabled — an
    // offscreen buffer per card — which the shared integrated-GPU budget
    // does not have room for.
    RectangularShadow {
        anchors.fill: parent
        radius: card.radius
        blur: card._elev.blur
        spread: card._elev.spread
        offset.y: card._elev.offsetY
        color: Qt.rgba(0, 0, 0, card._elev.opacity)
        visible: card._elev.opacity > 0
        z: -1
    }

    Item {
        id: inner
        anchors.fill: parent
    }
}
