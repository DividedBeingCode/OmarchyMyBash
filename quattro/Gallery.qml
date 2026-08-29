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
// (the IpcHandler registered below) or the plugin's own shell.summon flow.
// Browse the daemon's
// Looks (control verb `looks`), preview each one as a REAL dry-run render
// (preview message with the `look` override, protocol v0.4+), then Try it
// transiently (`looks_apply {transient:true}` — reverted by reload_config)
// or Apply it persistently (`looks_apply`).
//
// The detail sheet doubles as an editor: its patch rows are editable, every
// edit re-requests the preview with { look, patch } (protocol 0.5, additive),
// and the result can be saved over a user Look or as a new one
// (config_set [looks.<name>] table) or deleted (looks_delete).
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

    // Editor rows: static field catalog over the patch. Hex rows take text
    // input; cycle rows step through daemon glyph/style catalogs (values are
    // the same keys the daemon's Look patches carry — resolved at render).
    readonly property var editorFields: [
        { label: "accent", kind: "hex", key: "theme.custom.accent" },
        { label: "foreground", kind: "hex", key: "theme.custom.foreground" },
        { label: "muted", kind: "hex", key: "theme.custom.muted" },
        { label: "background", kind: "hex", key: "theme.custom.background" },
        { label: "style preset", kind: "cycle", key: "style.preset", catalog: root.stylePresets },
        { label: "separators", kind: "cycle", key: "style.separators.shape", catalog: root.separatorShapes },
        { label: "frame enabled", kind: "cycle", key: "style.frame.enabled", catalog: [true, false] },
        { label: "frame gap char", kind: "cycle", key: "style.frame.gap_char", catalog: [" ", "·", "─"] },
        { label: "character", kind: "cycle", key: "segments.character.success", catalog: root.promptChars },
        { label: "os icon", kind: "cycle", key: "segments.os.icon", catalog: root.osIcons }
    ]

    readonly property int previewCols: 64
    readonly property int eagerPreviews: 9

    // ── Editor state (detail sheet) ────────────────────────────────────────
    // Working patch: config_set-shaped, edited rows write into it and every
    // change re-requests the preview with { look, patch } (protocol 0.5).
    property var _editPatch: null
    property string editPreviewText: ""
    property int _editSeq: 0
    property bool _deleteArmed: false

    // Catalogs mirrored from the daemon (crates/omarchy10kd/src/style.rs
    // available_presets/available_separators/available_prompt_chars/
    // available_os_icons). Values are the same keys the daemon's own Look
    // patches carry, so the merged config renders them identically.
    readonly property var stylePresets: ["omarchy", "powerline", "rainbow", "gradient",
        "framed", "classic", "lean", "dense", "slanted", "minimal", "pure"]
    readonly property var separatorShapes: ["none", "powerline", "powerline_thin",
        "slanted", "round", "vertical", "dot", "diamond", "fade", "fade_rev",
        "trapezoid", "trapezoid_rev", "flame", "dither"]
    readonly property var promptChars: ["chevron", "arrow", "lambda", "dollar",
        "angle", "percent", "triangle", "hash"]
    readonly property var osIcons: ["arch", "ubuntu", "debian", "fedora", "nixos",
        "macos", "windows", "linux", "omarchy", "alpine", "void", "gentoo",
        "manjaro", "opensuse", "centos", "raspberry_pi", "none"]

    // Current value of a patch leaf for editor display. Old values come from
    // the daemon's flattened live config; "keep" marks the patched value as
    // untouched so the row only enters the working patch when edited.
    function _editValue(key) {
        var flat = Model.flattenConfig(root._editPatch || {})
        if (flat[key] !== undefined) return String(flat[key])
        if (root._cfgFlat[key] !== undefined) return String(root._cfgFlat[key])
        return ""
    }

    // Write one dotted key into the working patch (flatten → set → rebuild
    // keeps the nested config_set shape canonical) and re-request the
    // preview with { look, patch } so the card render updates live.
    function _setPatchKey(key, value) {
        var patch = root._editPatch ? Object.assign({}, root._editPatch) : {}
        var flat = Model.flattenConfig(patch)
        flat[key] = value
        root._editPatch = Model.unflattenPatch(flat)
        root.requestEditPreview()
    }

    function _cyclePatchKey(key, catalog, reverse) {
        var cur = root._editValue(key)
        var idx = -1
        for (var i = 0; i < catalog.length; i++) {
            if (String(catalog[i]) === cur) { idx = i; break }
        }
        if (idx < 0)
            idx = 0
        else
            idx = (idx + (reverse ? catalog.length - 1 : 1)) % catalog.length
        root._setPatchKey(key, catalog[idx])
    }

    // Curated Looks are compiled into the daemon; user Looks live as
    // [looks.<name>] tables in config.toml, visible in the flattened config.
    function _isUserLook(name) {
        return root._cfgFlat["looks." + name + ".label"] !== undefined
    }

    function _validHex(s) {
        return /^#[0-9a-fA-F]{6}$/.test(String(s)) || /^#[0-9a-fA-F]{3}$/.test(String(s))
    }

    function _swatchFor(key) {
        var v = String(root._editValue(key))
        return root._validHex(v) ? v.toLowerCase() : "transparent"
    }

    function _commitHex(key, text) {
        var t = String(text).trim()
        if (!root._validHex(t)) {
            root._toast("Invalid hex \u201C" + t + "\u201D — use #rrggbb")
            return
        }
        root._setPatchKey(key, t.toLowerCase())
    }

    // 8-step client-side lerp between two hex colors (ramp preview strip).
    function _lerpHex(a, b, t) {
        var ca = root._hexToRgb(a)
        var cb = root._hexToRgb(b)
        if (ca === null || cb === null) return "transparent"
        var ch = function (x, y) { return Math.round(x + (y - x) * t) }
        var hx = function (v) {
            var s = Math.max(0, Math.min(255, v)).toString(16)
            return s.length < 2 ? "0" + s : s
        }
        return "#" + hx(ch(ca[0], cb[0])) + hx(ch(ca[1], cb[1])) + hx(ch(ca[2], cb[2]))
    }

    function _hexToRgb(hex) {
        var h = String(hex).replace("#", "")
        if (h.length === 3)
            h = h.charAt(0) + h.charAt(0) + h.charAt(1) + h.charAt(1) + h.charAt(2) + h.charAt(2)
        var n = parseInt(h, 16)
        if (isNaN(n)) return null
        return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
    }

    // Apply the ramp endpoints onto the working patch. Full multi-segment
    // ramps (and the frame gap_gradient fill) are daemon-side; the studio
    // maps start→accent, end→magenta so a two-pick gradient previews with
    // the shipped lerp engine.
    function _applyRamp() {
        var s = String(rampStartField.text).trim().toLowerCase()
        var e = String(rampEndField.text).trim().toLowerCase()
        if (!root._validHex(s) || !root._validHex(e)) {
            root._toast("Ramp colors need #rrggbb")
            return
        }
        root._setPatchKey("theme.custom.accent", s)
        root._setPatchKey("theme.custom.magenta", e)
    }

    // Preview request with the working patch applied. The daemon merges
    // base → look → patch (patch wins, protocol 0.5). An error response
    // keeps the last good preview; the handler toasts instead.
    function requestEditPreview() {
        if (!gallerySocket.connected || !root._editPatch) return
        root._editSeq++
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
            look: root.detailName,
            patch: root._editPatch,
            id: "edit-" + root._editSeq
        }
        gallerySocket.write(Model.buildPreview(ctx, null))
        gallerySocket.flush()
    }

    // ── Save / delete flows ────────────────────────────────────────────────
    // Saving an EDITED patch must go through config_set with the full
    // [looks.<name>] table — looks_save captures the daemon's current state
    // and cannot express the working patch. config_set's merge path accepts
    // the looks table (same shape looks_save itself writes).
    function _savePatch(name, label) {
        if (!gallerySocket.connected) {
            root._toast("No omarchy10k daemon running")
            return
        }
        if (String(name).length === 0) {
            root._toast("Enter a name first")
            return
        }
        var entry = { label: label || name, palette: "keep", patch: root._editPatch }
        var patch = {}
        patch[name] = entry
        var id = _nextId("save")
        root._pendingSave[id] = name
        gallerySocket.write(Model.buildConfigSet({ looks: patch }, id))
        gallerySocket.flush()
    }

    property var _pendingSave: ({})

    function _onSaveResponse(id, resp) {
        var name = root._pendingSave[id]
        if (name === undefined) return
        delete root._pendingSave[id]
        if (resp.status === "error") {
            root._toast("Save failed: " + (resp.error || "unknown error"))
            return
        }
        root._toast("Saved \u201C" + name + "\u201D")
        root.requestLooks()
    }

    function _deleteLook(name) {
        if (!gallerySocket.connected) {
            root._toast("No omarchy10k daemon running")
            return
        }
        var id = _nextId("del")
        var msg = { type: "control", command: "looks_delete", name: name, id: id }
        gallerySocket.write(JSON.stringify(msg) + "\n")
        gallerySocket.flush()
        root._pendingDelete[id] = name
    }

    property var _pendingDelete: ({})

    function _onDeleteResponse(id, resp) {
        var name = root._pendingDelete[id]
        if (name === undefined) return
        delete root._pendingDelete[id]
        root._deleteArmed = false
        if (resp.status === "error") {
            root._toast("Delete failed: " + (resp.error || "unknown error"))
            return
        }
        root._toast("Deleted \u201C" + name + "\u201D")
        root.closeDetail()
        root.requestLooks()
    }

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

    function openDetail(name) {
        var look = root._look(name)
        if (!look) return
        root.detailName = name
        root.requestPreview(name)
        if (Object.keys(root._cfgFlat).length === 0) root.requestConfig()
        root.detailRows = root._patchSummary(look.patch || {})
        // Editor starts from the Look's own patch; nothing is written into
        // the working patch until a row is edited.
        root._editPatch = Model.unflattenPatch(Model.flattenConfig(look.patch || {}))
        root.editPreviewText = ""
        root._deleteArmed = false
    }

    function closeDetail() {
        root.detailName = ""
        root.detailRows = []
        root._editPatch = null
        root.editPreviewText = ""
        root._deleteArmed = false
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

        // Live-edited preview (working patch). On error keep the last good
        // render so a bad hex or unrepresentable patch never blanks the card.
        if (resp.type === "preview" && resp.id && resp.id.indexOf("edit-") === 0) {
            if (resp.status === "error") {
                root._toast("Preview failed: " + (resp.error || "unrepresentable patch"))
            } else if (resp.left) {
                root.editPreviewText = Model.ansiToRich(resp.left)
            }
            return
        }

        if (resp.id !== undefined && root._pendingApply[resp.id] !== undefined) {
            root._completeApply(resp.id, resp)
            return
        }

        if (resp.id !== undefined && root._pendingSave[resp.id] !== undefined) {
            root._onSaveResponse(resp.id, resp)
            return
        }

        if (resp.id !== undefined && root._pendingDelete[resp.id] !== undefined) {
            root._onDeleteResponse(resp.id, resp)
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

                        // Everything below the header scrolls: the editor,
                        // ramp designer and save row made the sheet taller
                        // than the canvas.
                        Flickable {
                            id: detailScroll
                            width: parent.width
                            // Qualified: headerHeightD is declared on
                            // detailSheet, and the scope chain does not
                            // reach it from inside this Flickable — the
                            // unqualified form threw ReferenceError at
                            // runtime and collapsed the detail sheet.
                            height: parent.height - detailSheet.headerHeightD - Style.space(12)
                            contentHeight: detailContent.height
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds

                            Column {
                                id: detailContent
                                width: detailScroll.width
                                spacing: Style.space(12)

                                // Large real render of the Look's prompt —
                                // switches to the live-edited render while the
                                // editor has touched the working patch.
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
                                            if (root.editPreviewText !== "") return root.editPreviewText
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

                                // ── Editor (A1): one row per patch field.
                                // Hex rows edit palette roles with a swatch;
                                // cycle rows step daemon style/glyph catalogs.
                                // Every edit writes the working patch and
                                // re-requests the preview with { look, patch }.
                                Column {
                                    width: parent.width
                                    spacing: Style.space(4)

                                    Text {
                                        text: "Editor"
                                        color: Color.muted
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        font.bold: true
                                    }

                                    Repeater {
                                        model: root.editorFields

                                        delegate: Row {
                                            id: editorRow
                                            required property var modelData
                                            width: detailContent.width
                                            height: Style.spacing.controlHeight
                                            spacing: Style.space(6)

                                            Text {
                                                width: detailContent.width * 0.28
                                                text: editorRow.modelData.label
                                                color: Color.menu.text
                                                font.family: Style.fontFamily
                                                font.pixelSize: Style.font.caption
                                                elide: Text.ElideRight
                                                anchors.verticalCenter: parent.verticalCenter
                                            }

                                            // Palette roles: swatch + hex input.
                                            Row {
                                                visible: editorRow.modelData.kind === "hex"
                                                spacing: Style.space(6)
                                                anchors.verticalCenter: parent.verticalCenter

                                                Rectangle {
                                                    width: Style.space(16)
                                                    height: Style.space(16)
                                                    radius: Style.cornerRadius
                                                    color: root._swatchFor(editorRow.modelData.key)
                                                    border.width: 1
                                                    border.color: Color.menu.border
                                                    anchors.verticalCenter: parent.verticalCenter
                                                }

                                                TextField {
                                                    id: hexField
                                                    width: Style.space(96)
                                                    height: Style.spacing.controlHeight - Style.space(8)
                                                    text: root._editValue(editorRow.modelData.key)
                                                    placeholderText: "#rrggbb"
                                                    placeholderTextColor: Color.muted
                                                    color: Color.menu.text
                                                    font.family: Style.fontFamily
                                                    font.pixelSize: Style.font.caption
                                                    onEditingFinished: root._commitHex(editorRow.modelData.key, hexField.text)
                                                    background: Rectangle {
                                                        radius: Style.cornerRadius
                                                        color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)
                                                        border.width: hexField.activeFocus ? 1 : 0
                                                        border.color: Color.accent
                                                    }
                                                }
                                            }

                                            // Catalog fields: ‹ value › cycle.
                                            Row {
                                                visible: editorRow.modelData.kind === "cycle"
                                                spacing: Style.space(4)
                                                anchors.verticalCenter: parent.verticalCenter

                                                Rectangle {
                                                    width: cyclePrevText.implicitWidth + Style.space(12)
                                                    height: Style.spacing.controlHeight - Style.space(8)
                                                    radius: Style.cornerRadius
                                                    color: cyclePrevMa.containsMouse
                                                        ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                                        : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                                    Text {
                                                        id: cyclePrevText
                                                        anchors.centerIn: parent
                                                        text: "‹"
                                                        color: Color.menu.text
                                                        font.family: Style.fontFamily
                                                        font.pixelSize: Style.font.caption
                                                    }

                                                    MouseArea {
                                                        id: cyclePrevMa
                                                        anchors.fill: parent
                                                        hoverEnabled: true
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: root._cyclePatchKey(editorRow.modelData.key, editorRow.modelData.catalog, true)
                                                    }
                                                }

                                                Rectangle {
                                                    width: cycleValText.implicitWidth + Style.space(16)
                                                    height: Style.spacing.controlHeight - Style.space(8)
                                                    radius: Style.cornerRadius
                                                    color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                                    Text {
                                                        id: cycleValText
                                                        anchors.centerIn: parent
                                                        text: {
                                                            var v = root._editValue(editorRow.modelData.key)
                                                            if (editorRow.modelData.key === "style.frame.gap_char" && v === " ") return "space"
                                                            return v === "" ? "—" : v
                                                        }
                                                        color: Color.accent
                                                        font.family: Style.fontFamily
                                                        font.pixelSize: Style.font.caption
                                                        font.bold: true
                                                    }

                                                    MouseArea {
                                                        anchors.fill: parent
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: root._cyclePatchKey(editorRow.modelData.key, editorRow.modelData.catalog, false)
                                                    }
                                                }

                                                Rectangle {
                                                    width: cycleNextText.implicitWidth + Style.space(12)
                                                    height: Style.spacing.controlHeight - Style.space(8)
                                                    radius: Style.cornerRadius
                                                    color: cycleNextMa.containsMouse
                                                        ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                                        : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                                    Text {
                                                        id: cycleNextText
                                                        anchors.centerIn: parent
                                                        text: "›"
                                                        color: Color.menu.text
                                                        font.family: Style.fontFamily
                                                        font.pixelSize: Style.font.caption
                                                    }

                                                    MouseArea {
                                                        id: cycleNextMa
                                                        anchors.fill: parent
                                                        hoverEnabled: true
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: root._cyclePatchKey(editorRow.modelData.key, editorRow.modelData.catalog, false)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // ── Gradient ramp designer (A2): two picks,
                                // client-side 8-step lerp strip, apply maps the
                                // endpoints onto theme.custom accent + magenta.
                                Column {
                                    id: rampSection
                                    width: parent.width
                                    spacing: Style.space(6)

                                    readonly property var rampColors: {
                                        var out = []
                                        for (var i = 0; i < 8; i++)
                                            out.push(root._lerpHex(String(rampStartField.text).trim(), String(rampEndField.text).trim(), i / 7))
                                        return out
                                    }

                                    Text {
                                        text: "Gradient ramp"
                                        color: Color.muted
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        font.bold: true
                                    }

                                    Row {
                                        spacing: Style.space(8)

                                        TextField {
                                            id: rampStartField
                                            width: Style.space(96)
                                            height: Style.spacing.controlHeight - Style.space(8)
                                            text: "#7aa2f7"
                                            placeholderText: "#rrggbb"
                                            placeholderTextColor: Color.muted
                                            color: Color.menu.text
                                            font.family: Style.fontFamily
                                            font.pixelSize: Style.font.caption
                                            background: Rectangle {
                                                radius: Style.cornerRadius
                                                color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)
                                                border.width: rampStartField.activeFocus ? 1 : 0
                                                border.color: Color.accent
                                            }
                                        }

                                        TextField {
                                            id: rampEndField
                                            width: Style.space(96)
                                            height: Style.spacing.controlHeight - Style.space(8)
                                            text: "#bb9af7"
                                            placeholderText: "#rrggbb"
                                            placeholderTextColor: Color.muted
                                            color: Color.menu.text
                                            font.family: Style.fontFamily
                                            font.pixelSize: Style.font.caption
                                            background: Rectangle {
                                                radius: Style.cornerRadius
                                                color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)
                                                border.width: rampEndField.activeFocus ? 1 : 0
                                                border.color: Color.accent
                                            }
                                        }

                                        Row {
                                            spacing: 1
                                            anchors.verticalCenter: parent.verticalCenter

                                            Repeater {
                                                model: rampSection.rampColors

                                                delegate: Rectangle {
                                                    required property var modelData
                                                    width: Style.space(14)
                                                    height: Style.space(18)
                                                    color: modelData
                                                }
                                            }
                                        }

                                        Rectangle {
                                            width: rampApplyText.implicitWidth + Style.space(24)
                                            height: Style.spacing.controlHeight - Style.space(8)
                                            radius: Style.cornerRadius
                                            anchors.verticalCenter: parent.verticalCenter
                                            color: rampApplyMa.containsMouse
                                                ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                                : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                            Text {
                                                id: rampApplyText
                                                anchors.centerIn: parent
                                                text: "Apply ramp"
                                                color: Color.menu.text
                                                font.family: Style.fontFamily
                                                font.pixelSize: Style.font.caption
                                                font.bold: true
                                            }

                                            MouseArea {
                                                id: rampApplyMa
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: root._applyRamp()
                                            }
                                        }
                                    }
                                }

                                // ── Save / Overwrite / Delete. Overwrite and
                                // Delete exist only for user Looks ([looks.<name>]
                                // tables in config.toml); curated Looks force
                                // Save-as-new. Overwrite sends the working patch
                                // via config_set looks.<name>; Delete is two-tap.
                                Row {
                                    spacing: Style.spacing.controlGap

                                    TextField {
                                        id: saveNameField
                                        width: Style.space(160)
                                        height: Style.spacing.controlHeight
                                        placeholderText: "new-look-name"
                                        placeholderTextColor: Color.muted
                                        color: Color.menu.text
                                        font.family: Style.fontFamily
                                        font.pixelSize: Style.font.caption
                                        background: Rectangle {
                                            radius: Style.cornerRadius
                                            color: Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)
                                            border.width: saveNameField.activeFocus ? 1 : 0
                                            border.color: Color.accent
                                        }
                                    }

                                    Rectangle {
                                        width: saveNewText.implicitWidth + Style.space(24)
                                        height: Style.spacing.controlHeight
                                        radius: Style.cornerRadius
                                        color: saveNewMa.containsMouse
                                            ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                            : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                        Text {
                                            id: saveNewText
                                            anchors.centerIn: parent
                                            text: "Save as new Look"
                                            color: Color.menu.text
                                            font.family: Style.fontFamily
                                            font.pixelSize: Style.font.body
                                            font.bold: true
                                        }

                                        MouseArea {
                                            id: saveNewMa
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                var n = saveNameField.text.trim()
                                                root._savePatch(n, n)
                                            }
                                        }
                                    }

                                    Rectangle {
                                        visible: root._isUserLook(root.detailName)
                                        width: overwriteText.implicitWidth + Style.space(24)
                                        height: Style.spacing.controlHeight
                                        radius: Style.cornerRadius
                                        color: overwriteMa.containsMouse
                                            ? Style.hoverFillFor(Color.foreground, Color.accent, Color.urgent)
                                            : Style.normalFillFor(Color.foreground, Color.accent, Color.urgent)

                                        Text {
                                            id: overwriteText
                                            anchors.centerIn: parent
                                            text: "Overwrite"
                                            color: Color.menu.text
                                            font.family: Style.fontFamily
                                            font.pixelSize: Style.font.body
                                            font.bold: true
                                        }

                                        MouseArea {
                                            id: overwriteMa
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                var l = root._look(root.detailName)
                                                root._savePatch(root.detailName, l ? (l.label || l.name) : root.detailName)
                                            }
                                        }
                                    }

                                    Rectangle {
                                        visible: root._isUserLook(root.detailName)
                                        width: deleteText.implicitWidth + Style.space(24)
                                        height: Style.spacing.controlHeight
                                        radius: Style.cornerRadius
                                        color: root._deleteArmed ? Color.urgent : "transparent"

                                        Text {
                                            id: deleteText
                                            anchors.centerIn: parent
                                            text: root._deleteArmed ? "Really delete?" : "Delete"
                                            color: root._deleteArmed ? Color.background : Color.muted
                                            font.family: Style.fontFamily
                                            font.pixelSize: Style.font.body
                                            font.bold: root._deleteArmed
                                        }

                                        MouseArea {
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                if (!root._deleteArmed) {
                                                    root._deleteArmed = true
                                                    return
                                                }
                                                root._deleteLook(root.detailName)
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
