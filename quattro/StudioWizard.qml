pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx

// Studio → first-run wizard.
//
// The step list and every option catalog come from
// `omarchy10k configure --describe`, NOT from this file. The CLI wizard is
// the single source of truth; restating the options here is exactly the
// drift that let the CLI wizard rot (segment toggles writing config paths
// the daemon never read, a step falling out of the chain) while nothing
// noticed.
Flickable {
    id: wizard

    property var service: null
    signal finished()

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    property var steps: []
    property var execTier: []
    property int stepIndex: 0
    property var answers: ({})
    property bool loaded: false
    property string loadError: ""

    readonly property var currentStep:
        (wizard.stepIndex >= 0 && wizard.stepIndex < wizard.steps.length)
            ? wizard.steps[wizard.stepIndex] : null

    Component.onCompleted: describeProc.running = true

    Process {
        id: describeProc
        command: ["omarchy10k", "configure", "--describe"]
        stdout: StdioCollector {
            onStreamFinished: {
                var parsed = null
                try { parsed = JSON.parse(String(this.text)) } catch (e) { parsed = null }
                if (!parsed || !parsed.steps) {
                    wizard.loadError = "Could not read the wizard steps "
                                     + "(needs omarchy10k on PATH)."
                    return
                }
                wizard.steps = parsed.steps
                wizard.execTier = (parsed.segments && parsed.segments.exec_tier)
                    ? parsed.segments.exec_tier : []
                wizard.loaded = true
            }
        }
    }

    // Each answer maps onto the config key the daemon actually reads. The
    // mapping lives here because it is UI-side; the OPTIONS do not.
    readonly property var keyFor: ({
        "preset":      "style.preset",
        "separator":   "style.separators.shape",
        "frame":       "frame.enabled",
        "gap_char":    "style.frame.gap_char",
        "prompt_char": "segments.character.success",
        "os_icon":     "segments.os.icon"
    })

    function choose(value) {
        var a = {}
        for (var k in wizard.answers) a[k] = wizard.answers[k]
        a[wizard.currentStep.key] = value
        wizard.answers = a
        if (wizard.stepIndex < wizard.steps.length - 1)
            wizard.stepIndex++
    }

    function applyAll() {
        if (!wizard.service) return
        for (var key in wizard.answers) {
            var tomlKey = wizard.keyFor[key]
            if (!tomlKey) continue
            var v = wizard.answers[key]
            // `frame` is an enum in the wizard but a bool in config.
            if (key === "frame") v = (v !== "none")
            wizard.service.setConfigValue(tomlKey, v)
        }
        wizard.finished()
    }

    Column {
        id: body
        width: wizard.width
        spacing: Style.space(14)

        Text {
            text: "SETUP WIZARD"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            visible: wizard.loadError.length > 0
            text: wizard.loadError
            color: Color.urgent
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            wrapMode: Text.WordWrap
            width: parent.width
        }

        Text {
            visible: wizard.loaded
            text: wizard.currentStep
                ? (wizard.currentStep.label + "  ("
                   + (wizard.stepIndex + 1) + "/" + wizard.steps.length + ")")
                : ""
            color: Color.foreground
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(8)
            visible: wizard.loaded

            Repeater {
                model: wizard.currentStep ? wizard.currentStep.options : []
                delegate: Rectangle {
                    id: optChip
                    required property string modelData
                    readonly property bool chosen: wizard.currentStep
                        && wizard.answers[wizard.currentStep.key] === optChip.modelData
                    width: optText.implicitWidth + Style.space(20)
                    height: optText.implicitHeight + Style.space(12)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: optChip.chosen ? Color.accent
                        : (optArea.containsMouse ? Style.hoverFill : Style.normalFill)

                    Text {
                        id: optText
                        anchors.centerIn: parent
                        // Blank is a legitimate gap-fill option; name it.
                        text: optChip.modelData.length > 0 ? optChip.modelData : "(blank)"
                        color: optChip.chosen ? Color.background : Color.foreground
                        font.family: Style.font.family
                        font.pixelSize: Style.font.bodySmall
                    }

                    MouseArea {
                        id: optArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: wizard.choose(optChip.modelData)
                    }
                }
            }
        }

        Row {
            spacing: Style.space(8)
            visible: wizard.loaded

            Button {
                text: "Back"
                bordered: true
                enabled: wizard.stepIndex > 0
                onClicked: wizard.stepIndex = Math.max(0, wizard.stepIndex - 1)
            }

            Button {
                text: "Skip"
                bordered: true
                enabled: wizard.stepIndex < wizard.steps.length - 1
                onClicked: wizard.stepIndex++
            }

            Button {
                text: "Apply"
                bordered: true
                enabled: Object.keys(wizard.answers).length > 0
                onClicked: wizard.applyAll()
            }
        }

        Text {
            visible: wizard.loaded && wizard.execTier.length > 0
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Note: " + wizard.execTier.join(", ")
                  + " each run an external command, so they stay off unless you "
                  + "enable them on the Prompt tab."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }
    }
}
