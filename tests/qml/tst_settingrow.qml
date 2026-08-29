import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "SettingRow"

    SettingRow {
        id: unchanged
        label: "Transient"
        value: true
        defaultValue: true
    }

    SettingRow {
        id: changed
        label: "Transient"
        value: false
        defaultValue: true
    }

    SettingRow {
        id: unknownDefault
        label: "Custom key"
        value: "anything"
        // defaultValue deliberately left undefined
    }

    SignalSpy {
        id: resetSpy
        target: changed
        signalName: "resetRequested"
    }

    function test_value_equal_to_default_is_not_modified() {
        compare(unchanged.modified, false)
    }

    function test_value_differing_from_default_is_modified() {
        compare(changed.modified, true)
    }

    // A key with no known default must never render as modified — that ink
    // would be a lie, and the reset chip would have nothing to reset to.
    function test_unknown_default_is_never_modified() {
        compare(unknownDefault.modified, false)
    }

    function test_reset_emits_signal() {
        resetSpy.clear()
        changed.requestReset()
        compare(resetSpy.count, 1)
    }
}
