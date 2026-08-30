pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// A palette's colors, shown as colors.
//
// The Theme tab listed 22 themes and 16 palettes as grey text with, at most,
// a single 12px dot. A color picker that does not show colors makes the user
// apply a palette just to find out what it is — which is the slowest possible
// way to browse thirty of them.
//
// Bound: the Repeater delegate reads `dotSize` and the strip length off the
// root to size and round itself. The kit's "must stay unbound" rule applies
// to INLINE `Component {}` blocks (Card, SettingRow), not to file components.
Item {
    id: swatches

    /// Flat role → hex map, as the `palettes` verb's `colors` field returns.
    /// Read directly rather than reconstructed from a theme patch — the
    /// daemon already flattened it precisely so this component would not have
    /// to know the patch shape.
    property var colors: ({})
    /// Roles to draw, in order. Hue roles first: they are what distinguishes
    /// one palette from another at a glance, while background and foreground
    /// are nearly the same across every dark scheme.
    property var roles: ["accent", "red", "green", "yellow", "blue", "magenta", "cyan", "orange"]
    property real dotSize: Style.space(10)
    property real gap: Style.space(3)
    /// Draw as a continuous bar rather than separate dots. Reads as a single
    /// object at card scale, where eight loose dots read as noise.
    property bool joined: false
    /// Stretch the strip to exactly this width, dividing it between the
    /// roles present. Eight small dots show that a palette EXISTS; a band
    /// across the full card shows what the palette IS.
    property real stretchTo: 0

    readonly property real cellWidth: swatches.stretchTo > 0
        ? swatches.stretchTo / Math.max(1, swatches.present.length)
        : swatches.dotSize

    readonly property var present: {
        var out = []
        for (var i = 0; i < swatches.roles.length; i++) {
            var hex = swatches.colors ? swatches.colors[swatches.roles[i]] : undefined
            // A derived palette can legitimately omit a role the theme had no
            // source for. Drawing a transparent hole would look like a bug.
            if (hex !== undefined && hex !== null && String(hex).length > 0)
                out.push(String(hex))
        }
        return out
    }

    implicitWidth: swatches.present.length === 0
        ? 0
        : swatches.stretchTo > 0
            ? swatches.stretchTo
            : (swatches.joined
                ? swatches.present.length * swatches.dotSize
                : swatches.present.length * swatches.dotSize
                  + (swatches.present.length - 1) * swatches.gap)
    implicitHeight: swatches.dotSize

    Row {
        spacing: (swatches.joined || swatches.stretchTo > 0) ? 0 : swatches.gap

        Repeater {
            model: swatches.present

            delegate: Rectangle {
                id: dot
                required property string modelData
                required property int index

                width: swatches.cellWidth
                height: swatches.dotSize
                color: dot.modelData

                // Separate dots are circles. A joined strip squares the inner
                // edges so the run reads as one bar, and rounds only the two
                // outer ends.
                readonly property bool isEnd: dot.index === 0
                    || dot.index === swatches.present.length - 1
                readonly property bool banded: swatches.joined || swatches.stretchTo > 0
                radius: !dot.banded
                    ? width / 2
                    : (dot.isEnd ? height / 4 : 0)
            }
        }
    }
}
