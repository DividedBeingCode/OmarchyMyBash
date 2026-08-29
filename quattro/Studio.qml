pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Ui
import "o10k"
import "o10k/Fx.js" as Fx
import "Model.js" as Model

// Omarchy10k Studio — the full-screen Control Center surface.
//
// Registered as the plugin's `panel` entry point, so it is summonable:
//   omarchy-shell shell summon community.omarchy10k
//
// IMPORTANT: the host resolves exactly ONE kind per plugin, and `panel` wins
// over `overlay` (shell.qml computePanelEntries). Declaring `panel` therefore
// UNMOUNTS the overlay entry point, so this file must also serve the pages
// SessionPicker.qml used to route. It does that by delegating to that
// component unchanged rather than reimplementing it — the sessions list and
// the Looks gallery keep their existing behavior, and the gallery folds into
// the Looks tab properly in a later increment.
Item {
    id: studio

    // Injected by the host panel loader (feature-detected there).
    property var shell: null
    property var manifest: null
    property var service: null
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")

    readonly property string pluginId:
        manifest && manifest.id ? String(manifest.id) : "community.omarchy10k"

    property bool opened: false
    // "studio" (tabs) | "sessions" | "gallery"
    property string page: "studio"
    property int currentTab: 0

    readonly property var tabs: [
        { key: "looks",    label: "Looks" },
        { key: "prompt",   label: "Prompt" },
        { key: "rice",     label: "Rice" },
        { key: "theme",    label: "Theme" },
        { key: "system",   label: "System" }
    ]

    // ── Panel entry-point contract ─────────────────────────────────────────
    function open(payloadJson) {
        var payload = null
        try { payload = JSON.parse(payloadJson || "{}") } catch (e) { payload = null }
        var requested = payload && payload.page ? String(payload.page) : "studio"
        studio.page = (requested === "sessions" || requested === "gallery")
            ? requested : "studio"
        studio.opened = true

        if (studio.page !== "studio") {
            legacyLoader.active = true
            if (legacyLoader.item && legacyLoader.item.open)
                legacyLoader.item.open(payloadJson || "{}")
        }
    }

    function close() {
        studio.opened = false
        if (legacyLoader.item && legacyLoader.item.close)
            legacyLoader.item.close()
        legacyLoader.active = false
    }

    // User-initiated dismissal routes through the host so its open-panel
    // state stays consistent (the first-party panel convention).
    function dismiss() {
        if (studio.shell && typeof studio.shell.hide === "function")
            studio.shell.hide(studio.pluginId)
        else
            studio.close()
    }

    // ── Legacy overlay pages ───────────────────────────────────────────────
    // Delegated wholesale so summoning `{"page":"gallery"}` or
    // `{"page":"sessions"}` behaves exactly as it did before this file
    // claimed the entry point.
    Loader {
        id: legacyLoader
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
            opacity: 0.55
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
                width: Math.min(1040, parent.width - Style.space(64))
                height: Math.min(720, parent.height - Style.space(64))
                elevation: "raised"

                // Swallow clicks so the scrim's dismiss does not fire through
                // the canvas.
                MouseArea { anchors.fill: parent }

                Column {
                    anchors.fill: parent
                    anchors.margins: Style.space(20)
                    spacing: Style.space(16)

                    Row {
                        width: parent.width
                        spacing: Style.space(10)

                        Text {
                            text: "Omarchy10k Studio"
                            color: Color.foreground
                            font.family: Style.font.family
                            font.pixelSize: Style.font.subtitle
                            font.bold: true
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: studio.service && studio.service.daemonStatus === "running"
                                ? "● daemon running" : "○ no daemon"
                            color: studio.service && studio.service.daemonStatus === "running"
                                ? Color.accent : Color.muted
                            font.family: Style.font.family
                            font.pixelSize: Style.font.caption
                        }
                    }

                    // Tab rail
                    Row {
                        spacing: Style.space(6)

                        Repeater {
                            model: studio.tabs
                            delegate: Rectangle {
                                id: tabChip
                                required property var modelData
                                required property int index
                                width: tabText.implicitWidth + Style.space(20)
                                height: tabText.implicitHeight + Style.space(12)
                                radius: Fx.radius(Style.cornerRadius) / 2
                                color: studio.currentTab === tabChip.index
                                    ? Color.accent
                                    : (tabArea.containsMouse ? Style.hoverFill : Style.normalFill)

                                Text {
                                    id: tabText
                                    anchors.centerIn: parent
                                    text: tabChip.modelData.label
                                    color: studio.currentTab === tabChip.index
                                        ? Color.background : Color.foreground
                                    font.family: Style.font.family
                                    font.pixelSize: Style.font.body
                                    font.bold: studio.currentTab === tabChip.index
                                }

                                MouseArea {
                                    id: tabArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: studio.currentTab = tabChip.index
                                }
                            }
                        }
                    }

                    PanelSeparator { foreground: Color.foreground }

                    // Tab body. One Loader keeps the inactive tabs
                    // uninstantiated, so opening the Studio costs only the
                    // active tab.
                    Loader {
                        id: tabBody
                        width: parent.width
                        height: parent.height - y
                        sourceComponent: {
                            switch (studio.tabs[studio.currentTab].key) {
                                case "looks":  return looksPage
                                case "prompt": return promptPage
                                case "system": return systemPage
                                default:       return pendingPage
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Pages ──────────────────────────────────────────────────────────────

    Component {
        id: looksPage

        Column {
            spacing: Style.space(14)

            ThemeBindRow {
                width: parent.width
                cfgFlat: studio.service ? studio.service._cfgFlat : ({})
                palettes: (studio.service && studio.service.palettes
                           && Object.keys(studio.service.palettes).length > 0)
                    ? studio.service.palettes : Model.CURATED_PALETTES
                desktopTheme: studio.service ? studio.service.desktopTheme : ""
                onSyncRequested: {
                    if (studio.service && studio.service.applyPaletteTheme)
                        studio.service.applyPaletteTheme()
                }
            }

            Text {
                text: "LOOKS"
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                font.bold: true
            }

            Grid {
                columns: 4
                spacing: Style.space(10)
                width: parent.width

                Repeater {
                    model: studio.service && studio.service.looks
                        ? studio.service.looks : []
                    delegate: Card {
                        id: lookCard
                        required property var modelData
                        width: (parent.width - Style.space(30)) / 4
                        height: Style.space(56)
                        elevation: lookArea.containsMouse ? "raised" : "rest"

                        Text {
                            anchors.centerIn: parent
                            width: parent.width - Style.space(16)
                            horizontalAlignment: Text.AlignHCenter
                            text: lookCard.modelData.label && lookCard.modelData.label.length > 0
                                ? lookCard.modelData.label : lookCard.modelData.name
                            color: Color.foreground
                            font.family: Style.font.family
                            font.pixelSize: Style.font.body
                            elide: Text.ElideRight
                        }

                        MouseArea {
                            id: lookArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (studio.service && studio.service.applyLook)
                                    studio.service.applyLook(lookCard.modelData.name, false)
                            }
                        }
                    }
                }
            }

            Text {
                visible: !studio.service || !studio.service.looks
                         || studio.service.looks.length === 0
                text: "No Looks yet — start a shell with the Omarchy10k prompt, "
                      + "or run: omarchy10k look list"
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                wrapMode: Text.WordWrap
                width: parent.width
            }

            // First-party Button (qs.Ui): PanelActionButton is an ICON
            // button (iconText), which is why `text` did not exist on it.
            Button {
                text: "Open the full Looks gallery"
                bordered: true
                onClicked: {
                    if (studio.shell && typeof studio.shell.summon === "function")
                        studio.shell.summon(studio.pluginId, JSON.stringify({ page: "gallery" }))
                }
            }
        }
    }

    Component {
        id: promptPage
        StudioPrompt { service: studio.service }
    }

    Component {
        id: systemPage
        StudioSystem { service: studio.service; shell: studio.shell }
    }

    // Tabs whose content lands in later increments. Named honestly rather
    // than shipped empty, so the surface never looks broken.
    Component {
        id: pendingPage

        Column {
            spacing: Style.space(10)

            Text {
                text: {
                    switch (studio.tabs[studio.currentTab].key) {
                        case "prompt": return "Prompt"
                        case "rice":   return "Rice"
                        case "theme":  return "Theme"
                        default:       return "System"
                    }
                }
                color: Color.foreground
                font.family: Style.font.family
                font.pixelSize: Style.font.body
                font.bold: true
            }

            Text {
                width: parent.width
                wrapMode: Text.WordWrap
                color: Color.muted
                font.family: Style.font.family
                font.pixelSize: Style.font.caption
                text: {
                    switch (studio.tabs[studio.currentTab].key) {
                        case "prompt":
                            return "Presets, separators, glyphs, frame, per-segment "
                                 + "toggles and the right rail.\n\nAvailable today in the "
                                 + "bar panel under Style and Behavior."
                        case "rice":
                            return "Terminal, git, file-manager and system tool theming, "
                                 + "with the o10k template wiring surfaced.\n\nThe templates "
                                 + "already render on every theme switch; this tab gives them "
                                 + "a UI."
                        case "theme":
                            return "Browse installed Omarchy themes and apply them "
                                 + "desktop-wide, pin terminal colors, or resync.\n\nThe bind "
                                 + "state is live on the Looks tab already."
                        default:
                            return "Doctor, benchmark, sessions, segment plugins and the "
                                 + "shell-layer claim map.\n\nAvailable today in the bar panel "
                                 + "under System."
                    }
                }
            }
        }
    }
}
