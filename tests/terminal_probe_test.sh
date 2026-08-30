#!/usr/bin/env bash
# Terminal identification: the resolver's decision table.
#
# The probe itself needs a real terminal and is covered by the end-to-end
# test; this pins the logic AROUND it -- which branch fires, and that the
# probe is not attempted where it must not be.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAIL=0

# Source only the identification block, minus its trailing invocation.
BLOCK=$(mktemp); trap 'rm -f "$BLOCK"' EXIT
sed -n '/^# ── Terminal Identification/,/^__o10k_resolve_terminal$/p' \
    "$ROOT/shell/omarchy10k.bash" | head -n -1 > "$BLOCK"

check() { # desc expected actual
    if [[ "$2" != "$3" ]]; then
        echo "FAIL: $1"; echo "  expected [$2] got [$3]"; FAIL=1
    fi
}

# Run the resolver in a clean subshell with a controlled environment.
resolve() { # env assignments...
    (
        unset O10K_TERM O10K_TERM_VERSION GHOSTTY_RESOURCES_DIR KITTY_WINDOW_ID \
              TERM_PROGRAM TERM_PROGRAM_VERSION TMUX SSH_TTY SSH_CONNECTION \
              O10K_FORCE_PROBE
        export TERM=dumb
        for a in "$@"; do export "${a?}"; done
        source "$BLOCK"
        __o10k_resolve_terminal
        printf '%s' "${O10K_TERM:-<unset>}"
    )
}

check "explicit override wins over everything" \
    "kitty" "$(resolve O10K_TERM=kitty GHOSTTY_RESOURCES_DIR=/x TERM=xterm-ghostty)"

check "ghostty via GHOSTTY_RESOURCES_DIR" \
    "ghostty" "$(resolve GHOSTTY_RESOURCES_DIR=/usr/share/ghostty)"

check "ghostty via TERM" \
    "ghostty" "$(resolve TERM=xterm-ghostty)"

# foot's own terminfo, for installs that have not overridden `term`.
check "foot via TERM=foot" "foot" "$(resolve TERM=foot)"
check "foot via TERM=foot-extra" "foot" "$(resolve TERM=foot-extra)"

check "kitty via KITTY_WINDOW_ID" "kitty" "$(resolve KITTY_WINDOW_ID=1)"

check "TERM_PROGRAM is the last resort" \
    "wezterm" "$(resolve TERM_PROGRAM=wezterm)"

# The regression that started all of this: a real foot session looks exactly
# like this, and must NOT be silently mislabelled. With no tty to probe it is
# honestly `unknown` rather than a guess.
check "a foot-shaped environment with no tty degrades honestly" \
    "unknown" "$(resolve TERM=xterm-256color COLORTERM=truecolor)"

# The probe must not run under tmux: tmux answers as itself and interposes
# its own capability set.
check "no probe under tmux" \
    "unknown" "$(resolve TERM=xterm-256color TMUX=/tmp/tmux-1000/default,123,0 O10K_FORCE_PROBE=1)"

# Non-interactive shells never write escape sequences at a terminal.
check "no probe in a non-interactive shell" \
    "unknown" "$(resolve TERM=xterm-256color)"

# An override we have no profile for is honoured rather than ignored.
check "unknown override is honoured" \
    "somethingelse" "$(resolve O10K_TERM=somethingelse)"

if (( FAIL )); then echo "terminal probe tests FAILED"; exit 1; fi
echo "terminal probe tests passed"
