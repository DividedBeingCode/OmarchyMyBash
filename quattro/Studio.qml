pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx
import "o10k/Motion.js" as Motion
import "o10k/Preview.js" as Preview

// Omarchy10k Studio — the full-screen Control Center.
//
// Registered as the plugin's `panel` entry point, so it is summonable:
//   omarchy-shell shell summon community.omarchy10k
//
// IMPORTANT: the host resolves exactly ONE kind per plugin, and `panel` wins
// over `overlay` (shell.qml computePanelEntries). Declaring `panel` therefore
// UNMOUNTS the overlay entry point, so this file must also serve the pages
// SessionPicker.qml used to route.
//
// ── Layout ─────────────────────────────────────────────────────────────────
//
// Two panes: controls left, a pinned live preview right. Every earlier
// version of this surface let you choose a preset, a separator, a prompt
// character and a glyph without rendering any of them — you found out what
// you had picked by opening a new shell. The preview pane is the fix, and it
// is pinned rather than inline so it never scrolls away from the control you
// are actually turning.
//
// The canvas also grew. It was hardcoded at 1040x720 with content ending
// around y=530, so roughly two thirds of the surface was empty.
Item {
    id: studio

    // Injected by the host panel loader (feature-detected there).
    property var shell: null
    property var manifest: null
    property var service: null
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")

    /// Where this plugin was installed. `omarchy plugin add` clones the whole
    /// repo, so install.sh sits right here and the banner can name its path.
    readonly property string pluginRoot:
        Quickshell.env("HOME") + "/.config/omarchy/plugins/" + studio.pluginId

    readonly property string pluginId:
        manifest && manifest.id ? String(manifest.id) : "community.omarchy10k"

    property bool opened: false
    // "studio" | "sessions"
    property string page: "studio"
    property int currentTab: 0

    readonly property var tabs: [
        { key: "looks",  label: "Looks",  preview: true },
        { key: "prompt", label: "Prompt", preview: true },
        { key: "theme",  label: "Theme",  preview: true },
        { key: "rice",   label: "Rice",   preview: false },
        { key: "system", label: "System", preview: false },
        { key: "setup",  label: "Setup",  preview: true }
    ]

    readonly property var currentTabDef: studio.tabs[studio.currentTab]
    readonly property bool showPreview: studio.currentTabDef
        && studio.currentTabDef.preview === true

    // ── Panel entry-point contract ─────────────────────────────────────────
    function open(payloadJson) {
        var payload = null
        try { payload = JSON.parse(payloadJson || "{}") } catch (e) { payload = null }
        var requested = payload && payload.page ? String(payload.page) : "studio"

        // `gallery` no longer names a separate surface — the Looks tab IS the
        // gallery now. The route is kept so `omarchy-shell shell summon
        // ... {"page":"gallery"}` and Service.openGallery() keep working
        // exactly as they did; only what they land on has changed.
        if (requested === "gallery") {
            studio.page = "studio"
            studio.currentTab = 0
        } else if (requested === "sessions") {
            studio.page = "sessions"
        } else {
            studio.page = "studio"
        }
        studio.opened = true

        if (studio.page === "sessions") {
            sessionsLoader.active = true
            if (sessionsLoader.item && sessionsLoader.item.open)
                sessionsLoader.item.open(payloadJson || "{}")
        }
    }

    function close() {
        studio.opened = false
        if (sessionsLoader.item && sessionsLoader.item.close)
            sessionsLoader.item.close()
        sessionsLoader.active = false
    }

    // User-initiated dismissal routes through the host so its open-panel
    // state stays consistent (the first-party panel convention).
    function dismiss() {
        if (studio.shell && typeof studio.shell.hide === "function")
            studio.shell.hide(studio.pluginId)
        else
            studio.close()
    }

    // ── Sessions page ──────────────────────────────────────────────────────
    Loader {
        id: sessionsLoader
        active: false
        source: Qt.resolvedUrl("SessionPicker.qml")
        onLoaded: {
            if (!item) return
            if ("shell" in item) item.shell = studio.shell
            if ("manifest" in item) item.manifest = studio.manifest
            if ("service" in item) item.service = studio.service
        }
    }

    // ── Studio surface ─────────────────────────────────────────────────────
    PanelWindow {
        id: surface
        visible: studio.opened && studio.page === "studio"
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        WlrLayershell.namespace: "omarchy10k-studio"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        exclusionMode: ExclusionMode.Ignore

        Rectangle {
            anchors.fill: parent
            color: Color.background
            opacity: 0.62
        }

        MouseArea {
            anchors.fill: parent
            onClicked: studio.dismiss()
        }

        Item {
            anchors.fill: parent
            focus: surface.visible
            Keys.onEscapePressed: studio.dismiss()
            Keys.onLeftPressed: studio.currentTab =
                (studio.currentTab + studio.tabs.length - 1) % studio.tabs.length
            Keys.onRightPressed: studio.currentTab =
                (studio.currentTab + 1) % studio.tabs.length

            Card {
                id: canvas
                anchors.centerIn: parent
                // Was min(1040, w-64) x min(720, h-64) with content filling
                // about a third; the preview pane needs room to be legible.
                width: Math.min(1440, parent.width * 0.92)
                height: Math.min(920, parent.height * 0.90)
                elevation: "raised"

                // Swallow clicks so the scrim's dismiss does not fire through.
                MouseArea { anchors.fill: parent }

                Column {
                    anchors.fill: parent
                    anchors.margins: Style.space(22)
                    spacing: Style.space(14)

                    // ── Header ─────────────────────────────────────────────
                    Item {
                        width: parent.width
                        height: title.implicitHeight

                        Row {
                            spacing: Style.space(10)

                            Text {
                                id: title
                                text: "Omarchy10k Studio"
                                color: Color.foreground
                                font.family: Style.font.family
                                font.pixelSize: Style.font.subtitle
                                font.bold: true
                            }

                            Text {
                                anchors.verticalCenter: title.verticalCenter
                                text: studio.service
                                      && studio.service.daemonStatus === "running"
                                    ? "● daemon running" : "○ no daemon"
                                color: studio.service
                                       && studio.service.daemonStatus === "running"
                                    ? Color.accent : Color.muted
                                font.family: Style.font.family
                                font.pixelSize: Style.font.caption
                            }
                        }

                        Text {
                            anchors.right: parent.right
                            // parent, not `title`: title lives inside the Row
                            // above, so it is a nephew rather than a sibling
                            // and the anchor silently does nothing.
                            anchors.verticalCenter: parent.verticalCenter
                            text: "esc to close"
                            color: Color.muted
                            font.family: Style.font.family
                            font.pixelSize: Style.font.caption
                        }
                    }

                    // ── Tab rail ───────────────────────────────────────────
                    Row {
                        spacing: Style.space(6)

                        Repeater {
                            model: studio.tabs

                            delegate: Chip {
                                required property var modelData
                                required property int index
                                label: modelData.label
                                labelSize: Style.font.body
                                active: studio.currentTab === index
                                onClicked: studio.currentTab = index
                            }
                        }
                    }

                    // The binary is missing. `omarchy plugin add` installs
                    // the QML only -- it never builds anything -- so this is
                    // a reachable state, and the one thing that must not
                    // happen is the surfaces silently looking broken.
                    Rectangle {
                        width: parent.width
                        visible: studio.service && studio.service.binaryProbed
                                 && !studio.service.binaryInstalled
                        height: visible ? missingCol.implicitHeight + Style.space(20) : 0
                        radius: Fx.radius(Style.cornerRadius)
                        color: Color.background

                        Rectangle {
                            anchors.fill: parent
                            radius: parent.radius
                            color: Style.normalFill
                        }

                        Column {
                            id: missingCol
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.margins: Style.space(14)
                            spacing: Style.space(6)

                            Text {
                                text: "\u26a0  The omarchy10k binary is not installed"
                                color: Color.urgent
                                font.family: Style.font.family
                                font.pixelSize: Style.font.body
                                font.bold: true
                            }

                            Text {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                color: Color.foreground
                                font.family: Style.font.family
                                font.pixelSize: Style.font.bodySmall
                                text: "This panel configures a prompt daemon, and `omarchy plugin "
                                      + "add` installs only the plugin \u2014 it never builds "
                                      + "anything. Until the binary is on PATH there is nothing "
                                      + "to preview and nothing to apply."
                            }

                            Text {
                                width: parent.width
                                wrapMode: Text.WrapAnywhere
                                color: Color.accent
                                font.family: studio.service && studio.service.terminalFont
                                    ? studio.service.terminalFont : Style.font.family
                                font.pixelSize: Style.font.bodySmall
                                text: "  " + studio.pluginRoot + "/install.sh"
                            }
                        }
                    }

                    PanelSeparator { foreground: Color.foreground }

                    // ── Body: controls | pinned preview ────────────────────
                    Item {
                        id: bodyRow
                        width: parent.width
                        height: parent.height - y

                        readonly property real paneGap: Style.space(18)
                        readonly property real previewWidth: studio.showPreview
                            ? Math.min(Style.space(420), bodyRow.width * 0.38) : 0

                        // One Loader keeps inactive tabs uninstantiated, so
                        // opening the Studio costs only the active tab.
                        Loader {
                            id: tabBody
                            anchors.left: parent.left
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: parent.width - bodyRow.previewWidth
                                   - (studio.showPreview ? bodyRow.paneGap : 0)

                            sourceComponent: {
                                switch (studio.currentTabDef.key) {
                                case "looks":  return looksPage
                                case "prompt": return promptPage
                                case "system": return systemPage
                                case "rice":   return ricePage
                                case "theme":  return themePage
                                case "setup":  return wizardPage
                                default:       return looksPage
                                }
                            }

                            onLoaded: studio._wirePreview()
                        }

                        // ── The pinned preview ─────────────────────────────
                        Column {
                            id: previewColumn
                            anchors.right: parent.right
                            anchors.top: parent.top
                            width: bodyRow.previewWidth
                            spacing: Style.space(10)
                            visible: studio.showPreview

                            TerminalPreview {
                                id: previewPane
                                width: parent.width
                                terminalFont: (studio.service && studio.service.terminalFont)
                                    ? studio.service.terminalFont : Style.font.family
                                renderState: "idle"
                                caption: "preview"
                            }

                            Text {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                color: Color.muted
                                font.family: Style.font.family
                                font.pixelSize: Style.font.caption
                                text: "Hover a preset to preview it here. "
                                      + "Click to select, then Apply."
                                visible: studio.currentTabDef.key === "looks"
                            }
                        }
                    }
                }
            }
        }
    }

    // Hand the freshly loaded tab the preview pane it should drive. Done in
    // onLoaded rather than as a binding because the Loader's item changes
    // identity on every tab switch.
    function _wirePreview() {
        if (!tabBody.item) return

        // Reset BEFORE handing over the pane, never after. Assigning
        // `previewPane` fires the tab's onPreviewPaneChanged, which kicks off
        // its own first render; clearing afterwards clobbered that and left
        // the pane reading "No daemon" while the daemon was plainly running.
        // `idle`, not `empty`: nothing has been requested yet, which is not
        // the same as having no daemon to request it from.
        previewPane.renderState = "idle"
        previewPane.renders = []
        previewPane.errorText = ""
        previewPane.caption = studio.currentTabDef.label.toLowerCase() + " preview"

        if ("previewPane" in tabBody.item)
            tabBody.item.previewPane = previewPane
    }

    // ── Pages ──────────────────────────────────────────────────────────────

    Component {
        id: looksPage
        StudioLooks { service: studio.service }
    }

    Component {
        id: promptPage
        StudioPrompt { service: studio.service }
    }

    Component {
        id: systemPage
        StudioSystem { service: studio.service; shell: studio.shell }
    }

    Component {
        id: ricePage
        StudioRice { service: studio.service }
    }

    Component {
        id: themePage
        StudioTheme { service: studio.service }
    }

    Component {
        id: wizardPage
        StudioWizard {
            service: studio.service
            onFinished: studio.currentTab = 0
        }
    }
}
