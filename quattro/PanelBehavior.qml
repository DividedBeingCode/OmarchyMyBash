pragma ComponentBehavior: Bound
// BEHAVIOR bucket of the Control Center (B3 decomposition — extracted from
// Panel.qml's behaviorTab, behavior-identical). State stays in Panel.qml and
// arrives via the injected `panel` property. NOTE: the "Toggle prompt
// segments" text, the segment-toggle Grid and the first Time Format row are
// declared inside the "Exit Status" ControlRow exactly as they were before
// the extraction — this file intentionally reproduces that structure.
import QtQuick
import qs.Commons
import qs.Ui

Column {
    id: behaviorBucket

    // Injected panel root: config state, search query, config writes.
    property var panel

    spacing: Style.space(12)

    PanelKit.SectionLabel {
        label: "Prompt"
        panel: behaviorBucket.panel
    }

    PanelKit.ControlRow {
        label: "Lines"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgNewline ? "Two-line" : "One-line"
        options: ["Two-line", "One-line"]
        onChanged: function(val) { behaviorBucket.panel.setConfigValue("prompt.newline", val === "Two-line") }
    }

    PanelKit.ControlRow {
        label: "Spacer"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgBlankLine ? "On" : "Off"
        options: ["On", "Off"]
        onChanged: function(val) { behaviorBucket.panel.setConfigValue("prompt.blank_line", val === "On") }
    }

    PanelKit.ControlRow {
        label: "Transient"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgTransient ? "On" : "Off"
        options: ["On", "Off"]
        onChanged: function(val) { behaviorBucket.panel.setConfigValue("prompt.transient", val === "On") }
    }

    PanelSeparator { foreground: behaviorBucket.panel.barForeground }

    PanelKit.SectionLabel {
        label: "Glyphs"
        panel: behaviorBucket.panel
    }

    PanelKit.GlyphRow {
        label: "Prompt Char"
        configKey: "segments.character.success"
        panel: behaviorBucket.panel
        currentValue: behaviorBucket.panel.cfgCharSuccess
        customHandler: function(key) {
            behaviorBucket.panel.setConfigValue("segments.character.success", key)
            behaviorBucket.panel.setConfigValue("segments.character.error", key)
            behaviorBucket.panel.setConfigValue("segments.character.transient", key)
        }
        glyphs: [
            { key: "\u276f",  glyph: "\u276f",  label: "Chevron" },
            { key: "\u279c",  glyph: "\u279c",  label: "Arrow" },
            { key: "\u03bb",  glyph: "\u03bb",  label: "Lambda" },
            { key: "$",       glyph: "$",       label: "Dollar" },
            { key: ">",       glyph: ">",       label: "Angle" },
            { key: "%",       glyph: "%",       label: "Percent" },
            { key: "\u25b6",  glyph: "\u25b6",  label: "Triangle" },
            { key: "#",       glyph: "#",       label: "Hash" }
        ]
    }

    PanelKit.GlyphRow {
        label: "Animals"
        configKey: "segments.character.success"
        panel: behaviorBucket.panel
        currentValue: behaviorBucket.panel.cfgCharSuccess
        customHandler: function(key) {
            behaviorBucket.panel.setConfigValue("segments.character.success", key)
            behaviorBucket.panel.setConfigValue("segments.character.error", key)
            behaviorBucket.panel.setConfigValue("segments.character.transient", key)
        }
        glyphs: [
            { key: "cat",          glyph: "\uf0b58", label: "Cat" },
            { key: "penguin",      glyph: "\uf0752", label: "Penguin" },
            { key: "fox",          glyph: "\uf0f86", label: "Fox" },
            { key: "owl",          glyph: "\uf1041", label: "Owl" },
            { key: "duck",         glyph: "\uf095f", label: "Duck" },
            { key: "butterfly",    glyph: "\uf10a9", label: "Fly" },
            { key: "ladybug",      glyph: "\uf0828", label: "Ladybug" },
            { key: "bee",          glyph: "\uf0fa1", label: "Bee" },
            { key: "dog",          glyph: "\uf094c", label: "Dog" },
            { key: "rabbit",       glyph: "\uf0810", label: "Rabbit" },
            { key: "turtle",       glyph: "\uf0be0", label: "Turtle" },
            { key: "paw",          glyph: "\uf02f2", label: "Paw" },
            { key: "fish",         glyph: "\uf0143", label: "Fish" },
            { key: "frog",         glyph: "\ued01",  label: "Frog" },
            { key: "dragon",       glyph: "\uee01",  label: "Dragon" },
            { key: "panda",        glyph: "\uf02e3", label: "Panda" },
            { key: "koala",        glyph: "\uf1648", label: "Koala" },
            { key: "unicorn",      glyph: "\uf14cb", label: "Unicorn" },
            { key: "teddy",        glyph: "\uf1804", label: "Teddy" },
            { key: "cow",          glyph: "\uf01e4", label: "Cow" },
            { key: "horse",        glyph: "\uf0f12", label: "Horse" },
            { key: "pig",          glyph: "\uf1045", label: "Pig" },
            { key: "sheep",        glyph: "\uf1077", label: "Sheep" }
        ]
    }

    PanelKit.GlyphRow {
        label: "Kaomoji"
        configKey: "segments.character.success"
        panel: behaviorBucket.panel
        currentValue: behaviorBucket.panel.cfgCharSuccess
        customHandler: function(key) {
            behaviorBucket.panel.setConfigValue("segments.character.success", key)
            behaviorBucket.panel.setConfigValue("segments.character.error", key)
            behaviorBucket.panel.setConfigValue("segments.character.transient", key)
        }
        glyphs: [
            { key: "kaomoji_bear",       glyph: "ʕ•ᴥ•ʔ",   label: "Bear" },
            { key: "kaomoji_smile",      glyph: "(◕‿◕)",   label: "Smile" },
            { key: "kaomoji_rage",       glyph: "(╯°□°)╯", label: "Rage" },
            { key: "kaomoji_relaxed",    glyph: "ヽ(´ー`)ノ", label: "Relaxed" },
            { key: "kaomoji_smirk",      glyph: "(¬‿¬)",   label: "Smirk" },
            { key: "kaomoji_disapprove", glyph: "ಠ_ಠ",     label: "No" }
        ]
    }

    PanelKit.GlyphRow {
        label: "OS Icon"
        configKey: "segments.os.icon"
        panel: behaviorBucket.panel
        currentValue: behaviorBucket.panel.cfgOsIcon
        glyphs: [
            { key: "arch",    glyph: "\uf303",  label: "Arch" },
            { key: "ubuntu",  glyph: "\uf31b",  label: "Ubuntu" },
            { key: "debian",  glyph: "\uf306",  label: "Debian" },
            { key: "fedora",  glyph: "\uf30a",  label: "Fedora" },
            { key: "nixos",   glyph: "\uf313",  label: "NixOS" },
            { key: "macos",   glyph: "\uf179",  label: "macOS" },
            { key: "windows", glyph: "\uf17a",  label: "Win" },
            { key: "linux",   glyph: "\uf17c",  label: "Linux" },
            { key: "omarchy", glyph: "\uf312",  label: "Omarchy" },
            { key: "alpine",  glyph: "\uf300",  label: "Alpine" },
            { key: "void",    glyph: "\uf32e",  label: "Void" },
            { key: "gentoo",  glyph: "\uf30d",  label: "Gentoo" },
            { key: "none",    glyph: "\u2205",  label: "None" }
        ]
    }

    PanelKit.GlyphRow {
        label: "Git Icon"
        configKey: "git.branch_icon"
        panel: behaviorBucket.panel
        currentValue: behaviorBucket.panel.cfgGitBranchIcon
        glyphs: [
            { key: "powerline", glyph: "\ue0a0",  label: "Powerline" },
            { key: "octicon",   glyph: "\uf418",  label: "Octicon" },
            { key: "nerd",      glyph: "\uf126",  label: "Nerd" },
            { key: "text",      glyph: "git:",    label: "Text" },
            { key: "none",      glyph: "\u2205",  label: "None" }
        ]
    }

    PanelSeparator { foreground: behaviorBucket.panel.barForeground }

    PanelKit.SectionLabel {
        label: "Context"
        panel: behaviorBucket.panel
    }

    PanelKit.ControlRow {
        label: "Git"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgGitMode
        options: ["adaptive", "compact", "expanded", "hidden"]
        onChanged: function(val) { behaviorBucket.panel.setConfigValue("git.mode", val) }
    }

    PanelKit.ControlRow {
        label: "Duration"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgCmdDurationMs + "ms"
        options: ["500ms", "1000ms", "1500ms", "3000ms", "5000ms"]
        onChanged: function(val) {
            var ms = parseInt(val)
            behaviorBucket.panel.setConfigValue("segments.command_duration.show_above_ms", ms)
        }
    }

    PanelKit.ControlRow {
        label: "SSH"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgSshShow
        options: ["auto", "always", "never"]
        onChanged: function(val) { behaviorBucket.panel.setConfigValue("segments.ssh.show", val) }
    }

    PanelKit.ControlRow {
        label: "Exit Status"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgExitSignalNames ? "Signal names" : "Codes only"
        options: ["Signal names", "Codes only"]
        onChanged: function(val) {
            behaviorBucket.panel.setConfigValue("segments.exit_status.show_signal_name", val === "Signal names")
        }

        Text {
            text: "Toggle prompt segments on or off."
            color: Color.muted
            font.family: behaviorBucket.panel.bar ? behaviorBucket.panel.bar.fontFamily : Style.font.family
            font.pixelSize: Style.font.caption
            wrapMode: Text.WordWrap
            width: parent.width
        }

        Grid {
            columns: 2
            spacing: Style.space(6)
            width: parent.width

            Repeater {
                model: [
                    { label: "Container", key: "segments.container.enabled", prop: "cfgContainerEnabled" },
                    { label: "Python", key: "segments.python.enabled", prop: "cfgPythonEnabled" },
                    { label: "Toolchain", key: "segments.toolchain.enabled", prop: "cfgToolchainEnabled" },
                    { label: "Nix", key: "segments.nix.enabled", prop: "cfgNixEnabled" },
                    { label: "Kubernetes", key: "segments.k8s.enabled", prop: "cfgK8sEnabled" },
                    { label: "Time", key: "segments.time.enabled", prop: "cfgTimeEnabled" },
                    { label: "Load", key: "segments.load.enabled", prop: "cfgLoadEnabled" },
                    { label: "Battery", key: "segments.battery.enabled", prop: "cfgBatteryEnabled" },
                    { label: "Terminal Title", key: "terminal.title.enabled", prop: "cfgTitleEnabled" }
                ]
                delegate: Rectangle {
                    id: segChip
                    required property var modelData
                    width: (parent.width - Style.space(6)) / 2
                    height: segLabel.implicitHeight + Style.spacing.panelGap
                    visible: behaviorBucket.panel.searchQuery.length === 0
                        || segChip.modelData.label.toLowerCase().indexOf(behaviorBucket.panel.searchQuery.toLowerCase()) >= 0
                    radius: Style.cornerRadius
                    color: behaviorBucket.panel[segChip.modelData.prop]
                        ? (Color.accent)
                        : (Style.normalFillFor(behaviorBucket.panel.barForeground, Color.accent, Color.urgent))

                    Text {
                        id: segLabel
                        anchors.centerIn: parent
                        text: segChip.modelData.label
                        color: behaviorBucket.panel[segChip.modelData.prop]
                            ? (Color.background)
                            : (behaviorBucket.panel.barForeground || "#a9b1d6")
                        font.family: behaviorBucket.panel.bar ? behaviorBucket.panel.bar.fontFamily : Style.font.family
                        font.pixelSize: Style.font.caption
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: behaviorBucket.panel.setConfigValue(segChip.modelData.key, !behaviorBucket.panel[segChip.modelData.prop])
                    }
                }
            }
        }

        PanelKit.ControlRow {
            label: "Time Format"
            configKey: "segments.time.format"
            panel: behaviorBucket.panel
            visible: behaviorBucket.panel.cfgTimeEnabled
            value: behaviorBucket.panel.cfgTimeFormat === "%H:%M" ? "HH:MM"
                 : behaviorBucket.panel.cfgTimeFormat === "%H:%M:%S" ? "HH:MM:SS"
                 : behaviorBucket.panel.cfgTimeFormat === "%I:%M %p" ? "hh:mm AM/PM"
                 : "HH:MM"
            options: ["HH:MM", "HH:MM:SS", "hh:mm AM/PM"]
            onChanged: function(val) {
                var fmt = val === "HH:MM:SS" ? "%H:%M:%S"
                        : val === "hh:mm AM/PM" ? "%I:%M %p"
                        : "%H:%M"
                behaviorBucket.panel.setConfigValue("segments.time.format", fmt)
            }
        }
    }

    PanelSeparator { foreground: behaviorBucket.panel.barForeground }

    PanelKit.ControlRow {
        label: "Time Format"
        panel: behaviorBucket.panel
        visible: behaviorBucket.panel.cfgTimeEnabled
        value: behaviorBucket.panel.cfgTimeFormat === "%H:%M" ? "HH:MM"
             : behaviorBucket.panel.cfgTimeFormat === "%H:%M:%S" ? "HH:MM:SS"
             : behaviorBucket.panel.cfgTimeFormat === "%I:%M %p" ? "hh:mm AM/PM"
             : "HH:MM"
        options: ["HH:MM", "HH:MM:SS", "hh:mm AM/PM"]
        onChanged: function(val) {
            var fmt = val === "HH:MM:SS" ? "%H:%M:%S"
                    : val === "hh:mm AM/PM" ? "%I:%M %p"
                    : "%H:%M"
            behaviorBucket.panel.setConfigValue("segments.time.format", fmt)
        }
    }

    PanelSeparator { foreground: behaviorBucket.panel.barForeground }

    PanelKit.SectionLabel {
        label: "Notifications"
        panel: behaviorBucket.panel
    }

    PanelKit.ControlRow {
        label: "Notify After"
        panel: behaviorBucket.panel
        value: behaviorBucket.panel.cfgNotifyThresholdMs === 5000 ? "5s"
             : behaviorBucket.panel.cfgNotifyThresholdMs === 10000 ? "10s"
             : behaviorBucket.panel.cfgNotifyThresholdMs === 30000 ? "30s"
             : behaviorBucket.panel.cfgNotifyThresholdMs + "ms"
        options: ["5s", "10s", "30s"]
        onChanged: function(val) {
            var ms = val === "5s" ? 5000 : val === "30s" ? 30000 : 10000
            behaviorBucket.panel.setConfigValue("segments.notification.threshold_ms", ms)
        }
    }
}
