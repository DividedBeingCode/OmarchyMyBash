pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx

// Studio → System tab: sessions, segment plugins, the shell-layer claim map,
// and diagnostics.
//
// Two of these had no UI anywhere before: `omarchy10k plugin list` and
// `omarchy10k layer --json`. Both are read here through the CLI's own
// machine-readable output rather than reimplemented.
Flickable {
    id: systemTab

    property var service: null
    property var shell: null

    contentWidth: width
    contentHeight: body.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    // Touchpad scrolling at the same rate as the bar popout.
    WheelBoost { flick: systemTab }

    property var plugins: []
    property var layerClaims: []
    property string doctorText: ""
    property bool loading: false

    Component.onCompleted: systemTab.refresh()

    function refresh() {
        systemTab.loading = true
        pluginLister.running = true
        layerLister.running = true
    }

    // ── Data sources ───────────────────────────────────────────────────────
    Process {
        id: pluginLister
        command: ["omarchy10k", "plugin", "list"]
        stdout: StdioCollector {
            onStreamFinished: {
                // `plugin list` prints one plugin per line; the enabled state
                // is the trailing marker. Parsed leniently: a format change
                // must degrade to "no plugins listed", never to a broken tab.
                // Anchor on the bracketed state, which is the only reliable
                // marker in this output:
                //
                //   plugins (/path/to/root):            <- header
                //     <name> <version> [enabled] — ...  <- a real plugin
                //     (invalid) /path — unreadable      <- broken manifest
                //     <name> — enabled in config.toml but NOT installed
                //
                // Splitting on whitespace and taking [0] turned the HEADER
                // into a plugin called "plugins", with a toggle that ran
                // `plugin enable plugins`. And the drift line contains the
                // word "enabled", so a MISSING plugin parsed as an enabled
                // one -- exactly backwards.
                var out = []
                var lines = String(this.text).split("\n")
                var rowRe = /^\s+(\S+)\s+(\S+)\s+\[(enabled|disabled)\]/
                for (var i = 0; i < lines.length; i++) {
                    var raw = lines[i]
                    var t = raw.trim()
                    if (t.length === 0 || t.indexOf("no plugins") >= 0) continue

                    var m = rowRe.exec(raw)
                    if (m) {
                        out.push({ name: m[1], version: m[2],
                                   enabled: m[3] === "enabled", state: m[3] })
                        continue
                    }
                    // Drift and breakage are worth surfacing, not skipping:
                    // both are states the user needs to act on.
                    if (t.indexOf("NOT installed") >= 0) {
                        out.push({ name: t.split(/\s+/)[0], version: "",
                                   enabled: false, state: "missing" })
                    } else if (t.indexOf("(invalid)") >= 0) {
                        out.push({ name: "(invalid manifest)", version: "",
                                   enabled: false, state: "invalid" })
                    }
                    // Anything else -- the header included -- is not a row.
                }
                systemTab.plugins = out
                systemTab.loading = false
            }
        }
    }

    Process {
        id: layerLister
        command: ["omarchy10k", "layer", "--json"]
        stdout: StdioCollector {
            onStreamFinished: {
                var parsed = null
                try { parsed = JSON.parse(String(this.text)) } catch (e) { parsed = null }
                var claims = []
                if (parsed) {
                    var list = Array.isArray(parsed) ? parsed
                        : (Array.isArray(parsed.claims) ? parsed.claims : [])
                    for (var i = 0; i < list.length; i++) {
                        var c = list[i]
                        if (!c) continue
                        claims.push({
                            name: String(c.name || c.claim || "?"),
                            // `omarchy10k layer --json` names the resolved
                            // policy `effective`; the older keys are kept as
                            // fallbacks so a format change degrades rather
                            // than blanking the column.
                            action: String(c.effective || c.action || c.policy || ""),
                            category: String(c.category || ""),
                            note: String(c.note || c.notes || "")
                        })
                    }
                }
                systemTab.layerClaims = claims
            }
        }
    }

    Process {
        id: doctorRunner
        command: ["omarchy10k", "doctor"]
        stdout: StdioCollector {
            onStreamFinished: systemTab.doctorText = String(this.text)
        }
    }

    Process { id: pluginToggler }

    function togglePlugin(name, enabled) {
        pluginToggler.command = ["omarchy10k", "plugin", enabled ? "disable" : "enable", name]
        pluginToggler.running = true
        // The daemon rebuilds its registry on the config reload the CLI
        // triggers; re-list shortly after so the row reflects reality.
        relistTimer.restart()
    }

    Timer {
        id: relistTimer
        interval: 700
        onTriggered: pluginLister.running = true
    }

    Column {
        id: body
        width: systemTab.width
        spacing: Style.space(14)

        // ── Sessions ───────────────────────────────────────────────────────
        Text {
            text: "SESSIONS"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Repeater {
            model: systemTab.service && systemTab.service.sessions
                ? systemTab.service.sessions : []
            delegate: Row {
                id: sessionRow
                required property var modelData
                spacing: Style.space(10)

                Text {
                    text: "shell " + (sessionRow.modelData.shellPid || "?")
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                }

                Text {
                    text: sessionRow.modelData.cwd || ""
                    color: Color.muted
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                    elide: Text.ElideMiddle
                    width: Math.max(0, body.width - Style.space(160))
                }
            }
        }

        Text {
            visible: !systemTab.service || !systemTab.service.sessions
                     || systemTab.service.sessions.length === 0
            text: "No shell sessions found."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Segment plugins ────────────────────────────────────────────────
        Text {
            text: "SEGMENT PLUGINS"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Repeater {
            model: systemTab.plugins
            delegate: SettingRow {
                id: pluginRow
                required property var modelData
                width: body.width
                label: pluginRow.modelData.state === "missing"
                    ? pluginRow.modelData.name + " — enabled but not installed"
                    : pluginRow.modelData.name
                value: pluginRow.modelData.enabled
                // No recorded default for a third-party plugin, so the row
                // never claims "modified" — see SettingRow.
                defaultValue: undefined

                Toggle {
                    checked: pluginRow.modelData.enabled
                    // A plugin that is missing from disk or has an unreadable
                    // manifest has nothing to enable.
                    enabled: pluginRow.modelData.state === "enabled"
                             || pluginRow.modelData.state === "disabled"
                    onClicked: systemTab.togglePlugin(pluginRow.modelData.name,
                                                      pluginRow.modelData.enabled)
                }
            }
        }

        Text {
            visible: systemTab.plugins.length === 0
            text: "No segment plugins installed.\n"
                  + "Add one:  omarchy10k plugin add <git-url>"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            wrapMode: Text.WordWrap
            width: parent.width
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Shell layer ────────────────────────────────────────────────────
        Text {
            text: "SHELL LAYER"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
            font.bold: true
        }

        Text {
            text: "Who owns ls, cd, cat and friends in your shell."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        Repeater {
            model: systemTab.layerClaims
            delegate: Row {
                id: claimRow
                required property var modelData
                spacing: Style.space(10)

                Text {
                    width: Style.space(110)
                    text: claimRow.modelData.name
                    color: Color.foreground
                    font.family: Style.font.family
                    font.pixelSize: Style.font.bodySmall
                }

                Rectangle {
                    width: actionText.implicitWidth + Style.space(12)
                    height: actionText.implicitHeight + Style.space(4)
                    radius: Fx.radius(Style.cornerRadius) / 2
                    color: Style.normalFill

                    Text {
                        id: actionText
                        anchors.centerIn: parent
                        text: claimRow.modelData.action
                        color: claimRow.modelData.action === "defer"
                            ? Color.muted : Color.accent
                        font.family: Style.font.family
                        font.pixelSize: Style.font.caption
                    }
                }

                Text {
                    text: claimRow.modelData.note
                    color: Color.muted
                    font.family: Style.font.family
                    font.pixelSize: Style.font.caption
                    elide: Text.ElideRight
                    width: Math.max(0, body.width - Style.space(220))
                }
            }
        }

        Text {
            visible: systemTab.layerClaims.length === 0
            text: "Claim map unavailable (needs omarchy10k on PATH)."
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        PanelSeparator { foreground: Color.foreground }

        // ── Diagnostics ────────────────────────────────────────────────────
        Row {
            spacing: Style.space(8)

            Button {
                text: "Run doctor"
                bordered: true
                onClicked: doctorRunner.running = true
            }

            Button {
                text: "Refresh"
                bordered: true
                onClicked: systemTab.refresh()
            }
        }

        Card {
            width: parent.width
            height: Math.min(Style.space(220), doctorOut.implicitHeight + Style.space(20))
            visible: systemTab.doctorText.length > 0
            elevation: "flat"

            Text {
                id: doctorOut
                anchors.fill: parent
                anchors.margins: Style.space(10)
                text: systemTab.doctorText
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.NoWrap
            }
        }
    }
}
