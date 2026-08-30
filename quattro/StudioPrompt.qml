pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx
import "o10k/Preview.js" as Preview

// Studio → Prompt tab: style preset, separators, glyphs, frame and the
// prompt-behavior toggles.
//
// Reads and writes go through the service, never through a panel root, so
// this surface and the bar panel share one config state, one dirty set and
// one debounce.
Flickable {
    id: promptTab

    property var service: null
    /// Injected by Studio: the pinned preview pane this tab drives.
    property var previewPane: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // Touchpad scrolling at the same rate as the bar popout.
    WheelBoost { flick: promptTab }

    // Bindings must reference a PROPERTY to re-evaluate. Reading through a
    // function call only creates a dependency on `service` itself, so the
    // whole tab stayed frozen at its first paint once the config arrived.
    readonly property var cfg: promptTab.service ? promptTab.service.cfgFlat : ({})
    readonly property var defaults: promptTab.service ? promptTab.service.defaultsFlat : ({})

    function _get(key, fallback) {
        var v = promptTab.cfg[key]
        return v === undefined ? fallback : v
    }

    function _set(key, value) {
        if (promptTab.service) promptTab.service.setConfigValue(key, value)
        // The config just changed, so re-render against it. Every control on
        // this tab routes through here, which is why one call covers all of
        // them rather than each control remembering to refresh.
        promptTab.refreshPreview()
    }

    /// Render the CURRENT config (no look, no patch) into the pinned pane.
    ///
    /// Immediate rather than debounced: this follows a click, and a click
    /// should never wait out the hover delay.
    function refreshPreview() {
        if (!promptTab.previewPane || !promptTab.service) return
        promptTab.previewPane.caption = "your prompt"
        promptTab.previewPane.colors = promptTab.service.currentPaletteColors()
        promptTab.previewPane.renderState = "loading"
        promptTab.service.requestPreview(null, null, Preview.SCENES, true,
            function (res) {
                promptTab.previewPane.renderState = res.state
                promptTab.previewPane.renders = res.renders
                promptTab.previewPane.errorText = res.error
            }, promptTab.previewPane.cols)
    }

    onPreviewPaneChanged: promptTab.refreshPreview()

    function _default(key) {
        return promptTab.defaults[key]
    }

    readonly property var presets: [
        "omarchy", "powerline", "rainbow", "gradient", "framed",
        "classic", "lean", "dense", "slanted", "minimal", "pure"
    ]

    readonly property var separators: [
        { key: "powerline",      glyph: "" },
        { key: "powerline_thin", glyph: "" },
        { key: "slanted",        glyph: "" },
        { key: "round",          glyph: "" },
        { key: "vertical",       glyph: "│" },
        { key: "dot",            glyph: "·" },
        { key: "diamond",        glyph: "◆" },
        { key: "none",           glyph: "∅" }
    ]

    // {key, glyph} pairs, NOT bare glyphs.
    //
    // This row used to write the glyph itself into
    // `segments.character.success`. The daemon tolerates that -- its catalog
    // lookup falls through to `_ => key` -- so it appeared to work, but it
    // wrote a different format than every other writer of the same config
    // key: the glyph browser below writes catalog keys, and so does every
    // Look. The visible symptom was that applying a Look (which writes
    // "chevron") left this row with nothing selected.
    //
    // Keys mirror GlyphCatalog::prompt_char in style.rs.
    readonly property var promptChars: [
        { key: "chevron",  glyph: "\u276f" },
        { key: "arrow",    glyph: "\u279c" },
        { key: "lambda",   glyph: "\u03bb" },
        { key: "dollar",   glyph: "$" },
        { key: "angle",    glyph: ">" },
        { key: "percent",  glyph: "%" },
        { key: "triangle", glyph: "\u25b6" },
        { key: "hash",     glyph: "#" }
    ]

    // Every glyph the project offers, in one searchable place. Generated from
    // the same catalogs the bar panel uses so the two cannot drift.
    readonly property var glyphCatalog: [
        { key: "cat", glyph: "\u{f011b}", label: "Cat", category: "Animals" },
        { key: "dog", glyph: "\u{f0a43}", label: "Dog", category: "Animals" },
        { key: "owl", glyph: "\u{f03d2}", label: "Owl", category: "Animals" },
        { key: "duck", glyph: "\u{f01e5}", label: "Duck", category: "Animals" },
        { key: "penguin", glyph: "\u{f0ec0}", label: "Penguin", category: "Animals" },
        { key: "rabbit", glyph: "\u{f0907}", label: "Rabbit", category: "Animals" },
        { key: "turtle", glyph: "\u{f0cd7}", label: "Turtle", category: "Animals" },
        { key: "panda", glyph: "\u{f03da}", label: "Panda", category: "Animals" },
        { key: "koala", glyph: "\u{f173f}", label: "Koala", category: "Animals" },
        { key: "unicorn", glyph: "\u{f15c2}", label: "Unicorn", category: "Animals" },
        { key: "cow", glyph: "\u{f019a}", label: "Cow", category: "Animals" },
        { key: "horse", glyph: "\u{f15bf}", label: "Horse", category: "Animals" },
        { key: "pig", glyph: "\u{f0401}", label: "Pig", category: "Animals" },
        { key: "sheep", glyph: "\u{f0cc6}", label: "Sheep", category: "Animals" },
        { key: "bee", glyph: "\u{f0fa1}", label: "Bee", category: "Animals" },
        { key: "butterfly", glyph: "\u{f1589}", label: "Butterfly", category: "Animals" },
        { key: "ladybug", glyph: "\u{f082d}", label: "Ladybug", category: "Animals" },
        { key: "snail", glyph: "\u{f1677}", label: "Snail", category: "Animals" },
        { key: "spider", glyph: "\u{f11ea}", label: "Spider", category: "Animals" },
        { key: "snake", glyph: "\u{f150e}", label: "Snake", category: "Animals" },
        { key: "bird", glyph: "\u{f15c6}", label: "Bird", category: "Animals" },
        { key: "fish", glyph: "\u{f023a}", label: "Fish", category: "Animals" },
        { key: "dolphin", glyph: "\u{f18b4}", label: "Dolphin", category: "Animals" },
        { key: "shark", glyph: "\u{f18ba}", label: "Shark", category: "Animals" },
        { key: "jellyfish", glyph: "\u{f0f01}", label: "Jellyfish", category: "Animals" },
        { key: "elephant", glyph: "\u{f07c6}", label: "Elephant", category: "Animals" },
        { key: "kangaroo", glyph: "\u{f1558}", label: "Kangaroo", category: "Animals" },
        { key: "donkey", glyph: "\u{f07c2}", label: "Donkey", category: "Animals" },
        { key: "rodent", glyph: "\u{f1327}", label: "Rodent", category: "Animals" },
        { key: "bat", glyph: "\u{f0b5f}", label: "Bat", category: "Animals" },
        { key: "paw", glyph: "\u{f03e9}", label: "Paw", category: "Animals" },
        { key: "bone", glyph: "\u{f00b9}", label: "Bone", category: "Animals" },
        { key: "egg", glyph: "\u{f0aaf}", label: "Egg", category: "Animals" },
        { key: "feather", glyph: "\u{f06d3}", label: "Feather", category: "Animals" },
        { key: "bug", glyph: "\u{f00e4}", label: "Bug", category: "Animals" },
        { key: "dragon", glyph: "\ueef8", label: "Dragon", category: "Animals" },
        { key: "frog", glyph: "\uedf8", label: "Frog", category: "Animals" },
        { key: "squirrel", glyph: "\ueb58", label: "Squirrel", category: "Animals" },
        { key: "ninja", glyph: "\u{f0774}", label: "Ninja", category: "Japan / Geek" },
        { key: "torii", glyph: "\ueee6", label: "Torii", category: "Japan / Geek" },
        { key: "sushi", glyph: "\ue21a", label: "Sushi", category: "Japan / Geek" },
        { key: "noodles", glyph: "\u{f117e}", label: "Noodles", category: "Japan / Geek" },
        { key: "rice", glyph: "\u{f07ea}", label: "Rice", category: "Japan / Geek" },
        { key: "tea", glyph: "\u{f0d9e}", label: "Tea", category: "Japan / Geek" },
        { key: "fan", glyph: "\u{f0210}", label: "Fan", category: "Japan / Geek" },
        { key: "mask", glyph: "\u{f1023}", label: "Mask", category: "Japan / Geek" },
        { key: "drama", glyph: "\u{f0d02}", label: "Drama", category: "Japan / Geek" },
        { key: "katana", glyph: "\u{f18be}", label: "Katana", category: "Japan / Geek" },
        { key: "alien", glyph: "\u{f089a}", label: "Alien", category: "Japan / Geek" },
        { key: "robot", glyph: "\u{f1719}", label: "Robot", category: "Japan / Geek" },
        { key: "ghost", glyph: "\u{f02a0}", label: "Ghost", category: "Japan / Geek" },
        { key: "sakura", glyph: "\u{f09f1}", label: "Sakura", category: "Japan / Geek" },
        { key: "crown", glyph: "\uedeb", label: "Crown", category: "Japan / Geek" },
        { key: "sword", glyph: "\u{f04e5}", label: "Sword", category: "Japan / Geek" },
        { key: "emoticon", glyph: "\u{f0c68}", label: "Emoticon", category: "Japan / Geek" },
        { key: "cool", glyph: "\u{f0c6b}", label: "Cool", category: "Japan / Geek" },
        { key: "wink", glyph: "\u{f0c78}", label: "Wink", category: "Japan / Geek" },
        { key: "heart", glyph: "\u{f02d1}", label: "Heart", category: "Japan / Geek" },
        { key: "star", glyph: "\u{f04ce}", label: "Star", category: "Japan / Geek" },
        { key: "kaomoji_smile", glyph: "(\u25d5\u203f\u25d5)", label: "Happy", category: "Kaomoji" },
        { key: "kaomoji_soft", glyph: "(\u00b4\u2022\u1d17\u2022`)", label: "Soft", category: "Kaomoji" },
        { key: "kaomoji_sleepy", glyph: "( \u02d8\u03c9\u02d8 )", label: "Sleepy", category: "Kaomoji" },
        { key: "kaomoji_cheer", glyph: "\u30fd(\u2022\u203f\u2022)\u30ce", label: "Cheer", category: "Kaomoji" },
        { key: "kaomoji_rage", glyph: "(\u256f\u00b0\u25a1\u00b0)\u256f", label: "Flip", category: "Kaomoji" },
        { key: "kaomoji_shrug", glyph: "\u00af\\_(\u30c4)_/\u00af", label: "Shrug", category: "Kaomoji" },
        { key: "kaomoji_happy", glyph: "(\u2022\u203f\u2022)", label: "Smile", category: "Kaomoji" },
        { key: "kaomoji_bear", glyph: "\u0295\u2022\u1d25\u2022\u0294", label: "Bear", category: "Kaomoji" },
        { key: "kaomoji_relaxed", glyph: "\u{30fd}(\u{00b4}\u{30fc}`)\u{30ce}", label: "Relaxed", category: "Kaomoji" },
        { key: "kaomoji_smirk", glyph: "(\u{00ac}\u{203f}\u{00ac})", label: "Smirk", category: "Kaomoji" },
        { key: "kaomoji_disapprove", glyph: "\u{ca0}_\u{ca0}", label: "Disapprove", category: "Kaomoji" },
        { key: "chevron", glyph: "\u276f", label: "Chevron", category: "Prompt" },
        { key: "arrow", glyph: "\u279c", label: "Arrow", category: "Prompt" },
        { key: "lambda", glyph: "\u03bb", label: "Lambda", category: "Prompt" },
        { key: "dollar", glyph: "$", label: "Dollar", category: "Prompt" },
        { key: "angle", glyph: ">", label: "Angle", category: "Prompt" },
        { key: "percent", glyph: "%", label: "Percent", category: "Prompt" },
        { key: "triangle", glyph: "\u25b6", label: "Triangle", category: "Prompt" },
        { key: "hash", glyph: "#", label: "Hash", category: "Prompt" }
    ]

    Column {
        id: body
        width: promptTab.width
        spacing: Style.space(14)

        // ── Style preset ───────────────────────────────────────────────────
        Text {
            text: "STYLE PRESET"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.presets
                delegate: Rectangle {
                    id: presetChip
                    required property string modelData
                    readonly property bool active:
                        promptTab._get("style.preset", "omarchy") === presetChip.modelData
                    width: presetText.implicitWidth + Style.space(20)
                    height: presetText.implicitHeight + Style.space(12)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: presetChip.active ? Color.accent
                        : (presetArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        id: presetText
                        anchors.centerIn: parent
                        text: presetChip.modelData
                        color: presetChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        id: presetArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: promptTab._set("style.preset", presetChip.modelData)
                    }
                }
            }
        }

        // ── Separators ─────────────────────────────────────────────────────
        Text {
            text: "SEPARATOR"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.separators
                delegate: Rectangle {
                    id: sepChip
                    required property var modelData
                    readonly property bool active:
                        promptTab._get("style.separators.shape", "") === sepChip.modelData.key
                    width: Style.space(52)
                    height: Style.space(34)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: sepChip.active ? Color.accent
                        : (sepArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        anchors.centerIn: parent
                        text: sepChip.modelData.glyph
                        color: sepChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                    }

                    MouseArea {
                        id: sepArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            promptTab._set("style.separators.shape", sepChip.modelData.key)
                            promptTab._set("style.separators.left", sepChip.modelData.key)
                            promptTab._set("style.separators.right", sepChip.modelData.key)
                        }
                    }
                }
            }
        }

        // ── Prompt character ───────────────────────────────────────────────
        Text {
            text: "PROMPT CHARACTER"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.promptChars
                delegate: Rectangle {
                    id: charChip
                    required property var modelData
                    readonly property bool active:
                        promptTab._get("segments.character.success", "chevron")
                        === charChip.modelData.key
                    width: Style.space(42)
                    height: Style.space(34)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: charChip.active ? Color.accent
                        : (charArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        anchors.centerIn: parent
                        text: charChip.modelData.glyph
                        color: charChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                    }

                    MouseArea {
                        id: charArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            // The three character roles move together here;
                            // per-role editing lives in the Looks Studio.
                            promptTab._set("segments.character.success", charChip.modelData.key)
                            promptTab._set("segments.character.error", charChip.modelData.key)
                            promptTab._set("segments.character.transient", charChip.modelData.key)
                        }
                    }
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Glyph browser ──────────────────────────────────────────────────
        Text {
            text: "ALL GLYPHS"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        GlyphBrowser {
            width: parent.width
            catalog: promptTab.glyphCatalog
            selected: promptTab._get("segments.character.success", "")
            // Preview in the terminal's font, not the panel's: a glyph that
            // is tofu in the terminal must look like tofu here.
            previewFont: (promptTab.service && promptTab.service.terminalFont)
                ? promptTab.service.terminalFont : Style.font.family
            onPicked: function (key) {
                promptTab._set("segments.character.success", key)
                promptTab._set("segments.character.error", key)
                promptTab._set("segments.character.transient", key)
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Behavior toggles ───────────────────────────────────────────────
        Text {
            text: "BEHAVIOR"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Repeater {
            model: [
                { key: "prompt.newline",      label: "Two-line prompt" },
                { key: "prompt.transient",    label: "Transient prompt" },
                { key: "prompt.blank_line",   label: "Blank line before prompt" },
                { key: "prompt.right_prompt", label: "Right prompt rail" },
                { key: "git.enabled",         label: "Git segment" },
                { key: "style.frame.enabled", label: "Frame lines" },
                { key: "terminal.title.enabled", label: "Set terminal title" }
            ]

            delegate: SettingRow {
                id: row
                required property var modelData
                width: body.width
                label: row.modelData.label
                value: promptTab._get(row.modelData.key, undefined)
                defaultValue: promptTab._default(row.modelData.key)
                onResetRequested: promptTab.service
                    ? promptTab.service.resetConfigValue(row.modelData.key) : undefined

                Toggle {
                    checked: row.value === true
                    onClicked: promptTab._set(row.modelData.key, !(row.value === true))
                }
            }
        }
    }
}
