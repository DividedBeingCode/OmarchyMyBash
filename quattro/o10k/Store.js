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
