pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx
import "o10k/Preview.js" as Preview
import "Model.js" as Model

// Studio → Theme tab: the bind state machine and the two things that drive
// it — Omarchy's own themes, and the terminal-only palette override.
//
// Applying an Omarchy theme is DESKTOP-WIDE and shells out to
// `omarchy theme set`; this surface never writes theme files, matching the
// rule both sibling projects hold.
Flickable {
    id: themeTab

    property var service: null
    /// Injected by Studio: the pinned preview pane this tab drives.
    property var previewPane: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // Touchpad scrolling at the same rate as the bar popout.
    WheelBoost { flick: themeTab }

    property var themes: []
    property string current: ""
    /// Palette search. At 8 palettes a plain list was fine; at 53 it is a
    /// wall, and the curated/derived split is the distinction people actually
    /// filter on.
    property string paletteQuery: ""
    /// "" | "curated" | "derived"
    property string paletteSource: ""

    readonly property var cfg: themeTab.service ? themeTab.service.cfgFlat : ({})
    readonly property var palettes:
        (themeTab.service && themeTab.service.palettes
         && Object.keys(themeTab.service.palettes).length > 0)
            ? themeTab.service.palettes : Model.CURATED_PALETTES

    /// Daemon order — curated first, then derived alphabetically. Built from
    /// Object.keys() a thirty-entry picker reshuffles between opens.
    readonly property var paletteList: {
        if (themeTab.service && themeTab.service.paletteList
                && themeTab.service.paletteList.length > 0)
            return themeTab.service.paletteList
        var out = []
        var keys = Object.keys(themeTab.palettes)
        keys.sort()
        for (var i = 0; i < keys.length; i++) {
            var p = themeTab.palettes[keys[i]]
            out.push({ key: keys[i], label: p.label || keys[i],
                       blurb: p.blurb || "", source: p.source || "curated",
                       accent: p.accent || "",
                       colors: p.colors ? p.colors : p })
        }
        return out
    }

    readonly property var visiblePalettes: {
        var q = themeTab.paletteQuery.trim().toLowerCase()
        var out = []
        for (var i = 0; i < themeTab.paletteList.length; i++) {
            var e = themeTab.paletteList[i]
            if (themeTab.paletteSource.length > 0
                    && String(e.source || "curated") !== themeTab.paletteSource)
                continue
            if (q.length > 0) {
                var hay = String(e.label || "") + " " + String(e.key || "")
                        + " " + String(e.blurb || "")
                if (hay.toLowerCase().indexOf(q) < 0) continue
            }
            out.push(e)
        }
        return out
    }

    /// Preview a palette without applying it — the try-before-buy the theme
    /// picker never had. The current config is rendered against the hovered
    /// palette by patching only `theme.custom`.
    function previewPalette(entry, immediate) {
        if (!themeTab.previewPane || !themeTab.service) return
        themeTab.previewPane.caption = entry.label + " · preview"
        themeTab.previewPane.colors = entry.colors
        themeTab.previewPane.renderState = "loading"
        var patch = { theme: { source: "hybrid", custom: entry.colors } }
        themeTab.service.requestPreview(null, patch, Preview.SCENES, immediate,
            function (res) {
                themeTab.previewPane.renderState = res.state
                themeTab.previewPane.renders = res.renders
                themeTab.previewPane.errorText = res.error
            }, themeTab.previewPane.cols)
    }

    function showCurrent() {
        if (!themeTab.previewPane || !themeTab.service) return
        themeTab.previewPane.caption = "your prompt"
        themeTab.previewPane.colors = themeTab.service.currentPaletteColors()
        themeTab.previewPane.renderState = "loading"
        themeTab.service.requestPreview(null, null, Preview.SCENES, true,
            function (res) {
                themeTab.previewPane.renderState = res.state
                themeTab.previewPane.renders = res.renders
                themeTab.previewPane.errorText = res.error
            }, themeTab.previewPane.cols)
    }

    onPreviewPaneChanged: themeTab.showCurrent()

    Component.onCompleted: themeTab.refresh()

    function refresh() {
        themeLister.running = true
        currentProbe.running = true
    }

    Process {
        id: themeLister
        command: ["omarchy", "theme", "list"]
        stdout: StdioCollector {
            onStreamFinished: {
                var out = []
                var lines = String(this.text).split("\n")
                for (var i = 0; i < lines.length; i++) {
                    var t = lines[i].trim()
                    if (t.length > 0) out.push(t)
                }
                themeTab.themes = out
            }
        }
    }

    Process {
        id: currentProbe
        command: ["omarchy", "theme", "current"]
        stdout: StdioCollector {
            onStreamFinished: themeTab.current = String(this.text).trim()
        }
    }

    Process { id: themeSetter }

    function applyTheme(name) {
        themeSetter.command = ["omarchy", "theme", "set", name]
        themeSetter.running = true
        recheckTimer.restart()
    }

    Timer {
        id: recheckTimer
        interval: 1200
        onTriggered: themeTab.refresh()
    }

    Column {
        id: body
        width: themeTab.width
        spacing: Style.space(14)

        ThemeBindRow {
            width: parent.width
            cfgFlat: themeTab.cfg
            palettes: themeTab.palettes
            desktopTheme: themeTab.service ? themeTab.service.desktopTheme : themeTab.current
            onSyncRequested: {
                if (themeTab.service && themeTab.service.applyPaletteTheme)
                    themeTab.service.applyPaletteTheme()
            }
        }

        // ── Omarchy themes (desktop-wide) ──────────────────────────────────
        Text {
            text: "OMARCHY THEMES"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Applies desktop-wide — every themed app follows, not just the terminal."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: themeTab.themes

                // A theme chip now carries the colors it will apply. The list
                // used to be 22 identical grey words, so choosing meant
                // applying one desktop-wide just to find out what it was.
                delegate: Chip {
                    id: themeChip
                    required property string modelData

                    readonly property var pal: themeTab.palettes[
                        String(themeChip.modelData).toLowerCase().replace(/ /g, "-")]

                    label: themeChip.modelData
                    active: themeTab.current === themeChip.modelData
                    swatches: themeChip.pal && themeChip.pal.colors
                        ? themeChip.pal.colors : null
                    onClicked: themeTab.applyTheme(themeChip.modelData)
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Terminal-only palette override ─────────────────────────────────
        Text {
            text: "PIN TERMINAL COLORS"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Overrides the prompt palette only, leaving the desktop theme alone. "
                  + "This is what unbinds the two — use Sync above to rebind."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Row {
            width: parent.width
            spacing: Style.space(8)

            TextField {
                id: palSearch
                width: parent.width - Style.space(300)
                placeholderText: "Search palettes — try \"neon\", \"cyber\", \"gruvbox\"…"
                text: themeTab.paletteQuery
                onTextChanged: themeTab.paletteQuery = text
            }

            Chip {
                anchors.verticalCenter: parent.verticalCenter
                label: "all"
                active: themeTab.paletteSource.length === 0
                onClicked: themeTab.paletteSource = ""
            }

            Chip {
                anchors.verticalCenter: parent.verticalCenter
                label: "curated"
                active: themeTab.paletteSource === "curated"
                onClicked: themeTab.paletteSource =
                    themeTab.paletteSource === "curated" ? "" : "curated"
            }

            Chip {
                anchors.verticalCenter: parent.verticalCenter
                label: "from themes"
                active: themeTab.paletteSource === "derived"
                onClicked: themeTab.paletteSource =
                    themeTab.paletteSource === "derived" ? "" : "derived"
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: themeTab.visiblePalettes.length + " / " + themeTab.paletteList.length
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
            }
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: themeTab.visiblePalettes

                delegate: Chip {
                    id: palChip
                    required property var modelData

                    readonly property bool isActive:
                        String(themeTab.cfg["theme.custom.accent"] || "").toLowerCase()
                        === String(palChip.modelData.accent || "").toLowerCase()
                        && String(palChip.modelData.accent || "").length > 0

                    label: palChip.modelData.label
                    active: palChip.isActive
                    swatches: palChip.modelData.colors

                    onClicked: {
                        if (themeTab.service && themeTab.service.applyPalette)
                            themeTab.service.applyPalette(palChip.modelData.key)
                    }

                    // Hovering previews the palette against your CURRENT
                    // prompt without applying it -- the thing this picker
                    // could never do.
                    HoverHandler {
                        onHoveredChanged: {
                            if (hovered) themeTab.previewPalette(palChip.modelData, false)
                            else if (themeTab.service) themeTab.service.cancelPreview()
                        }
                    }
                }
            }
        }
    }
}
