#!/usr/bin/env node
// Unit tests for quattro/o10k/Preview.js — scene catalog, request building,
// cache keys and hover coalescing.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'o10k', 'Preview.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, '');
const P = new Function(src + '\n;return {' +
    ' SCENES, CARD_SCENES, SCENE_FIELDS, buildRequest, cacheKey, debouncer, cardColsFor };')();

let failures = 0;
function check(desc, actual, expected) {
    if (actual !== expected) {
        console.error(`FAIL: ${desc}\n  expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
        failures++;
    }
}
function ok(desc, cond) { check(desc, !!cond, true); }

// ── Scene catalog ──────────────────────────────────────────────────────────

ok('catalog is non-trivial', P.SCENES.length >= 5);

(() => {
    const keys = P.SCENES.map(s => s.key);
    check('scene keys are unique', new Set(keys).size, keys.length);
    ok('every scene has a human label', P.SCENES.every(s => s.label && s.label.length > 0));
})();

// The states a picker cannot show you by standing still.
['dirty', 'failed', 'ssh', 'deep'].forEach(k => {
    ok(`catalog covers the "${k}" state`, P.SCENES.some(s => s.key === k));
});

check('the failure scene actually fails',
    P.SCENES.find(s => s.key === 'failed').exit_code !== 0, true);
check('the ssh scene is actually remote',
    P.SCENES.find(s => s.key === 'ssh').in_ssh, true);

// ── Request building ───────────────────────────────────────────────────────

(() => {
    const line = P.buildRequest({ cwd: '~/x', cols: 100 }, null, null, null, 'id1');
    check('request ends with a newline', line.endsWith('\n'), true);
    check('request is exactly one line', line.trim().includes('\n'), false);
    const msg = JSON.parse(line);
    check('type is preview', msg.type, 'preview');
    check('context is carried through', msg.cwd, '~/x');
    check('id is carried through', msg.id, 'id1');
    check('no scenes key when none asked for', 'scenes' in msg, false);
    check('no patch key when none given', 'patch' in msg, false);
})();

(() => {
    const msg = JSON.parse(P.buildRequest({}, { style: { preset: 'lean' } }, 'polar-lean', P.SCENES, 'x'));
    check('patch is carried', msg.patch.style.preset, 'lean');
    check('look is carried', msg.look, 'polar-lean');
    check('every scene is sent', msg.scenes.length, P.SCENES.length);
})();

(() => {
    // A scene carries only what it varies; unset fields must be ABSENT so the
    // daemon's request-level values show through rather than being clobbered
    // by an explicit null.
    const msg = JSON.parse(P.buildRequest({ cwd: '~/base' }, null, null,
        [{ key: 'failed', label: 'boom', exit_code: 127 }], 'x'));
    check('scene keeps what it sets', msg.scenes[0].exit_code, 127);
    check('scene label is sent', msg.scenes[0].label, 'boom');
    check('unset scene field is omitted, not nulled', 'cwd' in msg.scenes[0], false);
    check('the internal key is not sent to the daemon', 'key' in msg.scenes[0], false);
})();

(() => {
    // A field not in SCENE_FIELDS is dropped rather than sent -- a typo should
    // fail visibly here, not be silently swallowed by serde(default).
    const msg = JSON.parse(P.buildRequest({}, null, null,
        [{ exit_code: 1, git_branchh: 'typo' }], 'x'));
    check('unknown scene field is dropped', 'git_branchh' in msg.scenes[0], false);
})();

(() => {
    const msg = JSON.parse(P.buildRequest({}, null, '', P.SCENES, 'x'));
    check('empty look is omitted', 'look' in msg, false);
})();

// ── Cache keys ─────────────────────────────────────────────────────────────

(() => {
    // Two hovers over the same card must key identically or the broker caches
    // nothing and every hover is a round-trip.
    const a = P.cacheKey('omnarchy', { b: 2, a: 1 }, P.SCENES);
    const b = P.cacheKey('omnarchy', { a: 1, b: 2 }, P.SCENES);
    check('key is stable across object key order', a, b);

    check('different look gives a different key',
        P.cacheKey('omnarchy', null, P.SCENES) === P.cacheKey('lean-pure', null, P.SCENES), false);
    check('different patch gives a different key',
        P.cacheKey('x', { a: 1 }, null) === P.cacheKey('x', { a: 2 }, null), false);
    check('different scene set gives a different key',
        P.cacheKey('x', null, P.SCENES) === P.cacheKey('x', null, P.CARD_SCENES), false);
    check('nested key order is also stable',
        P.cacheKey('x', { o: { z: 1, a: 2 } }, null),
        P.cacheKey('x', { o: { a: 2, z: 1 } }, null));
})();

// ── Debounce ───────────────────────────────────────────────────────────────

function fakeClock() {
    let seq = 0;
    const timers = new Map();
    return {
        schedule: (fn, ms) => { timers.set(++seq, { fn, ms }); return seq; },
        cancel: (h) => timers.delete(h),
        runAll: () => { const t = [...timers.values()]; timers.clear(); t.forEach(x => x.fn()); },
        live: () => timers.size
    };
}

(() => {
    // Crossing a grid of 18 cards fires 18 hovers. Exactly one request should
    // survive, and it must be the LAST card -- the one under the cursor.
    const clock = fakeClock();
    const d = P.debouncer(90, clock.schedule, clock.cancel);
    let calls = [];
    for (let i = 0; i < 18; i++) d.request(() => calls.push(i));
    check('only one timer is outstanding', clock.live(), 1);
    clock.runAll();
    check('a burst coalesces to one call', calls.length, 1);
    check('the surviving call is the last one', calls[0], 17);
})();

(() => {
    // A click should not wait out the hover delay.
    const clock = fakeClock();
    const d = P.debouncer(90, clock.schedule, clock.cancel);
    let ran = false;
    d.request(() => { ran = true; });
    check('nothing ran yet', ran, false);
    d.flush();
    check('flush runs immediately', ran, true);
    check('flush leaves no timer behind', clock.live(), 0);
})();

(() => {
    // The mouse leaving the grid should cancel, not fire.
    const clock = fakeClock();
    const d = P.debouncer(90, clock.schedule, clock.cancel);
    let ran = false;
    d.request(() => { ran = true; });
    d.clear();
    check('clear drops the pending call', d.pending(), false);
    clock.runAll();
    check('cleared call never runs', ran, false);
})();

(() => {
    const clock = fakeClock();
    const d = P.debouncer(90, clock.schedule, clock.cancel);
    d.flush();  // nothing queued
    check('flushing an empty debouncer is harmless', d.pending(), false);
})();


// The card baseline flag is gone: applying a Look is atomic, so a card
// rendered on the live config is already stable across applies — and it also
// reflects the user's own segment toggles, which the default baseline hid.
(() => {
    const req = JSON.parse(P.buildRequest(
        { cwd: '~/app', cols: 38 }, null, 'synthwave', null, 'x1'));
    check('no baseline field is sent', Object.prototype.hasOwnProperty.call(req, 'base'), false);

    // Collisions, not identity: `cacheKey` is a pure function, so comparing
    // two identical calls only asserts that JavaScript is deterministic and
    // cannot fail. What can actually go wrong is two DIFFERENT card requests
    // hashing the same, which serves one card's render for another.
    check('two different Looks at the same width do not collide',
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38) !==
          P.cacheKey('gruvbox-drift', null, P.CARD_SCENES, 38), true);
    check('two different scene sets at the same width do not collide',
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38) !==
          P.cacheKey('synthwave', null, P.SCENES, 38), true);
    check('width still separates renders',
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38) !==
          P.cacheKey('synthwave', null, P.CARD_SCENES, 80), true);
    // The identity direction still matters — two hovers over the same card
    // must share an entry — but it is only meaningful next to the collision
    // checks above, which is what makes the key non-trivial.
    check('the same request still shares a cache entry',
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38) ===
          P.cacheKey('synthwave', null, P.CARD_SCENES, 38), true);
})();


// ── Card render width ──────────────────────────────────────────────────────
//
// The bug: cardColsFor clamped to 20 for a pane that had not been laid out,
// so the gallery rendered every card for a 20-column terminal and only
// refetched at the real width when something else triggered a refresh —
// making an unrelated Apply look like it rewrote every other preset.
(() => {
    const GAP = 10, PAD = 20, CW = 7.1875;
    check('an unlaid-out pane renders nothing, rather than 20 columns',
          P.cardColsFor(0, 3, CW, GAP, PAD), 0);
    check('a pane with no font metrics renders nothing',
          P.cardColsFor(1160, 3, 0, GAP, PAD), 0);
    check('a zero column count renders nothing',
          P.cardColsFor(1160, 0, CW, GAP, PAD), 0);

    const laid = P.cardColsFor(1160, 3, CW, GAP, PAD);
    check('a laid-out pane is far wider than the floor', laid > 40, true);
    check('the floor still protects a genuinely narrow pane',
          P.cardColsFor(200, 3, CW, GAP, PAD), 20);

    // The regression itself: these two must not be confusable.
    check('unlaid-out and narrow are distinguishable',
          P.cardColsFor(0, 3, CW, GAP, PAD) !== P.cardColsFor(200, 3, CW, GAP, PAD), true);
})();

if (failures) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
}
console.log('Preview.js tests passed');
