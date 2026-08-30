#!/usr/bin/env bash
# End-to-end terminal integration, in REAL ghostty and foot windows.
#
# This is the test that would have caught every fault in the terminal
# integration work, and no unit test could have. All three were invisible to
# static reasoning:
#
#   * foot unsets TERM_PROGRAM and (under Omarchy) reports
#     TERM=xterm-256color, so it was identified as `unknown` and denied OSC 8,
#     OSC 52, sixel, undercurl and synchronised output.
#   * Ghostty sets GHOSTTY_SHELL_FEATURES whether or not its shell integration
#     is active, so the OSC 133;C/D gate always bailed.
#   * A first-byte timeout of 80ms was flaky against a cold-starting terminal.
#
# Requires a Wayland session and the terminal binaries; skips cleanly without
# them so the suite still runs on a headless machine or in CI.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADAPTER="$ROOT/shell/omarchy10k.bash"
FAIL=0

if [[ -z "${WAYLAND_DISPLAY:-}" ]]; then
    echo "SKIP: no WAYLAND_DISPLAY (terminal e2e needs a real display)" >&2
    exit 0
fi

WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT

# The probe needs an interactive shell; these harness shells are not one, so
# they set O10K_FORCE_PROBE. Everything else is the adapter's real code,
# extracted rather than reimplemented so the test cannot drift from it.
cat > "$WORK/inside.sh" <<'INNER'
#!/usr/bin/env bash
adapter="$1"; out="$2"
export O10K_FORCE_PROBE=1

# Terminal identification block, minus its trailing invocation.
sed -n '/^# ── Terminal Identification/,/^__o10k_resolve_terminal$/p' "$adapter" \
    | head -n -1 > "$out.ident"
source "$out.ident"
__o10k_resolve_terminal

# The OSC 133;C/D gate, with the state it depends on.
__O10K_SHELL_INTEGRATION="${O10K_SHELL_INTEGRATION:-auto}"
__O10K_SEMANTIC_PROMPTS=1     # as if [terminal.semantic_prompts] were enabled
__O10K_EMIT_133CD=0
sed -n '/^__o10k_update_133cd() {/,/^}$/p' "$adapter" > "$out.gate"
source "$out.gate"
__o10k_update_133cd

{
    printf 'O10K_TERM=%s\n' "${O10K_TERM:-}"
    printf 'O10K_TERM_VERSION=%s\n' "${O10K_TERM_VERSION:-}"
    printf 'EMIT_133CD=%s\n' "${__O10K_EMIT_133CD:-}"
    printf 'TERM=%s\n' "${TERM:-}"
    printf 'TERM_PROGRAM=%s\n' "${TERM_PROGRAM:-}"
} > "$out"
INNER
chmod +x "$WORK/inside.sh"

check() { # desc expected actual
    if [[ "$2" != "$3" ]]; then
        echo "FAIL: $1"
        echo "  expected [$2] got [$3]"
        FAIL=1
    else
        echo "  ok: $1"
    fi
}

field() { grep "^$2=" "$1" 2>/dev/null | head -1 | cut -d= -f2-; }

# ── foot ───────────────────────────────────────────────────────────────────
if command -v foot >/dev/null; then
    echo "foot:"
    out="$WORK/foot.txt"
    timeout 30 foot sh -c "$WORK/inside.sh '$ADAPTER' '$out'" >/dev/null 2>&1
    if [[ -s "$out" ]]; then
        # The headline regression. foot offers NO identifying environment
        # variable; only the XTVERSION probe can name it.
        check "identified as foot" "foot" "$(field "$out" O10K_TERM)"
        check "OSC 133;C/D enabled" "1" "$(field "$out" EMIT_133CD)"
        [[ -n "$(field "$out" O10K_TERM_VERSION)" ]] \
            && echo "  ok: version reported ($(field "$out" O10K_TERM_VERSION))" \
            || { echo "FAIL: no version from the probe"; FAIL=1; }
        # Documents WHY the probe is needed: if this ever stops being true,
        # env detection became viable and the probe could be reconsidered.
        [[ -z "$(field "$out" TERM_PROGRAM)" ]] \
            && echo "  ok: foot still sets no TERM_PROGRAM (probe justified)" \
            || echo "  note: foot now sets TERM_PROGRAM=$(field "$out" TERM_PROGRAM)"
    else
        echo "FAIL: foot produced no output"; FAIL=1
    fi
else
    echo "SKIP: foot not installed"
fi

# ── ghostty ────────────────────────────────────────────────────────────────
if command -v ghostty >/dev/null; then
    echo "ghostty:"
    out="$WORK/ghostty.txt"
    timeout 30 ghostty -e sh -c "$WORK/inside.sh '$ADAPTER' '$out'" >/dev/null 2>&1 &
    for _ in $(seq 1 30); do [[ -s "$out" ]] && break; sleep 0.5; done
    if [[ -s "$out" ]]; then
        check "identified as ghostty" "ghostty" "$(field "$out" O10K_TERM)"
        # The gate that GHOSTTY_SHELL_FEATURES used to suppress
        # unconditionally, since Ghostty sets it whether or not its own
        # integration is active.
        check "OSC 133;C/D enabled" "1" "$(field "$out" EMIT_133CD)"
    else
        echo "FAIL: ghostty produced no output"; FAIL=1
    fi
else
    echo "SKIP: ghostty not installed"
fi

if (( FAIL )); then
    echo "terminal e2e FAILED"
    exit 1
fi
echo "terminal e2e passed"
