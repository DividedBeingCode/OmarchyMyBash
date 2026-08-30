import QtQuick
import QtTest

// Guards the glyph tables against a subtle, silent corruption.
//
// QML/JavaScript `\uXXXX` takes EXACTLY four hex digits. A Nerd Font
// codepoint above U+FFFF written as `"b"` therefore parses as U+F011
// followed by a literal "b" — it renders as the wrong glyph plus a stray
// character, and nothing errors. Every 5-digit codepoint must use the ES6
// brace form `\u{f011b}`.
TestCase {
    name: "GlyphEscapes"

    function test_brace_form_yields_the_right_codepoint() {
        // U+F011B is above the BMP, so .length is 2 (a surrogate pair) —
        // codePointAt is the meaningful check, and there is no stray char.
        compare("\u{f011b}".codePointAt(0), 0xf011b)
        compare([..."\u{f011b}"].length, 1, "must be exactly one character")
    }

    // The bug, pinned so the distinction is not lost.
    function test_four_digit_form_truncates_and_leaves_a_stray_char() {
        compare("b".length, 2)
        compare("b".charCodeAt(0), 0xf011)
        compare("b".charAt(1), "b")
    }
}
