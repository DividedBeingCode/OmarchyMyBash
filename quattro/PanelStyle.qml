pragma ComponentBehavior: Bound
// STYLE bucket of the Control Center (B3 decomposition — extracted from
// Panel.qml's appearanceTab, behavior-identical). State stays in Panel.qml
// and arrives via the injected `panel` property.
import QtQuick
import qs.Commons
import qs.Ui
import "o10k"
import "Model.js" as Model

Column {
    id: styleBucket

    // Injected panel root: config state, palette state, config writes.
    property var panel

    spacing: Style.space(10)

    // ── Style Gallery ──────────────────────────────────────────────────────
    PanelKit.SectionLabel {
        label: "Style"
        panel: styleBucket.panel
    }

    Grid {
        columns: 4
        spacing: Style.space(6)
        width: parent.width

        Repeater {
            model: styleBucket.panel.presetCards
            delegate: Rectangle {
                id: presetCard
                required property var modelData
                width: (parent.width - Style.space(18)) / 4
                height: styleCardCol.implicitHeight + Style.spacing.panelGap
                radius: Style.cornerRadius
                color: styleBucket.panel.cfgStylePreset === presetCard.modelData.name
                    ? (Color.accent)
                    : (Style.normalFillFor(styleBucket.panel.barForeground, Color.accent, Color.urgent))
                border.width: styleBucket.panel.cfgStylePreset === presetCard.modelData.name ? 2 : 0
                border.color: Color.accent

                Column {
                    id: styleCardCol
                    anchors.centerIn: parent
                    spacing: Style.space(2)

                    Text {
                        text: styleBucket.panel.presetPreviews[presetCard.modelData.name] || presetCard.modelData.preview
                        textFormat: Text.StyledText
                        color: styleBucket.panel.cfgStylePreset === presetCard.modelData.name
                            ? (Color.background)
                            : (Color.foreground)
                        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                        horizontalAlignment: Text.AlignHCenter
                        width: parent.parent.width - Style.space(8)
                        elide: Text.ElideRight
                    }

                    Text {
                        text: presetCard.modelData.name
                        color: styleBucket.panel.cfgStylePreset === presetCard.modelData.name
                            ? (Color.background)
                            : (styleBucket.panel.barForeground || "#a9b1d6")
                        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                        font.bold: true
                        horizontalAlignment: Text.AlignHCenter
                        width: parent.parent.width - Style.space(8)
                    }

                    Text {
                        text: presetCard.modelData.desc
                        color: styleBucket.panel.cfgStylePreset === presetCard.modelData.name
                            ? Qt.lighter(Color.background, 1.5)
                            : (Color.muted)
                        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                        horizontalAlignment: Text.AlignHCenter
                        width: parent.parent.width - Style.space(8)
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        styleBucket.panel.setConfigValue("style.preset", presetCard.modelData.name)
                        // Preset-controlled granular keys must follow the
                        // preset: _flushSave stamps every CONFIG_MAP key, so
                        // a stale frame/separator toggle from an earlier
                        // preset would silently override the new one.
                        var framed = presetCard.modelData.name === "framed"
                        styleBucket.panel.setConfigValue("style.frame.enabled", framed)
                        styleBucket.panel.setConfigValue("style.frame.gap_char", framed ? "\u2500" : "")
                        styleBucket.panel.setConfigValue("style.separators.left", "")
                        styleBucket.panel.setConfigValue("style.separators.right", "")
                    }
                }
            }
        }
    }

    PanelSeparator { foreground: styleBucket.panel.barForeground }

    // ── Glyph Pickers ──────────────────────────────────────────────────────
    PanelKit.SectionLabel {
        label: "Glyphs"
        panel: styleBucket.panel
    }

    PanelKit.GlyphRow {
        label: "Separator"
        configKey: "style.separators.left"
        panel: styleBucket.panel
        currentValue: styleBucket.panel.cfgSepLeft || "none"
        customHandler: function(key) {
            var val = key === "none" ? "" : key
            styleBucket.panel.setConfigValue("style.separators.left", val)
            styleBucket.panel.setConfigValue("style.separators.right", val)
        }
        glyphs: [
            { key: "none",           glyph: "\u2205",  label: "Default" },
            { key: "powerline",      glyph: "\ue0b0",  label: "Arrow" },
            { key: "powerline_thin", glyph: "\ue0b1",  label: "Thin" },
            { key: "slanted",        glyph: "\ue0bc",  label: "Slant" },
            { key: "round",          glyph: "\ue0b4",  label: "Round" },
            { key: "trapezoid",      glyph: "\ue0d2",  label: "Trap" },
            { key: "trapezoid_rev",  glyph: "\ue0d5",  label: "Trap\u00b7" },
            { key: "flame",          glyph: "\ue0c0",  label: "Flame" },
            { key: "dither",         glyph: "\ue0c4",  label: "Dither" },
            { key: "vertical",       glyph: "\u2502",  label: "Bar" },
            { key: "dot",            glyph: "\u00b7",  label: "Dot" },
            { key: "diamond",        glyph: "\u25c6",  label: "Diamond" },
            { key: "fade",           glyph: "\u2593\u2592\u2591",  label: "Fade" },
            { key: "fade_rev",       glyph: "\u2591\u2592\u2593",  label: "Fade Rev" }
        ]
    }

    PanelSeparator { foreground: styleBucket.panel.barForeground }

    // ── Frame Controls ─────────────────────────────────────────────────────
    Text {
        text: "Frame & Layout"
        color: styleBucket.panel.barForeground || "#a9b1d6"
        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.body
        font.bold: true
    }

    PanelKit.ControlRow {
        label: "Frame Lines"
        panel: styleBucket.panel
        value: styleBucket.panel.cfgFrameEnabled ? "On" : "Off"
        options: ["On", "Off"]
        onChanged: function(val) { styleBucket.panel.setConfigValue("style.frame.enabled", val === "On") }
    }

    PanelKit.ControlRow {
        visible: styleBucket.panel.cfgFrameEnabled
        label: "Gap Fill"
        panel: styleBucket.panel
        value: styleBucket.panel.cfgFrameGapChar === "\u2500" ? "Line \u2500"
             : styleBucket.panel.cfgFrameGapChar === "\u00b7" ? "Dots \u00b7"
             : styleBucket.panel.cfgFrameGapChar === "\u22ef" ? "Ellipsis \u22ef"
             : "None"
        options: ["Line \u2500", "Dots \u00b7", "Ellipsis \u22ef", "None"]
        onChanged: function(val) {
            var ch = val.indexOf("\u2500") >= 0 ? "\u2500"
                   : val.indexOf("\u00b7") >= 0 ? "\u00b7"
                   : val.indexOf("\u22ef") >= 0 ? "\u22ef"
                   : ""
            styleBucket.panel.setConfigValue("style.frame.gap_char", ch)
        }
    }

    PanelSeparator { foreground: styleBucket.panel.barForeground }

    // ── Palette ────────────────────────────────────────────────────────────
    Text {
        text: "Palette"
        color: styleBucket.panel.barForeground || "#a9b1d6"
        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.body
        font.bold: true
    }

    Grid {
        columns: 4
        spacing: Style.space(6)
        width: parent.width

        Repeater {
            // Sourced from the service, which serves 16 curated palettes plus
            // one derived from every installed Omarchy theme. This list used
            // to be the 8 hardcoded entries in Model.js, so the bar panel
            // offered 8 palettes while the Studio offered 30 -- the same
            // drift that had the panel showing a stale Look list.
            // Model.CURATED_PALETTES survives as the offline fallback.
            model: {
                var cards = [{ key: "theme", label: "Omarchy Theme", p: null }]
                var svc = styleBucket.panel.omarchyService
                if (svc && svc.paletteList && svc.paletteList.length > 0) {
                    for (var i = 0; i < svc.paletteList.length; i++) {
                        var e = svc.paletteList[i]
                        cards.push({ key: e.key, label: e.label, p: e.colors })
                    }
                    return cards
                }
                var keys = Object.keys(Model.CURATED_PALETTES)
                for (var j = 0; j < keys.length; j++) {
                    cards.push({ key: keys[j], label: Model.CURATED_PALETTES[keys[j]].label,
                                 p: Model.CURATED_PALETTES[keys[j]] })
                }
                return cards
            }
            delegate: Rectangle {
                required property var modelData
                id: palCard
                property string palKey: modelData.key
                property var pal: modelData.p
                readonly property bool active: styleBucket.panel.cfgPalette === palKey

                width: (parent.width - Style.space(18)) / 4
                height: palCardCol.implicitHeight + Style.spacing.panelGap
                radius: Style.cornerRadius
                color: active ? (Color.accent) : (Style.normalFillFor(styleBucket.panel.barForeground, Color.accent, Color.urgent))
                border.width: active ? 2 : 0
                border.color: Color.accent

                Column {
                    id: palCardCol
                    anchors.centerIn: parent
                    spacing: Style.space(4)

                    Swatches {
                        anchors.horizontalCenter: parent.horizontalCenter
                        visible: !!palCard.pal
                        colors: palCard.pal ? palCard.pal : ({})
                        dotSize: Style.space(7)
                        joined: true
                    }

                    Text {
                        text: palCard.modelData.label
                        color: palCard.active ? (Color.background) : (styleBucket.panel.barForeground || "#a9b1d6")
                        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.bodySmall
                        font.bold: palCard.active
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: styleBucket.panel.applyPalette(palCard.palKey)
                }
            }
        }
    }

    PanelSeparator { foreground: styleBucket.panel.barForeground }

    // ── Theme ──────────────────────────────────────────────────────────────
    Text {
        text: "Theme"
        color: styleBucket.panel.barForeground || "#a9b1d6"
        font.family: styleBucket.panel.bar ? styleBucket.panel.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.body
        font.bold: true
    }

    PanelKit.ControlRow {
        label: "Source"
        configKey: "theme.source"
        panel: styleBucket.panel
        value: styleBucket.panel.cfgThemeSource
        options: ["omarchy", "custom", "hybrid", "terminal"]
        onChanged: function(val) {
            styleBucket.panel.setConfigValue("theme.source", val)
            styleBucket.panel.requestPalette()
        }
    }

    Row {
        spacing: Style.space(3)
        visible: Object.keys(styleBucket.panel.paletteColors).length > 0 || !styleBucket.panel._featureAvailable("0.3")
        Repeater {
            model: ["accent", "foreground", "muted", "background", "red", "green", "yellow", "blue"]
            delegate: Column {
                id: swatch
                required property var modelData
                spacing: 1
                Rectangle {
                    width: 20; height: 20; radius: Style.cornerRadius
                    color: styleBucket.panel.paletteColors[swatch.modelData] || "#333"
                    border.width: 1
                    border.color: Color.muted
                }
                Text {
                    text: swatch.modelData.charAt(0).toUpperCase()
                    color: Color.muted
                    font.pixelSize: 8
                    horizontalAlignment: Text.AlignHCenter
                    width: 20
                }
            }
        }
    }
}
