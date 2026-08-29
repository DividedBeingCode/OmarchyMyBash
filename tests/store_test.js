#!/usr/bin/env node
// Unit tests for quattro/o10k/Store.js — the Control Center's state logic.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'o10k', 'Store.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, '');
const S = new Function(src + '\n;return {' +
    ' previewKey, newBroker, brokerLookup, brokerBegin, brokerResolve,' +
    ' brokerRelease, brokerInvalidate };')();

let failures = 0;
function check(desc, actual, expected) {
    if (actual !== expected) {
        console.error(`FAIL: ${desc}\n  expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
        failures++;
    }
}

// ── previewKey ─────────────────────────────────────────────────────────────
// Property insertion order must not change the key, or two identical
// requests built by different call sites miss each other's cache entries.
check('key is order-independent',
    S.previewKey({ a: 1, b: 2 }, null),
    S.previewKey({ b: 2, a: 1 }, null));
if (S.previewKey({ a: 1 }, null) === S.previewKey({ a: 2 }, null)) {
    console.error('FAIL: different context must produce a different key');
    failures++;
}
if (S.previewKey({ a: 1 }, { x: 1 }) === S.previewKey({ a: 1 }, { x: 2 })) {
    console.error('FAIL: different patch must produce a different key');
    failures++;
}

// ── lookup / begin / resolve ───────────────────────────────────────────────
const b = S.newBroker();
const k = S.previewKey({ look: 'omnarchy' }, null);

let r = S.brokerLookup(b, k);
check('cold lookup is a miss', r.hit, false);
check('cold lookup asks to send', r.shouldSend, true);

S.brokerBegin(b, k, 'req-1');
r = S.brokerLookup(b, k);
check('in-flight lookup is still a miss', r.hit, false);
// The whole point of in-flight tracking: a fast prompt loop must not spawn
// a request storm for the same key.
check('in-flight lookup does NOT re-send', r.shouldSend, false);

check('resolve accepts a current-generation response',
    S.brokerResolve(b, k, 'req-1', '<rendered>'), true);
r = S.brokerLookup(b, k);
check('resolved lookup hits', r.hit, true);
check('resolved lookup returns the value', r.value, '<rendered>');
check('resolved lookup does not re-send', r.shouldSend, false);

// ── release: the in-flight leak fix ────────────────────────────────────────
// Gallery.qml sets _inFlight[name] and clears it only on a matching preview
// response. A disconnect or an {"type":"error"} shape leaves it set forever
// and the card never retries. Release must make the key sendable again.
const b2 = S.newBroker();
const k2 = S.previewKey({ look: 'lean-pure' }, null);
S.brokerBegin(b2, k2, 'req-2');
check('in-flight blocks sending', S.brokerLookup(b2, k2).shouldSend, false);
S.brokerRelease(b2, k2);
check('released key is sendable again', S.brokerLookup(b2, k2).shouldSend, true);
check('released key stored nothing', S.brokerLookup(b2, k2).hit, false);

// ── invalidate: the stale-cache fix ────────────────────────────────────────
// previewCache keyed on Look name alone kept serving pre-palette-change
// renders forever. A config change must drop the cache.
const b3 = S.newBroker();
const k3 = S.previewKey({ look: 'gruvbox-drift' }, null);
S.brokerBegin(b3, k3, 'req-3');
S.brokerResolve(b3, k3, 'req-3', '<old>');
check('cached before invalidate', S.brokerLookup(b3, k3).hit, true);
S.brokerInvalidate(b3);
check('cache dropped on invalidate', S.brokerLookup(b3, k3).hit, false);
check('re-sends after invalidate', S.brokerLookup(b3, k3).shouldSend, true);

// A response that raced an invalidate must not resurrect stale content.
const b4 = S.newBroker();
const k4 = S.previewKey({ look: 'polar-lean' }, null);
S.brokerBegin(b4, k4, 'req-4');
S.brokerInvalidate(b4);
check('stale-generation resolve is rejected',
    S.brokerResolve(b4, k4, 'req-4', '<stale>'), false);
check('rejected resolve stored nothing', S.brokerLookup(b4, k4).hit, false);
check('rejected resolve released in-flight',
    S.brokerLookup(b4, k4).shouldSend, true);

// A response for a superseded request id must not land either.
const b5 = S.newBroker();
const k5 = S.previewKey({ look: 'rose-classic' }, null);
S.brokerBegin(b5, k5, 'req-5a');
S.brokerBegin(b5, k5, 'req-5b');
check('response for a superseded id is rejected',
    S.brokerResolve(b5, k5, 'req-5a', '<superseded>'), false);
check('response for the current id is accepted',
    S.brokerResolve(b5, k5, 'req-5b', '<current>'), true);

if (failures > 0) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
}
console.log('Store.js broker: all checks passed');
