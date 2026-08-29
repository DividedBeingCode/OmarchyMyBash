# Control Center `o10k/` Kit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the shared `o10k/` component kit — radius floor, elevation, motion tokens, and a `SettingRow` — plus the QML test infrastructure the rest of the Control Center rebuild depends on.

**Architecture:** Pure, testable logic lives in `.pragma library` JavaScript (`Fx.js`, `Motion.js`) following the existing `Model.js` pattern, so it is unit-testable under Node. Thin QML components consume those libraries and route all state colors to Omarchy's existing `Style.*Fill` tokens rather than inventing parallel ones. A stubbed `qs.Commons` module lets `qmltestrunner` instantiate components headlessly.

**Tech Stack:** QML (Qt 6.11), Quickshell 0.3.1, `qmltestrunner`, `qmllint` (both at `/usr/lib/qt6/bin/`), Node 26 for JS unit tests, bash test harness.

**Spec:** `docs/superpowers/specs/2026-08-29-control-center-rebuild-design.md` — this plan implements **Increment 1** of that spec's eight build increments.

## Global Constraints

Every task's requirements implicitly include these. Values copied from the spec.

- **Zero `layer.enabled` uses.** Standing constraint, currently met across the plugin. Introducing one requires explicit justification.
- **Surface shadows use `RectangularShadow` only** — never `MultiEffect` or `DropShadow`, which need an offscreen buffer per surface. Target hardware is a ThinkPad T480 / Intel UHD 620, shared with the Spatial UX plugin in one Quickshell process.
- **The shared kit stays *unbound*.** Kit component definitions must not rely on outer-scope binding — bound inline components cannot be instantiated cross-file (the key finding of the C4 Panel decomposition). Consumer files keep `pragma ComponentBehavior: Bound`.
- **State fills route to `Style`.** Use `Style.normalFill`, `Style.hoverFill`, `Style.selectedFill`, `Style.pressedFill`, `Style.focusFill` and the `*For(fg, accent, urgent)` / `*BorderFor(...)` variants. Never define parallel alpha constants — those tokens are themeable via `Style.styleOverrides` and reinventing them is what makes our controls look foreign.
- **No per-frame work.** Color composites evaluate on change, never in a binding re-evaluated each frame.
- **`omarchy plugin validate quattro` must pass** after every task. It passes today.
- **Zero `unqualified` qmllint warnings in `quattro/o10k/`.** Plugin-wide baseline today is 341 warnings / 0 errors, of which 134 are `unqualified` — the class that caused the `wheelBoost` id-collision bug. New kit files start clean and stay clean.
- **Motion values mirror the Spatial UX plugin** (`~/syncthing/OMPSpacialUX/lib/Motion.qml`): micro 90 ms, short 140 ms, medium 220 ms, long 360 ms, so both plugins read as one product.

## Verified environment facts

These were confirmed on this machine before the plan was written. Do not re-derive them.

| Fact | Value |
|---|---|
| `qmllint` / `qmltestrunner` | `/usr/lib/qt6/bin/` (not on `PATH`) |
| `qs.Commons` real module | `/usr/share/omarchy/shell/Commons/qmldir`, module name `qs.Commons` |
| Resolving `qs.*` for tooling | Symlink `<dir>/qs -> /usr/share/omarchy/shell`, pass `-I <dir>` |
| Real `qs.Commons` under `qmltestrunner` | **Fails** — `Type Border unavailable` (needs the Quickshell runtime). Tests must use a stub. |
| `Style.cornerRadius` default | `0` |
| Headless runs | Require `QT_QPA_PLATFORM=offscreen` |

---

### Task 1: QML test harness

**Files:**
- Create: `tests/qml/stubs/qs/Commons/qmldir`
- Create: `tests/qml/stubs/qs/Commons/Style.qml`
- Create: `tests/qml/stubs/qs/Commons/Color.qml`
- Create: `tests/qml/run.sh`
- Test: `tests/qml/tst_harness.qml`

**Interfaces:**
- Consumes: nothing.
- Produces: `tests/qml/run.sh` — runs every `tests/qml/tst_*.qml` under `qmltestrunner` with the stub import path. Exit 0 on all-pass, non-zero otherwise. Later tasks add `tst_*.qml` files and re-run this script unchanged.

- [ ] **Step 1: Write the failing test**

Create `tests/qml/tst_harness.qml`:

```qml
import QtQuick
import QtTest
import qs.Commons

// Proves the stub qs.Commons module loads and carries the values the kit
// depends on. The REAL qs.Commons cannot load here (Border.qml needs the
// Quickshell runtime), which is why the stub exists.
TestCase {
    name: "Harness"

    function test_stub_style_is_stock_omarchy() {
        // Stock Omarchy ships rounding at 0 — the kit's radius floor exists
        // precisely because of this value.
        compare(Style.cornerRadius, 0)
    }

    function test_stub_exposes_state_fills() {
        verify(Style.normalFill !== undefined)
        verify(Style.hoverFill !== undefined)
        verify(Style.selectedFill !== undefined)
        verify(Color.accent !== undefined)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash tests/qml/run.sh`
Expected: FAIL — `run.sh: No such file or directory`

- [ ] **Step 3: Write the stub module and runner**

Create `tests/qml/stubs/qs/Commons/qmldir`:

```
module qs.Commons
singleton Color 1.0 Color.qml
singleton Style 1.0 Style.qml
```

Create `tests/qml/stubs/qs/Commons/Color.qml`:

```qml
pragma Singleton
import QtQuick

// Test stub for the omarchy-shell Color singleton. Tokyo Night values —
// only role NAMES matter to the kit, not these specific colors.
QtObject {
    property color accent: "#7aa2f7"
    property color foreground: "#c0caf5"
    property color background: "#1a1b26"
    property color muted: "#565f89"
    property color urgent: "#f7768e"
}
```

Create `tests/qml/stubs/qs/Commons/Style.qml`:

```qml
pragma Singleton
import QtQuick

// Test stub for the omarchy-shell Style singleton. Mirrors the real
// singleton's SHAPE (/usr/share/omarchy/shell/Commons/Style.qml) for the
// members the o10k kit touches, with stock-Omarchy defaults.
QtObject {
    id: stub

    // Mirrors Hyprland decoration:rounding. Omarchy ships 0.
    property int cornerRadius: 0
    property int gapsOut: 5

    property real normalFillAlpha: 0.04
    property real hoverFillAlpha: 0.08
    property real selectedFillAlpha: 0.18
    property real pressedFillAlpha: 0.22
    property real focusFillAlpha: 0.08

    property color normalFill: Qt.rgba(0.75, 0.79, 0.96, stub.normalFillAlpha)
    property color hoverFill: Qt.rgba(0.75, 0.79, 0.96, stub.hoverFillAlpha)
    property color selectedFill: Qt.rgba(0.48, 0.64, 0.97, stub.selectedFillAlpha)
    property color pressedFill: Qt.rgba(0.48, 0.64, 0.97, stub.pressedFillAlpha)
    property color focusFill: Qt.rgba(0.75, 0.79, 0.96, stub.focusFillAlpha)

    property var font: ({ family: "monospace", body: 12, bodySmall: 11,
                          caption: 10, subtitle: 14 })
    property var spacing: ({ controlGap: 6, controlHeight: 28,
                             controlPaddingX: 10, panelGap: 12 })

    function space(n) { return n }
}
```

Create `tests/qml/run.sh`:

```bash
#!/usr/bin/env bash
# Headless QML component tests for the o10k kit.
#
# The real qs.Commons cannot load under qmltestrunner (Border.qml needs the
# Quickshell runtime), so components are instantiated against the stub
# module in tests/qml/stubs. Stub SHAPE must track the real singleton at
# /usr/share/omarchy/shell/Commons/Style.qml.
set -uo pipefail

RUNNER=/usr/lib/qt6/bin/qmltestrunner
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STUBS="$ROOT/tests/qml/stubs"

if [[ ! -x "$RUNNER" ]]; then
    echo "SKIP: $RUNNER not found (install qt6-declarative)" >&2
    exit 0
fi

FAIL=0
shopt -s nullglob
for t in "$ROOT"/tests/qml/tst_*.qml; do
    # Run from quattro/ so relative directory imports (import "o10k")
    # resolve against the plugin source.
    if ! QT_QPA_PLATFORM=offscreen "$RUNNER" \
            -input "$t" -import "$STUBS" 2>&1 | tail -20; then
        FAIL=1
    fi
done

if (( FAIL )); then
    echo "QML tests FAILED"
    exit 1
fi
echo "QML tests passed"
```

Then: `chmod +x tests/qml/run.sh`

- [ ] **Step 4: Run test to verify it passes**

Run: `bash tests/qml/run.sh`
Expected: PASS — `Totals: 4 passed, 0 failed` and `QML tests passed`

- [ ] **Step 5: Commit**

```bash
git add tests/qml/
git commit -m "test: headless QML component harness with stubbed qs.Commons

The real qs.Commons cannot load under qmltestrunner (Border.qml needs the
Quickshell runtime), so components are tested against a stub mirroring the
real singleton's shape with stock-Omarchy defaults (cornerRadius = 0)."
```

---

### Task 2: `Fx.js` radius floor

**Files:**
- Create: `quattro/o10k/Fx.js`
- Test: `tests/fx_test.js`

**Interfaces:**
- Consumes: nothing.
- Produces: `Fx.radius(styleCornerRadius) -> Number` (always `>= Fx.RADIUS_FLOOR`), and the constant `Fx.RADIUS_FLOOR = 8`.

- [ ] **Step 1: Write the failing test**

Create `tests/fx_test.js`:

```javascript
#!/usr/bin/env node
// Unit tests for quattro/o10k/Fx.js. Loads the .pragma library source the
// same way tests/model_parity_test.js loads Model.js.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'o10k', 'Fx.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, '');
const Fx = new Function(src +
    '\n;return { RADIUS_FLOOR: RADIUS_FLOOR, radius: radius };')();

let failures = 0;
function check(desc, actual, expected) {
    if (actual !== expected) {
        console.error(`FAIL: ${desc}\n  expected ${expected}, got ${actual}`);
        failures++;
    }
}

// Stock Omarchy ships decoration:rounding = 0, so Style.cornerRadius is 0.
// Honoring it faithfully renders every card as a hard rectangle — the floor
// is the fix.
check('stock omarchy (0) is floored', Fx.radius(0), Fx.RADIUS_FLOOR);
check('below floor is raised', Fx.radius(3), Fx.RADIUS_FLOOR);
check('at floor is unchanged', Fx.radius(8), 8);
// A theme that asks for more rounding than the floor keeps its value —
// the floor is a minimum, not an override.
check('above floor is respected', Fx.radius(16), 16);
check('negative is treated as zero', Fx.radius(-4), Fx.RADIUS_FLOOR);
check('non-numeric is treated as zero', Fx.radius(undefined), Fx.RADIUS_FLOOR);
check('string number is coerced', Fx.radius('20'), 20);

if (failures > 0) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
}
console.log('Fx.js radius: all checks passed');
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/fx_test.js`
Expected: FAIL — `ENOENT: no such file or directory, open '.../quattro/o10k/Fx.js'`

- [ ] **Step 3: Write minimal implementation**

Create `quattro/o10k/Fx.js`:

```javascript
.pragma library

// Shared surface effects for the Omarchy10k Control Center.
//
// This library owns ONLY what omarchy-shell's Style singleton does not
// provide: a corner-radius floor and elevation parameters. Interactive
// state colors are NOT here — Style already exposes normalFill / hoverFill /
// selectedFill / pressedFill / focusFill, which themes can override via
// Style.styleOverrides. Defining parallel alphas would make our controls
// un-themeable and visually foreign.

// Style.cornerRadius mirrors Hyprland's decoration:rounding, which Omarchy
// ships at 0. Honoring it faithfully renders every surface as a hard
// rectangle, so the kit floors it. A theme asking for MORE rounding keeps
// its value — this is a minimum, not an override.
var RADIUS_FLOOR = 8;

function radius(styleCornerRadius) {
    var r = Number(styleCornerRadius);
    if (!isFinite(r) || r < 0)
        r = 0;
    return Math.max(r, RADIUS_FLOOR);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/fx_test.js`
Expected: PASS — `Fx.js radius: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Fx.js tests/fx_test.js
git commit -m "feat(quattro): Fx.js corner-radius floor

Style.cornerRadius mirrors Hyprland decoration:rounding, which Omarchy
ships at 0 — so every Control Center card currently renders as a hard
rectangle. The floor is a minimum, not an override: a theme asking for
more rounding keeps its value."
```

---

### Task 3: `Fx.js` elevation parameters

**Files:**
- Modify: `quattro/o10k/Fx.js`
- Test: `tests/fx_test.js`

**Interfaces:**
- Consumes: `Fx.RADIUS_FLOOR`, `Fx.radius` from Task 2.
- Produces: `Fx.elevation(level, shadowsEnabled) -> { blur, spread, offsetY, opacity }`, where `level` is `"flat" | "rest" | "raised"`. Unknown levels and `shadowsEnabled === false` both return the flat (all-zero, opacity 0) shape. Consumers feed these into a `RectangularShadow`.

- [ ] **Step 1: Write the failing test**

Append to `tests/fx_test.js`, immediately before the final `if (failures > 0)` block:

```javascript
const FxElev = new Function(src +
    '\n;return { elevation: elevation, ELEVATION: ELEVATION };')();

function checkShape(desc, obj) {
    for (const k of ['blur', 'spread', 'offsetY', 'opacity']) {
        if (typeof obj[k] !== 'number') {
            console.error(`FAIL: ${desc} — missing numeric "${k}"`);
            failures++;
        }
    }
}

checkShape('rest elevation shape', FxElev.elevation('rest', true));
checkShape('raised elevation shape', FxElev.elevation('raised', true));

// Raised surfaces must read as further from the background than resting
// ones, or elevation carries no information.
const rest = FxElev.elevation('rest', true);
const raised = FxElev.elevation('raised', true);
if (!(raised.blur > rest.blur && raised.offsetY > rest.offsetY)) {
    console.error('FAIL: raised must exceed rest in blur and offsetY');
    failures++;
}

// The accessibility escape hatch: shadows off must cost nothing to draw.
check('shadows disabled is transparent',
    FxElev.elevation('raised', false).opacity, 0);
check('flat is transparent', FxElev.elevation('flat', true).opacity, 0);
check('unknown level falls back to flat',
    FxElev.elevation('nonsense', true).opacity, 0);
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/fx_test.js`
Expected: FAIL — `ReferenceError: elevation is not defined`

- [ ] **Step 3: Write minimal implementation**

Append to `quattro/o10k/Fx.js`:

```javascript
// Elevation levels expressed as RectangularShadow parameters.
//
// RectangularShadow computes a rounded-rect falloff analytically in ONE
// quad. MultiEffect/DropShadow instead require layer.enabled on the
// shadowed item — an offscreen buffer per surface — plus a multi-tap blur.
// On the integrated-GPU target shared with the Spatial UX plugin that is
// the difference between free and not, so consumers must use
// RectangularShadow exclusively.
var ELEVATION = {
    flat:   { blur: 0,  spread: 0, offsetY: 0, opacity: 0.0 },
    rest:   { blur: 12, spread: 0, offsetY: 2, opacity: 0.18 },
    raised: { blur: 24, spread: 0, offsetY: 6, opacity: 0.28 }
};

function elevation(level, shadowsEnabled) {
    if (shadowsEnabled === false)
        return ELEVATION.flat;
    var e = ELEVATION[level];
    return e ? e : ELEVATION.flat;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/fx_test.js`
Expected: PASS — `Fx.js radius: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Fx.js tests/fx_test.js
git commit -m "feat(quattro): Fx.js elevation parameters

RectangularShadow-shaped values only. MultiEffect/DropShadow would need
layer.enabled — an offscreen buffer per surface — which the integrated-GPU
budget shared with the Spatial UX plugin does not have room for.
shadowsEnabled=false is the accessibility escape hatch."
```

---

### Task 4: `Motion.js` tokens

**Files:**
- Create: `quattro/o10k/Motion.js`
- Test: `tests/motion_test.js`

**Interfaces:**
- Consumes: nothing.
- Produces: `Motion.MICRO_MS = 90`, `SHORT_MS = 140`, `MEDIUM_MS = 220`, `LONG_MS = 360`, and `Motion.scaled(ms, speed) -> Number` (rounded, never negative; `speed = 0` yields `0` for reduced-motion).

- [ ] **Step 1: Write the failing test**

Create `tests/motion_test.js`:

```javascript
#!/usr/bin/env node
// Unit tests for quattro/o10k/Motion.js.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'o10k', 'Motion.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, '');
const M = new Function(src + '\n;return { MICRO_MS: MICRO_MS, ' +
    'SHORT_MS: SHORT_MS, MEDIUM_MS: MEDIUM_MS, LONG_MS: LONG_MS, ' +
    'scaled: scaled };')();

let failures = 0;
function check(desc, actual, expected) {
    if (actual !== expected) {
        console.error(`FAIL: ${desc}\n  expected ${expected}, got ${actual}`);
        failures++;
    }
}

// Values mirror ~/syncthing/OMPSpacialUX/lib/Motion.qml so both plugins in
// the omarchy-shell process read as one product. Changing them here without
// changing them there is the drift this test exists to catch.
check('micro matches Spatial UX', M.MICRO_MS, 90);
check('short matches Spatial UX', M.SHORT_MS, 140);
check('medium matches Spatial UX', M.MEDIUM_MS, 220);
check('long matches Spatial UX', M.LONG_MS, 360);

check('unscaled passes through', M.scaled(220, 1), 220);
check('half speed doubles nothing, halves duration', M.scaled(220, 0.5), 110);
// Reduced motion: animations collapse to instant rather than being skipped
// by every call site individually.
check('zero speed is instant', M.scaled(220, 0), 0);
check('negative speed clamps to instant', M.scaled(220, -2), 0);
check('missing speed defaults to 1x', M.scaled(140, undefined), 140);
check('result is rounded', M.scaled(90, 0.333), 30);

if (failures > 0) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
}
console.log('Motion.js: all checks passed');
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node tests/motion_test.js`
Expected: FAIL — `ENOENT: no such file or directory, open '.../quattro/o10k/Motion.js'`

- [ ] **Step 3: Write minimal implementation**

Create `quattro/o10k/Motion.js`:

```javascript
.pragma library

// Shared motion tokens for the Omarchy10k Control Center.
//
// Values are mirrored from the Omarchy Spatial UX plugin's lib/Motion.qml
// (~/syncthing/OMPSpacialUX). Both plugins load into the same omarchy-shell
// Quickshell process, so divergent timings read as two different apps
// sharing a desktop. Their singleton is plugin-private and cannot be
// imported, hence the mirror. tests/motion_test.js pins the values.
var MICRO_MS = 90;
var SHORT_MS = 140;
var MEDIUM_MS = 220;
var LONG_MS = 360;

// Every animation duration goes through scaled(), so a reduced-motion
// setting collapses the whole surface at once instead of relying on each
// call site to check.
function scaled(ms, speed) {
    var s = (speed === undefined || speed === null) ? 1 : Number(speed);
    if (!isFinite(s) || s < 0)
        s = 0;
    var v = Math.round(Number(ms) * s);
    return v > 0 ? v : 0;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node tests/motion_test.js`
Expected: PASS — `Motion.js: all checks passed`

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Motion.js tests/motion_test.js
git commit -m "feat(quattro): Motion.js duration tokens

Values mirrored from the Spatial UX plugin's lib/Motion.qml (90/140/220/360).
Both plugins share one omarchy-shell process, so divergent timings read as
two apps sharing a desktop. Their singleton is plugin-private, so the test
pins the values against drift."
```

---

### Task 5: `Card.qml` elevated surface

**Files:**
- Create: `quattro/o10k/Card.qml`
- Test: `tests/qml/tst_card.qml`

**Interfaces:**
- Consumes: `Fx.radius`, `Fx.elevation` (Tasks 2–3).
- Produces: `Card` — a `Rectangle` with `property string elevation: "rest"`, `property bool shadowsEnabled: true`, and `default property alias content: inner.data`. Radius is always floored; the shadow is a sibling `RectangularShadow`, never a layer effect.

- [ ] **Step 1: Write the failing test**

Create `tests/qml/tst_card.qml`:

```qml
import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "Card"

    Card {
        id: restCard
        width: 200; height: 80
    }

    Card {
        id: flatCard
        width: 200; height: 80
        elevation: "flat"
    }

    Card {
        id: noShadowCard
        width: 200; height: 80
        elevation: "raised"
        shadowsEnabled: false
    }

    // Stock Omarchy has Style.cornerRadius = 0; the card must still be
    // rounded or every Control Center surface reads as a hard rectangle.
    function test_radius_is_floored_on_stock_omarchy() {
        compare(restCard.radius, 8)
    }

    function test_default_elevation_is_rest() {
        compare(restCard.elevation, "rest")
        verify(restCard.shadowOpacity > 0)
    }

    function test_flat_elevation_draws_no_shadow() {
        compare(flatCard.shadowOpacity, 0)
    }

    // The accessibility escape hatch must actually remove the shadow.
    function test_shadows_disabled_draws_no_shadow() {
        compare(noShadowCard.shadowOpacity, 0)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash tests/qml/run.sh`
Expected: FAIL — `Card is not a type`

- [ ] **Step 3: Write minimal implementation**

Create `quattro/o10k/Card.qml`:

```qml
import QtQuick
import QtQuick.Effects
import qs.Commons
import "Fx.js" as Fx

// Elevated surface primitive for the Control Center.
//
// Deliberately UNBOUND (no pragma ComponentBehavior: Bound): bound inline
// components cannot be instantiated cross-file, which was the key finding
// of the C4 Panel decomposition. Consumers may be bound; this must not be.
Rectangle {
    id: card

    // "flat" | "rest" | "raised"
    property string elevation: "rest"
    // Accessibility escape hatch — shadows off costs nothing to draw.
    property bool shadowsEnabled: true

    // Resolved shadow parameters, exposed so tests and consumers can read
    // them without reaching into the RectangularShadow.
    readonly property var _elev: Fx.elevation(card.elevation, card.shadowsEnabled)
    readonly property real shadowOpacity: card._elev.opacity

    default property alias content: inner.data

    radius: Fx.radius(Style.cornerRadius)
    color: Style.normalFill

    // RectangularShadow computes the falloff analytically in one quad. A
    // MultiEffect/DropShadow here would require layer.enabled — an
    // offscreen buffer per card — which the shared integrated-GPU budget
    // does not have room for.
    RectangularShadow {
        anchors.fill: parent
        radius: card.radius
        blur: card._elev.blur
        spread: card._elev.spread
        offset.y: card._elev.offsetY
        color: Qt.rgba(0, 0, 0, card._elev.opacity)
        visible: card._elev.opacity > 0
        z: -1
    }

    Item {
        id: inner
        anchors.fill: parent
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash tests/qml/run.sh`
Expected: PASS — `Totals: 6 passed, 0 failed` for the Card case

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/Card.qml tests/qml/tst_card.qml
git commit -m "feat(quattro): o10k Card elevated surface

Floors the corner radius (stock Omarchy ships 0) and draws elevation with
RectangularShadow — one analytic quad, no offscreen buffer, no
layer.enabled. Unbound by design so it can be instantiated cross-file."
```

---

### Task 6: `SettingRow.qml`

**Files:**
- Create: `quattro/o10k/SettingRow.qml`
- Test: `tests/qml/tst_settingrow.qml`

**Interfaces:**
- Consumes: `Fx.radius` (Task 2), `Card` is NOT used here (rows are not elevated).
- Produces: `SettingRow` with `property string label`, `property var value`, `property var defaultValue`, `readonly property bool modified`, `signal resetRequested()`, and `default property alias control: controlSlot.data`. `modified` is `true` only when both `value` and `defaultValue` are defined and differ.

This is the load-bearing component: `isModified()` and `resetConfigKey()` exist in `Panel.qml` today but are applied by hand and inconsistently. As a component, every setting in both surfaces gets modified-ink and per-row reset for free.

- [ ] **Step 1: Write the failing test**

Create `tests/qml/tst_settingrow.qml`:

```qml
import QtQuick
import QtTest
import "../../quattro/o10k"

TestCase {
    name: "SettingRow"

    SettingRow {
        id: unchanged
        label: "Transient"
        value: true
        defaultValue: true
    }

    SettingRow {
        id: changed
        label: "Transient"
        value: false
        defaultValue: true
    }

    SettingRow {
        id: unknownDefault
        label: "Custom key"
        value: "anything"
        // defaultValue deliberately left undefined
    }

    SignalSpy {
        id: resetSpy
        target: changed
        signalName: "resetRequested"
    }

    function test_value_equal_to_default_is_not_modified() {
        compare(unchanged.modified, false)
    }

    function test_value_differing_from_default_is_modified() {
        compare(changed.modified, true)
    }

    // A key with no known default must never render as modified — that ink
    // would be a lie, and the reset chip would have nothing to reset to.
    function test_unknown_default_is_never_modified() {
        compare(unknownDefault.modified, false)
    }

    function test_reset_emits_signal() {
        resetSpy.clear()
        changed.requestReset()
        compare(resetSpy.count, 1)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash tests/qml/run.sh`
Expected: FAIL — `SettingRow is not a type`

- [ ] **Step 3: Write minimal implementation**

Create `quattro/o10k/SettingRow.qml`:

```qml
import QtQuick
import qs.Commons
import "Fx.js" as Fx

// One settings row: label, control slot, modified-vs-default ink, and a
// per-row reset affordance.
//
// Panel.qml already had isModified()/resetConfigKey(), but applied by hand
// per row and inconsistently. Making it a component is what stops the
// Studio's larger surface area from multiplying that inconsistency.
//
// Unbound by design (see Card.qml).
Item {
    id: row

    property string label: ""
    property var value: undefined
    property var defaultValue: undefined

    // Modified ink requires BOTH sides to be known. A key with no recorded
    // default is not "modified" — it is unknown, and claiming otherwise
    // would offer a reset with no target.
    readonly property bool modified:
        row.value !== undefined
        && row.defaultValue !== undefined
        && row.value !== row.defaultValue

    signal resetRequested()

    function requestReset() {
        row.resetRequested()
    }

    default property alias control: controlSlot.data

    implicitHeight: Math.max(labelText.implicitHeight, controlSlot.childrenRect.height)
                    + Style.space(8)
    implicitWidth: parent ? parent.width : 320

    // Modified ink: a 3px accent bar on the leading edge.
    Rectangle {
        id: ink
        width: 3
        height: parent.height
        radius: Fx.radius(0) / 4
        color: Color.accent
        visible: row.modified
        anchors.left: parent.left
    }

    Text {
        id: labelText
        anchors.left: ink.right
        anchors.leftMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        text: row.label
        color: Color.foreground
        font.family: Style.font.family
        font.pixelSize: Style.font.body
    }

    Item {
        id: controlSlot
        anchors.right: resetChip.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        width: childrenRect.width
        height: childrenRect.height
    }

    Rectangle {
        id: resetChip
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: row.modified ? resetLabel.implicitWidth + Style.space(10) : 0
        height: row.modified ? resetLabel.implicitHeight + Style.space(4) : 0
        radius: Fx.radius(Style.cornerRadius) / 2
        color: resetArea.containsMouse ? Style.hoverFill : Style.normalFill
        visible: row.modified

        Text {
            id: resetLabel
            anchors.centerIn: parent
            text: "↺"
            color: Color.muted
            font.family: Style.font.family
            font.pixelSize: Style.font.caption
        }

        MouseArea {
            id: resetArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: row.requestReset()
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `bash tests/qml/run.sh`
Expected: PASS — `Totals: 6 passed, 0 failed` for the SettingRow case

- [ ] **Step 5: Commit**

```bash
git add quattro/o10k/SettingRow.qml tests/qml/tst_settingrow.qml
git commit -m "feat(quattro): o10k SettingRow with modified ink and reset

Panel.qml had isModified()/resetConfigKey() applied by hand per row and
inconsistently. As a component every setting gets both for free. A key with
no recorded default is never 'modified' — that ink would offer a reset with
no target."
```

---

### Task 7: qmllint gate

**Files:**
- Create: `tests/qmllint.sh`

**Interfaces:**
- Consumes: nothing.
- Produces: `tests/qmllint.sh` — exits non-zero if any `quattro/o10k/*.qml` file produces an `unqualified` warning or any file produces an `Error:`. Prints the plugin-wide warning count as information only.

- [ ] **Step 1: Write the failing test**

Create `tests/qmllint.sh`:

```bash
#!/usr/bin/env bash
# Static QML gate.
#
# Two rules:
#   1. NO file may produce an Error:.
#   2. NO file in quattro/o10k/ may produce an [unqualified] warning.
#
# Rule 2 is scoped to the new kit deliberately: the plugin-wide baseline is
# 341 warnings / 0 errors, of which 134 are [unqualified] — the class that
# produced the wheelBoost id-collision bug (a property sharing its name with
# a handler id made every lookup resolve to the handler and the scroll step
# compute NaN). New code starts clean; the legacy backlog is reported, not
# gated, so this can land before the rewrite finishes.
set -uo pipefail

LINT=/usr/lib/qt6/bin/qmllint
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ ! -x "$LINT" ]]; then
    echo "SKIP: $LINT not found (install qt6-declarative)" >&2
    exit 0
fi

# qmllint resolves the qs.* modules only if a directory containing a `qs`
# entry pointing at the omarchy shell is on the import path.
SHIM="$(mktemp -d)"
trap 'rm -rf "$SHIM"' EXIT
ln -sfn /usr/share/omarchy/shell "$SHIM/qs"

FAIL=0
cd "$ROOT/quattro" || exit 1

for f in *.qml o10k/*.qml; do
    [[ -e "$f" ]] || continue
    out="$("$LINT" -I "$SHIM" "$f" 2>&1)"

    if grep -q "^Error:" <<<"$out"; then
        echo "ERROR in $f:"
        grep "^Error:" <<<"$out"
        FAIL=1
    fi

    if [[ "$f" == o10k/* ]]; then
        if grep -q "\[unqualified\]" <<<"$out"; then
            echo "UNQUALIFIED ACCESS in $f (not allowed in the o10k kit):"
            grep -B1 "\[unqualified\]" <<<"$out"
            FAIL=1
        fi
    fi
done

total=$(for f in *.qml o10k/*.qml; do
            [[ -e "$f" ]] && "$LINT" -I "$SHIM" "$f" 2>&1
        done | grep -c "^Warning:")
echo "qmllint: plugin-wide warnings = $total (informational; baseline 341)"

if (( FAIL )); then
    echo "qmllint gate FAILED"
    exit 1
fi
echo "qmllint gate passed"
```

Then: `chmod +x tests/qmllint.sh`

- [ ] **Step 2: Run it to confirm the gate is real**

Run: `bash tests/qmllint.sh`
Expected: PASS — `qmllint gate passed`, and an informational warning count.

Then deliberately break it to prove the gate bites. Temporarily append to `quattro/o10k/Card.qml` inside the `Rectangle`:

```qml
    Text { text: someUndeclaredThing }
```

Run: `bash tests/qmllint.sh`
Expected: FAIL — `UNQUALIFIED ACCESS in o10k/Card.qml`. **Remove the temporary line** and re-run to confirm it passes again.

- [ ] **Step 3: Commit**

```bash
git add tests/qmllint.sh
git commit -m "test: qmllint gate for the o10k kit

No file may error; no o10k/ file may have unqualified access. The
plugin-wide [unqualified] backlog (134 of 341 warnings) is reported but not
gated, so this lands before the rewrite finishes. That warning class is what
produced the wheelBoost id-collision bug."
```

---

### Task 8: Wire the gates into the test suite

**Files:**
- Modify: `tests/integration_test.sh` (append a new section before the final results block)

**Interfaces:**
- Consumes: `tests/qml/run.sh` (Task 1), `tests/fx_test.js` (Task 2), `tests/motion_test.js` (Task 4), `tests/qmllint.sh` (Task 7).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Find the insertion point**

Run: `grep -n "Model.js Parity\|^section\|Results:" tests/integration_test.sh | tail -10`
Expected: the `Model.js Parity` section and the final results block are printed with line numbers. Insert the new section immediately after the `Model.js Parity` section.

- [ ] **Step 2: Add the section**

Insert after the `Model.js Parity` section:

```bash
section "Control Center Kit"

if node "${SCRIPT_DIR}/fx_test.js" >/dev/null 2>&1; then
    pass "Fx.js unit tests"
else
    fail "Fx.js unit tests" "$(node "${SCRIPT_DIR}/fx_test.js" 2>&1 | tail -3)"
fi

if node "${SCRIPT_DIR}/motion_test.js" >/dev/null 2>&1; then
    pass "Motion.js unit tests"
else
    fail "Motion.js unit tests" "$(node "${SCRIPT_DIR}/motion_test.js" 2>&1 | tail -3)"
fi

if [[ -x /usr/lib/qt6/bin/qmltestrunner ]]; then
    if bash "${SCRIPT_DIR}/qml/run.sh" >/dev/null 2>&1; then
        pass "QML component tests"
    else
        fail "QML component tests" "$(bash "${SCRIPT_DIR}/qml/run.sh" 2>&1 | tail -5)"
    fi
else
    skip "QML component tests" "qmltestrunner not installed"
fi

if [[ -x /usr/lib/qt6/bin/qmllint ]]; then
    if bash "${SCRIPT_DIR}/qmllint.sh" >/dev/null 2>&1; then
        pass "qmllint gate"
    else
        fail "qmllint gate" "$(bash "${SCRIPT_DIR}/qmllint.sh" 2>&1 | tail -5)"
    fi
else
    skip "qmllint gate" "qmllint not installed"
fi

if command -v omarchy >/dev/null 2>&1; then
    if omarchy plugin validate "${SCRIPT_DIR}/../quattro" >/dev/null 2>&1; then
        pass "omarchy plugin manifest valid"
    else
        fail "omarchy plugin manifest valid" "schema validation failed"
    fi
else
    skip "omarchy plugin manifest valid" "omarchy CLI not available"
fi
```

**Note:** if `tests/integration_test.sh` does not already define `SCRIPT_DIR`, add near the top, after `set -uo pipefail`:

```bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
```

- [ ] **Step 3: Run the full suite**

Run: `bash tests/integration_test.sh 2>&1 | tail -25`
Expected: a `━━ Control Center Kit ━━` section with 5 passing checks, and the final `Results:` line showing 0 failed.

- [ ] **Step 4: Verify nothing else regressed**

Run: `cargo test 2>&1 | grep -E "test result"`
Expected: both crates report `0 failed`.

- [ ] **Step 5: Commit**

```bash
git add tests/integration_test.sh
git commit -m "test: run the o10k kit gates from the integration suite

Adds Fx.js/Motion.js unit tests, headless QML component tests, the qmllint
gate, and omarchy plugin manifest validation. Each degrades to a skip when
its tooling is absent so the suite still runs on a bare box."
```

---

## Self-review

**Spec coverage (Increment 1 only):** the spec's Increment 1 is "`o10k/` kit — `Fx`, `Motion`, `SettingRow`, first-party wrappers", done when "smoke harness instantiates every component headless; a demo tab renders them with rounded corners and elevation on a stock (`cornerRadius = 0`) install."

- `Fx` → Tasks 2–3. `Motion` → Task 4. `SettingRow` → Task 6. Smoke harness → Tasks 1, 5, 6, 8.
- **Gap accepted deliberately:** "first-party wrappers" (thin `Button`/`Toggle`/`Dropdown` wrappers) and the "demo tab" are NOT in this plan. Wrappers have no behavior of their own to test until a consumer exists, and a demo tab is a consumer — both belong with Increment 3 (`QuickPanel`), which is their first real caller. Writing them now would mean untested code with no caller, which is exactly what the spec's `PanelLooks` finding warns against. Increment 1's acceptance criterion is met by `Card` + `SettingRow` rendering correctly on a stock install.

**Placeholder scan:** no TBD/TODO; every code step contains complete, runnable content; every test step has an exact command and expected output.

**Type consistency:** `Fx.radius`, `Fx.elevation`, `Fx.RADIUS_FLOOR`, `Fx.ELEVATION` are defined in Tasks 2–3 and consumed with those exact names in Tasks 5–6. `Motion.scaled` / `MICRO_MS` / `SHORT_MS` / `MEDIUM_MS` / `LONG_MS` are defined in Task 4 and not yet consumed (first consumer is Increment 3). `Card.shadowOpacity` is produced in Task 5 and asserted in Task 5's own test. `SettingRow.modified` / `requestReset()` / `resetRequested()` are produced and asserted in Task 6.
