import QtQuick
import QtTest
import "../../quattro/o10k"

// Layout regression tests.
//
// A bare Item in a Column has height 0 by default — setting implicitHeight
// alone is NOT enough, because Column positions children by `height`. Both
// components below shipped with that defect and rendered as nothing in the
// live panel while every logic test passed.
TestCase {
    name: "Layout"

    Column {
        id: col
        width: 340

        ThemeBindRow {
            id: bindRow
            width: parent.width
            cfgFlat: ({ "theme.source": "omarchy" })
            desktopTheme: "tokyo-night"
        }

        SettingRow {
            id: settingRow
            width: parent.width
            label: "Transient"
            value: true
            defaultValue: true
        }
    }

    function test_bind_row_has_height_in_a_column() {
        verify(bindRow.implicitHeight > 0)
        verify(bindRow.height > 0)
    }

    function test_setting_row_has_height_in_a_column() {
        verify(settingRow.implicitHeight > 0)
        verify(settingRow.height > 0)
    }

    // If children have no height the row is invisible even when the row
    // itself is sized.
    function test_column_stacks_both_rows() {
        verify(col.implicitHeight >= bindRow.height + settingRow.height)
    }
}
