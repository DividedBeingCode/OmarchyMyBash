import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model
import "o10k/Preview.js" as Preview
import "o10k/Motion.js" as Motion
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

    // ── Is the binary even installed? ──────────────────────────────────────
    //
    // `omarchy plugin add` installs the QML and nothing else -- it never
    // builds or installs anything, by design. So this plugin can be present
    // and complete while the omarchy10k binary it is a control surface FOR is
    // absent entirely. Without saying so, the surfaces just look broken: no
    // presets, no preview, one small "no daemon" in a corner.
    //
    // `daemonStatus` cannot answer this: no daemon runs until a shell starts
    // one, so "not running" is the normal state on a fresh login even when
    // everything is installed correctly. The binary's presence is a separate
    // question and needs a separate probe.
    property bool binaryInstalled: true    // assume yes until the probe says otherwise
    property bool binaryProbed: false

    Process {
        id: binaryProbe
        running: true
        command: ["sh", "-c", "command -v omarchy10k >/dev/null 2>&1 && echo yes || echo no"]
        stdout: StdioCollector {
            onStreamFinished: {
                service.binaryInstalled = String(this.text).trim() === "yes"
                service.binaryProbed = true
            }
        }
    }

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
        // Cached renders are now stale in exactly the same way: a config or
        // theme change means every preview was rendered against the old
        // effective config.
        if (service._previewCache) Store.brokerInvalidate(service._previewCache)
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

    // ── Derived daemon state (Increment 3) ─────────────────────────────────
    // Fetched once per connection and refreshed when the config changes.
    // Surfaces bind to these instead of each running their own queries —
    // the drift between Panel.qml's hardcoded Look list and Gallery.qml's
    // real `looks` verb is exactly what one owner prevents.
    property var looks: []
    property var palettes: ({})
    /// The same palettes in daemon order (curated first, then derived
    /// alphabetically). A picker built from Object.keys() reshuffles between
    /// opens, which is disorienting when there are thirty of them.
    property var paletteList: []
    property var defaultsFlat: ({})
    property var scripts: []
    // Active Omarchy theme, so a pinned surface can name what it diverges
    // from. Read from the omarchy CLI, not a theme file — Omarchy owns that
    // state and both sibling projects treat it read-only.
    property string desktopTheme: ""

    // ── Terminal font ──────────────────────────────────────────────────────
    //
    // Every preview surface claims to render "in the terminal's font, so a
    // glyph that is tofu there looks like tofu here". None of them did: the
    // property existed on TerminalPreview, PresetCard and GlyphBrowser, but
    // nothing ever set it, so all three silently fell back to the panel font
    // and the tofu promise was worthless.
    //
    // Read from the terminal's own config rather than guessed. Ghostty uses
    // `font-family = "..."`, foot uses `font=name:size`.
    /// Empty until the probe lands. Deliberately NOT defaulted to
    /// Style.font.family: this is a non-visual service with no qs.Commons
    /// import, and reaching for a UI singleton here threw
    /// "ReferenceError: Style is not defined" on every startup. Consumers
    /// fall back themselves.
    property string terminalFont: ""

    Process {
        id: fontProbe
        running: true
        command: ["sh", "-c",
            "sed -n 's/^ *font-family *= *//p' ~/.config/ghostty/config 2>/dev/null | head -1;" +
            "sed -n 's/^ *font *= *//p' ~/.config/foot/foot.ini 2>/dev/null | head -1"]
        stdout: StdioCollector {
            onStreamFinished: {
                var lines = String(this.text).split("\n")
                for (var i = 0; i < lines.length; i++) {
                    var v = lines[i].trim()
                    if (v.length === 0) continue
                    // Strip quotes, then foot's ":size=11" suffix and any
                    // fallback list after the first comma.
                    v = v.replace(/^["']|["']$/g, "")
                    v = v.split(",")[0].split(":")[0].trim()
                    if (v.length > 0) { service.terminalFont = v; return }
                }
            }
        }
    }

    function fetchLooks() {
        service._rpc(Model.buildCommand("looks", "svc-looks"), "svc-looks",
                     function (resp) {
                         if (resp.looks !== undefined) service.looks = resp.looks
                     })
    }

    function fetchPalettes() {
        service._rpc(Model.buildCommand("palettes", "svc-palettes"), "svc-palettes",
                     function (resp) {
                         if (resp.palettes === undefined) return
                         // The verb returns a list; consumers index by key, so
                         // flatten to {key: {...}} while keeping the ordered
                         // list for pickers that must not reshuffle.
                         var out = {}
                         var ordered = []
                         for (var i = 0; i < resp.palettes.length; i++) {
                             var entry = resp.palettes[i]
                             if (!entry || !entry.key) continue
                             var custom = (entry.theme && entry.theme.custom) ? entry.theme.custom : {}
                             // `colors` is sent flat precisely so a swatch
                             // strip does not have to dig it out of the patch;
                             // fall back for an older daemon.
                             var colors = entry.colors ? entry.colors : custom
                             var rec = {
                                 key: entry.key,
                                 label: entry.label || entry.key,
                                 blurb: entry.blurb || "",
                                 source: entry.source || "curated",
                                 lowContrast: !!entry.low_contrast,
                                 accent: colors.accent || custom.accent || "",
                                 colors: colors,
                                 // Sampled daemon-side by the same ramp_color
                                 // the prompt renders with. Empty from an
                                 // older daemon, which the Ramp component
                                 // treats as "nothing to draw".
                                 ramp: entry.ramp || [],
                                 custom: custom
                             }
                             out[entry.key] = rec
                             ordered.push(rec)
                         }
                         service.palettes = out
                         service.paletteList = ordered
                     })
    }

    // ── Live preview (beautification pass) ─────────────────────────────────
    //
    // One owner for preview fetching, so every surface shares the broker's
    // cache: browsing eighteen Looks costs at most eighteen renders, then
    // zero. Hover is debounced through Preview.js so crossing a grid issues
    // one request rather than one per card.

    /// Render `look`/`patch` against `scenes` and hand the callback
    /// `{ state, renders, error }`. `state` is "ok" | "error" | "empty".
    ///
    /// Cached: an identical (look, patch, scenes) triple resolves from the
    /// broker without touching the socket. `immediate` skips the hover
    /// debounce, which is what a click wants.
    function requestPreview(look, patch, scenes, immediate, cb, cols, base) {
        // The pane knows how wide it is; the service does not. A render is
        // laid out to a column count, so the width has to reach both the
        // daemon and the cache key.
        var useCols = (cols && cols > 0) ? cols : service.previewCols
        var key = Preview.cacheKey(look, patch, scenes, useCols, base)

        // Fast path: already rendered. No socket, no timer, no frame cost.
        var probe = Store.brokerLookup(service._previewCache, key)
        if (probe.hit) {
            cb(probe.value)
            return
        }
        // Already in flight — join the waiters rather than issue a duplicate.
        if (!probe.shouldSend) {
            service._addPreviewWaiter(key, cb)
            return
        }

        function issue() {
            // Re-probe: the debounce means the world may have moved since the
            // request was queued.
            var now = Store.brokerLookup(service._previewCache, key)
            if (now.hit) { cb(now.value); return }
            if (!now.shouldSend) { service._addPreviewWaiter(key, cb); return }

            if (!controlSocket.connected) {
                cb({ state: "empty", renders: [], error: "" })
                return
            }

            var id = service._nextId("prev")
            Store.brokerBegin(service._previewCache, key, id)
            service._addPreviewWaiter(key, cb)

            service._pending[id] = function (resp) {
                var result
                if (resp.status === "error") {
                    result = { state: "error", renders: [],
                               error: resp.error || "render failed" }
                    // Released, not cached: a stranded key would block this
                    // preset forever, and the next hover should retry.
                    Store.brokerRelease(service._previewCache, key)
                } else {
                    // A daemon without `scenes` support answers with the flat
                    // left/right pair; treat that as a single unlabelled row.
                    var renders = resp.renders
                        ? resp.renders
                        : [{ label: "", left: resp.left, right: resp.right }]
                    result = { state: "ok", renders: renders, error: "" }
                    Store.brokerResolve(service._previewCache, key, id, result)
                }
                service._flushPreviewWaiters(key, result)
            }

            controlSocket.write(Preview.buildRequest(
                { cwd: service.previewCwd, cols: useCols, base: base },
                patch, look, scenes, id))
            controlSocket.flush()
        }

        if (immediate) {
            // A click must not wait out the hover delay.
            service._previewDebounce.clear()
            issue()
        } else {
            service._previewDebounce.request(issue)
        }
    }

    function _addPreviewWaiter(key, cb) {
        if (!service._previewWaiters[key]) service._previewWaiters[key] = []
        service._previewWaiters[key].push(cb)
    }

    function _flushPreviewWaiters(key, result) {
        var waiting = service._previewWaiters[key] || []
        delete service._previewWaiters[key]
        for (var i = 0; i < waiting.length; i++) waiting[i](result)
    }

    /// Drop a queued hover — for a pointer leaving the grid.
    function cancelPreview() {
        service._previewDebounce.clear()
    }

    /// The palette a preview will actually render in, for a preset that does
    /// not bring its own. Falls back to the pinned custom palette, then to
    /// whatever the daemon reports.
    function currentPaletteColors() {
        var pinned = service.cfgFlat ? service.cfgFlat["theme.custom.accent"] : ""
        if (pinned) {
            for (var k in service.palettes) {
                var p = service.palettes[k]
                if (p.accent && String(p.accent).toLowerCase()
                        === String(pinned).toLowerCase())
                    return p.colors
            }
        }
        var desktop = service.palettes[service.desktopTheme]
        return desktop ? desktop.colors : ({})
    }

    property var _previewCache: Store.newBroker()
    property var _previewWaiters: ({})
    /// Context every preview renders against. A real-looking path makes the
    /// mock read as a terminal rather than a diagram.
    property string previewCwd: "~/projects/my-app"
    /// Narrower than a real terminal on purpose: the preview pane is about a
    /// third of the Studio, and a 120-col render pads its frame rules out
    /// past the pane and wraps every line. 88 fills the pane without
    /// misrepresenting how the prompt lays out.
    /// Fallback only. Every preview surface passes its own measured width;
    /// this is what a caller that cannot measure gets.
    property int previewCols: 88

    readonly property var _previewDebounce: Preview.debouncer(
        Motion.MICRO_MS,
        function (fn) { previewTimer.callback = fn; previewTimer.restart(); return 1 },
        function () { previewTimer.stop() })

    Timer {
        id: previewTimer
        property var callback: null
        interval: Motion.MICRO_MS
        repeat: false
        onTriggered: {
            var fn = previewTimer.callback
            previewTimer.callback = null
            if (fn) fn()
        }
    }

    function fetchDefaults() {
        service._rpc(Model.buildCommand("defaults", "svc-defaults"), "svc-defaults",
                     function (resp) {
                         if (resp.config !== undefined)
                             service.defaultsFlat = Model.flattenConfig(resp.config)
                     })
    }

    function fetchScripts() {
        service._rpc(Model.buildCommand("script_list", "svc-scripts"), "svc-scripts",
                     function (resp) {
                         if (resp.scripts !== undefined) service.scripts = resp.scripts
                     })
    }

    function runScript(name, cb) {
        var msg = JSON.stringify({
            type: "control", command: "script_run", name: name, id: "svc-run-" + name
        }) + "\n"
        return service._rpc(msg, "svc-run-" + name, cb || function () {})
    }

    // Apply a Look persistently (transient=true is the gallery's "Try").
    function applyLook(name, transient) {
        var id = "svc-apply-" + name
        var msg = JSON.stringify({
            type: "control", command: "looks_apply",
            name: name, transient: !!transient, id: id
        }) + "\n"
        return service._rpc(msg, id, function () {
            // A Look can change the palette, so every cached render is stale.
            service.invalidateDerived()
            service.fetchLooks()
        })
    }

    // Pin terminal colors to a curated palette — the deliberate unbind.
    /// Flat role -> hex for a palette key, from the DAEMON's catalog first.
    ///
    /// Model.CURATED_PALETTES holds 8 entries and exists only so the bind
    /// indicator can name a palette before the first fetch lands. The daemon
    /// serves 53 (39 curated + one derived per installed Omarchy theme).
    /// Resolving against the 8 made every chip beyond them a dead button:
    /// no write, no error, no toast -- clicking Neon or Outrun Electric
    /// simply did nothing.
    function paletteColors(key) {
        var rec = service.palettes ? service.palettes[key] : null
        if (rec) {
            var c = rec.colors && Object.keys(rec.colors).length > 0
                ? rec.colors : rec.custom
            if (c && Object.keys(c).length > 0) return c
        }
        var pal = Model.CURATED_PALETTES[key]
        if (!pal) return null
        return {
            accent: pal.accent, foreground: pal.foreground, muted: pal.muted,
            background: pal.background, red: pal.red, green: pal.green,
            yellow: pal.yellow, blue: pal.blue, magenta: pal.magenta,
            cyan: pal.cyan, orange: pal.orange
        }
    }

    /// Palette-level gradient intensity: "auto", "off" or "full".
    ///
    /// Palette-level rather than style-level because a gradient is made of
    /// colors: the segment ramp and the frame rule are the same sweep seen in
    /// two places, and they used to be able to disagree.
    function setGradient(mode) {
        var m = String(mode || "auto")
        if (m !== "auto" && m !== "off" && m !== "full") m = "auto"
        return service._configSet({ "theme": { "gradient": m } })
    }

    function applyPalette(key) {
        // "theme" is the resync directive, not a palette.
        if (key === "theme") return service.applyPaletteTheme()
        var colors = service.paletteColors(key)
        if (!colors) return service._err("unknown palette: " + key)
        var id = "svc-palette-" + key
        var msg = JSON.stringify({
            type: "config", command: "set",
            config: { theme: { source: "hybrid", custom: colors } }, id: id
        }) + "\n"
        return service._rpc(msg, id, function () { service.invalidateDerived() })
    }

    // Return terminal colors to the Omarchy desktop theme — the resync half
    // of the bind indicator.
    function applyPaletteTheme() {
        var id = "svc-sync-theme"
        var msg = JSON.stringify({
            type: "config", command: "set",
            config: { theme: { source: "omarchy" } }, id: id
        }) + "\n"
        return service._rpc(msg, id, function () {
            service.invalidateDerived()
        })
    }

    // ── Config writes (service-owned) ──────────────────────────────────────
    // Surfaces mutate config ONLY through here, so the Quick Panel and the
    // Studio share one dirty set and one debounce and cannot race each
    // other's saves. Reads come from `cfgFlat`.
    readonly property var cfgFlat: service._cfgFlat

    /// Save the daemon's CURRENT effective config as a named Look.
    ///
    /// Lives here rather than on each surface because Panel.qml and
    /// Gallery.qml each had their own copy writing to their own socket —
    /// the duplication this pass exists to remove.
    /// Save the daemon's current effective config as a named Look.
    ///
    /// `theme` is an optional `{ source, custom }` object. The daemon can only
    /// snapshot the colours it is currently rendering with, so without this
    /// the Studio's palette editor -- the one place eleven roles can be tuned
    /// by hand -- discarded every edit the moment you pressed Save.
    function saveLook(name, cb, theme) {
        var safe = String(name || "").trim()
        if (safe.length === 0) return service._err("a Look needs a name")
        var id = service._nextId("looksave")
        var msg = { type: "control", command: "looks_save",
                    name: safe, label: safe, id: id }
        if (theme && Object.keys(theme).length > 0) msg.theme = theme
        var sent = service._rpc(JSON.stringify(msg) + "\n", id, function (resp) {
            service.fetchLooks()
            // A new Look changes nothing about existing renders, but the
            // saved one must not inherit a cache entry keyed on its name.
            service.invalidateDerived()
            if (cb) cb(resp)
        })
        return sent ? JSON.stringify({ ok: true, queued: true })
                    : service._err("no omarchy10k daemon running")
    }

    function deleteLook(name, cb) {
        var safe = String(name || "").trim()
        if (safe.length === 0) return service._err("which Look?")
        var id = service._nextId("looksdel")
        var sent = service._rpc(JSON.stringify({
            type: "control", command: "looks_delete", name: safe, id: id
        }) + "\n", id, function (resp) {
            service.fetchLooks()
            service.invalidateDerived()
            if (cb) cb(resp)
        })
        return sent ? JSON.stringify({ ok: true, queued: true })
                    : service._err("no omarchy10k daemon running")
    }

    function configValue(tomlKey, fallback) {
        var v = service._cfgFlat[tomlKey]
        return v === undefined ? fallback : v
    }

    function defaultValue(tomlKey) {
        return service.defaultsFlat[tomlKey]
    }

    // Stage a change: applied to the local view immediately so the UI is
    // responsive, then flushed as a delta.
    function setConfigValue(tomlKey, value) {
        var flat = service._cfgFlat
        if (flat[tomlKey] === value) return
        service.pushUndo(flat)
        var next = {}
        for (var k in flat) next[k] = flat[k]
        next[tomlKey] = value
        service._cfgFlat = next
        service.touchConfigKey(tomlKey)
        configSaveTimer.restart()
    }

    function resetConfigValue(tomlKey) {
        var d = service.defaultValue(tomlKey)
        if (d !== undefined) service.setConfigValue(tomlKey, d)
    }

    function undoConfig() {
        var prev = service.popUndo()
        if (!prev) return
        service._cfgFlat = prev
        for (var k in prev) service.touchConfigKey(k)
        configSaveTimer.restart()
    }

    function _flushConfig() {
        var patchFlat = service.collectDelta(service._cfgFlat)
        if (Object.keys(patchFlat).length === 0) return
        var id = "svc-cfg-set"
        service._rpc(Model.buildConfigSet(Model.unflattenPatch(patchFlat), id), id,
                     function () { service.invalidateDerived() })
    }

    Timer {
        id: configSaveTimer
        interval: 300
        repeat: false
        onTriggered: service._flushConfig()
    }

    // Everything derived from daemon state, refetched together.
    function refreshDerived() {
        service.invalidateDerived()
        service.fetchLooks()
        service.fetchPalettes()
        service.fetchDefaults()
        service.fetchScripts()
        themeNameProbe.running = true
    }

    Process {
        id: themeNameProbe
        command: ["omarchy", "theme", "current"]
        stdout: StdioCollector {
            onStreamFinished: service.desktopTheme = String(this.text).trim()
        }
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
            service.refreshDerived()
            return
        }
        if (resp.type === "config" && resp.config) {
            service._cfgFlat = Model.flattenConfig(resp.config)
            // Every cached preview was rendered against the previous config.
            service.invalidateDerived()
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
