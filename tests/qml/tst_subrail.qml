import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "SubRail"

    SubRail { id: rail; width: 400; tabs: ["Style", "Glyphs", "Segments"] }
    SubRail { id: empty; width: 400; tabs: [] }

    // A call site: the tab owns the index, the rail renders it.
    QtObject { id: host; property int subTab: 0 }
    SubRail {
        id: bound
        width: 400
        tabs: ["Style", "Glyphs", "Segments"]
        current: host.subTab
        onSwitched: (i) => host.subTab = i
    }

    function test_it_starts_on_the_first_tab() {
        compare(rail.clamped, 0)
    }

    function test_an_out_of_range_index_is_clamped() {
        // A tab body keyed on the rendered index would render nothing at all
        // for an out-of-range value — a blank tab with no error anywhere.
        // `current` is the raw input; `clamped` is what the rail draws from.
        rail.current = 99
        compare(rail.clamped, rail.tabs.length - 1)
        rail.current = -3
        compare(rail.clamped, 0)
        rail.current = 0
    }

    function test_an_empty_rail_does_not_go_negative() {
        compare(empty.clamped, 0)
        verify(!empty.visible, "a rail with nothing to switch between is hidden")
    }

    function test_switching_emits_once() {
        var seen = []
        function record(i) { seen.push(i) }
        rail.switched.connect(record)
        rail.select(2)
        rail.switched.disconnect(record)
        compare(seen.length, 1)
        compare(seen[0], 2)
    }

    function test_select_clamps_what_it_emits() {
        var seen = []
        function record(i) { seen.push(i) }
        rail.switched.connect(record)
        rail.select(99)
        rail.select(-4)
        rail.switched.disconnect(record)
        compare(seen[0], rail.tabs.length - 1)
        compare(seen[1], 0)
    }

    function test_selecting_does_not_break_the_call_sites_binding() {
        // The rail used to assign `current` itself — both to clamp and in
        // select(). A single imperative write destroys `current: host.subTab`
        // permanently, and the rail then silently stops following its tab.
        host.subTab = 0
        bound.select(2)
        compare(host.subTab, 2, "onSwitched moved the owner")
        compare(bound.current, 2, "the binding carried it back")

        // The real proof: the OWNER moving on its own still reaches the rail.
        host.subTab = 1
        compare(bound.current, 1, "the binding into `current` was destroyed")
        compare(bound.clamped, 1)
        host.subTab = 0
    }
}
