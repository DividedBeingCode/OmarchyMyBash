import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model
import "o10k/Store.js" as Store

// Omarchy10k service-kind plugin (manifest kind "service").
//
// One persistent connection hub for every discovered omarchy10k-*.sock.
// Mounted by the omarchy-shell host at startup (third-party services load
// when the plugin id is present in shell.json plugins[] — `omarchy plugin
// add`/`enable` does this). Survives panel open/close; BarWidget, Panel and
// SessionPicker consume its state instead of running their own pollers.
//
// Exposes:
//   - daemonStatus: "running" | "not running" (mirrors BarWidget semantics)
//   - sessions: [{ path, shellPid, pid, cwd, branch, dirty, lastCmdMs, ageSecs }]
//   - lastStatus: enriched `status` response of the primary session
//   - notifyThresholdMs: [notifications].threshold_ms from cached config (0 = off)
//   - eventReceived(var event): signal bus for daemon-side push events
//   - IpcHandler target "community.omarchy10k" (omarchy-shell call …)
//
// Every IPC method returns a string and never throws; failures come back as
// {"ok":false,"error":"…"}.

Item {
    id: service

    // Injected by the host service loader (feature-detected — declare so the
    // host's `if ("prop" in inst)` injection lands when supported).
    property var shell: null
    property string omarchyPath: Quickshell.env("OMARCHY_PATH")
    property var manifest: null

    // ── Public reactive state ──────────────────────────────────────────────
    property string daemonStatus: "not running"
    property var sessions: []
    property var lastStatus: ({})
    // Notification threshold in ms from the daemon config ([notifications]
    // with the deprecated [segments.notification] alias). 0 = notifications
    // off; 10000 (the daemon default) until the first config_get lands.
    readonly property int notifyThresholdMs: {
        var cfg = service._cfgFlat || {}
        if (cfg["notifications.enabled"] === false) return 0
        var t = cfg["notifications.threshold_ms"] !== undefined
            ? cfg["notifications.threshold_ms"]
            : cfg["segments.notification.threshold_ms"]
        var n = parseInt(t, 10)
        return n > 0 ? n : 10000
    }
    signal eventReceived(var event)

    // ── Internal ───────────────────────────────────────────────────────────
    readonly property string runtimeDir: Model.runtimeDir(Quickshell.env("XDG_RUNTIME_DIR"))
    property var socketPaths: []
    property int primaryIndex: 0
    property var _pending: ({})
    property int _reqSeq: 0
    property var _cfgFlat: ({})

    // ── Owned state (Increment 2) ──────────────────────────────────────────
    // The service is the single owner of preview caching, config delta
    // tracking and undo, so surfaces cannot drift apart or race each other's
    // saves. Logic lives in o10k/Store.js because Quickshell's Socket type
    // cannot load under qmltestrunner — see tests/store_test.js.
    property var _broker: Store.newBroker()
    property var _delta: Store.newDelta()
    property var _undo: Store.newUndo(10)

    // Plain property, not a binding: the stack is a plain JS object, so a
    // binding over it would never re-evaluate. The mutators below keep it
    // in sync.
    property int undoDepth: 0

    function previewLookup(ctx, patch) {
        return Store.brokerLookup(service._broker, Store.previewKey(ctx, patch))
    }

    function previewBegin(ctx, patch, id) {
        var key = Store.previewKey(ctx, patch)
        Store.brokerBegin(service._broker, key, id)
        return key
    }

    function previewResolve(key, id, value) {
        return Store.brokerResolve(service._broker, key, id, value)
    }

    // Call on disconnect and on any error response, so a stranded request
    // does not block that key forever.
    function previewRelease(key) {
        Store.brokerRelease(service._broker, key)
    }

    // Any config or theme change invalidates every cached render.
    function invalidateDerived() {
        Store.brokerInvalidate(service._broker)
    }

    function touchConfigKey(key) {
        Store.deltaTouch(service._delta, key)
    }

    function collectDelta(fullFlat) {
        return Store.deltaCollect(service._delta, fullFlat)
    }

    function pushUndo(flat) {
        Store.undoPush(service._undo, flat)
        service.undoDepth = Store.undoDepth(service._undo)
    }

    function popUndo() {
        var prev = Store.undoPop(service._undo)
        service.undoDepth = Store.undoDepth(service._undo)
        return prev
    }

    Component.onCompleted: discoverSockets()

    function _err(msg) {
        return JSON.stringify({ ok: false, error: msg })
    }

    function openGallery() {
        if (service.shell && typeof service.shell.summon === "function")
            service.shell.summon("community.omarchy10k", JSON.stringify({ page: "gallery" }))
    }

    function _nextId(prefix) {
        service._reqSeq++
        return prefix + "-" + service._reqSeq
    }

    // ── Discovery ──────────────────────────────────────────────────────────
    function discoverSockets() {
        // Only sockets whose owning shell PID is still alive; dead shells
        // leave socket files behind that would surface as ghost sessions.
        serviceSocketFinder.exec(["sh", "-c",
            "for f in '" + service.runtimeDir + "'/omarchy10k-*.sock; do " +
            "[[ -e \"$f\" ]] || continue; p=${f##*-}; p=${p%.sock}; " +
            "case \"$p\" in *[!0-9]*) ;; *) kill -0 \"$p\" 2>/dev/null || continue ;; esac; " +
            "timeout 1 socat -u OPEN:/dev/null UNIX-CONNECT:\"$f\" 2>/dev/null && echo \"$f\"; done"])
    }

    // ── Session bookkeeping ────────────────────────────────────────────────
    function _rebuildSessions() {
        var list = []
        for (var i = 0; i < service.socketPaths.length; i++) {
            var pidMatch = service.socketPaths[i].match(/omarchy10k-(\d+)\.sock$/)
            list.push({
                path: service.socketPaths[i],
                shellPid: pidMatch ? pidMatch[1] : "?",
                pid: "", cwd: "", branch: "", dirty: false,
                lastCmdMs: 0, ageSecs: 0
            })
        }
        service.sessions = list
        service.primaryIndex = 0
        if (list.length > 0) {
            controlSocket.path = list[0].path
            controlSocket.connected = true
        }
    }

    function _applySessionStatus(index, resp) {
        var list = service.sessions
        if (index < 0 || index >= list.length) return
        var updated = list.slice()
        var s = updated[index]
        var git = resp.git || {}
        s.pid = resp.pid !== undefined ? String(resp.pid) : s.pid
        s.cwd = resp.cwd || s.cwd
        s.branch = git.branch || s.branch
        s.dirty = git.dirty !== undefined ? !!git.dirty : s.dirty
        s.lastCmdMs = resp.last_cmd_duration_ms !== undefined ? resp.last_cmd_duration_ms : s.lastCmdMs
        s.ageSecs = resp.session_age_secs !== undefined ? resp.session_age_secs : s.ageSecs
        service.sessions = updated
        if (index === service.primaryIndex) {
            service.lastStatus = resp
            service.daemonStatus = "running"
        }
    }

    function _onSessionSocketError(index) {
        // Defer + match by path: this fires from inside the Instantiator's
        // delegate teardown; mutating socketPaths synchronously re-enters the
        // model binding and trips a binding loop.
        var path = service.socketPaths[index]
        if (!path) return
        Qt.callLater(service._removeSessionPath, path)
    }

    function _removeSessionPath(path) {
        var idx = service.socketPaths.indexOf(path)
        if (idx < 0) return
        var paths = service.socketPaths.slice()
        paths.splice(idx, 1)
        service.socketPaths = paths
        if (paths.length === 0) {
            service.sessions = []
            service.lastStatus = {}
            service.daemonStatus = "not running"
            controlSocket.connected = false
        } else {
            service._rebuildSessions()
        }
    }

    function _sessionWrite(index, msg) {
        var item = sessionSockets.objectAt(index)
        if (item && item.sock && item.sock.connected) {
            item.sock.write(msg)
            item.sock.flush()
        }
    }

    function _handleSessionMessage(index, raw) {
        var resp = Model.parseDaemonResponse(raw)
        if (resp.type === "hello") {
            _sessionWrite(index, Model.buildCommand("status", "session-" + index))
            return
        }
        if (resp.type === "control" && resp.status === "ok" && resp.pid !== undefined)
            _applySessionStatus(index, resp)
    }

    // ── Control socket (IPC config ops + primary status) ──────────────────
    function _handleControlMessage(raw) {
        var resp = Model.parseDaemonResponse(raw)
        if (resp.type === "hello") {
            controlSocket.write(Model.buildConfigGet("service-cfg"))
            controlSocket.flush()
            return
        }
        if (resp.type === "config" && resp.config) {
            service._cfgFlat = Model.flattenConfig(resp.config)
            return
        }
        if (resp.id !== undefined && service._pending[resp.id] !== undefined) {
            var cb = service._pending[resp.id]
            delete service._pending[resp.id]
            cb(resp)
            return
        }
        if (resp.type === "control" && resp.status === "ok" && resp.pid !== undefined)
            _applySessionStatus(service.primaryIndex, resp)
        if (resp.status === "bye")
            service.daemonStatus = "not running"
    }

    // ── IPC helpers (all return strings, never throw) ─────────────────────
    function _rpc(msgString, id, cb) {
        if (!controlSocket.connected) return false
        service._pending[id] = cb
        controlSocket.write(msgString)
        controlSocket.flush()
        return true
    }

    // Config-affecting IPC methods go through config_set (Model.buildConfigSet)
    // on the service's persistent control socket. The write is fire-and-queue:
    // the daemon's ok/error reply updates the cached config; failures surface
    // on the next status poll. Returns {"ok":true,"queued":true} when queued.
    function _configSet(patch) {
        try {
            if (!controlSocket.connected)
                return service._err("no omarchy10k daemon running")
            var id = service._nextId("ipc")
            var sent = controlSocket.write(Model.buildConfigSet(patch, id))
            controlSocket.flush()
            if (!sent) return service._err("daemon write failed")
            service._pending[id] = function (resp) {
                if (resp.status === "error")
                    service.eventReceived({ kind: "config_set_error", error: resp.error || "unknown" })
            }
            return JSON.stringify({ ok: true, queued: true })
        } catch (e) {
            return service._err("config_set failed: " + e)
        }
    }

    // ── IPC target: omarchy-shell call community.omarchy10k <method> ──────
    IpcHandler {
        target: "community.omarchy10k"

        function status(): string {
            try {
                var s = service.lastStatus || {}
                return JSON.stringify({
                    ok: service.daemonStatus === "running",
                    daemon: service.daemonStatus,
                    sessions: service.sessions.length,
                    pid: s.pid !== undefined ? s.pid : null,
                    version: s.version || null,
                    protocol_version: s.protocol_version || null,
                    cwd: s.cwd || null,
                    git: s.git || null,
                    agent: s.agent || null,
                    last_cmd_duration_ms: s.last_cmd_duration_ms || 0,
                    last_exit_code: s.last_exit_code !== undefined ? s.last_exit_code : null,
                    session_age_secs: s.session_age_secs || 0,
                    battery: s.battery || null
                })
            } catch (e) { return service._err("status failed: " + e) }
        }

        function sessions(): string {
            try {
                var out = []
                for (var i = 0; i < service.sessions.length; i++) {
                    var s = service.sessions[i]
                    out.push({
                        shell_pid: s.shellPid,
                        pid: s.pid || null,
                        cwd: s.cwd || null,
                        branch: s.branch || null,
                        dirty: !!s.dirty,
                        last_cmd_duration_ms: s.lastCmdMs || 0,
                        session_age_secs: s.ageSecs || 0
                    })
                }
                return JSON.stringify(out)
            } catch (e) { return service._err("sessions failed: " + e) }
        }

        function setLayout(preset: string): string {
            return service._configSet({ "style": { "preset": String(preset || "") } })
        }

        function toggleTransient(): string {
            try {
                var cur = service._cfgFlat["prompt.transient"]
                if (cur === undefined)
                    return service._err("config not loaded yet; try again in a moment")
                return service._configSet({ "prompt": { "transient": !cur } })
            } catch (e) { return service._err("toggleTransient failed: " + e) }
        }

        function gallery(): string {
            try {
                if (service.shell && typeof service.shell.summon === "function")
                    return service.shell.summon("community.omarchy10k", JSON.stringify({ page: "gallery" }))
                        ? "ok"
                        : service._err("host refused summon (plugin enabled?)")
                return service._err("shell host unavailable")
            } catch (e) { return service._err("gallery failed: " + e) }
        }

        function picker(): string {
            try {
                if (service.shell && typeof service.shell.summon === "function")
                    return service.shell.summon("community.omarchy10k", "{}")
                        ? "ok"
                        : service._err("host refused summon (plugin enabled?)")
                return service._err("shell host unavailable")
            } catch (e) { return service._err("picker failed: " + e) }
        }

        function invalidateGit(): string {
            try {
                if (!controlSocket.connected)
                    return service._err("no omarchy10k daemon running")
                var id = service._nextId("ipc")
                var sent = service._rpc(Model.buildCommand("invalidate_git", id), id,
                                        function (resp) { /* ok / error both fine */ })
                return sent ? JSON.stringify({ ok: true, queued: true })
                            : service._err("daemon write failed")
            } catch (e) { return service._err("invalidateGit failed: " + e) }
        }
    }

    // ── I/O components ─────────────────────────────────────────────────────
    Process {
        id: serviceSocketFinder
        stdout: StdioCollector {
            onStreamFinished: {
                var text = this.text.trim()
                var list = []
                if (text.length > 0) {
                    var lines = text.split("\n")
                    for (var i = 0; i < lines.length; i++) {
                        var p = lines[i].trim()
                        if (p.length > 0) list.push(p)
                    }
                }
                if (list.join("\n") === service.socketPaths.join("\n")) return
                service.socketPaths = list
                if (list.length === 0) {
                    service.sessions = []
                    service.lastStatus = {}
                    service.daemonStatus = "not running"
                    controlSocket.connected = false
                } else {
                    service._rebuildSessions()
                }
            }
        }
    }

    // One persistent Socket per discovered session. Instantiator rebuilds on
    // socketPaths change; discovery only reassigns when the set differs.
    Instantiator {
        id: sessionSockets
        model: service.socketPaths

        delegate: QtObject {
            required property var modelData
            required property int index

            property Socket sock: Socket {
                path: modelData
                connected: true
                parser: SplitParser {
                    onRead: message => service._handleSessionMessage(index, message)
                }
                onConnectedChanged: {
                    if (connected) {
                        write(Model.buildHello("service-session"))
                        flush()
                    }
                }
                onError: service._onSessionSocketError(index)
            }
        }
    }

    Socket {
        id: controlSocket
        parser: SplitParser {
            onRead: message => service._handleControlMessage(message)
        }
        onConnectedChanged: {
            if (connected) {
                write(Model.buildHello("service-ipc"))
                flush()
            }
        }
        onError: {
            connected = false
            service.daemonStatus = "not running"
        }
    }

    Timer {
        id: discoverTimer
        interval: 10000
        repeat: true
        running: true
        onTriggered: service.discoverSockets()
    }
}
