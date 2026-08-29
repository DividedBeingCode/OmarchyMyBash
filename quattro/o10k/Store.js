.pragma library

// State logic for the Omarchy10k Control Center service.
//
// Every surface (Quick Panel, Studio, bar widget) is a VIEW over this
// state; Service.qml owns the instances. The logic lives here rather than
// in QML because Quickshell's Socket/Process types cannot load under
// qmltestrunner — keeping it in a .pragma library is what makes it
// testable at all (same reason Model.js is a library).

// ── Preview broker ─────────────────────────────────────────────────────────
//
// Replaces the per-surface caches in Panel.qml and Gallery.qml, which had
// two defects this design fixes by construction:
//
//   1. In-flight entries were released only on a matching response, so a
//      disconnect or an unexpected error shape stranded a card forever.
//      brokerRelease() exists for exactly those paths.
//   2. The cache was keyed on Look name alone, so it kept serving renders
//      from before a palette change. The key includes the full context and
//      patch, and brokerInvalidate() drops everything on a config change.

// Stable stringify: JSON.stringify does not guarantee key order across
// objects built by different call sites, and an order-dependent key means
// two identical requests miss each other's cache entries.
function _stable(v) {
    if (v === null || v === undefined)
        return 'null';
    if (typeof v !== 'object')
        return JSON.stringify(v);
    if (Array.isArray(v))
        return '[' + v.map(_stable).join(',') + ']';
    var keys = Object.keys(v).sort();
    var parts = [];
    for (var i = 0; i < keys.length; i++)
        parts.push(JSON.stringify(keys[i]) + ':' + _stable(v[keys[i]]));
    return '{' + parts.join(',') + '}';
}

function previewKey(ctx, patch) {
    return _stable(ctx) + '|' + _stable(patch);
}

function newBroker() {
    return { cache: {}, inFlight: {}, generation: 0 };
}

function brokerLookup(broker, key) {
    if (Object.prototype.hasOwnProperty.call(broker.cache, key))
        return { hit: true, value: broker.cache[key], shouldSend: false };
    if (Object.prototype.hasOwnProperty.call(broker.inFlight, key))
        return { hit: false, value: undefined, shouldSend: false };
    return { hit: false, value: undefined, shouldSend: true };
}

function brokerBegin(broker, key, id) {
    broker.inFlight[key] = { id: id, generation: broker.generation };
}

function brokerResolve(broker, key, id, value) {
    var pending = broker.inFlight[key];
    // A response is only valid for the request currently outstanding on
    // this key, and only if no invalidate happened in between.
    if (!pending || pending.id !== id) {
        return false;
    }
    delete broker.inFlight[key];
    if (pending.generation !== broker.generation) {
        return false;
    }
    broker.cache[key] = value;
    return true;
}

function brokerRelease(broker, key) {
    delete broker.inFlight[key];
}

function brokerInvalidate(broker) {
    broker.generation++;
    broker.cache = {};
}

// ── Config delta ───────────────────────────────────────────────────────────
//
// The save path sends ONLY keys changed on this surface. Stamping every
// mapped key instead would clobber edits made elsewhere (the CLI, another
// panel) with UI state captured at load time — a bug the panel already hit
// once and fixed with this discipline. One owner means the Quick Panel and
// the Studio cannot race each other's saves.

function newDelta() {
    return { dirty: {} };
}

function deltaTouch(delta, key) {
    delta.dirty[key] = true;
}

function deltaPending(delta) {
    return Object.keys(delta.dirty).length > 0;
}

function deltaCollect(delta, fullFlat) {
    var out = {};
    var keys = Object.keys(delta.dirty);
    for (var i = 0; i < keys.length; i++) {
        var k = keys[i];
        // A dirty key absent from the snapshot has no value to send;
        // inventing one would write undefined into the user's TOML.
        if (Object.prototype.hasOwnProperty.call(fullFlat, k))
            out[k] = fullFlat[k];
    }
    delta.dirty = {};
    return out;
}

// ── Undo stack ─────────────────────────────────────────────────────────────
//
// Owned by the service rather than a surface, so an edit made in the Studio
// is undoable from the Quick Panel and vice versa.

function newUndo(limit) {
    var n = Number(limit);
    return { entries: [], limit: (isFinite(n) && n > 0) ? n : 10 };
}

function undoPush(undo, snapshot) {
    // Deep copy: storing the live object means a later mutation by the
    // caller silently rewrites history.
    undo.entries.push(JSON.parse(JSON.stringify(snapshot)));
    while (undo.entries.length > undo.limit)
        undo.entries.shift();
}

function undoDepth(undo) {
    return undo.entries.length;
}

function undoPop(undo) {
    if (undo.entries.length === 0)
        return null;
    return undo.entries.pop();
}

// ── Theme bind state ───────────────────────────────────────────────────────
//
// Whether the terminal's colors follow the Omarchy desktop theme or are
// pinned to an override. The daemon already models this through the Look
// schema's `palette` directive ("theme" | "keep" | <curated key>); what was
// missing is any way for a surface to SHOW the state, so applying a palette
// silently desynced the terminal from the desktop with no road back.
//
// Returns:
//   state        "bound" | "pinned" | "index"
//   desktopTheme the active Omarchy theme name (always reported, so a
//                pinned surface can show what it is diverging FROM)
//   palette      curated palette key when the pin is recognisable, else null
//   paletteLabel display label, "Custom" for hand-set colors
//   syncPatch    the config_set patch that returns to bound
function themeBindState(cfgFlat, curatedPalettes, desktopTheme) {
    var flat = cfgFlat || {};
    var source = flat['theme.source'];
    var out = {
        state: 'bound',
        desktopTheme: desktopTheme || '',
        palette: null,
        paletteLabel: '',
        syncPatch: { theme: { source: 'omarchy' } }
    };

    // A fresh config has no [theme] table at all; that is bound, not broken.
    if (!source || source === 'omarchy')
        return out;

    if (source === 'terminal') {
        out.state = 'index';
        return out;
    }

    // Anything else ("hybrid", "custom") means the terminal is pinned away
    // from the desktop theme.
    out.state = 'pinned';
    out.paletteLabel = 'Custom';

    var accent = flat['theme.custom.accent'];
    if (accent && curatedPalettes) {
        var target = String(accent).toLowerCase();
        var keys = Object.keys(curatedPalettes);
        for (var i = 0; i < keys.length; i++) {
            var p = curatedPalettes[keys[i]];
            if (p && p.accent && String(p.accent).toLowerCase() === target) {
                out.palette = keys[i];
                out.paletteLabel = p.label || keys[i];
                break;
            }
        }
    }
    return out;
}
