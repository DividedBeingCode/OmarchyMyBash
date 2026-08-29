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
