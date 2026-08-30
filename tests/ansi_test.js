#!/usr/bin/env node
// Unit tests for Model.js's ANSI → rich-text renderer.
//
// The palette argument is the point: without it, every indexed color rendered
// in Tokyo Night hexes no matter which palette was actually being previewed.
'use strict';
const fs = require('fs');
const path = require('path');

const src = fs.readFileSync(
    path.join(__dirname, '..', 'quattro', 'Model.js'), 'utf8')
    .replace(/^\.pragma library.*$/m, '');
const M = new Function(src + '\n;return { ansiToRich, stripAnsi, stripLeadingBlankLines };')();

let failures = 0;
function check(desc, actual, expected) {
    if (actual !== expected) {
        console.error(`FAIL: ${desc}\n  expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
        failures++;
    }
}
function ok(desc, cond) { check(desc, !!cond, true); }

const ESC = '\x1b';
const GRUVBOX = {
    muted: '#a89984', red: '#fb4934', green: '#b8bb26', yellow: '#fabd2f',
    blue: '#83a598', magenta: '#d3869b', cyan: '#8ec07c', foreground: '#ebdbb2'
};

// ── The bug this fixes ─────────────────────────────────────────────────────

(() => {
    const seq = `${ESC}[32mmain${ESC}[0m`;
    const noPalette = M.ansiToRich(seq);
    const gruvbox = M.ansiToRich(seq, GRUVBOX);

    ok('without a palette, falls back to the built-in table',
        noPalette.includes('#9ece6a'));
    ok('with a palette, green resolves to THAT palette',
        gruvbox.includes('#b8bb26'));
    ok('the Tokyo Night hex is gone once a palette is supplied',
        !gruvbox.includes('#9ece6a'));
})();

(() => {
    // Bright variants (90-97) must resolve against the palette too.
    const out = M.ansiToRich(`${ESC}[91merror${ESC}[0m`, GRUVBOX);
    ok('bright red resolves against the palette', out.includes('#fb4934'));
})();

(() => {
    // 256-color indices below 16 are the same base-16 roles by another name.
    const out = M.ansiToRich(`${ESC}[38;5;4mbranch${ESC}[0m`, GRUVBOX);
    ok('xterm-256 index 4 resolves to the palette blue', out.includes('#83a598'));
})();

(() => {
    // Index 0 is "black". Resolving it to the background would paint prompt
    // text in the background color -- i.e. invisibly.
    const out = M.ansiToRich(`${ESC}[30mdim${ESC}[0m`, GRUVBOX);
    ok('index 0 maps to muted, not background', out.includes('#a89984'));
})();

(() => {
    // A palette missing a role must not blank the color out.
    const out = M.ansiToRich(`${ESC}[35mx${ESC}[0m`, { red: '#ff0000' });
    ok('a partial palette falls back per-role', out.includes('#bb9af7'));
})();

// ── Truecolor and structure are unaffected ─────────────────────────────────

(() => {
    // The daemon emits truecolor for themed roles; those are absolute and
    // must NOT be remapped by the palette.
    const out = M.ansiToRich(`${ESC}[38;2;255;0;128mx${ESC}[0m`, GRUVBOX);
    ok('truecolor passes through untouched', out.includes('#ff0080'));
})();

(() => {
    const out = M.ansiToRich(`${ESC}[1mbold${ESC}[22mplain`, GRUVBOX);
    ok('bold is emitted', out.includes('font-weight:bold'));
    ok('bold is closed', out.includes('</span>'));
})();

(() => {
    const out = M.ansiToRich('a <b> & c', GRUVBOX);
    ok('markup in prompt text is escaped', out.includes('&lt;b&gt;'));
    ok('ampersand is escaped', out.includes('&amp;'));
})();

(() => {
    // The daemon wraps the prompt in OSC title sequences and readline
    // non-printing markers; both must vanish rather than render.
    const out = M.ansiToRich(`${ESC}]2;title\x07\x01${ESC}[32mx\x02`, GRUVBOX);
    ok('OSC title is dropped', !out.includes('title'));
    ok('readline markers are dropped', !out.includes('\x01') && !out.includes('\x02'));
})();

check('empty input yields empty output', M.ansiToRich('', GRUVBOX), '');
check('null input yields empty output', M.ansiToRich(null, GRUVBOX), '');

(() => {
    // A malformed sequence must not swallow the rest of the line silently in
    // a way that differs with and without a palette.
    const a = M.ansiToRich(`${ESC}[38;5mx`, GRUVBOX);
    ok('a malformed sequence still returns a string', typeof a === 'string');
})();

// Two-line prompts. StyledText collapses whitespace like HTML, so a raw \n
// renders as a SPACE and the second line runs onto the first -- which is what
// made every card read "~/app git: main ╰─❯" and then clip.
(() => {
    const out = M.ansiToRich(`~/app${'\n'}> `);
    ok('a newline becomes a real line break', out.includes('<br/>'));
    ok('the raw newline is gone', !/[^>]\n/.test(out));
})();

(() => {
    const out = M.ansiToRich(`a${'\r\n'}b`);
    ok('CRLF is handled too', out.includes('<br/>') && !out.includes('\r'));
})();

(() => {
    // A prompt with no newline must not gain a break.
    ok('single-line prompts are untouched', !M.ansiToRich('~/app > ').includes('<br'));
})();

// prompt.blank_line puts an empty line before every prompt. Right in a
// terminal; in a two-line-tall card it consumed the line the "╰─❯" needed.
(() => {
    check('a leading blank line is dropped',
        M.stripLeadingBlankLines('\n~/app\n> '), '~/app\n> ');
    check('several are dropped',
        M.stripLeadingBlankLines('\n\n\n~/app'), '~/app');
    check('interior blank lines are kept',
        M.stripLeadingBlankLines('~/app\n\n> '), '~/app\n\n> ');
    check('a prompt with no blank line is untouched',
        M.stripLeadingBlankLines('~/app'), '~/app');
    check('empty input is safe', M.stripLeadingBlankLines(''), '');
    check('null input is safe', M.stripLeadingBlankLines(null), '');
})();

if (failures) {
    console.error(`\n${failures} failure(s)`);
    process.exit(1);
}
console.log('Model.js ANSI tests passed');
