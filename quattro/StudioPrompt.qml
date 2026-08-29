pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx

// Studio → Prompt tab: style preset, separators, glyphs, frame and the
// prompt-behavior toggles.
//
// Reads and writes go through the service, never through a panel root, so
// this surface and the bar panel share one config state, one dirty set and
// one debounce.
Flickable {
    id: promptTab

    property var service: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // Bindings must reference a PROPERTY to re-evaluate. Reading through a
    // function call only creates a dependency on `service` itself, so the
    // whole tab stayed frozen at its first paint once the config arrived.
    readonly property var cfg: promptTab.service ? promptTab.service.cfgFlat : ({})
    readonly property var defaults: promptTab.service ? promptTab.service.defaultsFlat : ({})

    function _get(key, fallback) {
        var v = promptTab.cfg[key]
        return v === undefined ? fallback : v
    }

    function _set(key, value) {
        if (promptTab.service) promptTab.service.setConfigValue(key, value)
    }

    function _default(key) {
        return promptTab.defaults[key]
    }

    readonly property var presets: [
        "omarchy", "powerline", "rainbow", "gradient", "framed",
        "classic", "lean", "dense", "slanted", "minimal", "pure"
    ]

    readonly property var separators: [
        { key: "powerline",      glyph: "" },
        { key: "powerline_thin", glyph: "" },
        { key: "slanted",        glyph: "" },
        { key: "round",          glyph: "" },
        { key: "vertical",       glyph: "│" },
        { key: "dot",            glyph: "·" },
        { key: "diamond",        glyph: "◆" },
        { key: "none",           glyph: "∅" }
    ]

    readonly property var promptChars: ["❯", "➜", "λ", "$", ">", "%", "▶", "#"]

    Column {
        id: body
        width: promptTab.width
        spacing: Style.space(14)

        // ── Style preset ───────────────────────────────────────────────────
        Text {
            text: "STYLE PRESET"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.presets
                delegate: Rectangle {
                    id: presetChip
                    required property string modelData
                    readonly property bool active:
                        promptTab._get("style.preset", "omarchy") === presetChip.modelData
                    width: presetText.implicitWidth + Style.space(20)
                    height: presetText.implicitHeight + Style.space(12)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: presetChip.active ? Color.accent
                        : (presetArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        id: presetText
                        anchors.centerIn: parent
                        text: presetChip.modelData
                        color: presetChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        id: presetArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: promptTab._set("style.preset", presetChip.modelData)
                    }
                }
            }
        }

        // ── Separators ─────────────────────────────────────────────────────
        Text {
            text: "SEPARATOR"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.separators
                delegate: Rectangle {
                    id: sepChip
                    required property var modelData
                    readonly property bool active:
                        promptTab._get("style.separators.shape", "") === sepChip.modelData.key
                    width: Style.space(52)
                    height: Style.space(34)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: sepChip.active ? Color.accent
                        : (sepArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        anchors.centerIn: parent
                        text: sepChip.modelData.glyph
                        color: sepChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                    }

                    MouseArea {
                        id: sepArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            promptTab._set("style.separators.shape", sepChip.modelData.key)
                            promptTab._set("style.separators.left", sepChip.modelData.key)
                            promptTab._set("style.separators.right", sepChip.modelData.key)
                        }
                    }
                }
            }
        }

        // ── Prompt character ───────────────────────────────────────────────
        Text {
            text: "PROMPT CHARACTER"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)

            Repeater {
                model: promptTab.promptChars
                delegate: Rectangle {
                    id: charChip
                    required property string modelData
                    readonly property bool active:
                        promptTab._get("segments.character.success", "❯") === charChip.modelData
                    width: Style.space(42)
                    height: Style.space(34)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: charChip.active ? Color.accent
                        : (charArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        anchors.centerIn: parent
                        text: charChip.modelData
                        color: charChip.active ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                    }

                    MouseArea {
                        id: charArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            // The three character roles move together here;
                            // per-role editing lives in the Looks Studio.
                            promptTab._set("segments.character.success", charChip.modelData)
                            promptTab._set("segments.character.error", charChip.modelData)
                            promptTab._set("segments.character.transient", charChip.modelData)
                        }
                    }
                }
            }
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Behavior toggles ───────────────────────────────────────────────
        Text {
            text: "BEHAVIOR"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Repeater {
            model: [
                { key: "prompt.newline",      label: "Two-line prompt" },
                { key: "prompt.transient",    label: "Transient prompt" },
                { key: "prompt.blank_line",   label: "Blank line before prompt" },
                { key: "prompt.right_prompt", label: "Right prompt rail" },
                { key: "git.enabled",         label: "Git segment" },
                { key: "frame.enabled",       label: "Frame lines" },
                { key: "terminal.title.enabled", label: "Set terminal title" }
            ]

            delegate: SettingRow {
                id: row
                required property var modelData
                width: body.width
                label: row.modelData.label
                value: promptTab._get(row.modelData.key, undefined)
                defaultValue: promptTab._default(row.modelData.key)
                onResetRequested: promptTab.service
                    ? promptTab.service.resetConfigValue(row.modelData.key) : undefined

                Toggle {
                    checked: row.value === true
                    onClicked: promptTab._set(row.modelData.key, !(row.value === true))
                }
            }
        }
    }
}
