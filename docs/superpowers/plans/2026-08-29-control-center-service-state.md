# Control Center Service State Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the pure state logic the Control Center service will own — a preview broker, config delta tracking, and an undo stack — as a node-testable library, then expose it from `Service.qml`.

**Architecture:** All state logic lives in `quattro/o10k/Store.js` (`.pragma library`), unit-tested under Node like `Model.js` and `Fx.js`. `Service.qml` holds instances and wires them to sockets. Quickshell's `Socket`/`Process` types cannot load under `qmltestrunner`, so keeping the logic out of QML is what makes it testable at all.

**Tech Stack:** `.pragma library` JavaScript, Node 26, `qmllint`.

**Spec:** `docs/superpowers/specs/2026-08-29-control-center-rebuild-design.md` — implements **Increment 2**, with one deliberate deviation recorded below.

## Deviation from the spec

The spec's Increment 2 acceptance includes "existing Panel and Gallery still work, now as consumers." This plan **does not rewire `Panel.qml` or `Gallery.qml`.**

Those two files total 2,822 lines and are slated for replacement (`QuickPanel`, Increment 3) and folding (Studio Looks tab, Increment 5). Refactoring them to consume the service, only to delete them two increments later, is throwaway work on the riskiest files in the plugin. The service state layer is built and tested standalone here; its first consumer is `QuickPanel` in Increment 3.

**Consequence:** the spec's "3 sockets → 1" outcome moves from the end of Increment 2 to the end of Increment 5, when the old surfaces are removed. Nothing else changes. The spec's performance budget still holds at the end state.

## Global Constraints

Same as the kit plan, and they still apply:

- Zero `layer.enabled`; `RectangularShadow` only for shadows.
- Shared kit and libraries stay *unbound*.
- State fills route to `Style.*Fill` — no parallel alphas.
- `omarchy plugin validate quattro` must pass after every task.
- Zero `unqualified` qmllint warnings in `quattro/o10k/`.
- Pure logic goes in `.pragma library` JS so it is node-testable; QML stays thin.

## Bugs in the existing preview path this must fix

Both were verified in `quattro/Gallery.qml` and are the reason the broker exists rather than a straight port:

| Bug | Detail |
|---|---|
| In-flight leak | `_inFlight[name]` is set in `requestPreview` (line 397) and cleared only when a response arrives whose `id` starts with `look-` (line 519). A disconnect, or an `{"type":"error"}` response shape that does not match `resp.type === "preview"`, leaves the entry set forever — that card shows `--` and never retries. |
| Stale cache | `previewCache` is keyed on Look name alone. After a palette or preset change every cached preview is wrong, but `requestPreview` returns early on the cache hit (line 395), so cards keep rendering the old config indefinitely. |

---

### Task 1: `Store.js` preview broker

**Files:**
- Create: `quattro/o10k/Store.js`
- Test: `tests/store_test.js`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `previewKey(ctx, patch) -> String` — stable key; property order in `ctx`/`patch` must not change the key.
  - `newBroker() -> Broker` with fields `{ cache: {}, inFlight: {}, generation: 0 }`.
  - `brokerLookup(broker, key) -> { hit: Bool, value: String|undefined, shouldSend: Bool }` — `shouldSend` is true only when there is no cache hit and no in-flight entry for `key`.
  - `brokerBegin(broker, key, id)` — marks in-flight, recording the request `id` and the generation.
  - `brokerResolve(broker, key, id, value) -> Bool` — stores the value and releases in-flight; returns `false` (and stores nothing) if the generation moved since `brokerBegin`.
  - `brokerRelease(broker, key)` — releases in-flight without storing, for errors and disconnects.
  - `brokerInvalidate(broker)` — bumps the generation and clears the cache; in-flight entries are released as they resolve.

- [ ] **Step 1: Write the failing test**

Create `tests/store_test.js`:

```javascript
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/store_test.js`
Expected: FAIL — `ENOENT: no such file or directory, open '.../quattro/o10k/Store.js'`

- [ ] **Step 3: Write minimal implementation**

Create `quattro/o10k/Store.js`:

```javascript
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/store_test.js`
Expected: PASS — `Store.js broker: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Store.js tests/store_test.js
git commit -m "feat(quattro): Store.js preview broker

Fixes two defects in the per-surface caches it replaces: in-flight entries
were released only on a matching response (a disconnect stranded a card
forever), and the cache was keyed on Look name alone (a palette change left
every cached preview stale but un-refetchable)."
```

---

### Task 2: `Store.js` config delta tracking

**Files:**
- Modify: `quattro/o10k/Store.js`
- Test: `tests/store_test.js`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `newDelta() -> Delta` with `{ dirty: {} }`.
  - `deltaTouch(delta, key)` — marks one config key dirty.
  - `deltaPending(delta) -> Bool`.
  - `deltaCollect(delta, fullFlat) -> Object` — returns a flat object of ONLY the dirty keys present in `fullFlat`, then clears the dirty set.

- [ ] **Step 1: Write the failing test**

Append to `tests/store_test.js`, immediately before the final `if (failures > 0)` block:

```javascript
// ── Config delta ───────────────────────────────────────────────────────────
const D = new Function(src + '\n;return {' +
    ' newDelta, deltaTouch, deltaPending, deltaCollect };')();

const d = D.newDelta();
check('fresh delta has nothing pending', D.deltaPending(d), false);

D.deltaTouch(d, 'git.mode');
check('touched delta is pending', D.deltaPending(d), true);

// The delta discipline: a save must send ONLY what changed here. Stamping
// every mapped key would clobber edits made outside this surface (CLI,
// another panel) with UI state captured at load time.
const full = { 'git.mode': 'compact', 'git.enabled': true, 'style.preset': 'lean' };
const patch = D.deltaCollect(d, full);
check('collect returns only dirty keys', Object.keys(patch).join(','), 'git.mode');
check('collect returns the current value', patch['git.mode'], 'compact');
check('collect clears the dirty set', D.deltaPending(d), false);

// A key touched but absent from the config snapshot must not invent a
// value — that would write undefined into the user's TOML.
const d2 = D.newDelta();
D.deltaTouch(d2, 'nonexistent.key');
check('unknown key is dropped', Object.keys(D.deltaCollect(d2, full)).length, 0);

// Touching the same key twice must not send it twice.
const d3 = D.newDelta();
D.deltaTouch(d3, 'git.mode');
D.deltaTouch(d3, 'git.mode');
check('repeat touch collapses', Object.keys(D.deltaCollect(d3, full)).length, 1);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/store_test.js`
Expected: FAIL — `ReferenceError: newDelta is not defined`

- [ ] **Step 3: Write minimal implementation**

Append to `quattro/o10k/Store.js`:

```javascript
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/store_test.js`
Expected: PASS — `Store.js broker: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Store.js tests/store_test.js
git commit -m "feat(quattro): Store.js config delta tracking

Sends only keys changed on this surface. One owner means the Quick Panel
and Studio cannot race each other's saves, and a dirty key with no value in
the snapshot is dropped rather than written as undefined."
```

---

### Task 3: `Store.js` undo stack

**Files:**
- Modify: `quattro/o10k/Store.js`
- Test: `tests/store_test.js`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `newUndo(limit) -> Undo` (default limit 10).
  - `undoPush(undo, snapshot)` — deep-copies the snapshot; drops the oldest past `limit`.
  - `undoDepth(undo) -> Number`.
  - `undoPop(undo) -> Object|null`.

- [ ] **Step 1: Write the failing test**

Append to `tests/store_test.js`, before the final `if (failures > 0)` block:

```javascript
// ── Undo stack ─────────────────────────────────────────────────────────────
const U = new Function(src + '\n;return {' +
    ' newUndo, undoPush, undoDepth, undoPop };')();

const u = U.newUndo(3);
check('fresh stack is empty', U.undoDepth(u), 0);
check('popping an empty stack is null', U.undoPop(u), null);

U.undoPush(u, { a: 1 });
U.undoPush(u, { a: 2 });
check('depth tracks pushes', U.undoDepth(u), 2);
check('pop returns the most recent', U.undoPop(u).a, 2);
check('pop shrinks the stack', U.undoDepth(u), 1);

// Bounded: a long editing session must not grow memory without limit.
const u2 = U.newUndo(3);
for (let i = 0; i < 10; i++) U.undoPush(u2, { n: i });
check('stack is bounded by its limit', U.undoDepth(u2), 3);
check('the newest entry survives', U.undoPop(u2).n, 9);

// Snapshots must be deep-copied. Storing a live reference means the caller
// mutating its config object silently rewrites undo history.
const u3 = U.newUndo(3);
const live = { nested: { v: 'before' } };
U.undoPush(u3, live);
live.nested.v = 'after';
check('snapshot is deep-copied', U.undoPop(u3).nested.v, 'before');
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/store_test.js`
Expected: FAIL — `ReferenceError: newUndo is not defined`

- [ ] **Step 3: Write minimal implementation**

Append to `quattro/o10k/Store.js`:

```javascript
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/store_test.js`
Expected: PASS — `Store.js broker: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Store.js tests/store_test.js
git commit -m "feat(quattro): Store.js bounded undo stack

Service-owned so an edit made in the Studio is undoable from the Quick
Panel. Snapshots are deep-copied — storing a live reference let a later
mutation silently rewrite history."
```

---

### Task 4: Expose the store from `Service.qml`

**Files:**
- Modify: `quattro/Service.qml`
- Test: `tests/integration_test.sh` (extend the Control Center Kit section)

**Interfaces:**
- Consumes: everything from Tasks 1–3.
- Produces: `Service.previewLookup(ctx, patch)`, `Service.previewBegin(ctx, patch, id)`, `Service.previewResolve(key, id, value)`, `Service.previewRelease(key)`, `Service.invalidateDerived()`, `Service.touchConfigKey(key)`, `Service.pushUndo(flat)`, `Service.popUndo()`, and the read-only `Service.undoDepth`. Increment 3's `QuickPanel` consumes these.

- [ ] **Step 1: Add the store to Service.qml**

In `quattro/Service.qml`, add to the imports at the top (after `import "Model.js" as Model`):

```qml
import "o10k/Store.js" as Store
```

Then add inside the root `Item`, after the `property var _cfgFlat: ({})` line:

```qml
    // ── Owned state (Increment 2) ──────────────────────────────────────────
    // The service is the single owner of preview caching, config delta
    // tracking and undo, so surfaces cannot drift apart or race each other's
    // saves. Logic lives in o10k/Store.js because Quickshell's Socket type
    // cannot load under qmltestrunner — see tests/store_test.js.
    property var _broker: Store.newBroker()
    property var _delta: Store.newDelta()
    property var _undo: Store.newUndo(10)

    readonly property int undoDepth: service._undoRevision, Store.undoDepth(service._undo)
    // Bumped on every undo mutation: the stack is a plain JS object, so
    // nothing else would re-evaluate the binding above.
    property int _undoRevision: 0

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
        service._undoRevision++
    }

    function popUndo() {
        var prev = Store.undoPop(service._undo)
        service._undoRevision++
        return prev
    }
```

- [ ] **Step 2: Verify the lint gate still passes**

Run: `bash tests/qmllint.sh`
Expected: PASS — `qmllint gate passed`. If it reports unqualified access in `Service.qml`, qualify the reference with `service.` and re-run.

- [ ] **Step 3: Verify the manifest still validates**

Run: `omarchy plugin validate quattro && echo VALID`
Expected: `VALID`

- [ ] **Step 4: Add the store test to the integration suite**

In `tests/integration_test.sh`, inside the `Control Center Kit` section, after the `Motion.js unit tests` block and before the closing `else` of the `command -v node` check, add:

```bash
    if node "$SCRIPT_DIR/tests/store_test.js" >/dev/null 2>&1; then
        pass "Store.js unit tests"
    else
        fail "Store.js unit tests" "$(node "$SCRIPT_DIR/tests/store_test.js" 2>&1 | tail -3)"
    fi
```

And in the matching `else` branch, alongside the other skips:

```bash
    skip "Store.js unit tests" "node not available"
```

- [ ] **Step 5: Run everything**

Run: `node tests/store_test.js && bash tests/qml/run.sh && bash tests/qmllint.sh`
Expected: all three pass.

Run: `bash tests/integration_test.sh 2>&1 | tail -4`
Expected: `Results: 86 passed, 0 failed, 1 skipped`

- [ ] **Step 6: Commit**

```bash
git add quattro/Service.qml tests/integration_test.sh
git commit -m "feat(quattro): expose the store from Service.qml

The service becomes the owner of preview caching, config delta tracking and
undo. Surfaces are views over this state, so they cannot drift apart or race
each other's saves. Panel.qml and Gallery.qml are intentionally NOT rewired
— they are replaced in increments 3 and 5."
```

---

## Self-review

**Spec coverage:** Increment 2 is "Service state ownership", done when "One socket; config/looks/palettes/defaults/undo served from Service.qml; preview broker coalesces; existing Panel and Gallery still work, now as consumers."

- Preview broker → Tasks 1, 4. Config delta → Tasks 2, 4. Undo → Tasks 3, 4.
- **Deliberate deviation, recorded above:** Panel/Gallery are not rewired and the socket count does not drop yet. Both move to Increment 5, when those files are removed. "Existing Panel and Gallery still work" holds trivially — they are untouched.
- `looks` / `palettes` / `defaults` fetching is deferred to Increment 3, where `QuickPanel` is the first surface that needs them; adding fetchers with no consumer now would be untested code with no caller.

**Placeholder scan:** none — every step has complete code and an exact command with expected output.

**Type consistency:** `previewKey`, `newBroker`, `brokerLookup`, `brokerBegin`, `brokerResolve`, `brokerRelease`, `brokerInvalidate` (Task 1); `newDelta`, `deltaTouch`, `deltaPending`, `deltaCollect` (Task 2); `newUndo`, `undoPush`, `undoDepth`, `undoPop` (Task 3) are used with exactly those names in Task 4.
