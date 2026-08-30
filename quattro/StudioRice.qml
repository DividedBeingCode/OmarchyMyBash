pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx

// Studio → Rice tab: the theme-reactive tool layer.
//
// Shows the honest picture rather than pretending Omarchy10k owns theming.
// Omarchy already themes a dozen apps; o10k ADDS a handful more through
// `~/.config/omarchy/themed/*.tpl`, which the Omarchy theme engine renders
// into the current-theme directory on every switch.
//
// Two rules this surface obeys:
//   1. It never writes theme files. It manages TEMPLATES and the include
//      lines in the user's own configs. Rendering stays Omarchy's job.
//   2. It surfaces the silent failure: an o10k include only takes effect if
//      the user's terminal config actually references it, which is
//      otherwise invisible.
Flickable {
    id: riceTab

    property var service: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // Touchpad scrolling at the same rate as the bar popout.
    WheelBoost { flick: riceTab }

    readonly property string home: Quickshell.env("HOME")
    readonly property string themeDir: riceTab.home + "/.local/state/omarchy/current/theme"

    property var omarchyThemed: []
    property var o10kThemed: []
    property var wiring: ({})

    Component.onCompleted: riceTab.refresh()

    function refresh() {
        scanner.running = true
        wiringProbe.running = true
    }

    // Split the rendered theme directory into what Omarchy themes natively
    // and what o10k adds (the `o10k-` prefix is the whole distinction).
    Process {
        id: scanner
        command: ["sh", "-c",
            "ls -1 '" + riceTab.themeDir + "' 2>/dev/null"]
        stdout: StdioCollector {
            onStreamFinished: {
                var native = []
                var ours = []
                var lines = String(this.text).split("\n")
                for (var i = 0; i < lines.length; i++) {
                    var f = lines[i].trim()
                    if (f.length === 0) continue
                    if (f === "backgrounds" || f.indexOf(".png") >= 0) continue
                    if (f.indexOf("o10k-") === 0) ours.push(f)
                    else native.push(f.replace(/\.(conf|ini|toml|theme|lua|json|css|yml)$/, ""))
                }
                riceTab.omarchyThemed = native
                riceTab.o10kThemed = ours
            }
        }
    }

    // An include only works if the user's config references it. "Template
    // installed but not included" is the silent failure this catches.
    Process {
        id: wiringProbe
        command: ["sh", "-c",
            "printf 'ghostty=%s\\n' \"$(grep -c o10k-ghostty " + riceTab.home
            + "/.config/ghostty/config 2>/dev/null || echo 0)\"; "
            + "printf 'foot=%s\\n' \"$(grep -c o10k-foot " + riceTab.home
            + "/.config/foot/foot.ini 2>/dev/null || echo 0)\"; "
            + "printf 'blesh=%s\\n' \"$(grep -rc o10k-blesh " + riceTab.home
            + "/.bashrc 2>/dev/null || echo 0)\""]
        stdout: StdioCollector {
            onStreamFinished: {
                var map = {}
                var lines = String(this.text).split("\n")
                for (var i = 0; i < lines.length; i++) {
                    var parts = lines[i].trim().split("=")
                    if (parts.length === 2)
                        map[parts[0]] = parseInt(parts[1], 10) > 0
                }
                riceTab.wiring = map
            }
        }
    }

    Process { id: wirer }

    // Appends the include line to the user's own config — never to a theme
    // file. Idempotent: the probe re-runs and the row flips to "wired".
    function wireGhostty() {
        wirer.command = ["sh", "-c",
            "grep -q o10k-ghostty " + riceTab.home + "/.config/ghostty/config 2>/dev/null || "
            + "printf '\\nconfig-file = ?\"%s/o10k-ghostty.conf\"\\n' '"
            + riceTab.themeDir + "' >> " + riceTab.home + "/.config/ghostty/config"]
        wirer.running = true
        rewireTimer.restart()
    }

    Timer {
        id: rewireTimer
        interval: 500
        onTriggered: wiringProbe.running = true
    }

    Column {
        id: body
        width: riceTab.width
        spacing: Style.space(14)

        Text {
            width: parent.width
            wrapMode: Text.WordWrap
            text: "Omarchy renders these on every theme switch. Omarchy10k adds "
                  + "the o10k- entries; it never writes theme files itself."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        // ── Themed by Omarchy ──────────────────────────────────────────────
        Text {
            text: "THEMED BY OMARCHY"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Flow {
            width: parent.width
            spacing: Style.space(6)

            Repeater {
                model: riceTab.omarchyThemed
                delegate: Rectangle {
                    id: nativeChip
                    required property string modelData
                    width: nativeText.implicitWidth + Style.space(14)
                    height: nativeText.implicitHeight + Style.space(8)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: Style.normalFill

                    Text {
                        id: nativeText
                        anchors.centerIn: parent
                        text: nativeChip.modelData
                        color: Color.muted
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }
            }
        }

        // ── Themed by Omarchy10k ───────────────────────────────────────────
        Text {
            text: "THEMED BY OMARCHY10K"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Repeater {
            model: riceTab.o10kThemed
            delegate: Row {
                id: ourRow
                required property string modelData
                spacing: Style.space(10)

                readonly property string tool:
                    ourRow.modelData.replace(/^o10k-/, "").replace(/\..*$/, "")
                // Only the terminal includes need an opt-in line in a user
                // config; the rest are sourced by the adapter or read directly.
                readonly property bool needsWiring:
                    ourRow.tool === "ghostty" || ourRow.tool === "foot"
                readonly property bool wired: riceTab.wiring[ourRow.tool] === true

                Text {
                    width: Style.space(120)
                    text: ourRow.tool
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: !ourRow.needsWiring ? "● rendered"
                        : (ourRow.wired ? "● wired" : "⚠ template installed, not included")
                    color: !ourRow.needsWiring ? Color.muted
                        : (ourRow.wired ? Color.accent : Color.urgent)
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                }

                Button {
                    visible: ourRow.needsWiring && !ourRow.wired && ourRow.tool === "ghostty"
                    text: "Add include"
                    bordered: true
                    onClicked: riceTab.wireGhostty()
                }
            }
        }

        Text {
            visible: riceTab.o10kThemed.length === 0
            text: "No o10k templates installed. Run ./install.sh to add them."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Button {
            text: "Rescan"
            bordered: true
            onClicked: riceTab.refresh()
        }
    }
}
