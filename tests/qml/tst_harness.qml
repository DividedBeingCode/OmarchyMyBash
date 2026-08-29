import QtQuick
import QtTest
import qs.Commons

// Proves the stub qs.Commons module loads and carries the values the kit
// depends on. The REAL qs.Commons cannot load here (Border.qml needs the
// Quickshell runtime), which is why the stub exists.
TestCase {
    name: "Harness"

    function test_stub_style_is_stock_omarchy() {
        // Stock Omarchy ships rounding at 0 — the kit's radius floor exists
        // precisely because of this value.
        compare(Style.cornerRadius, 0)
    }

    function test_stub_exposes_state_fills() {
        verify(Style.normalFill !== undefined)
        verify(Style.hoverFill !== undefined)
        verify(Style.selectedFill !== undefined)
        verify(Color.accent !== undefined)
    }
}
