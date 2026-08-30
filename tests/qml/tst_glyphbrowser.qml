import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "GlyphBrowser"

    property var sample: [
        { key: "cat",  glyph: "\u{f011b}", label: "Cat",  category: "Animals" },
        { key: "torii", glyph: "\u{f0705}", label: "Torii", category: "Japan" },
        { key: "chevron", glyph: "❯", label: "Chevron", category: "Prompt" }
    ]

    GlyphBrowser { id: browser; width: 640; catalog: sample }
    GlyphCell { id: cell; width: 80; glyph: "❯"; label: "Chevron" }

    function test_the_glyph_scales_with_its_tile() {
        // Shipped at a fixed 13px inside a ~64px tile: a glyph browser whose
        // whole purpose is showing what a glyph looks like rendered it at a
        // fifth of its own cell.
        verify(cell.glyphSize > 30, "glyph is " + cell.glyphSize + "px in an 80px tile")
        verify(cell.glyphSize < cell.width, "glyph cannot exceed its tile")
    }

    function test_tiles_are_big_enough_to_read() {
        verify(browser.columns <= 8, "columns = " + browser.columns)
    }

    function test_a_category_narrows_the_grid() {
        browser.category = "Japan"
        compare(browser.results.length, 1)
        compare(browser.results[0].key, "torii")
    }

    function test_all_restores_the_full_set() {
        browser.category = ""
        compare(browser.results.length, 3)
    }

    function test_category_and_query_combine() {
        browser.category = "Animals"
        browser.query = "cat"
        compare(browser.results.length, 1)
        browser.query = "torii"
        compare(browser.results.length, 0, "a query outside the category matches nothing")
        browser.query = ""
        browser.category = ""
    }
}
