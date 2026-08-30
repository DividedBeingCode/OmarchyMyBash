pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Preview.js" as Preview

// Studio → Looks → the selected preset's editor.
//
// Carried over from Gallery.qml's detail sheet, which was deleted with the
// rest of that file. Dropping it would have been a silent regression: it is
// the only way to tune a palette by hand, design a gradient ramp, or remove a
// Look you saved.
//
// Rebuilt on Service rather than on a private socket, so its edits share the
// same preview broker and config debounce as every other surface.
Item {
    id: editor

    property var service: null
    /// The selected Look, as the `looks` verb returns it.
    property var look: null
    /// The Studio's pinned preview pane, so edits render live.
    property var previewPane: null

    signal closed()

    /// Working patch: the Look's own patch plus any edits made here. Never
    /// written until Save/Overwrite, so browsing an edit costs nothing.
    property var workingPatch: ({})

    readonly property bool isUserLook: editor.look
        && (editor.look.tags || []).indexOf("user") >= 0

    readonly property var editableRoles: [
        "accent", "foreground", "muted", "background",
        "red", "green", "yellow", "blue", "magenta", "cyan", "orange"
    ]

    onLookChanged: editor.reset()

    function reset() {
        if (!editor.look) { editor.workingPatch = ({}); return }
        // Deep-ish copy: the patch is handed straight back to the daemon, and
        // mutating the service's copy would corrupt the Look list in place.
        editor.workingPatch = JSON.parse(JSON.stringify(editor.look.patch || {}))
        editor.refresh()
    }

    function _custom() {
        var p = editor.workingPatch
        if (!p.theme) p.theme = {}
        if (!p.theme.custom) p.theme.custom = {}
        if (!p.theme.source) p.theme.source = "hybrid"
        return p.theme.custom
    }

    function roleValue(role) {
        var p = editor.workingPatch
        var c = p.theme && p.theme.custom ? p.theme.custom[role] : undefined
        if (c !== undefined) return String(c)
        // Falls through to whatever the preset would actually render in.
        var live = editor.service ? editor.service.currentPaletteColors() : ({})
        return live && live[role] ? String(live[role]) : ""
    }

    function setRole(role, hex) {
        var v = String(hex || "").trim()
        if (!/^#[0-9a-fA-F]{6}$/.test(v)) return false
        var next = JSON.parse(JSON.stringify(editor.workingPatch))
        if (!next.theme) next.theme = {}
        if (!next.theme.custom) next.theme.custom = {}
        next.theme.source = "hybrid"
        next.theme.custom[role] = v.toLowerCase()
        // Reassigned, not mutated: a binding over a mutated plain object
        // never re-evaluates.
        editor.workingPatch = next
        editor.refresh()
        return true
    }

    /// Render the working patch, immediately — this follows a keystroke or a
    /// click, not a hover.
    function refresh() {
        if (!editor.previewPane || !editor.service || !editor.look) return
        editor.previewPane.caption = (editor.look.label || editor.look.name) + " · editing"
        var custom = editor.workingPatch.theme && editor.workingPatch.theme.custom
            ? editor.workingPatch.theme.custom : null
        editor.previewPane.colors = (custom && Object.keys(custom).length > 0)
            ? custom
            : (editor.service ? editor.service.currentPaletteColors() : ({}))
        editor.previewPane.renderState = "loading"
        editor.service.requestPreview(editor.look.name, editor.workingPatch,
                                      Preview.SCENES, true, function (res) {
            editor.previewPane.renderState = res.state
            editor.previewPane.renders = res.renders
            editor.previewPane.errorText = res.error
        })
    }

    // ── Gradient ramp ──────────────────────────────────────────────────────
    property string rampStart: "#7aa2f7"
    property string rampEnd: "#bb9af7"

    /// Client-side lerp purely for the preview strip. Applying maps the two
    /// endpoints onto accent/magenta and lets the DAEMON's shipped ramp
    /// engine do the real interpolation — reimplementing it here would be a
    /// second implementation that could disagree with the prompt.
    function rampStep(t) {
        function ch(hex, i) {
            var h = String(hex).replace("#", "")
            return parseInt(h.substring(i * 2, i * 2 + 2), 16) || 0
        }
        function hx(v) {
            var s = Math.max(0, Math.min(255, Math.round(v))).toString(16)
            return s.length < 2 ? "0" + s : s
        }
        var out = "#"
        for (var i = 0; i < 3; i++)
            out += hx(ch(editor.rampStart, i) + (ch(editor.rampEnd, i) - ch(editor.rampStart, i)) * t)
        return out
    }

    readonly property var rampStrip: {
        var out = []
        for (var i = 0; i < 8; i++) out.push(editor.rampStep(i / 7))
        return out
    }

    implicitHeight: body.implicitHeight

    Column {
        id: body
        width: parent.width
        spacing: Style.space(12)

        Row {
            width: parent.width
            spacing: Style.space(8)

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: editor.look
                    ? (editor.look.label || editor.look.name) : ""
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
            }

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: editor.isUserLook ? "your preset" : "curated"
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
            }
        }

        // ── Palette rows ───────────────────────────────────────────────────
        Text {
            text: "PALETTE"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Grid {
            width: parent.width
            columns: 2
            spacing: Style.space(6)

            Repeater {
                model: editor.editableRoles

                delegate: Row {
                    id: roleRow
                    required property string modelData
                    spacing: Style.space(6)

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Style.space(14)
                        height: Style.space(14)
                        radius: width / 2
                        color: editor.roleValue(roleRow.modelData) || "transparent"
                        border.width: 1
                        border.color: Color.muted
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Style.space(76)
                        text: roleRow.modelData
                        color: Color.muted
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                        elide: Text.ElideRight
                    }

                    TextField {
                        width: Style.space(96)
                        text: editor.roleValue(roleRow.modelData)
                        // Committed on Enter or focus loss rather than on every
                        // keystroke, so typing "#7aa2f7" does not fire six
                        // renders for the six invalid prefixes.
                        onEditingFinished: editor.setRole(roleRow.modelData, text)
                    }
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Gradient ramp designer ─────────────────────────────────────────
        Text {
            text: "GRADIENT RAMP"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Row {
            width: parent.width
            spacing: Style.space(8)

            TextField {
                width: Style.space(110)
                text: editor.rampStart
                onEditingFinished: {
                    if (/^#[0-9a-fA-F]{6}$/.test(text)) editor.rampStart = text
                }
            }

            TextField {
                width: Style.space(110)
                text: editor.rampEnd
                onEditingFinished: {
                    if (/^#[0-9a-fA-F]{6}$/.test(text)) editor.rampEnd = text
                }
            }

            Button {
                text: "Apply ramp"
                bordered: true
                onClicked: {
                    editor.setRole("accent", editor.rampStart)
                    editor.setRole("magenta", editor.rampEnd)
                }
            }
        }

        Row {
            spacing: 0

            Repeater {
                model: editor.rampStrip

                delegate: Rectangle {
                    required property string modelData
                    width: Style.space(30)
                    height: Style.space(14)
                    color: modelData
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Save / overwrite / delete ──────────────────────────────────────
        Row {
            width: parent.width
            spacing: Style.space(8)

            TextField {
                id: saveAs
                width: Style.space(180)
                placeholderText: "Save as a new preset…"
            }

            Button {
                text: "Save as new"
                bordered: true
                enabled: saveAs.text.trim().length > 0
                onClicked: {
                    if (editor.service) editor.service.saveLook(saveAs.text.trim())
                    saveAs.text = ""
                }
            }

            // Curated Looks are compiled into the daemon and cannot be
            // overwritten or deleted; the button says so rather than failing.
            Button {
                text: "Overwrite"
                bordered: true
                enabled: editor.isUserLook
                onClicked: {
                    if (editor.service && editor.look)
                        editor.service.saveLook(editor.look.name)
                }
            }

            Button {
                text: editor._deleteArmed ? "Tap again to delete" : "Delete"
                bordered: true
                enabled: editor.isUserLook
                onClicked: {
                    // Two-tap confirm: deleting a preset someone tuned by hand
                    // should not be one stray click away.
                    if (!editor._deleteArmed) {
                        editor._deleteArmed = true
                        disarm.restart()
                        return
                    }
                    editor._deleteArmed = false
                    if (editor.service && editor.look)
                        editor.service.deleteLook(editor.look.name)
                    editor.closed()
                }
            }
        }

        Text {
            visible: !editor.isUserLook
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Curated presets ship with the daemon — save your edits as a "
                  + "new preset instead."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }
    }

    property bool _deleteArmed: false

    Timer {
        id: disarm
        interval: 3000
        onTriggered: editor._deleteArmed = false
    }
}
