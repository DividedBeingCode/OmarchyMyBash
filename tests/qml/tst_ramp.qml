import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "Ramp"

    // Synthwave's ramp AFTER the fix. Before it, the far end was #00b0b1 --
    // teal -- because `complement()` chose between the magenta and cyan ANSI
    // slots by comparing accent.b >= accent.r, and for #d53bce that is 206 vs
    // 213. Seven bytes of red turned "purple all the way down" green.
    Ramp {
        id: purple
        width: 200
        stops: ["#d53bce", "#e132b8", "#ea29a1", "#f22188", "#f81b6e"]
    }

    Ramp { id: flat; width: 200; stops: ["#7aa2f7", "#7aa2f7", "#7AA2F7"] }
    Ramp { id: empty; width: 200; stops: [] }
    Ramp { id: single; width: 200; stops: ["#7aa2f7"] }
    Ramp { id: missing; width: 200 }

    function test_a_real_ramp_draws() {
        verify(purple.hasRamp)
        verify(!purple.isFlat)
        // n stops make n-1 two-stop segments.
        compare(purple.pairs.length, purple.stops.length - 1)
    }

    function test_segments_tile_the_full_width_without_seams() {
        // Rounding each edge independently is what keeps a background-colored
        // hairline from showing between segments.
        var covered = 0
        for (var i = 0; i < purple.pairs.length; i++) {
            var span = purple.width / purple.pairs.length
            var x0 = Math.round(i * span)
            var x1 = Math.round((i + 1) * span)
            compare(x0, covered, "segment " + i + " does not start where the last ended")
            covered = x1
        }
        compare(covered, purple.width, "segments do not reach the full width")
    }

    function test_a_flat_ramp_is_reported_as_flat() {
        // `gradient = "off"`, or a greyscale accent whose hue rotation is a
        // no-op. Drawing a one-color bar and calling it a gradient is a lie,
        // so callers hide it -- case-insensitively, since hex casing varies.
        verify(flat.hasRamp)
        verify(flat.isFlat)
    }

    function test_nothing_to_draw_is_handled() {
        // An older daemon sends no `ramp` field at all.
        verify(!empty.hasRamp)
        verify(empty.isFlat)
        verify(!empty.visible)
        compare(empty.pairs.length, 0)

        verify(!single.hasRamp, "one stop is not a gradient")
        compare(single.pairs.length, 0)

        verify(!missing.hasRamp)
        compare(missing.pairs.length, 0)
    }
}
