#!/usr/bin/env bash
# Static QML gate.
#
# Three rules:
#   1. NO file may fail to PARSE.
#   2. NO file may produce an Error:.
#   3. NO file in quattro/o10k/ may produce an [unqualified] warning.
#
# Rule 1 is separate from rule 2 because qmllint reports a syntax error as
# `Warning: ... [syntax]`, NOT as `Error:`, and exits 255. A gate that greps
# only for `^Error:` therefore passes a file that does not parse -- which it
# did: StudioLooks.qml shipped a stray backslash that terminated a string
# early, cleared this gate, and only failed once Quickshell tried to load it.
# The exit code is the reliable signal; warning-only files exit 0.
#
# Rule 2 is scoped to new code deliberately: the plugin-wide baseline is
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
    rc=$?

    if (( rc != 0 )); then
        echo "PARSE FAILURE in $f (qmllint exit $rc):"
        grep -E "\[syntax\]|^Error:" <<<"$out" | head -5
        FAIL=1
    fi

    if grep -q "^Error:" <<<"$out"; then
        echo "ERROR in $f:"
        grep "^Error:" <<<"$out"
        FAIL=1
    fi

    # Files written against the o10k kit are held to the clean standard.
    # Studio.qml earned its place here immediately: the gate caught
    # `Fx.radius(...)` with no `import "o10k/Fx.js"`, which would have thrown
    # at runtime exactly like the Gallery headerHeightD bug.
    if [[ "$f" == o10k/* || "$f" == Studio*.qml ]]; then
        if grep -q "\[unqualified\]" <<<"$out"; then
            echo "UNQUALIFIED ACCESS in $f (not allowed in the o10k kit):"
            grep -B1 "\[unqualified\]" <<<"$out"
            FAIL=1
        fi
    fi
done

# Glyph escapes: QML `\uXXXX` takes exactly FOUR hex digits, so a Nerd Font
# codepoint above U+FFFF written as `\uf011b` silently becomes U+F011 plus a
# stray "b" — the wrong glyph and a trailing character, with no error anywhere.
# Above-BMP codepoints must use the ES6 brace form `\u{f011b}`.
if grep -rnoE '\\u[0-9a-fA-F]{5,6}' *.qml o10k/*.qml 2>/dev/null | grep -v 'u{' | grep -q .; then
    echo "BARE 5-DIGIT \\u ESCAPE (truncates to 4 digits + a stray char):"
    grep -rnoE '\\u[0-9a-fA-F]{5,6}' *.qml o10k/*.qml 2>/dev/null | grep -v 'u{'
    FAIL=1
fi

# The glyph browser's category chips are DERIVED from the catalog now, but
# the QML test's fixture is still hand-written. It used to invent a category
# ("Japan") that exists nowhere in the shipped catalog ("Japan / Geek"), so
# the test asserting the category filter works passed while the shipped chip
# matched nothing and hid 21 glyphs. A fixture that does not speak the real
# vocabulary tests the fixture. Every category string the product ships must
# appear in the test fixture.
TESTFIX="$ROOT/tests/qml/tst_glyphbrowser.qml"
while IFS= read -r cat; do
    if ! grep -qF "category: \"$cat\"" "$TESTFIX"; then
        echo "GLYPH CATEGORY \"$cat\" is shipped in StudioPrompt.qml but absent from"
        echo "  tests/qml/tst_glyphbrowser.qml -- the fixture has drifted from the catalog."
        FAIL=1
    fi
done < <(grep -o 'category: "[^"]*"' StudioPrompt.qml | sed 's/category: "//; s/"$//' | sort -u)

total=$(for f in *.qml o10k/*.qml; do
            [[ -e "$f" ]] && "$LINT" -I "$SHIM" "$f" 2>&1
        done | grep -c "^Warning:")
echo "qmllint: plugin-wide warnings = $total (informational; baseline 341)"

if (( FAIL )); then
    echo "qmllint gate FAILED"
    exit 1
fi
echo "qmllint gate passed"
