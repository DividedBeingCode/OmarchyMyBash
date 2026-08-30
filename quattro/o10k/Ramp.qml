pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// A palette's gradient ramp, as a continuous sweep.
//
// Distinct from Swatches on purpose. Swatches answers "which colors does this
// palette contain"; this answers "what does its gradient DO" — the thing you
// could previously only discover by applying a preset and looking at your
// terminal. Synthwave shipped a ramp that ran purple → teal for months
// because nothing in the UI ever drew it.
//
// The stops come from the daemon's `palettes` verb, sampled by the same
// `ramp_color` the prompt renders with. Interpolating here from two endpoints
// would be a second implementation of the OKLCH sweep, and a second one is
// one that can disagree.
//
// Built from adjacent two-stop segments rather than one multi-stop Gradient
// because a Repeater cannot populate a Gradient: Repeater parents what it
// creates to an Item, and Gradient is not one.
Item {
    id: ramp

    /// Ordered hex stops, as the `palettes` verb's `ramp` field returns.
    property var stops: []
    property real barHeight: Style.space(6)

    readonly property bool hasRamp: ramp.stops && ramp.stops.length >= 2

    /// A flat ramp is a palette with `gradient = "off"`, or a greyscale accent
    /// whose hue rotation is a no-op. Drawing a one-color bar and calling it a
    /// gradient would be a lie, so the caller can hide it.
    readonly property bool isFlat: {
        if (!ramp.hasRamp) return true
        var first = String(ramp.stops[0]).toLowerCase()
        for (var i = 1; i < ramp.stops.length; i++)
            if (String(ramp.stops[i]).toLowerCase() !== first) return false
        return true
    }

    /// Consecutive pairs — one per drawn segment.
    readonly property var pairs: {
        var out = []
        if (!ramp.hasRamp) return out
        for (var i = 0; i < ramp.stops.length - 1; i++)
            out.push({ from: String(ramp.stops[i]), to: String(ramp.stops[i + 1]) })
        return out
    }

    implicitHeight: ramp.barHeight
    visible: ramp.hasRamp

    // NOT a Row: a Row assigns its children's x itself, which would fight the
    // seam-free distribution below.
    Repeater {
        model: ramp.pairs

        delegate: Rectangle {
            id: seg
            required property var modelData
            required property int index

            // Distributed by index rather than a plain divide, so rounding
            // never leaves a background-colored seam between segments or
            // overruns the last one.
            readonly property real span: ramp.width / Math.max(1, ramp.pairs.length)
            x: Math.round(seg.index * seg.span)
            width: Math.round((seg.index + 1) * seg.span) - Math.round(seg.index * seg.span)
            height: ramp.height

            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0.0; color: seg.modelData.from }
                GradientStop { position: 1.0; color: seg.modelData.to }
            }
        }
    }
}
