import QtQuick.Controls

// Test stub for the omarchy-shell qs.Ui PanelToolTip.
//
// The real component (/usr/share/omarchy/shell/Ui/PanelToolTip.qml) wraps
// QtQuick.Controls ToolTip and pulls in Border, which needs the Quickshell
// runtime — see TextField.qml stub in this directory for the full reason.
// GlyphCell only touches `visible` and `text`, both native to ToolTip.
ToolTip {
}
