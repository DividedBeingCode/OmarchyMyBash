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

    signal clicked()

    height: width
    radius: Fx.radius(Style.cornerRadius) / 2
    color: tile.active ? Color.accent
        : (area.containsMouse ? Style.hoverFill : Style.normalFill)

    Text {
        anchors.centerIn: parent
        text: tile.glyph
        color: tile.active ? Color.background : Color.foreground
        font.family: tile.previewFont
        font.pixelSize: Style.font.subtitle
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
