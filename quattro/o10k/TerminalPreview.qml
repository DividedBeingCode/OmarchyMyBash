pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import "Fx.js" as Fx
import "../Model.js" as Model

// A terminal, mocked, showing the prompt you are about to choose.
//
// The Control Center let you pick a preset, a separator, a prompt character
// and a glyph without ever rendering any of them — you found out what you had
// chosen by opening a new shell. The daemon has always been able to render a
// real prompt for a hypothetical config; this is the thing that finally shows
// it to you.
//
// Two decisions matter here:
//
//   1. It draws on the PREVIEWED palette's background, not the panel's. A
//      prompt swatched on the Control Center's surface is a preview of the
//      Control Center. The whole question being asked is "what will my
//      terminal look like", and a terminal is dark rectangle first.
//
//   2. Rows come from a single `renders` array, not one request each. Six
//      round-trips per hover is what makes a live preview feel broken.
//
// Performs no I/O: it is handed renders and a palette. The surface owns the
// socket, which is what keeps this testable and keeps preview caching in one
// place (Service.qml's broker).
//
// Bound, like GlyphBrowser: a Repeater delegate has to reach the root id to
// resolve the palette and the chrome colors, and `Bound` is what makes that
// resolve at runtime rather than throwing a ReferenceError. The kit's
// "must stay unbound" rule (Card, SettingRow) is about INLINE `Component {}`
// blocks, which cannot be instantiated cross-file when bound — a FILE
// component like this one is unaffected.
Item {
    id: preview

    /// `[{ label, left, right }]` from the daemon's `renders` field.
    property var renders: []
    /// Flat role → hex map for the palette being previewed. Drives both the
    /// mock's background and how ANSI-indexed colors resolve.
    ///
    /// Named `colors`, not `palette`: Item.palette already exists in Qt 6 and
    /// shadowing it is a real hazard, not a lint nit.
    property var colors: ({})
    /// The TERMINAL's font, not the UI's — a glyph that is tofu in the
    /// terminal must look like tofu here, or the preview is lying.
    property string terminalFont: Style.font.family
    property real fontSize: Style.font.bodySmall
    /// Column count the daemon rendered at, shown in the frame caption so the
    /// preview is honest about the width it assumed.
    property int cols: 120
    property string caption: "preview"
    /// "ok" | "loading" | "idle" | "empty" | "error".
    ///
    /// `idle` and `empty` are different states and used to share one message:
    /// the surface sets `empty` on every tab switch to mean "nothing
    /// requested yet", but empty's copy said "No daemon", so a perfectly
    /// healthy Studio accused itself of having no daemon right beside a
    /// header reading "daemon running".
    ///
    /// Named `renderState`, not `state`: Item.state drives QML's state
    /// machine, which would try to match "loading" against a StateGroup.
    property string renderState: "ok"
    property string errorText: ""
    /// Show each row's scene label in the gutter.
    property bool showLabels: true

    readonly property color bg: {
        var c = preview.colors ? preview.colors["background"] : undefined
        return (c !== undefined && c !== null && String(c).length > 0)
            ? String(c) : Color.background
    }
    readonly property color fg: {
        var c = preview.colors ? preview.colors["foreground"] : undefined
        return (c !== undefined && c !== null && String(c).length > 0)
            ? String(c) : Color.foreground
    }
    /// Gutter/caption ink, dimmed off the terminal's own foreground so the
    /// chrome never competes with the prompt it frames.
    readonly property color chrome: Qt.rgba(preview.fg.r, preview.fg.g, preview.fg.b, 0.45)

    readonly property bool hasRows: preview.renderState === "ok"
        && preview.renders && preview.renders.length > 0

    implicitHeight: frame.implicitHeight

    Rectangle {
        id: frame
        width: parent.width
        implicitHeight: body.implicitHeight + Style.space(24)
        radius: Fx.radius(Style.cornerRadius)
        // The terminal's own background — the point of the component.
        color: preview.bg
        // A hairline in the terminal's ink, so a near-black palette still
        // reads as a bounded object against a near-black panel.
        border.width: 1
        border.color: Qt.rgba(preview.fg.r, preview.fg.g, preview.fg.b, 0.14)
        clip: true

        Column {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: Style.space(12)
            spacing: Style.space(8)

            // ── Caption ────────────────────────────────────────────────────
            Item {
                width: parent.width
                height: captionText.implicitHeight

                Text {
                    id: captionText
                    text: preview.caption
                    color: preview.chrome
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                }

                Text {
                    anchors.right: parent.right
                    text: preview.cols + " cols"
                    color: preview.chrome
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                }
            }

            // ── Rows ───────────────────────────────────────────────────────
            Repeater {
                model: preview.hasRows ? preview.renders : []

                delegate: Column {
                    // The id is load-bearing: a nested Text reaching
                    // `modelData` unqualified resolves at parse time but
                    // throws a ReferenceError at runtime -- exactly the bug
                    // that shipped in Gallery.qml.
                    id: row
                    required property var modelData
                    width: body.width
                    spacing: Style.space(2)

                    Text {
                        visible: preview.showLabels && !!row.modelData.label
                        text: row.modelData.label ? String(row.modelData.label) : ""
                        color: preview.chrome
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption - 1
                    }

                    // Left prompt. StyledText, not RichText: it parses the
                    // small span/colour subset ansiToRich emits and skips the
                    // full HTML document model, which matters when six of
                    // these re-parse on every hover.
                    Text {
                        width: row.width
                        text: Model.ansiToRich(row.modelData.left, preview.colors)
                        textFormat: Text.StyledText
                        color: preview.fg
                        font.family: preview.terminalFont
                        font.pixelSize: preview.fontSize
                        // A prompt is a terminal line: it wraps rather than
                        // eliding, because eliding is what made the old
                        // gallery cards useless.
                        wrapMode: Text.WrapAnywhere
                        lineHeight: 1.25
                    }

                    Text {
                        visible: !!row.modelData.right
                        width: row.width
                        horizontalAlignment: Text.AlignRight
                        text: row.modelData.right
                            ? Model.ansiToRich(row.modelData.right, preview.colors) : ""
                        textFormat: Text.StyledText
                        color: preview.fg
                        font.family: preview.terminalFont
                        font.pixelSize: preview.fontSize
                    }
                }
            }

            // ── Non-ok states, named rather than left blank ────────────────
            //
            // A blank frame reads as a broken component. Each of these is a
            // real condition the surface can be in, so each says which.
            Text {
                visible: !preview.hasRows
                width: parent.width
                wrapMode: Text.WordWrap
                color: preview.chrome
                font.family: Style.font.family
                font.pixelSize: Style.font.bodySmall
                text: {
                    switch (preview.renderState) {
                    case "loading": return "Rendering…"
                    case "error":
                        return preview.errorText.length > 0
                            ? preview.errorText
                            : "The daemon could not render this configuration."
                    case "idle":
                        return "Nothing selected yet."
                    case "empty":
                        return "No daemon — open a shell with the Omarchy10k "
                             + "prompt to see a live preview."
                    default:
                        return "Nothing to preview yet."
                    }
                }
            }

            // ── Palette strip ──────────────────────────────────────────────
            Swatches {
                visible: preview.colors && Object.keys(preview.colors).length > 0
                colors: preview.colors ? preview.colors : ({})
                dotSize: Style.space(9)
            }
        }
    }
}
