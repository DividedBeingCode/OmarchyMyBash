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
