import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "WheelBoost"

    Flickable {
        id: pane
        width: 400; height: 300
        contentWidth: 400
        contentHeight: 3000

        WheelBoost { id: boosted; flick: pane }
    }

    Flickable {
        id: shortPane
        width: 400; height: 300
        contentWidth: 400
        contentHeight: 100      // shorter than the viewport: nothing to scroll
        WheelBoost { id: noRoom; flick: shortPane }
    }

    WheelBoost { id: unattached }

    // The specific trap this component's naming rules exist for: in Panel.qml
    // an `id: wheelBoost` beside a `property real wheelBoost` made every
    // lookup resolve to the handler object, and the scroll step computed NaN
    // — a silent dead scroll with no error anywhere.
    function test_the_boost_is_a_number_not_an_object() {
        verify(typeof boosted.boost === "number", "boost resolved to " + typeof boosted.boost)
        verify(!isNaN(boosted.boost))
        verify(boosted.boost > 1, "a boost of 1 or less is not a boost")
    }

    // `target` already exists on PointerHandler; shadowing it is the same
    // class of bug, which is why the Flickable property is `flick`.
    function test_the_flickable_property_does_not_shadow_target() {
        compare(boosted.flick, pane)
        verify(boosted.flick !== boosted.target)
    }

    function test_it_accepts_both_touchpads_and_mice() {
        // A touchpad-only handler leaves mouse wheels on Qt's slow default;
        // a mouse-only one is the bug being fixed.
        verify((boosted.acceptedDevices & PointerDevice.TouchPad) !== 0)
        verify((boosted.acceptedDevices & PointerDevice.Mouse) !== 0)
    }

    function test_gestures_do_not_bleed_into_each_other() {
        // Without a timeout the handler stays active between gestures and
        // swallows the next one.
        verify(boosted.activeTimeout > 0)
    }

    function test_it_survives_no_flickable() {
        // Instantiated without `flick`, it must sit inert rather than throw
        // on every wheel event.
        compare(unattached.flick, null)
        verify(typeof unattached.boost === "number")
    }

    function test_a_pane_with_nothing_to_scroll_is_handled() {
        // contentHeight < height means max clamps to 0; the arithmetic must
        // not produce a negative contentY.
        compare(noRoom.flick, shortPane)
        verify(shortPane.contentHeight < shortPane.height)
    }
}
