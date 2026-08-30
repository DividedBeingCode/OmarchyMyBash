import QtQuick
import qs.Commons
import qs.Ui
import "Fx.js" as Fx

// One glyph tile in the browser.
//
// Extracted from GlyphBrowser's Repeater delegate so the tile is a reusable
// component with plain properties rather than a delegate reaching into outer
// scope. Left unbound — it needs nothing from an enclosing scope.
Rectangle {
    id: tile

    property string glyph: ""
    property string label: ""
    property bool active: false
    /// The TERMINAL's font, so a glyph that is tofu there looks like tofu here.
    property string previewFont: Style.font.family
    /// Glyph size, derived from the tile rather than fixed.
    ///
    /// Was Style.font.subtitle — a flat 13px inside a ~64px tile, so the
    /// glyph took up about a fifth of its own cell. A browser whose entire
    /// job is showing how a glyph renders has to render it big enough to
    /// judge.
    property real glyphSize: Math.max(Style.font.subtitle, tile.width * 0.5)

    signal clicked()

    height: width
    radius: Fx.radius(Style.cornerRadius) / 2
    color: tile.active ? Color.accent
        : (area.containsMouse ? Style.hoverFill : Style.normalFill)

    // Bounded, not just centred. `glyphSize` is the MAXIMUM, not a fixed
    // size: the catalog holds multi-character kaomoji (`¯\_(ツ)_/¯`,
    // `(╯°□°)╯`), and an unbounded centred Text renders those ~250px wide in
    // a ~72px tile, painting straight over its neighbours. The Kaomoji
    // category is a contiguous block of 11 tiles, so filtering to it turned
    // the grid into a smear. HorizontalFit shrinks those to fit while a
    // single glyph still gets the full tile-derived size.
    Text {
        anchors.centerIn: parent
        width: parent.width - Style.space(6)
        horizontalAlignment: Text.AlignHCenter
        text: tile.glyph
        color: tile.active ? Color.background : Color.foreground
        font.family: tile.previewFont
        font.pixelSize: tile.glyphSize
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Style.font.caption
    }

    Text {
        visible: tile.active && tile.label.length > 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Style.space(3)
        width: parent.width - Style.space(6)
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        text: tile.label
        color: Color.background
        font.family: Style.font.family
        font.pixelSize: Style.font.caption
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: tile.clicked()
    }

    PanelToolTip {
        visible: area.containsMouse
        text: tile.label.length > 0 ? tile.label : tile.glyph
    }
}
