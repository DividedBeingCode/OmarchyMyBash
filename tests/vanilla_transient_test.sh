#!/usr/bin/env bash
# Vanilla-bash transient + poor-man right-rail fixture tests (brainstorm B7).
#
# Standalone: bash tests/vanilla_transient_test.sh
# Requires only bash >= 4.4 — no daemon, no bridge, no ble.sh, no TTY.
#
# Strategy: source the real adapter with O10K_HARNESS_ONLY=1 (loads the
# function definitions and returns before any daemon/hook startup), then
# drive __o10k_emit_transient against a tiny in-memory terminal emulator
# and __o10k_apply_right_rail against exact-string expectations.

set -u

ADAPTER="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/shell/omarchy10k.bash"
PASS=0
FAIL=0
NL=$'\n'

fail() { printf 'FAIL: %s\n' "$1"; FAIL=$((FAIL + 1)); }
ok()   { PASS=$((PASS + 1)); }

check_eq() { # desc expected actual
    if [[ "$2" == "$3" ]]; then
        ok
    else
        fail "$1 | expected: $(printf '%q' "$2") | got: $(printf '%q' "$3")"
    fi
}

check_contains() { # desc needle haystack
    if [[ "$3" == *"$2"* ]]; then
        ok
    else
        fail "$1 | missing: $(printf '%q' "$2")"
    fi
}

check_not_contains() { # desc needle haystack
    if [[ "$3" != *"$2"* ]]; then
        ok
    else
        fail "$1 | unexpected: $(printf '%q' "$2")"
    fi
}

# Run a snippet inside a plain bash that has sourced the adapter.
run_adapter() {
    local snippet=$1
    {
        printf "O10K_HARNESS_ONLY=1 source '%s'\n" "$ADAPTER"
        printf 'COLUMNS=80\n'
        printf '%s\n' "$snippet"
    } | bash --noprofile --norc 2>/dev/null
}

# ── Minimal terminal emulator ──────────────────────────────────────────────
# Supports exactly what the transient emitter emits: relative moves \e[nA,
# \e[nB, CR, LF, EL (\e[K), OSC (skipped), other CSI (skipped), and plain
# printable overwrite at the cursor.
TERM_ROWS=()
TERM_ROW=0
TERM_COL=0
# A semicolon inside an unquoted =~ RHS is a parse error — keep it quoted.
csi_re='^[0-9;]*[a-zA-Z]'

term_init() { # rows...  cursor starts on the last row, column 0
    TERM_ROWS=("$@")
    TERM_ROW=$(( ${#TERM_ROWS[@]} - 1 ))
    TERM_COL=0
}

term_ensure() {
    while (( TERM_ROW >= ${#TERM_ROWS[@]} )); do
        TERM_ROWS+=("")
    done
}

term_apply() {
    local s=$1 rest="" line=""
    while [[ -n "$s" ]]; do
        if [[ "$s" == $'\e['* ]]; then
            rest="${s#*$'\e['}"
            if [[ "$rest" =~ ^([0-9]*)([AB]) ]]; then
                local mv=${BASH_REMATCH[1]:-1}
                if [[ "${BASH_REMATCH[2]}" == A ]]; then
                    TERM_ROW=$(( TERM_ROW - mv ))
                else
                    TERM_ROW=$(( TERM_ROW + mv ))
                fi
                (( TERM_ROW < 0 )) && TERM_ROW=0
                term_ensure
                s="${rest:${#BASH_REMATCH[0]}}"
            elif [[ "$rest" == K* ]]; then
                TERM_ROWS[TERM_ROW]="${TERM_ROWS[TERM_ROW]:0:TERM_COL}"
                s="${rest:1}"
            elif [[ "$rest" =~ $csi_re ]]; then
                s="${rest:${#BASH_REMATCH[0]}}"
            else
                s="${s:1}"
            fi
        elif [[ "$s" == $'\e]'* ]]; then
            rest="${s#*$'\e]'}"
            if [[ "$rest" == *$'\a'* ]]; then
                s="${rest#*$'\a'}"
            elif [[ "$rest" == *$'\e\\'* ]]; then
                s="${rest#*$'\e\\'}"
            else
                s=""
            fi
        elif [[ "$s" == $'\r'* ]]; then
            TERM_COL=0
            s="${s:1}"
        elif [[ "$s" == $'\n'* ]]; then
            TERM_ROW=$(( TERM_ROW + 1 ))
            term_ensure
            s="${s:1}"
        else
            # Printable run: the bracket class %%[...]* trick fails to cut
            # at ESC in [[ == ]] matching, so walk one char at a time.
            line="${TERM_ROWS[TERM_ROW]}"
            while [[ -n "$s" ]]; do
                case "${s:0:1}" in
                    $'\e' | $'\r' | $'\n') break ;;
                esac
                line="${line:0:TERM_COL}${s:0:1}${line:TERM_COL + 1}"
                TERM_ROWS[TERM_ROW]="$line"
                (( TERM_COL++ ))
                s="${s:1}"
            done
        fi
    done
}

# ── Shared fixtures ─────────────────────────────────────────────────────────
# Daemon-shaped strings: escapes wrapped in \x01..\x02 readline markers.
PROMPT_1LINE=$'\x01\E[1;34m\x02~/proj \x01\E[1;32m\x02❯ \x01\E[0m\x02'
PROMPT_FRAME=$'\x01\E[38;5;244m\x02╭─ \x01\E[0m\x02~/proj\n\x01\E[38;5;244m\x02╰─❯ \x01\E[0m\x02'
TRANSIENT=$'\x01\E]133;A\x07\x02❯ \x01\E]133;B\x07\x02'
RIGHT_MAIN=$'\x01\E[32m\x02main\x01\E[0m\x02'

# ═══ 1. Transient, single-line prompt: previous line replaced ══════════════
out=$(run_adapter "
PS1=\$'${PROMPT_1LINE//\\/\\\\}'
__O10K_TRANSIENT=\$'${TRANSIENT//\\/\\\\}'
__o10k_emit_transient 'echo hi'
")
term_init "~/proj ❯ echo hi" ""
term_apply "$out"
check_eq "1a previous line replaced with transient + command" "❯ echo hi" "${TERM_ROWS[0]}"
check_eq "1b cursor returned to next line col 0" "1:0" "$TERM_ROW:$TERM_COL"
check_contains "1c moves up one line" $'\E[1A\r' "$out"
check_contains "1d clears to end of line" $'\E[K' "$out"
check_not_contains "1e no readline markers leak" $'\x01' "$out"
check_not_contains "1f no STX leak" $'\x02' "$out"
[[ "${TERM_ROWS[0]}" != *"~/proj"* ]] && ok || fail "1g old prompt text erased"

# ═══ 2. Transient, framed (multiline) prompt: both lines collapse ══════════
out=$(run_adapter "
PS1=\$'${PROMPT_FRAME//\\/\\\\}'
__O10K_TRANSIENT=\$'${TRANSIENT//\\/\\\\}'
__o10k_emit_transient 'git status'
")
term_init "╭─ ~/proj" "╰─❯ git status" ""
term_apply "$out"
check_eq "2a frame top line erased" "" "${TERM_ROWS[0]}"
check_eq "2b input line replaced with transient + command" "❯ git status" "${TERM_ROWS[1]}"
check_eq "2c cursor below the collapsed prompt" "2:0" "$TERM_ROW:$TERM_COL"
check_contains "2d moves up two lines" $'\E[2A\r' "$out"
check_not_contains "2e no marker leak" $'\x01' "$out"

# ═══ 3. Transient, blank_line + frame (3 prompt lines) ════════════════════
out=$(run_adapter "
PS1=\$'\n${PROMPT_FRAME//\\/\\\\}'
__O10K_TRANSIENT=\$'${TRANSIENT//\\/\\\\}'
__o10k_emit_transient 'ls'
")
term_init "" "╭─ ~/proj" "╰─❯ ls" ""
term_apply "$out"
check_eq "3a all three prompt lines collapse" "❯ ls" "${TERM_ROWS[2]}"
check_eq "3b blank line above cleared" "" "${TERM_ROWS[1]}"
check_eq "3c top blank line stays empty" "" "${TERM_ROWS[0]}"
check_eq "3d cursor on the fresh line" "3:0" "$TERM_ROW:$TERM_COL"
check_contains "3e moves up three lines" $'\E[3A\r' "$out"

# ═══ 4. Oversized command falls back to a clean erase ═════════════════════
LONG_CMD=$(printf 'a%.0s' $(seq 1 200))
out=$(run_adapter "
PS1=\$'${PROMPT_1LINE//\\/\\\\}'
__O10K_TRANSIENT=\$'${TRANSIENT//\\/\\\\}'
__o10k_emit_transient '$LONG_CMD'
")
term_init "~/proj ❯ $LONG_CMD" ""
term_apply "$out"
check_eq "4a line reduced to bare transient char" "❯ " "${TERM_ROWS[0]}"
check_not_contains "4b long command not re-printed" "aaaa" "$out"

# ═══ 5. Empty transient emits nothing ═════════════════════════════════════
out=$(run_adapter "
PS1=\$'${PROMPT_1LINE//\\/\\\\}'
__O10K_TRANSIENT=''
__o10k_emit_transient 'echo hi'
")
check_eq "5a no output without a transient" "" "$out"

# ═══ 6. Right rail, two-line prompt: padded on the line above input ═══════
GAP33=$(printf '%*s' 33 '')
GAP36=$(printf '%*s' 36 '')
out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT='main'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "6a plain content right-aligned at col 40" "src${GAP33}main${NL}❯ " "$out"

out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT=\$'${RIGHT_MAIN//\\/\\\\}'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "6b wrapped rail content measured escape-aware" "src${GAP33}${RIGHT_MAIN}${NL}❯ " "$out"

out=$(run_adapter "
PS1=\$'\x01\E[1;34m\x02src\x01\E[0m\x02\n❯ '
__O10K_LAST_RIGHT=\$'${RIGHT_MAIN//\\/\\\\}'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "6c wrapped left line measured escape-aware" $'\x01\E[1;34m\x02src\x01\E[0m\x02'"${GAP33}${RIGHT_MAIN}${NL}❯ " "$out"

# ═══ 7. Right rail, single-line prompt: dedicated rail line above ═════════
out=$(run_adapter "
PS1='❯ '
__O10K_LAST_RIGHT='main'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "7a rail line above keeps input after left prompt" "${GAP36}main${NL}❯ " "$out"

# ═══ 8. blank_line + two-line: leading blank line preserved ═══════════════
out=$(run_adapter "
PS1=\$'\nsrc\n❯ '
__O10K_LAST_RIGHT='main'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "8a blank line survives, rail on content line" "${NL}src${GAP33}main${NL}❯ " "$out"

# ═══ 9. No room: rail dropped for the render, prompt untouched ════════════
X37=$(printf '%*s' 37 '' | tr ' ' 'x')
out=$(run_adapter "
PS1=\$'${X37}\n❯ '
__O10K_LAST_RIGHT='main'
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "9a unfitting rail leaves PS1 unchanged" "${X37}${NL}❯ " "$out"

# ═══ 10. ble.sh gate: rail never baked when ble.sh is present ═════════════
out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT='main'
BLE_VERSION=1.0
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "10a BLE_VERSION set → untouched" "src${NL}❯ " "$out"

out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT='main'
blehook() { :; }
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "10b blehook function present → untouched" "src${NL}❯ " "$out"

# ═══ 11. Empty right rail / zero cols: no-op ══════════════════════════════
out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT=''
__o10k_apply_right_rail 40
printf '%s' \"\$PS1\"
")
check_eq "11a empty rail → untouched" "src${NL}❯ " "$out"

out=$(run_adapter "
PS1=\$'src\n❯ '
__O10K_LAST_RIGHT='main'
__o10k_apply_right_rail 0
printf '%s' \"\$PS1\"
")
check_eq "11b cols=0 → untouched" "src${NL}❯ " "$out"

# ═══ 12. Width helper mirrors daemon rules ════════════════════════════════
out=$(run_adapter "
__o10k_prompt_visible_width \$'${PROMPT_1LINE//\\/\\\\}'
printf '%d' \$__O10K_WIDTH
")
check_eq "12a wrapped prompt fragment width" "9" "$out"

out=$(run_adapter "
__o10k_prompt_visible_width \$'\E[1;34mab\E[0m'
printf '%d' \$__O10K_WIDTH
")
check_eq "12b raw CSI stripped" "2" "$out"

out=$(run_adapter "
__o10k_prompt_visible_width \$'\E]2;title\aok'
printf '%d' \$__O10K_WIDTH
")
check_eq "12c raw OSC stripped" "2" "$out"

# ═══ Report ═══════════════════════════════════════════════════════════════
printf '%d passed, %d failed\n' "$PASS" "$FAIL"
(( FAIL == 0 ))
