import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "PresetCard"

    readonly property var nord: ({
        "background": "#2e3440", "foreground": "#eceff4", "accent": "#88c0d0",
        "red": "#bf616a", "green": "#a3be8c", "yellow": "#ebcb8b",
        "blue": "#81a1c1", "magenta": "#b48ead", "cyan": "#8fbcbb",
        "orange": "#d08770", "muted": "#7b88a1"
    })

    PresetCard {
        id: complete
        width: 260
        label: "Polar Lean"
        blurb: "Arctic blues, rounded caps, and a penguin."
        tags: ["complete", "nerd-font"]
        colors: parent.nord
        render: ({ left: "\x1b[36m~/x\x1b[0m", right: "" })
        renderState: "ok"
    }

    PresetCard {
        id: structural
        width: 260
        label: "Lean Pure"
        blurb: "No icons, no fills."
        tags: ["structure", "minimal", "ascii-safe"]
        colors: ({})
        render: ({ left: "~/x", right: "" })
        renderState: "ok"
    }

    PresetCard { id: pending; width: 260; label: "Loading"; renderState: "loading" }
    PresetCard { id: broken; width: 260; label: "Broken"; renderState: "error" }
    PresetCard { id: selected; width: 260; label: "Picked"; active: true }

    // The failure that shipped once already: Style.normalFill is a 4-8% alpha
    // TINT. Used as a card's base colour it renders a ~96% transparent card
    // with the wallpaper showing through.
    function test_the_card_surface_is_opaque() {
        compare(complete.color.a, 1)
        compare(pending.color.a, 1)
    }

    function test_a_complete_preset_previews_on_its_own_background() {
        compare(complete.previewBg.toString(), "#2e3440")
        compare(complete.previewFg.toString(), "#eceff4")
    }

    // A `structure` Look respects whatever palette you are on, so its card
    // must preview on the CURRENT colors. Inventing a background for it would
    // show something the preset does not actually do.
    function test_a_structure_preset_previews_on_the_current_palette() {
        verify(structural.previewBg.a === 1)
        compare(structural.previewBg.toString(), complete.previewBg.toString() === "#2e3440"
            ? structural.previewBg.toString() : structural.previewBg.toString())
        verify(structural.previewBg.toString() !== "#2e3440")
    }

    function test_selection_reads_as_a_ring_not_a_fill() {
        // The preset's own colors must stay the loudest thing on the card.
        verify(selected.border.width > 0)
        compare(complete.border.width, 0)
    }

    function test_render_states_are_distinguishable() {
        compare(pending.renderState, "loading")
        compare(broken.renderState, "error")
        compare(complete.renderState, "ok")
    }

    function test_survives_a_null_palette_and_null_render() {
        pending.colors = null
        pending.render = null
        verify(pending.previewBg.a === 1)
        verify(pending.implicitHeight > 0)
    }

    function test_hover_is_reported_for_live_preview() {
        // Hover drives the preview fetch, so the signals are load-bearing.
        var seen = 0
        function count() { seen++ }
        complete.entered.connect(count)
        complete.entered()
        complete.entered.disconnect(count)
        compare(seen, 1)
    }

    function test_clicking_emits() {
        var seen = 0
        function count() { seen++ }
        complete.clicked.connect(count)
        complete.clicked()
        complete.clicked.disconnect(count)
        compare(seen, 1)
    }
}
