import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "GlyphBrowser"

    // The REAL category strings from the shipped catalog in
    // quattro/StudioPrompt.qml. The fixture used to invent `category:
    // "Japan"`, a value that exists nowhere in the product — so the test
    // asserting the category filter works passed while the shipped japan
    // chip matched nothing and hid 21 glyphs. A fixture that does not use
    // the real vocabulary tests the fixture, not the feature.
    property var sample: [
        { key: "chevron", glyph: "❯", label: "Chevron", category: "Prompt" },
        { key: "cat",  glyph: "\u{f011b}", label: "Cat",  category: "Animals" },
        { key: "torii", glyph: "\u{f0705}", label: "Torii", category: "Japan / Geek" },
        { key: "ninja", glyph: "\u{f0774}", label: "Ninja", category: "Japan / Geek" },
        { key: "kaomoji_shrug", glyph: "¯\\_(ツ)_/¯", label: "Shrug", category: "Kaomoji" }
    ]

    GlyphBrowser { id: browser; width: 640; catalog: sample }
    GlyphCell { id: cell; width: 80; glyph: "❯"; label: "Chevron" }
    GlyphCell { id: wide; width: 80; glyph: "¯\\_(ツ)_/¯"; label: "Shrug" }

    function test_the_glyph_scales_with_its_tile() {
        // Shipped at a fixed 13px inside a ~64px tile: a glyph browser whose
        // whole purpose is showing what a glyph looks like rendered it at a
        // fifth of its own cell.
        verify(cell.glyphSize > 30, "glyph is " + cell.glyphSize + "px in an 80px tile")
        verify(cell.glyphSize < cell.width, "glyph cannot exceed its tile")
    }

    function test_a_multi_character_glyph_stays_inside_its_tile() {
        // Kaomoji are strings, not single glyphs. At glyphSize = width * 0.5
        // an unbounded centred Text painted `¯\_(ツ)_/¯` roughly 250px wide
        // in an 80px tile, straight over its neighbours — and the Kaomoji
        // category is a contiguous block of 11 tiles.
        var glyphText = null
        for (var i = 0; i < wide.children.length; i++) {
            var c = wide.children[i]
            if (c.text === wide.glyph) { glyphText = c; break }
        }
        verify(glyphText !== null, "found the glyph Text")
        verify(glyphText.width <= wide.width,
               "glyph box is " + glyphText.width + "px in an " + wide.width + "px tile")
        verify(glyphText.contentWidth <= wide.width,
               "painted glyph is " + glyphText.contentWidth
               + "px wide in an " + wide.width + "px tile")
    }

    function test_tiles_are_big_enough_to_read() {
        verify(browser.columns <= 8, "columns = " + browser.columns)
    }

    function test_the_chips_come_from_the_catalog() {
        // The chip list was hand-written and drifted from the data.
        compare(browser.categories[0], "", "the 'all' chip leads")
        compare(browser.categories.length, 5)
        verify(browser.categories.indexOf("Japan / Geek") > 0,
               "got " + JSON.stringify(browser.categories))
    }

    function test_every_category_chip_finds_something() {
        // The assertion that would have caught the shipped bug: a chip that
        // matches nothing is a chip that lies. `browser.categories` is what
        // the UI actually renders, so this covers every chip a user can click.
        var saved = browser.category
        for (var i = 0; i < browser.categories.length; i++) {
            browser.category = browser.categories[i]
            verify(browser.results.length > 0,
                   "category \"" + browser.categories[i] + "\" yields "
                   + browser.results.length + " / " + browser.catalog.length)
        }
        browser.category = saved
    }

    function test_a_category_narrows_the_grid() {
        browser.category = "Japan / Geek"
        compare(browser.results.length, 2)
        compare(browser.results[0].key, "torii")
        browser.category = ""
    }

    function test_all_restores_the_full_set() {
        browser.category = ""
        compare(browser.results.length, 5)
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
