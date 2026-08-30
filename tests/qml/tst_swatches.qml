import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "Swatches"

    readonly property var full: ({
        "accent": "#7aa2f7", "red": "#f7768e", "green": "#9ece6a",
        "yellow": "#e0af68", "blue": "#7aa2f7", "magenta": "#bb9af7",
        "cyan": "#7dcfff", "orange": "#ff9e64",
        "background": "#1a1b26", "foreground": "#c0caf5"
    })

    Swatches { id: full8; colors: parent.full }
    Swatches { id: empty; colors: ({}) }
    // A derived palette may legitimately lack a role the theme had no source
    // for; a hole in the strip would read as a rendering bug.
    Swatches { id: partial; colors: ({ "accent": "#7aa2f7", "red": "#f7768e" }) }
    Swatches { id: joined; colors: parent.full; joined: true }

    function test_draws_the_hue_roles_not_every_role() {
        // background/foreground are near-identical across dark schemes and
        // tell you nothing about which palette this is.
        compare(full8.present.length, 8)
        verify(full8.present.indexOf("#1a1b26") < 0)
    }

    function test_accent_leads_the_strip() {
        compare(full8.present[0], "#7aa2f7")
    }

    function test_missing_roles_are_skipped_not_drawn_transparent() {
        compare(partial.present.length, 2)
    }

    function test_an_empty_palette_takes_no_space() {
        compare(empty.present.length, 0)
        compare(empty.implicitWidth, 0)
    }

    function test_joined_strip_is_narrower_than_spaced_dots() {
        // The joined bar drops the inter-dot gaps, which is what lets it fit
        // inside a chip.
        verify(joined.implicitWidth < full8.implicitWidth)
    }

    function test_survives_a_null_palette() {
        // The service starts with palettes unset; binding to it must not throw.
        empty.colors = null
        compare(empty.present.length, 0)
        empty.colors = ({})
    }
}
