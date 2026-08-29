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
check('half speed halves duration', M.scaled(220, 0.5), 110);
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
