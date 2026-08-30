pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui
import "Fx.js" as Fx

// Searchable glyph picker.
//
// Shows every glyph AS IT WILL ACTUALLY RENDER in the user's terminal font,
// which is the whole point: two separate glyph bugs shipped here because
// codepoints were verified on paper rather than looked at. A wrong or missing
// glyph is obvious the moment it is on screen next to its name.
//
// Tofu (□) means the installed font lacks that codepoint — the browser cannot
// detect that programmatically, but it makes it visible, which is enough.
//
// Bound, unlike Card/SettingRow: a Repeater delegate has to reach the root id
// to size and select itself, and `Bound` is what makes that resolve cleanly.
// The C4 "must stay unbound" finding is about inline `Component {}` blocks,
// which cannot be instantiated cross-file when bound — a FILE component like
// this one is unaffected, which is why every Panel*.qml carries the pragma
// too.
Item {
    id: browser

    /// [{ key, glyph, label, category }]
    property var catalog: []
    property string query: ""
    property string category: ""
    property string selected: ""
    /// Font to preview in — should match the terminal's, not the UI's.
    property string previewFont: Style.font.family
    property int columns: 8

    signal picked(string key)

    /// Category chips, DERIVED from the catalog rather than hand-listed.
    ///
    /// The list used to be a literal `["", "Prompt", "Animals", "Japan",
    /// "Kaomoji"]` while the catalog's actual string is "Japan / Geek". The
    /// filter is an exact compare, so the japan chip matched nothing and hid
    /// 21 glyphs -- the whole ninja/torii/sushi/noodles/tea/katana family.
    /// Deriving it means the chips cannot drift from the data again.
    ///
    /// "" is the "all" chip and always leads.
    readonly property var categories: {
        var seen = {}
        var out = [""]
        for (var i = 0; i < browser.catalog.length; i++) {
            var c = String(browser.catalog[i].category || "")
            if (c.length > 0 && !seen[c]) {
                seen[c] = true
                out.push(c)
            }
        }
        return out
    }

    readonly property var results: {
        var q = browser.query.trim().toLowerCase()
        var cat = browser.category
        var out = []
        for (var i = 0; i < browser.catalog.length; i++) {
            var e = browser.catalog[i]
            if (cat.length > 0 && String(e.category || "") !== cat)
                continue
            if (q.length > 0) {
                var hay = String(e.label || "") + " " + String(e.key || "")
                        + " " + String(e.category || "")
                if (hay.toLowerCase().indexOf(q) < 0)
                    continue
            }
            out.push(e)
        }
        return out
    }

    implicitHeight: layout.implicitHeight

    Column {
        id: layout
        width: parent.width
        spacing: Style.space(10)

        // 78 glyphs in one grid is a wall. The catalog already carries a
        // category per entry; these just surface it.
        Row {
            width: parent.width
            spacing: Style.space(6)

            Repeater {
                model: browser.categories

                delegate: Chip {
                    required property string modelData
                    label: modelData.length === 0 ? "all" : modelData.toLowerCase()
                    active: browser.category === modelData
                    onClicked: browser.category = modelData
                }
            }
        }

        Row {
            width: parent.width
            spacing: Style.space(8)

            TextField {
                id: search
                width: parent.width - countLabel.implicitWidth - Style.space(8)
                placeholderText: "Search glyphs — try \"cat\", \"arrow\", \"ninja\"…"
                text: browser.query
                onTextChanged: browser.query = text
            }

            Text {
                id: countLabel
                anchors.verticalCenter: parent.verticalCenter
                text: browser.results.length + " / " + browser.catalog.length
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
            }
        }

        Grid {
            columns: browser.columns
            spacing: Style.space(6)
            width: parent.width

            Repeater {
                model: browser.results

                delegate: GlyphCell {
                    required property var modelData
                    width: (browser.width - Style.space(6) * (browser.columns - 1))
                           / browser.columns
                    glyph: modelData.glyph
                    label: modelData.label
                    previewFont: browser.previewFont
                    active: browser.selected === modelData.key
                    onClicked: {
                        browser.selected = modelData.key
                        browser.picked(modelData.key)
                    }
                }
            }
        }

        Text {
            visible: browser.results.length === 0
            width: parent.width
            wrapMode: Text.WordWrap
            text: "No glyph matches \"" + browser.query + "\"."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }
    }
}
