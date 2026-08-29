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
            { key: "cat", glyph: "\uf011b", label: "Cat" },
            { key: "dog", glyph: "\uf0a43", label: "Dog" },
            { key: "owl", glyph: "\uf03d2", label: "Owl" },
            { key: "duck", glyph: "\uf01e5", label: "Duck" },
            { key: "penguin", glyph: "\uf0ec0", label: "Penguin" },
            { key: "rabbit", glyph: "\uf0907", label: "Rabbit" },
            { key: "turtle", glyph: "\uf0cd7", label: "Turtle" },
            { key: "panda", glyph: "\uf03da", label: "Panda" },
            { key: "koala", glyph: "\uf173f", label: "Koala" },
            { key: "unicorn", glyph: "\uf15c2", label: "Unicorn" },
            { key: "cow", glyph: "\uf019a", label: "Cow" },
            { key: "horse", glyph: "\uf15bf", label: "Horse" },
            { key: "pig", glyph: "\uf0401", label: "Pig" },
            { key: "sheep", glyph: "\uf0cc6", label: "Sheep" },
            { key: "bee", glyph: "\uf0fa1", label: "Bee" },
            { key: "butterfly", glyph: "\uf1589", label: "Butterfly" },
            { key: "ladybug", glyph: "\uf082d", label: "Ladybug" },
            { key: "snail", glyph: "\uf1677", label: "Snail" },
            { key: "spider", glyph: "\uf11ea", label: "Spider" },
            { key: "snake", glyph: "\uf150e", label: "Snake" },
            { key: "bird", glyph: "\uf15c6", label: "Bird" },
            { key: "fish", glyph: "\uf023a", label: "Fish" },
            { key: "dolphin", glyph: "\uf18b4", label: "Dolphin" },
            { key: "shark", glyph: "\uf18ba", label: "Shark" },
            { key: "jellyfish", glyph: "\uf0f01", label: "Jellyfish" },
            { key: "elephant", glyph: "\uf07c6", label: "Elephant" },
            { key: "kangaroo", glyph: "\uf1558", label: "Kangaroo" },
            { key: "donkey", glyph: "\uf07c2", label: "Donkey" },
            { key: "rodent", glyph: "\uf1327", label: "Rodent" },
            { key: "bat", glyph: "\uf0b5f", label: "Bat" },
            { key: "paw", glyph: "\uf03e9", label: "Paw" },
            { key: "bone", glyph: "\uf00b9", label: "Bone" },
            { key: "egg", glyph: "\uf0aaf", label: "Egg" },
            { key: "feather", glyph: "\uf06d3", label: "Feather" },
            { key: "bug", glyph: "\uf00e4", label: "Bug" },
            { key: "dragon", glyph: "\ueef8", label: "Dragon" },
            { key: "frog", glyph: "\uedf8", label: "Frog" },
            { key: "squirrel", glyph: "\ueb58", label: "Squirrel" }
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
