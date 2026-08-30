#!/usr/bin/env node
// The StudioSystem plugin-list parser, against `omarchy10k plugin list`'s
// real output shapes.
'use strict';
const fs = require('fs');
const path = require('path');

// Extract the parser body from the QML so the test cannot drift from it.
const qml = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'StudioSystem.qml'), 'utf8');
const start = qml.indexOf('var out = []\n                var lines');
const end = qml.indexOf('systemTab.plugins = out');
if (start < 0 || end < 0) throw new Error('parser block not found in StudioSystem.qml');
const body = qml.slice(start, end);

const parse = new Function('text', `
    const _t = { text: text };
    ${body.replace(/String\(this\.text\)/g, '_t.text')}
    return out;
`);

let failures = 0;
function check(desc, actual, expected) {
    const a = JSON.stringify(actual), e = JSON.stringify(expected);
    if (a !== e) { console.error(`FAIL: ${desc}\n  expected ${e}\n  got      ${a}`); failures++; }
}

// Exactly what plugins_cli.rs prints when nothing is installed.
check('no plugins yields no rows',
    parse('no plugins installed — try: omarchy10k plugin add <git-url>\n'), []);

// The real shape with plugins present. The header line is the trap: splitting
// on whitespace made it a plugin named "plugins".
const real =
`plugins (/home/u/.config/omarchy10k/plugins):
  weather 1.2.0 [enabled] — current conditions (1 segment)
  moon 0.3.1 [disabled] — moon phase (2 segments)
`;
check('header is not a plugin',
    parse(real).map(p => p.name), ['weather', 'moon']);
check('enabled state comes from the bracket',
    parse(real).map(p => p.enabled), [true, false]);
check('version is captured',
    parse(real).map(p => p.version), ['1.2.0', '0.3.1']);

// Drift: enabled in config.toml but gone from disk. The line contains the
// word "enabled", which used to make it parse as an enabled plugin.
const drift =
`plugins (/home/u/.config/omarchy10k/plugins):
  weather 1.2.0 [enabled] — current conditions (1 segment)
  ghosted — enabled in config.toml but NOT installed
`;
check('missing plugin is not reported as enabled',
    parse(drift).map(p => [p.name, p.state]),
    [['weather', 'enabled'], ['ghosted', 'missing']]);

const invalid =
`plugins (/home/u/.config/omarchy10k/plugins):
  (invalid) /home/u/.config/omarchy10k/plugins/broken — plugin.toml unreadable or invalid
`;
check('an unreadable manifest is surfaced, not silently dropped',
    parse(invalid).map(p => p.state), ['invalid']);

check('blank input yields no rows', parse(''), []);
check('a description containing the word enabled does not confuse the state',
    parse('plugins (/x):\n  thing 1.0 [disabled] — can be enabled later\n').map(p => p.enabled),
    [false]);

if (failures) { console.error(`\n${failures} failure(s)`); process.exit(1); }
console.log('plugin list parser tests passed');
