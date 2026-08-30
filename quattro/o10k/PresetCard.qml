import QtQuick
import qs.Commons
import "Fx.js" as Fx
import "../Model.js" as Model

// One preset in the browser — a card that IS its preview.
//
// Replaces two separate cards that both showed a name and nothing else: the
// Studio's Look tile (label on grey) and the Gallery's card (one prompt line
// elided to "…" a third of the way through, on the panel's background rather
// than the preset's). Between them you could not tell Tokyo Rainbow from
// Polar Lean without applying one.
//
// So the card renders a real prompt line, on the preset's OWN background, at
// the size it will actually be, with the palette strip underneath. The name
// and blurb are captions for the picture, not a substitute for it.
//
// Unbound: nothing here reaches an outer id from a delegate, and consumers
// do instantiate it from inline `Component {}` blocks, which cannot use a
// bound component.
Rectangle {
    id: card

    property string label: ""
    property string blurb: ""
    property var tags: []
    /// One `{ left, right }` render from the daemon, or null while loading.
    property var render: null
    /// Flat role → hex map for this preset's palette. Empty means the preset
    /// respects whatever palette you are on — a `structure` Look — and the
    /// card then previews on the CURRENT palette, which is the truth.
    ///
    /// Named `colors`, not `palette`: Item.palette already exists in Qt 6.
    property var colors: ({})
    property string terminalFont: Style.font.family
    property bool active: false
    property bool hovered: area.containsMouse
    /// "ok" | "loading" | "error"
    property string renderState: "loading"

    signal clicked()
    signal entered()
    signal exited()

    readonly property color previewBg: {
        var c = card.colors ? card.colors["background"] : undefined
        return (c !== undefined && c !== null && String(c).length > 0)
            ? String(c) : Color.background
    }
    readonly property color previewFg: {
        var c = card.colors ? card.colors["foreground"] : undefined
        return (c !== undefined && c !== null && String(c).length > 0)
            ? String(c) : Color.foreground
    }

    implicitHeight: layout.implicitHeight + Style.space(20)
    radius: Fx.radius(Style.cornerRadius)

    // Opaque base, tint composited on top. Style.normalFill and friends are
    // 4-8% alpha TINTS, not surfaces: using one as the base colour renders a
    // ~96% transparent card with the wallpaper showing through.
    color: Color.background

    Rectangle {
        anchors.fill: parent
        radius: card.radius
        color: card.active ? Style.selectedFill
            : (card.hovered ? Style.hoverFill : Style.normalFill)
    }

    // Selection reads as a ring rather than a fill, so the preset's own
    // colors stay the loudest thing on the card.
    border.width: card.active ? 2 : 0
    border.color: card.active ? Color.accent : "transparent"

    Column {
        id: layout
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: Style.space(10)
        spacing: Style.space(8)

        // ── The preview ────────────────────────────────────────────────────
        Rectangle {
            id: previewFrame
            width: parent.width
            height: Style.space(46)
            radius: Fx.radius(Style.cornerRadius) / 2
            color: card.previewBg
            clip: true

            Text {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.leftMargin: Style.space(8)
                anchors.rightMargin: Style.space(8)
                visible: card.renderState === "ok" && !!card.render
                text: (card.render && card.render.left)
                    ? Model.ansiToRich(card.render.left, card.colors) : ""
                textFormat: Text.StyledText
                color: card.previewFg
                font.family: card.terminalFont
                font.pixelSize: Style.font.caption
                // One line, never wrapped. The scene is rendered to this
                // card's own column count, so it fits; wrapping would show a
                // layout the terminal will never produce.
                maximumLineCount: 1
                wrapMode: Text.NoWrap
                clip: true
                elide: Text.ElideRight
            }

            Text {
                anchors.centerIn: parent
                visible: card.renderState !== "ok" || !card.render
                text: card.renderState === "error" ? "render failed" : "…"
                color: Qt.rgba(card.previewFg.r, card.previewFg.g, card.previewFg.b, 0.4)
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
            }
        }

        // ── Caption ────────────────────────────────────────────────────────
        Text {
            width: parent.width
            text: card.label
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            font.bold: true
            elide: Text.ElideRight
        }

        Text {
            width: parent.width
            visible: card.blurb.length > 0
            text: card.blurb
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }

        // ── Palette strip + tags ───────────────────────────────────────────
        Item {
            width: parent.width
            height: Math.max(strip.implicitHeight, tagRow.implicitHeight)

            Swatches {
                id: strip
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                visible: card.colors && Object.keys(card.colors).length > 0
                colors: card.colors ? card.colors : ({})
                dotSize: Style.space(8)
                joined: true
            }

            Row {
                id: tagRow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.space(4)

                Repeater {
                    // `structure` / `complete` is the distinction that decides
                    // whether picking this changes your colors, so it is the
                    // one tag worth the space on a card.
                    model: card.tags ? card.tags.filter(function (t) {
                        return t === "structure" || t === "complete" || t === "user"
                    }) : []

                    delegate: Text {
                        required property string modelData
                        text: modelData
                        color: Color.muted
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption - 1
                    }
                }
            }
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: card.clicked()
        // Hover drives the live preview, so these are load-bearing, not chrome.
        onEntered: card.entered()
        onExited: card.exited()
    }
}
