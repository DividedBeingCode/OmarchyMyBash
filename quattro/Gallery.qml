import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import "Model.js" as Model

// Omarchy10k Looks Gallery — overlay-kind plugin entry point.
//
// Summoned via `omarchy-shell call community.omarchy10k.gallery toggle`
// (the IpcHandler registered below) or the plugin's own shell.summon flow
Browse the daemon's
// Looks (control verb `looks`), preview each one as a REAL dry-run render
// (preview message with the `look` override, protocol v0.4+), then Try it
// transiently (`looks_apply {transient:true}` — reverted by reload_config)
// or Apply it persistently (`looks_apply`).
//
// Escape (or clicking the scrim) closes. Escape inside the detail sheet
// returns to the grid first.
//
// The host injects omarchyPath/shell/manifest — all reads are
// feature-detected so the overlay still opens with an empty state.

Item {
    id: root

    // Injected by the host overlay loader (feature-detected).
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var shell: null
    property var manifest: null

    readonly property string pluginId: manifest && manifest.id ? String(manifest.id) : "community.omarchy10k"

    // ── State ──────────────────────────────────────────────────────────────
    property bool opened: false
    property var looks: []
    property string searchText: ""
    property string activeCategory: "All"
    property string detailName: ""
    property var detailRows: []
    property var previewCache: ({})
    property var _inFlight: ({})
    property var _cfgFlat: ({})
    property string daemonStatus: "not running"
    property string toastMessage: ""
    property bool _showToast: false
    property int _reqSeq: 0

    readonly property int previewCols: 64
    readonly property int eagerPreviews: 9

    // ── Derived ────────────────────────────────────────────────────────────
    readonly property var categories: {
        var seen = { "All": true }
        for (var i = 0; i < root.looks.length; i++) {
            var patch = root.looks[i].patch || {}
            for (var k in patch) {
                var label = root._categoryLabel(k)
                if (!seen[label]) seen[label] = true
            }
        }
        return Object.keys(seen)
    }

    readonly property var filteredLooks: {
        var q = root.searchText.toLowerCase()
        var out = []
        for (var i = 0; i < root.looks.length; i++) {
            var l = root.looks[i]
            if (q.length > 0
                    && String(l.name || "").toLowerCase().indexOf(q) < 0
                    && String(l.label || "").toLowerCase().indexOf(q) < 0)
                continue
            if (root.activeCategory !== "All") {
                var patch = l.patch || {}
                var hit = false
                for (var k in patch)
                    if (root._categoryLabel(k) === root.activeCategory) { hit = true; break }
                if (!hit) continue
            }
            out.push(l)
        }
        return out
    }

    function _categoryLabel(key) {
        if (key === "os") return "Segments"
        var map = { theme: "Theme", style: "Style", segments: "Segments",
                    frame: "Frame", git: "Git", directory: "Directory", prompt: "Prompt" }
        if (map[key]) return map[key]
        return key.charAt(0).toUpperCase() + key.substring(1)
    }

    function _look(name) {
        for (var i = 0; i < root.looks.length; i++)
            if (root.looks[i].name === name) return root.looks[i]
        return null
    }

    // ── Lifecycle (overlay entry point contract: open/close) ──────────────
    function open(payloadJson) {
        root.opened = true
        root.detailName = ""
        root.ensureConnection()
        if (root.looks.length === 0) root.requestLooks()
        Qt.callLater(function () { keyCatcher.forceActiveFocus() })
    }

    function close() {
        root.opened = false
    }

    function dismiss() {
        root.opened = false
        if (root.shell && typeof root.shell.hide === "function")
            root.shell.hide(root.pluginId)
    }

    function _toast(msg) {
        root.toastMessage = msg
        root._showToast = true
        toastTimer.restart()
    }

    // ── Socket: discovery + one shared connection per gallery instance ────
    function ensureConnection() {
        if (gallerySocket.connected) return
        socketFinder.exec(["sh", "-c",
            "for f in '" + Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR")) + "'/omarchy10k-*.sock; do " +
            "[[ -e \"$f\" ]] || continue; p=${f##*-}; p=${p%.sock}; " +
            "case \"$p\" in *[!0-9]*) ;; *) kill -0 \"$p\" 2>/dev/null || continue ;; esac; " +
            "timeout 1 socat -u OPEN:/dev/null UNIX-CONNECT:\"$f\" 2>/dev/null && echo \"$f\"; done"])
    }

    function connectTo(path) {
        gallerySocket.connected = false
        gallerySocket.path = path
        gallerySocket.connected = true
    }

    function _nextId(prefix) {
        root._reqSeq++
        return prefix + "-" + root._reqSeq
    }

    function requestLooks() {
        if (!gallerySocket.connected) return
        gallerySocket.write(Model.buildCommand("looks", _nextId("looks")))
        gallerySocket.flush()
    }

    function requestConfig() {
        if (!gallerySocket.connected) return
        gallerySocket.write(Model.buildConfigGet(_nextId("cfg")))
        gallerySocket.flush()
    }

    // Lazy real-render previews: one preview request per Look with the
    // daemon-side `look` override. Results are cached in previewCache
    // (name → StyledText markup); cards request on creation, so only the
    // first eagerPreviews and cards actually scrolled into view cost a
    // round-trip. Daemons that ignore `look` render the current look for
    // every card — the same graceful degradation the preset cards use.
    function requestPreview(name) {
        if (!gallerySocket.connected) return
        if (root.previewCache[name] !== undefined) return
        if (root._inFlight[name]) return
        root._inFlight[name] = true
        var ctx = {
            cwd: "~/projects/my-app",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: root.previewCols,
            jobs: 0,
            in_ssh: false,
            git_branch: "main",
            git_staged: 2,
            git_unstaged: 1,
            look: name
        }
        gallerySocket.write(Model.buildPreview(ctx, "look-" + name))
        gallerySocket.flush()
    }

    function requestEagerPreviews() {
        for (var i = 0; i < root.filteredLooks.length && i < root.eagerPreviews; i++)
            root.requestPreview(root.filteredLooks[i].name)
    }

    // ── Try / Apply ────────────────────────────────────────────────────────
    function applyLook(name, transient) {
        if (!gallerySocket.connected) {
            root._toast("No omarchy10k daemon running")
            return
        }
        var id = _nextId("apply")
        var msg = { type: "control", command: "looks_apply", name: name, transient: transient, id: id }
        gallerySocket.write(JSON.stringify(msg) + "\n")
        gallerySocket.flush()
        root._pendingApply[id] = { name: name, transient: transient }
    }

    property var _pendingApply: ({})

    function _completeApply(id, resp) {
        var req = root._pendingApply[id]
        if (!req) return
        delete root._pendingApply[id]
        if (resp.status === "error") {
            root._toast("Failed: " + (resp.error || "unknown error"))
            return
        }
        if (req.transient)
            root._toast("Trying \u201C" + req.name + "\u201D — reload_config reverts it")
        else
            root._toast("Applied \u201C" + req.name + "\u201D")
    }

    // ── Detail sheet ───────────────────────────────────────────────────────
    function openDetail(name) {
        var look = root._look(name)
        if (!look) return
        root.detailName = name
        root.requestPreview(name)
        if (Object.keys(root._cfgFlat).length === 0) root.requestConfig()
        root.detailRows = root._patchSummary(look.patch || {})
    }

    function closeDetail() {
        root.detailName = ""
        root.detailRows = []
    }

    // What the Look touches: every leaf of the patch as old → new. Old
    // values come from the daemon's flattened live config when available.
    function _patchSummary(patch) {
        var flat = Model.flattenConfig(patch, "")
        var keys = Object.keys(flat).sort()
        var rows = []
        for (var i = 0; i < keys.length && rows.length < 12; i++) {
            var k = keys[i]
            var oldV = root._cfgFlat[k]
            rows.push({
                key: k,
                old: oldV === undefined ? "\u2014" : String(oldV),
                neu: String(flat[k])
            })
        }
        if (keys.length > rows.length)
            rows.push({ key: "+ " + (keys.length - rows.length) + " more", old: "", neu: "" })
        return rows
    }

    // ── Daemon message handling ────────────────────────────────────────────
    function _handleMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)

        if (resp.type === "hello") {
            root.daemonStatus = "running"
            root.requestLooks()
            root.requestConfig()
            return
        }

        if (resp.type === "control" && resp.looks) {
            root.looks = resp.looks
            root.requestEagerPreviews()
            return
        }

        if (resp.type === "config" && resp.config) {
            root._cfgFlat = Model.flattenConfig(resp.config)
            if (root.detailName !== "") {
                var look = root._look(root.detailName)
                if (look) root.detailRows = root._patchSummary(look.patch || {})
            }
            return
        }

        if (resp.type === "preview" && resp.id && resp.id.indexOf("look-") === 0) {
            var name = resp.id.substring("look-".length)
            delete root._inFlight[name]
            if (resp.left) {
                var map = root.previewCache
                map[name] = Model.ansiToRich(resp.left)
                // Reassign a COPY — self-assigning the same object reference
                // does not notify index-access bindings, and the cards keep
                // their "--" placeholders forever.
                root.previewCache = Object.assign({}, map)
            }
            return
        }

        if (resp.id !== undefined && root._pendingApply[resp.id] !== undefined) {
            root._completeApply(resp.id, resp)
            return
        }

        if (resp.status === "error") {
            root._toast("Failed: " + (resp.error || "unknown error"))
            return
        }
        if (resp.status === "bye")
            root.daemonStatus = "not running"
    }

    function _onSocketConnected() {
        gallerySocket.write(Model.buildHello("gallery-handshake"))
        gallerySocket.flush()
    }

    function _onSocketError() {
        gallerySocket.connected = false
        root.daemonStatus = "not running"
    }

    onOpenedChanged: if (opened) root.ensureConnection()

    // ── IPC: omarchy-shell call community.omarchy10k.gallery <method> ─────
    IpcHandler {
        target: "community.omarchy10k.gallery"

        function toggle(): string {
            if (root.opened) { root.close(); return JSON.stringify({ ok: true, open: false }) }
            root.open("")
            return JSON.stringify({ ok: true, open: true })
        }

        function open(): string {
            root.open("")
            return JSON.stringify({ ok: true, open: true })
        }

        function close(): string {
            root.close()
            return JSON.stringify({ ok: true, open: false })
        }
    }

    // ── I/O components ─────────────────────────────────────────────────────
    Process {
        id: socketFinder
        stdout: StdioCollector {
            onStreamFinished: {
                var text = this.text.trim()
                if (text.length === 0) {
                    root.daemonStatus = "not running"
                    return
                }
                root.daemonStatus = "running"
                if (!gallerySocket.connected) root.connectTo(text.split("\n")[0].trim())
                else if (root.looks.length === 0) root.requestLooks()
            }
        }
    }

    Socket {
        id: gallerySocket
        connected: false
        parser: SplitParser {
            onRead: message => root._handleMessage(message)
        }
        onConnectedChanged: {
            if (gallerySocket.connected) root._onSocketConnected()
        }
        onError: root._onSocketError()
    }

    Timer {
        id: toastTimer
        interval: 2600
        onTriggered: root._showToast = false
    }

    // ── Overlay surface ────────────────────────────────────────────────────
    PanelWindow {
        id: overlay
        visible: root.opened
        anchors { top: true; bottom: true; left: true; right: true }
        color: "transparent"
        WlrLayershell.namespace: "omarchy10k-gallery"
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
        exclusionMode: ExclusionMode.Ignore

        Rectangle {
            anchors.fill: parent
            color: Color.menu.scrim
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismiss()
        }

        Rectangle {
            id: canvas
            width: Math.min(900, parent.width - Style.space(32))
            height: Math.min(640, parent.height - Style.space(32))
            radius: Style.cornerRadius
            anchors.centerIn: parent
            color: Color.menu.background
            border.width: 1
            border.color: Color.menu.border

            MouseArea { anchors.fill: parent; onClicked: { } }

            Item {
                id: keyCatcher
                anchors.fill: parent
                focus: true

                Keys.priority: Keys.BeforeItem
                Keys.onPressed: function (event) {
                    if (event.key === Qt.Key_Escape) {
                        if (root.detailName !== "") root.closeDetail()
                        else root.dismiss()
                        event.accepted = true
                    } else if (root.detailName === "") {
                        if (event.key === Qt.Key_Left) {
                            grid.moveCurrentIndexLeft()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Right) {
                            grid.moveCurrentIndexRight()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Up) {
                            grid.moveCurrentIndexUp()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Down) {
                            grid.moveCurrentIndexDown()
                            event.accepted = true
                        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            if (grid.currentIndex >= 0 && grid.currentIndex < root.filteredLooks.length)
                                root.openDetail(root.filteredLooks[grid.currentIndex].name)
                            event.accepted = true
                        }
                    }
                }

                Column {
                    anchors.fill: parent
                    anchors.margins: Style.space(16)
                    spacing: Style.space(12)

                    // ── Header ─────────────────────────────────────────
                    Row {
                        width: parent.width
                        spacing: Style.space(12)

                        Text {
                            text: "Looks Gallery"
                            color: Color.menu.text
                            font.family: Style.fontFamily
                            font.pixelSize: Style.font.heading
                            font.bold: true
                            anchors.verticalCenter: parent.verticalCenter
                        }

                        Item { width: 1; height: 1 } // spring
                        TextField {
                            id: searchField
                            width: Math.min(260, parent.width - Style.space(220))
                            height: Style.spacing.controlHeight
                            placeholderText: root.daemonStatus === "running"
                                ? "Search looks…"
                                : "No omarchy10k daemon running"
                            enabled: root.daemonStatus === "running"
                            color: Color.menu.text
                            placeholderTextColor: Color.muted
                            font.family: Style.fontFamily
                            font.pixelSize: Style.font.bodySmall
                            onTextChanged: {
                                root.searchText = text
                                root.requestEagerPreviews()
                            }
                            background: Rectangle {
                                radius: Style.cornerRadius
                                color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)
                                border.width: searchField.activeFocus ? 1 : 0
                                border.color: Color.accent
                            }
                        }
                    }

                    // ── Filters row ────────────────────────────────────
                    Row {
                        width: parent.width
                        spacing: Style.spacing.controlGap

                        Repeater {
                            id: filterChips
                            model: root.categories

                            delegate: Rectangle {
                                required property string modelData
                                width: chipLabel.implicitWidth + Style.spacing.controlPaddingX * 2
                                height: chipLabel.implicitHeight + Style.space(8)
                                radius: Style.cornerRadius
                                color: root.activeCategory === modelData
                                    ? Color.accent
                                    : (chipMa.containsMouse
                                        ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                        : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent))

                                Text {
                                    id: chipLabel
                                    anchors.centerIn: parent
                                    text: modelData
                                    color: root.activeCategory === modelData
                                        ? Color.background
                                        : Color.menu.text
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.bodySmall
                                    font.bold: root.activeCategory === modelData
                                }

                                MouseArea {
                                    id: chipMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        root.activeCategory = modelData
                                        root.requestEagerPreviews()
                                    }
                                }
                            }
                        }
                    }

                    // ── Grid view ──────────────────────────────────────
                    GridView {
                        id: grid
                        width: parent.width
                        height: root.detailName === "" ? parent.height - headerHeight - filterHeight - toastSpace : 0
                        visible: root.detailName === ""
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        cellWidth: Math.floor(width / 3)
                        cellHeight: Style.space(120)
                        model: root.filteredLooks
                        currentIndex: 0

                        // Heights of header + filters + spacers, kept in sync
                        // with the Column's spacing so the grid fills the rest.
                        readonly property int headerHeight: Style.font.heading + Style.space(16)
                        readonly property int filterHeight: Style.space(34)
                        readonly property int toastSpace: Style.space(28)

                        delegate: Rectangle {
                            required property var modelData
                            required property int index

                            width: grid.cellWidth
                            height: grid.cellHeight
                            radius: Style.cornerRadius
                            color: grid.currentIndex === index
                                ? Style.selectedFillFor(Color.foreground, Color.accent, Color.urgent)
                                : (cardMa.containsMouse
                                    ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                    : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent))
                            border.width: grid.currentIndex === index ? 1 : 0
                            border.color: Color.accent

                            Column {
                                anchors.fill: parent
                                anchors.margins: Style.space(8)
                                spacing: Style.space(4)
                                Rectangle {
                                    width: parent.width
                                    height: parent.height - lookLabelText.implicitHeight - Style.space(12)
                                    radius: Style.cornerRadius
                                    color: Color.background

                                    Text {
                                        anchors.fill: parent
                                        anchors.margins: Style.space(6)
                                        text: root.previewCache[modelData.name] !== undefined
                                            ? root.previewCache[modelData.name]
                                            : "…"
                                        textFormat: Text.StyledText
                                        color: Color.foreground
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        elide: Text.ElideRight
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }

                                Text {
                                    id: lookLabelText
                                    width: parent.width
                                    text: modelData.label || modelData.name
                                    color: Color.menu.text
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.bodySmall
                                    font.bold: true
                                    elide: Text.ElideRight
                                }
                            }

                            Component.onCompleted: root.requestPreview(modelData.name)

                            MouseArea {
                                id: cardMa
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onContainsMouseChanged: if (containsMouse) grid.currentIndex = index
                                onClicked: root.openDetail(modelData.name)
                            }
                        }
                    }

                    // ── Detail sheet (replaces the grid) ───────────────
                    Column {
                        id: detailSheet
                        width: parent.width
                        height: root.detailName === "" ? 0 : parent.height - headerHeightD - toastSpaceD
                        visible: root.detailName !== ""
                        spacing: Style.space(12)

                        readonly property int headerHeightD: Style.font.heading + Style.space(16)
                        readonly property int toastSpaceD: Style.space(28)

                        Row {
                            width: parent.width
                            spacing: Style.space(12)

                            Text {
                                text: {
                                    var l = root._look(root.detailName)
                                    return l ? (l.label || l.name) : ""
                                }
                                color: Color.menu.text
                                font.family: Style.fontFamily
                                font.pixelSize: Style.font.subtitle
                                font.bold: true
                                anchors.verticalCenter: parent.verticalCenter
                            }

                            Item { width: 1; height: 1 } // spring

                            Rectangle {
                                width: backText.implicitWidth + Style.space(16)
                                height: Style.spacing.controlHeight
                                radius: Style.cornerRadius
                                anchors.verticalCenter: parent.verticalCenter
                                color: backMa.containsMouse
                                    ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                    : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                Text {
                                    id: backText
                                    anchors.centerIn: parent
                                    text: "Back to grid"
                                    color: Color.menu.text
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.bodySmall
                                }

                                MouseArea {
                                    id: backMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.closeDetail()
                                }
                            }
                        }

                        // Large real render of the Look's prompt.
                        Rectangle {
                            width: parent.width
                            height: detailPreview.implicitHeight + Style.space(20)
                            radius: Style.cornerRadius
                            color: Color.background

                            Text {
                                id: detailPreview
                                x: Style.space(12)
                                y: Style.space(10)
                                text: {
                                    var cached = root.previewCache[root.detailName]
                                    return cached !== undefined ? cached : "Rendering…"
                                }
                                textFormat: Text.StyledText
                                color: Color.foreground
                                font.family: Style.fontFamily
                                font.pixelSize: Style.font.body
                            }
                        }

                        // What the Look touches: patch keys, old → new.
                        Column {
                            width: parent.width
                            spacing: Style.space(4)

                            Text {
                                text: "Touches"
                                color: Color.muted
                                font.family: Style.fontFamily
                                font.pixelSize: Style.font.caption
                                font.bold: true
                            }

                            Repeater {
                                model: root.detailRows

                                delegate: Row {
                                    required property var modelData
                                    width: detailSheet.width
                                    spacing: Style.space(6)

                                    Text {
                                        width: detailSheet.width * 0.42
                                        text: modelData.key
                                        color: Color.menu.text
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        elide: Text.ElideMiddle
                                    }

                                    Text {
                                        width: detailSheet.width * 0.26
                                        text: modelData.old
                                        color: Color.muted
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        elide: Text.ElideMiddle
                                    }

                                    Text {
                                        width: detailSheet.width * 0.26
                                        text: modelData.neu
                                        color: Color.accent
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        elide: Text.ElideMiddle
                                    }
                                }
                            }
                        }

                        Row {
                            spacing: Style.spacing.controlGap

                            Rectangle {
                                width: tryText.implicitWidth + Style.space(24)
                                height: Style.spacing.controlHeight
                                radius: Style.cornerRadius
                                color: tryMa.containsMouse
                                    ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                    : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                Text {
                                    id: tryText
                                    anchors.centerIn: parent
                                    text: "Try (transient)"
                                    color: Color.menu.text
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.body
                                    font.bold: true
                                }

                                MouseArea {
                                    id: tryMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.applyLook(root.detailName, true)
                                }
                            }

                            Rectangle {
                                width: applyText.implicitWidth + Style.space(24)
                                height: Style.spacing.controlHeight
                                radius: Style.cornerRadius
                                color: Color.accent

                                Text {
                                    id: applyText
                                    anchors.centerIn: parent
                                    text: "Apply"
                                    color: Color.background
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.body
                                    font.bold: true
                                }

                                MouseArea {
                                    id: applyMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.applyLook(root.detailName, false)
                                }
                            }

                            Rectangle {
                                width: closeDetailText.implicitWidth + Style.space(24)
                                height: Style.spacing.controlHeight
                                radius: Style.cornerRadius
                                color: "transparent"

                                Text {
                                    id: closeDetailText
                                    anchors.centerIn: parent
                                    text: "Close detail"
                                    color: Color.muted
                                    font.family: Style.fontFamily
                                    font.pixelSize: Style.font.body
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.closeDetail()
                                }
                            }
                        }
                    }

                    // Empty / offline state.
                    Column {
                        width: parent.width
                        spacing: Style.space(6)
                        visible: root.daemonStatus !== "running"
                                 || (root.detailName === "" && root.filteredLooks.length === 0)

                        Text {
                            width: parent.width
                            text: root.daemonStatus !== "running"
                                ? "No omarchy10k daemon running"
                                : "No looks match your filters"
                            color: Color.menu.text
                            font.family: Style.fontFamily
                            font.pixelSize: Style.font.body
                            font.bold: true
                            horizontalAlignment: Text.AlignHCenter
                        }

                        Text {
                            width: parent.width
                            text: root.daemonStatus !== "running"
                                ? "Start a shell with the omarchy10k bash adapter enabled,\nor spawn the headless daemon from the Control Center."
                                : "Try a different search or category chip."
                            color: Color.muted
                            font.family: Style.fontFamily
                            font.pixelSize: Style.font.caption
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }

                    // Toast-style confirmation (Try / Apply feedback).
                    Rectangle {
                        width: toastText.implicitWidth + Style.space(24)
                        height: root._showToast ? toastText.implicitHeight + Style.space(12) : 0
                        radius: Style.cornerRadius
                        color: Color.accent
                        opacity: root._showToast ? 1 : 0
                        anchors.horizontalCenter: parent.horizontalCenter

                        Text {
                            id: toastText
                            anchors.centerIn: parent
                            text: root.toastMessage
                            color: Color.background
                            font.family: Style.fontFamily
                            font.pixelSize: Style.font.caption
                        }
                    }
                }
            }
        }
    }
}
