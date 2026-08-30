import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "SubRail"

    SubRail { id: rail; width: 400; tabs: ["Style", "Glyphs", "Segments"] }
    SubRail { id: empty; width: 400; tabs: [] }

    function test_it_starts_on_the_first_tab() {
        compare(rail.current, 0)
    }

    function test_an_out_of_range_index_is_clamped() {
        // A tab body keyed on `current` would render nothing at all for an
        // out-of-range index — a blank tab with no error anywhere.
        rail.current = 99
        compare(rail.current, rail.tabs.length - 1)
        rail.current = -3
        compare(rail.current, 0)
    }

    function test_an_empty_rail_does_not_go_negative() {
        compare(empty.current, 0)
        verify(!empty.visible, "a rail with nothing to switch between is hidden")
    }

    function test_switching_emits_once() {
        rail.current = 0
        var seen = []
        function record(i) { seen.push(i) }
        rail.switched.connect(record)
        rail.select(2)
        rail.switched.disconnect(record)
        compare(seen.length, 1)
        compare(seen[0], 2)
        compare(rail.current, 2)
    }
}
