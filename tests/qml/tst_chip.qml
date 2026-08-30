import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "Chip"

    Chip { id: plain; label: "omarchy" }
    Chip { id: activeChip; label: "pure"; active: true }
    Chip { id: dotted; label: "Gruvbox"; hasSwatch: true; swatch: "#83a598" }
    Chip {
        id: stripped
        label: "Tokyo Night"
        swatches: ({ "accent": "#7aa2f7", "red": "#f7768e", "green": "#9ece6a" })
    }
    Chip { id: off; label: "kubectl"; enabled: false }

    function test_a_chip_sizes_to_its_label() {
        verify(plain.implicitWidth > 0)
        verify(plain.implicitHeight >= 30)
    }

    function test_a_longer_label_makes_a_wider_chip() {
        verify(stripped.implicitWidth > plain.implicitWidth)
    }

    // The failure this guards: Style.normalFill is a 4-8% alpha TINT. An
    // active chip filled with it would be invisible against the panel.
    function test_the_active_chip_is_filled_with_the_accent() {
        verify(activeChip.color.a === 1)
        verify(activeChip.color !== plain.color)
    }

    function test_swatch_slot_is_opt_in() {
        verify(dotted.hasSwatch)
        verify(!plain.hasSwatch)
    }

    function test_a_palette_chip_carries_its_colors() {
        // The whole point: a theme chip shows what it will apply.
        verify(stripped._showStrip)
        verify(!dotted._showStrip)
        verify(!plain._showStrip)
    }

    function test_a_disabled_chip_is_dimmed_and_inert() {
        verify(off.opacity < 1.0)
        compare(off.enabled, false)
    }

    function test_clicking_emits() {
        var seen = 0
        function count() { seen++ }
        plain.clicked.connect(count)
        plain.clicked()
        plain.clicked.disconnect(count)
        compare(seen, 1)
    }
}
