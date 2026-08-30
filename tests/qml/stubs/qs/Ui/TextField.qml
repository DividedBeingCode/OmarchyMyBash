import QtQuick.Controls

// Test stub for the omarchy-shell qs.Ui TextField.
//
// The real component (/usr/share/omarchy/shell/Ui/TextField.qml) wraps
// QtQuick.Controls TextField and pulls in Border, which needs the
// Quickshell runtime and cannot load under qmltestrunner — the same reason
// qs.Commons is stubbed rather than pointed at the real shell. GlyphBrowser
// only touches `text`, `placeholderText`, and `onTextChanged`, all of which
// QtQuick.Controls TextField already provides, so no extra surface is added
// here.
TextField {
}
