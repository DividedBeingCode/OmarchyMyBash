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
    out="$(QT_QPA_PLATFORM=offscreen "$RUNNER" -input "$t" -import "$STUBS" 2>&1)"
    echo "$out" | tail -20
    grep -qE "^Totals: .* 0 failed" <<<"$out" || FAIL=1
done

if (( FAIL )); then
    echo "QML tests FAILED"
    exit 1
fi
echo "QML tests passed"
