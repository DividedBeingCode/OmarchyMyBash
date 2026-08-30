pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Preview.js" as Preview

// Studio → Looks: the preset browser.
//
// This absorbs what Gallery.qml used to be. That file ran to 1550 lines and
// owned its OWN socket, its own IPC handler and its own hand-rolled widgets,
// duplicating everything Service.qml already does — which is why its buttons
// drifted out of sync with the ones that worked. There is now one socket and
// one code path.
//
// The browse experience it replaces showed a name on grey in the Studio, and
// in the Gallery a single prompt line elided to "…" a third of the way
// through, on the panel's background rather than the preset's. Here every
// card renders a real prompt in the preset's own colors, and hovering one
// previews it full-size in the pane alongside.
Item {
    id: looks

    property var service: null
    /// The pinned preview pane, injected by Studio so hovering a card here
    /// drives it. Null is fine — the grid still works.
    property var previewPane: null

    property string query: ""
    property string activeTag: ""
    property string selected: ""
    property bool editing: false

    readonly property var selectedLook: {
        for (var i = 0; i < looks.allLooks.length; i++) {
            if (looks.allLooks[i].name === looks.selected)
                return looks.allLooks[i]
        }
        return null
    }

    readonly property var allLooks: looks.service && looks.service.looks
        ? looks.service.looks : []

    /// Tags actually present, so the filter row never offers an empty result.
    readonly property var tags: {
        var seen = {}
        var out = []
        for (var i = 0; i < looks.allLooks.length; i++) {
            var t = looks.allLooks[i].tags || []
            for (var j = 0; j < t.length; j++) {
                if (!seen[t[j]]) { seen[t[j]] = true; out.push(t[j]) }
            }
        }
        out.sort()
        return out
    }

    readonly property var results: {
        var q = looks.query.trim().toLowerCase()
        var out = []
        for (var i = 0; i < looks.allLooks.length; i++) {
            var l = looks.allLooks[i]
            if (looks.activeTag.length > 0
                    && (l.tags || []).indexOf(looks.activeTag) < 0)
                continue
            if (q.length > 0) {
                var hay = String(l.label || "") + " " + String(l.name || "")
                        + " " + String(l.blurb || "") + " " + (l.tags || []).join(" ")
                if (hay.toLowerCase().indexOf(q) < 0) continue
            }
            out.push(l)
        }
        return out
    }

    /// Responsive column count. A fixed 4 leaves cards unreadably narrow on a
    /// laptop and absurdly wide on a desktop.
    readonly property int columns: Math.max(2, Math.min(4,
        Math.floor(looks.width / Style.space(260))))

    // ── Per-card renders ───────────────────────────────────────────────────
    //
    // Kept in one map keyed by Look name rather than in each card, so a card
    // scrolling out and back does not re-request what the broker already has.
    property var cardRenders: ({})

    function _paletteFor(look) {
        // A `complete` Look brings its own colors; a `structure` Look
        // respects yours, so its card must preview on the CURRENT palette or
        // it would show something the preset does not actually do.
        var custom = look.patch && look.patch.theme && look.patch.theme.custom
            ? look.patch.theme.custom : null
        if (custom && Object.keys(custom).length > 0) return custom
        return looks.service ? looks.service.currentPaletteColors() : ({})
    }

    function _fetchCard(look) {
        if (!looks.service) return
        looks.service.requestPreview(look.name, null, Preview.CARD_SCENES, true,
            function (res) {
                var next = {}
                for (var k in looks.cardRenders) next[k] = looks.cardRenders[k]
                next[look.name] = res
                // Reassigned, not mutated: a plain JS object mutated in place
                // never re-evaluates the bindings that read it.
                looks.cardRenders = next
            })
    }

    function refreshCards() {
        var r = looks.results
        for (var i = 0; i < r.length; i++) looks._fetchCard(r[i])
    }

    onResultsChanged: looks.refreshCards()
    Component.onCompleted: looks.refreshCards()

    function preview(look, immediate) {
        if (!looks.previewPane || !looks.service) return
        looks.previewPane.caption = look.label || look.name
        looks.previewPane.colors = looks._paletteFor(look)
        looks.previewPane.renderState = "loading"
        looks.service.requestPreview(look.name, null, Preview.SCENES, immediate,
            function (res) {
                looks.previewPane.renderState = res.state
                looks.previewPane.renders = res.renders
                looks.previewPane.errorText = res.error
            })
    }

    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: body
            width: scroll.width
            spacing: Style.space(12)

            // ── Search ─────────────────────────────────────────────────────
            Row {
                width: parent.width
                spacing: Style.space(8)

                TextField {
                    id: search
                    width: parent.width - countLabel.implicitWidth - Style.space(8)
                    placeholderText: "Search presets — try \"minimal\", \"powerline\", \"nord\"…"
                    text: looks.query
                    onTextChanged: looks.query = text
                }

                Text {
                    id: countLabel
                    anchors.verticalCenter: parent.verticalCenter
                    text: looks.results.length + " / " + looks.allLooks.length
                    color: Color.muted
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                }
            }

            // ── Tag filters ────────────────────────────────────────────────
            Flow {
                width: parent.width
                spacing: Style.space(6)

                Chip {
                    label: "all"
                    active: looks.activeTag.length === 0
                    onClicked: looks.activeTag = ""
                }

                Repeater {
                    model: looks.tags

                    delegate: Chip {
                        required property string modelData
                        label: modelData
                        active: looks.activeTag === modelData
                        // Clicking the active tag clears it, so the filter row
                        // never becomes a trap.
                        onClicked: looks.activeTag =
                            (looks.activeTag === modelData) ? "" : modelData
                    }
                }
            }

            // ── The grid ───────────────────────────────────────────────────
            Grid {
                id: grid
                width: parent.width
                columns: looks.columns
                spacing: Style.space(10)

                Repeater {
                    model: looks.results

                    delegate: PresetCard {
                        id: card
                        required property var modelData

                        width: (grid.width - Style.space(10) * (looks.columns - 1))
                               / looks.columns
                        label: card.modelData.label && card.modelData.label.length > 0
                            ? card.modelData.label : card.modelData.name
                        blurb: card.modelData.blurb || ""
                        tags: card.modelData.tags || []
                        colors: looks._paletteFor(card.modelData)
                        active: looks.selected === card.modelData.name

                        readonly property var cardResult:
                            looks.cardRenders[card.modelData.name]
                        renderState: card.cardResult ? card.cardResult.state : "loading"
                        render: (card.cardResult && card.cardResult.renders
                                 && card.cardResult.renders.length > 0)
                            ? card.cardResult.renders[0] : null

                        // Hover previews; click applies. Try-before-buy is the
                        // whole reason the pane is pinned.
                        onEntered: looks.preview(card.modelData, false)
                        onClicked: {
                            if (looks.selected !== card.modelData.name)
                                looks.editing = false
                            looks.selected = card.modelData.name
                            looks.preview(card.modelData, true)
                        }
                    }
                }
            }

            Text {
                visible: looks.results.length === 0
                width: parent.width
                wrapMode: Text.WordWrap
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                text: looks.allLooks.length === 0
                    ? "No presets yet — start a shell with the Omarchy10k prompt, "
                      + "or run: omarchy10k look list"
                    : "Nothing matches that search."
            }

            // ── Apply / edit ───────────────────────────────────────────────
            Row {
                width: parent.width
                spacing: Style.space(8)
                visible: looks.selected.length > 0

                Button {
                    text: "Apply " + looks.selected
                    bordered: true
                    onClicked: {
                        if (looks.service && looks.service.applyLook)
                            looks.service.applyLook(looks.selected, false)
                    }
                }

                Button {
                    text: "Try without saving"
                    bordered: true
                    onClicked: {
                        if (looks.service && looks.service.applyLook)
                            looks.service.applyLook(looks.selected, true)
                    }
                }

                Button {
                    text: looks.editing ? "Close editor" : "Edit\u2026"
                    bordered: true
                    onClicked: {
                        looks.editing = !looks.editing
                        // Leaving the editor puts the pane back on the preset
                        // itself rather than on an abandoned edit.
                        if (!looks.editing && looks.selectedLook)
                            looks.preview(looks.selectedLook, true)
                    }
                }
            }

            // Behind a Loader: the editor is a dozen text fields and a ramp
            // strip that most visits never open.
            Loader {
                id: editorLoader
                width: parent.width
                active: looks.editing && looks.selected.length > 0
                sourceComponent: editorPage
            }

            PanelSeparator { foreground: Color.foreground }

            // ── Save the current config as a Look ──────────────────────────
            Text {
                text: "SAVE CURRENT AS A PRESET"
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                font.bold: true
            }

            Row {
                width: parent.width
                spacing: Style.space(8)

                TextField {
                    id: saveName
                    width: parent.width - saveButton.implicitWidth - Style.space(8)
                    placeholderText: "Name for the current Look…"
                }

                Button {
                    id: saveButton
                    text: "Save"
                    bordered: true
                    enabled: saveName.text.trim().length > 0
                    onClicked: {
                        if (looks.service && looks.service.saveLook)
                            looks.service.saveLook(saveName.text.trim())
                        saveName.text = ""
                    }
                }
            }
        }
    }

    Component {
        id: editorPage

        StudioLookEditor {
            service: looks.service
            look: looks.selectedLook
            previewPane: looks.previewPane
            onClosed: {
                looks.editing = false
                looks.selected = ""
            }
        }
    }
}
