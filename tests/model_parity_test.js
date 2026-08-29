#!/usr/bin/env node
// Model.js CONFIG_MAP round-trip parity test (Z-Division rec #7).
// Every mapped config key must survive flattenConfig <-> unflattenPatch
// without being renamed, dropped, or clobbering siblings. This catches the
// class of bug where a panel toggle silently fails to update or overwrites
// the wrong TOML key — invisible to qmllint.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(path.join(__dirname, '..', 'quattro', 'Model.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, ''); // strip QML-only directive
const Model = new Function(src + '\n;return { CONFIG_MAP: typeof CONFIG_MAP !== "undefined" ? CONFIG_MAP : undefined, flattenConfig: typeof flattenConfig !== "undefined" ? flattenConfig : undefined, unflattenPatch: typeof unflattenPatch !== "undefined" ? unflattenPatch : undefined };')();

const CONFIG_MAP = Model.CONFIG_MAP;
if (!CONFIG_MAP || typeof CONFIG_MAP !== 'object')
    throw new Error('CONFIG_MAP missing or not an object');

let failures = 0;
const keys = Object.keys(CONFIG_MAP);
if (keys.length === 0) { console.error('CONFIG_MAP has no keys'); process.exit(1); }

// Sample values keyed by the QML property's expected type as seen in use:
// bool toggles use true, everything else a distinctive string.
function sampleFor(key) {
    if (/enabled|show|blank_line|newline|transient|unique|os_/.test(key)) return true;
    if (/threshold_ms|max_length|duration/.test(key)) return 42;
    return 'itest-' + key.replace(/\./g, '_');
}

for (const key of keys) {
    const flat = { [key]: sampleFor(key) };
    let nested, back;
    try {
        nested = Model.unflattenPatch(flat);
        back = Model.flattenConfig(nested);
    } catch (e) {
        console.error(`FAIL ${key}: round-trip threw: ${e.message}`);
        failures++;
        continue;
    }
    if (!(key in back)) {
        console.error(`FAIL ${key}: lost in round-trip (got ${JSON.stringify(Object.keys(back))})`);
        failures++;
        continue;
    }
    if (back[key] !== flat[key]) {
        console.error(`FAIL ${key}: value mutated ${JSON.stringify(flat[key])} -> ${JSON.stringify(back[key])}`);
        failures++;
    }
}

// Sibling safety: two keys sharing a table must not clobber each other.
const a = keys.find(k => k.includes('.')); const b = keys.filter(k => k.includes('.'))[1];
if (a && b) {
    const merged = Model.flattenConfig(Model.unflattenPatch({ [a]: 'A', [b]: 'B' }));
    if (merged[a] !== 'A' || merged[b] !== 'B') {
        console.error(`FAIL sibling clobber: ${a}/${b}`);
        failures++;
    }
}

// Shape contract: every mapped key must be a dotted (TOML-addressable) path
// and every value a distinct non-empty "cfg*" QML property name. Two TOML
// keys sharing one property would make panel edits clobber each other.
for (const key of keys) {
    if (!key.includes('.')) {
        console.error(`FAIL ${key}: not a dotted TOML path`);
        failures++;
    }
    const prop = CONFIG_MAP[key];
    if (typeof prop !== 'string' || !prop.startsWith('cfg')) {
        console.error(`FAIL ${key}: mapped property ${JSON.stringify(prop)} is not a "cfg*" name`);
        failures++;
    }
}
const seen = new Map();
for (const key of keys) {
    const prop = CONFIG_MAP[key];
    if (seen.has(prop) && seen.get(prop) !== key) {
        console.error(`FAIL ${prop}: shared by "${seen.get(prop)}" and "${key}"`);
        failures++;
    }
    seen.set(prop, key);
}

if (failures) { console.error(`${failures} of ${keys.length} CONFIG_MAP keys failed`); process.exit(1); }
console.log(`CONFIG_MAP parity: ${keys.length} keys round-trip clean`);
