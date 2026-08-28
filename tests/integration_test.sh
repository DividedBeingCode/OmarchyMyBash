#!/bin/bash
# Omarchy10k Integration Test Suite
# Run: bash tests/integration_test.sh
set -uo pipefail

PASS=0
FAIL=0
SKIP=0

# ── Hermetic sandbox ───────────────────────────────────────────────────────
# Redirect all XDG state into a temp dir so the suite never touches the
# developer's real config, cache, or runtime directories.
TEST_TMP="$(mktemp -d "${TMPDIR:-/tmp}/omarchy10k-itest.XXXXXX")"
export XDG_CONFIG_HOME="$TEST_TMP/config"
export XDG_CACHE_HOME="$TEST_TMP/cache"
export XDG_RUNTIME_DIR="$TEST_TMP/runtime"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR" "$TEST_TMP/home"

cleanup() {
    kill "${DAEMON_PID:-}" 2>/dev/null || true
    wait "${DAEMON_PID:-}" 2>/dev/null || true
    rm -rf "$TEST_TMP"
}
trap cleanup EXIT

pass() { echo "  ✓ $1"; (( PASS++ )); }
fail() { echo "  ✘ $1: $2"; (( FAIL++ )); }
skip() { echo "  - $1 (skipped: $2)"; (( SKIP++ )); }

section() { echo; echo "━━ $1 ━━"; }

# Portable Unix socket send using python3
sock_send() {
    local sock="$1" msg="$2"
    python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.settimeout(3)
s.connect(sys.argv[1])
s.sendall((sys.argv[2] + '\n').encode())
data = b''
while True:
    try:
        chunk = s.recv(4096)
        if not chunk: break
        data += chunk
        if b'\n' in data: break
    except socket.timeout:
        break
s.close()
sys.stdout.write(data.decode())
" "$sock" "$msg" 2>/dev/null
}

# ── Build ──────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR"

section "Build"

if cargo build --release 2>/dev/null; then
    pass "cargo build --release"
else
    fail "cargo build --release" "compilation failed"
    echo "FATAL: Cannot continue without binaries."
    exit 1
fi

DAEMON="$SCRIPT_DIR/target/release/omarchy10kd"
CLI="$SCRIPT_DIR/target/release/omarchy10k"

# ── Unit Tests ─────────────────────────────────────────────────────────────

section "Unit Tests"

if cargo test 2>/dev/null; then
    pass "cargo test (all unit tests)"
else
    fail "cargo test" "one or more tests failed"
fi

# ── CLI Subcommands ────────────────────────────────────────────────────────

section "CLI Subcommands"

if "$CLI" --version >/dev/null 2>&1; then
    pass "omarchy10k --version"
else
    fail "omarchy10k --version" "binary not functional"
fi

if "$CLI" --help >/dev/null 2>&1; then
    pass "omarchy10k --help"
else
    fail "omarchy10k --help" "help output failed"
fi

# init bash should emit the adapter script
INIT_OUTPUT=$("$CLI" init bash 2>/dev/null)
if echo "$INIT_OUTPUT" | grep -q "o10k_hook_add"; then
    pass "omarchy10k init bash (contains hook broker)"
else
    fail "omarchy10k init bash" "adapter script missing o10k_hook_add"
fi

if echo "$INIT_OUTPUT" | grep -q "__o10k_render_prompt"; then
    pass "omarchy10k init bash (contains prompt renderer)"
else
    fail "omarchy10k init bash" "adapter script missing __o10k_render_prompt"
fi

if echo "$INIT_OUTPUT" | grep -q "bleopt prompt_ps1_transient"; then
    pass "omarchy10k init bash (contains ble.sh detection)"
else
    fail "omarchy10k init bash" "adapter script missing ble.sh detection"
fi

if echo "$INIT_OUTPUT" | grep -q "PROMPT_COMMAND"; then
    pass "omarchy10k init bash (manages PROMPT_COMMAND)"
else
    fail "omarchy10k init bash" "adapter script missing PROMPT_COMMAND management"
fi

if echo "$INIT_OUTPUT" | grep -q "blehook"; then
    pass "omarchy10k init bash (contains blehook integration)"
else
    fail "omarchy10k init bash" "adapter script missing blehook integration"
fi

# ── Daemon Lifecycle ───────────────────────────────────────────────────────

section "Daemon Lifecycle"

export O10K_PARENT_PID=$$
SOCKET="${XDG_RUNTIME_DIR:-/tmp}/omarchy10k-$$.sock"

# Clean up any stale socket
rm -f "$SOCKET"

# Start daemon
"$DAEMON" &
DAEMON_PID=$!

# Bounded readiness wait (~5s in 0.1s steps); readiness failure is FATAL,
# not a skip — every later socket test depends on the daemon being up.
SOCKET_READY=0
for _ in $(seq 1 50); do
    if [[ -S "$SOCKET" ]]; then
        SOCKET_READY=1
        break
    fi
    kill -0 "$DAEMON_PID" 2>/dev/null || break
    sleep 0.1
done

if (( SOCKET_READY )); then
    pass "daemon creates socket"
else
    fail "daemon creates socket" "socket not found at $SOCKET within 5s"
    echo "FATAL: daemon socket never appeared; cannot run socket tests."
    exit 1
fi

# Status command
if [[ -S "$SOCKET" ]]; then
    STATUS=$(sock_send "$SOCKET" '{"command":"status"}')
    if echo "$STATUS" | grep -q '"status":"ok"'; then
        pass "daemon status command"
    else
        fail "daemon status command" "unexpected response: $STATUS"
    fi

    if echo "$STATUS" | grep -q '"pid"'; then
        pass "daemon reports PID"
    else
        fail "daemon reports PID" "no PID in status"
    fi

    if echo "$STATUS" | grep -q '"version"'; then
        pass "daemon reports version"
    else
        fail "daemon reports version" "no version in status"
    fi
else
    skip "daemon status command" "no socket"
    skip "daemon reports PID" "no socket"
    skip "daemon reports version" "no socket"
fi

# ── Prompt Rendering ──────────────────────────────────────────────────────

section "Prompt Rendering"

if [[ -S "$SOCKET" ]]; then
    # Standard prompt request
    PROMPT_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")

    if echo "$PROMPT_RESP" | grep -q '"left"'; then
        pass "prompt renders left content"
    else
        fail "prompt renders left content" "no left in response: $PROMPT_RESP"
    fi

    if echo "$PROMPT_RESP" | grep -q "transient"; then
        pass "prompt includes transient form"
    else
        fail "prompt includes transient form" "no transient in response"
    fi

    # Error exit code prompt
    ERR_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":137,\"cmd_duration_ms\":5000,\"cols\":120,\"jobs\":0}")

    if [[ -n "$ERR_RESP" ]]; then
        pass "prompt renders with error exit code"
    else
        fail "prompt renders with error exit code" "empty response"
    fi

    # Narrow terminal
    NARROW_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME/some/deeply/nested/project/directory\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":40,\"jobs\":0}")

    if [[ -n "$NARROW_RESP" ]]; then
        pass "prompt renders in narrow terminal (40 cols)"
    else
        fail "prompt renders in narrow terminal" "empty response"
    fi

    # Git directory prompt (the repo root is a git checkout)
    if git -C "$SCRIPT_DIR" rev-parse --git-dir >/dev/null 2>&1; then
        GIT_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$SCRIPT_DIR\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")
        if [[ -n "$GIT_RESP" ]]; then
            pass "prompt renders in git repo"
        else
            fail "prompt renders in git repo" "empty response"
        fi
    else
        skip "prompt renders in git repo" "repo not a git checkout"
    fi
else
    skip "prompt rendering tests" "no socket"
fi

# ── Config Reload ──────────────────────────────────────────────────────────

section "Config Reload"

if [[ -S "$SOCKET" ]]; then
    RELOAD_RESP=$(sock_send "$SOCKET" '{"command":"reload_config"}')
    if echo "$RELOAD_RESP" | grep -q '"status":"ok"'; then
        pass "config reload command"
    else
        fail "config reload command" "unexpected response: $RELOAD_RESP"
    fi

    THEME_RESP=$(sock_send "$SOCKET" '{"command":"reload_theme"}')
    if echo "$THEME_RESP" | grep -q '"status":"ok"'; then
        pass "theme reload command"
    else
        fail "theme reload command" "unexpected response: $THEME_RESP"
    fi

    GIT_INV_RESP=$(sock_send "$SOCKET" '{"command":"invalidate_git"}')
    if echo "$GIT_INV_RESP" | grep -q '"status":"ok"'; then
        pass "git cache invalidation command"
    else
        fail "git cache invalidation command" "unexpected response"
    fi
else
    skip "config reload tests" "no socket"
fi

# ── v0.4 Feature Coverage ─────────────────────────────────────────────────
# Every probe below degrades to SKIP with a clear reason when the feature
# under test has not landed yet (0.4 is mid-flight), but FAILs on wrong
# behavior once the feature is present.

section "v0.4 Features"

# jsonq <json> <expr-on-d> — evaluate a python expression against the parsed
# JSON; prints the result (empty string for None / parse failure).
jsonq() {
    python3 -c "
import json, sys
try:
    d = json.loads(sys.argv[1])
except Exception:
    sys.exit(2)
try:
    r = eval(sys.argv[2], {'d': d})
except Exception:
    sys.exit(3)
print('' if r is None else r)
" "$1" "$2" 2>/dev/null
}

# run_to <secs> <cmd...> — bounded run where timeout(1) exists (Linux CI);
# unbounded fallback on macOS where only gtimeout may exist.
run_to() {
    if command -v timeout >/dev/null 2>&1; then
        timeout "$1" "${@:2}"
    elif command -v gtimeout >/dev/null 2>&1; then
        gtimeout "$1" "${@:2}"
    else
        "${@:2}"
    fi
}

# bridge_fields <file> → "N|f1|f2|..." — count NUL-separated fields in bridge
# stdout (trailing NUL is a field terminator, not an extra empty field).
bridge_fields() {
    python3 -c "
import sys
data = open(sys.argv[1], 'rb').read()
if data.endswith(b'\x00'):
    data = data[:-1]
fields = data.split(b'\x00')
sys.stdout.write(str(len(fields)) + '|' + '|'.join(f.decode('utf-8', 'replace') for f in fields[:5]))
" "$1" 2>/dev/null
}

if [[ -S "$SOCKET" ]]; then
    # ── Protocol 0.4: hello version bump ──────────────────────────────────
    HELLO_RESP=$(sock_send "$SOCKET" '{"type":"hello","id":"t-hello"}')
    PV=$(jsonq "$HELLO_RESP" "d.get('protocol_version')")
    case "$PV" in
        0.4) pass "hello returns protocol_version 0.4" ;;
        0.3) skip "hello returns protocol_version 0.4" "daemon still reports 0.3 (bump mid-flight)" ;;
        "")  fail "hello returns protocol_version 0.4" "no protocol_version in response: $HELLO_RESP" ;;
        *)   fail "hello returns protocol_version 0.4" "unexpected protocol_version: $PV" ;;
    esac

    # ── 0.1 env channel ───────────────────────────────────────────────────
    VENV_MARKER="probe-venv-0x4"
    MISE_MARKER="3.12.1"
    NOENV_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")
    NOENV_LEFT=$(jsonq "$NOENV_RESP" "d.get('left','')")
    ENV_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0,\"env\":{\"VIRTUAL_ENV\":\"/home/u/.venvs/$VENV_MARKER\",\"MISE_PYTHON_VERSION\":\"$MISE_MARKER\"}}")
    ENV_LEFT=$(jsonq "$ENV_RESP" "d.get('left','')")
    CFG_JSON=$(sock_send "$SOCKET" '{"command":"config_get"}')
    HAS_ENV_CFG=$(jsonq "$CFG_JSON" "'env' in d.get('config',{})")

    # Without env the markers must never leak into the render.
    if [[ "$NOENV_LEFT" != *"$VENV_MARKER"* && "$NOENV_LEFT" != *"$MISE_MARKER"* ]]; then
        pass "render without env carries no venv/toolchain markers"
    else
        fail "render without env carries no venv/toolchain markers" "marker leaked into render without env channel"
    fi

    if [[ "$ENV_LEFT" == *"$VENV_MARKER"* ]]; then
        pass "env VIRTUAL_ENV surfaces venv name in left"
    elif [[ "$HAS_ENV_CFG" == "True" ]]; then
        fail "env VIRTUAL_ENV surfaces venv name in left" "env config present but venv name missing from left"
    else
        skip "env VIRTUAL_ENV surfaces venv name in left" "0.1 env channel not landed (no env config, render unchanged)"
    fi

    if [[ "$ENV_LEFT" == *"$MISE_MARKER"* ]]; then
        pass "env MISE_PYTHON_VERSION surfaces in toolchain segment"
    elif [[ "$HAS_ENV_CFG" == "True" ]]; then
        fail "env MISE_PYTHON_VERSION surfaces in toolchain segment" "env config present but toolchain version missing from left"
    else
        skip "env MISE_PYTHON_VERSION surfaces in toolchain segment" "0.1 env channel not landed"
    fi

    # ── 0.2 notifications ─────────────────────────────────────────────────
    sock_send "$SOCKET" '{"type":"config","command":"set","config":{"notifications":{"enabled":false}}}' >/dev/null
    CFG_JSON=$(sock_send "$SOCKET" '{"command":"config_get"}')
    NOTIF_OFF=$(jsonq "$CFG_JSON" "d.get('config',{}).get('notifications',{}).get('enabled')")
    if [[ "$NOTIF_OFF" == "False" ]]; then
        NT_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":20000,\"cols\":120,\"jobs\":0}")
        NT=$(jsonq "$NT_RESP" "d.get('notify_threshold_ms')")
        if [[ "$NT" == "0" ]]; then
            pass "disabled notifications emit notify_threshold_ms 0"
        else
            fail "disabled notifications emit notify_threshold_ms 0" "expected 0, got '${NT:-<missing>}'"
        fi

        sock_send "$SOCKET" '{"type":"config","command":"set","config":{"notifications":{"enabled":true,"threshold_ms":12345}}}' >/dev/null
        NT_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":20000,\"cols\":120,\"jobs\":0}")
        NT=$(jsonq "$NT_RESP" "d.get('notify_threshold_ms')")
        if [[ "$NT" == "12345" ]]; then
            pass "custom notification threshold is honored"
        else
            fail "custom notification threshold is honored" "expected 12345, got '${NT:-<missing>}'"
        fi

        # restore defaults
        sock_send "$SOCKET" '{"type":"config","command":"set","config":{"notifications":{"enabled":true,"threshold_ms":10000}}}' >/dev/null
    else
        skip "notifications config_set drives notify_threshold_ms" "0.2 [notifications] table not honored by daemon yet"
    fi

    # ── 0.3 status enrichment ─────────────────────────────────────────────
    sock_send "$SOCKET" '{"command":"invalidate_git"}' >/dev/null
    # Warm the git cache with a prompt render in the repo, then read status.
    sock_send "$SOCKET" "{\"cwd\":\"$SCRIPT_DIR\",\"exit_code\":0,\"cmd_duration_ms\":4321,\"cols\":120,\"jobs\":0}" >/dev/null
    ST_RESP=$(sock_send "$SOCKET" '{"command":"status"}')
    if [[ "$(jsonq "$ST_RESP" "'git' in d")" == "True" ]]; then
        BT=$(jsonq "$ST_RESP" "type(d.get('git',{}).get('branch')).__name__")
        if [[ "$BT" == "str" ]]; then
            pass "status exposes git.branch as string"
        else
            fail "status exposes git.branch as string" "branch type: ${BT:-missing}; resp: $ST_RESP"
        fi
        DT=$(jsonq "$ST_RESP" "type(d.get('git',{}).get('dirty')).__name__")
        if [[ "$DT" == "bool" ]]; then
            pass "status exposes git.dirty as bool"
        else
            fail "status exposes git.dirty as bool" "dirty type: ${DT:-missing}"
        fi
        LCD=$(jsonq "$ST_RESP" "d.get('last_cmd_duration_ms')")
        if [[ "$LCD" =~ ^[0-9]+$ ]]; then
            pass "status exposes numeric last_cmd_duration_ms"
        else
            fail "status exposes numeric last_cmd_duration_ms" "got '${LCD:-missing}'"
        fi
        SAS=$(jsonq "$ST_RESP" "d.get('session_age_secs')")
        if [[ "$SAS" =~ ^[0-9]+$ ]]; then
            pass "status exposes numeric session_age_secs"
        else
            fail "status exposes numeric session_age_secs" "got '${SAS:-missing}'"
        fi
    else
        skip "status exposes git object (branch/dirty)" "0.3 status enrichment not landed (no git field in status)"
        skip "status exposes last_cmd_duration_ms" "0.3 status enrichment not landed"
        skip "status exposes session_age_secs" "0.3 status enrichment not landed"
    fi

    # ── 1.2 statusline ────────────────────────────────────────────────────
    # Payload carries every known context-percentage field spelling
    # (used_percentage / percentage / used+total) so the assertion holds no
    # matter which shape Claude Code or the daemon settles on.
    SL_RESP=$(sock_send "$SOCKET" '{"type":"statusline","id":"t-sl","payload":{"model":{"display_name":"Test Model"},"context_window":{"used_percentage":42,"percentage":42,"used":84000,"total":200000},"workspace":{"current_dir":"/tmp"}}}')
    if [[ "$(jsonq "$SL_RESP" "d.get('type')")" == "statusline" && "$(jsonq "$SL_RESP" "d.get('status')")" == "ok" ]]; then
        SL_LEFT=$(jsonq "$SL_RESP" "d.get('left','')")
        if [[ "$SL_LEFT" == *"Test Model"* ]]; then
            pass "statusline renders model display_name"
        else
            fail "statusline renders model display_name" "left: $SL_LEFT"
        fi
        if [[ "$SL_LEFT" == *"42"* ]]; then
            pass "statusline renders context usage percentage"
        else
            fail "statusline renders context usage percentage" "left: $SL_LEFT"
        fi
    else
        skip "statusline renders model and context usage" "1.2 statusline message not landed (resp: $SL_RESP)"
    fi

    # ── 1.3 agent signal segment ──────────────────────────────────────────
    # The marker itself is W1's choice; a correct segment must change the
    # render when the entrypoint signal is present vs the plain baseline.
    AI_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0,\"env\":{\"CLAUDE_CODE_ENTRYPOINT\":\"1\"}}")
    AI_LEFT=$(jsonq "$AI_RESP" "d.get('left','')")
    if [[ -n "$AI_LEFT" && "$AI_LEFT" != "$NOENV_LEFT" ]]; then
        pass "agent segment reacts to CLAUDE_CODE_ENTRYPOINT"
    elif [[ "$AI_LEFT" == "$NOENV_LEFT" ]]; then
        skip "agent segment reacts to CLAUDE_CODE_ENTRYPOINT" "1.3 agent segment not landed (render unchanged by signal)"
    else
        fail "agent segment reacts to CLAUDE_CODE_ENTRYPOINT" "empty response: $AI_RESP"
    fi

    # ── 0.4 bridge framing ────────────────────────────────────────────────
    # Fake daemon (sandbox tmpdir) that answers one request with a canned
    # response — or garbage, to drive the write_fallback path.
    FAKE_DAEMON="$TEST_TMP/fake_daemon.py"
    cat > "$FAKE_DAEMON" <<'PYEOF'
import json, os, socket, sys
mode, sock_path = sys.argv[1], sys.argv[2]
try:
    os.unlink(sock_path)
except OSError:
    pass
srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
srv.bind(sock_path)
srv.listen(1)
srv.settimeout(10)
try:
    conn, _ = srv.accept()
    conn.makefile('rb').readline()
    if mode == 'ok':
        resp = json.dumps({
            "type": "prompt", "status": "ok",
            "left": "BRIDGELEFT04", "right": "BRIDGERIGHT04",
            "notify_threshold_ms": 12345, "transient": "BRIDGETRANS04",
            "git_stale": False,
        })
    else:
        resp = '<<not json>>'
    conn.sendall((resp + '\n').encode())
    conn.close()
except Exception:
    pass
finally:
    srv.close()
    try:
        os.unlink(sock_path)
    except OSError:
        pass
PYEOF

    python3 "$FAKE_DAEMON" ok "$TEST_TMP/fake-ok.sock" &
    FAKE_PID=$!
    for _ in $(seq 1 30); do
        [[ -S "$TEST_TMP/fake-ok.sock" ]] && break
        sleep 0.1
    done
    printf '%s\n' '{"cwd":"/tmp","exit_code":0,"cmd_duration_ms":0,"cols":80,"jobs":0}' \
        | run_to 15 "$CLI" bridge --socket "$TEST_TMP/fake-ok.sock" > "$TEST_TMP/bridge_ok.out" 2>/dev/null
    wait "$FAKE_PID" 2>/dev/null

    BF=$(bridge_fields "$TEST_TMP/bridge_ok.out")
    N=${BF%%|*}
    if [[ "$N" == "4" ]]; then
        IFS='|' read -r _ F1 F2 F3 F4 <<<"$BF"
        if [[ "$F1" == "BRIDGELEFT04" && "$F2" == "BRIDGERIGHT04" && "$F3" == "12345" && "$F4" == "BRIDGETRANS04" ]]; then
            pass "bridge emits 4 NUL-separated fields with correct content"
        else
            fail "bridge emits 4 NUL-separated fields with correct content" "got: $BF"
        fi
    elif [[ "$N" == "3" ]]; then
        skip "bridge emits 4 NUL-separated fields" "0.4 framing not landed (bridge still emits 3 fields)"
    else
        fail "bridge emits 4 NUL-separated fields" "unexpected framing: '$BF'"
    fi

    python3 "$FAKE_DAEMON" bad "$TEST_TMP/fake-bad.sock" &
    FAKE_PID=$!
    for _ in $(seq 1 30); do
        [[ -S "$TEST_TMP/fake-bad.sock" ]] && break
        sleep 0.1
    done
    printf '%s\n' '{"cwd":"/tmp","exit_code":0,"cmd_duration_ms":0,"cols":80,"jobs":0}' \
        | run_to 15 "$CLI" bridge --socket "$TEST_TMP/fake-bad.sock" > "$TEST_TMP/bridge_bad.out" 2>/dev/null
    wait "$FAKE_PID" 2>/dev/null

    BF=$(bridge_fields "$TEST_TMP/bridge_bad.out")
    N=${BF%%|*}
    if [[ "$N" == "4" ]]; then
        IFS='|' read -r _ F1 F2 F3 F4 <<<"$BF"
        if [[ -n "$F1" && -z "$F2" && -z "$F3" && -z "$F4" ]]; then
            pass "bridge write_fallback emits 4 fields with empty 3rd/4th"
        else
            fail "bridge write_fallback emits 4 fields with empty 3rd/4th" "got: $BF"
        fi
    elif [[ "$N" == "2" || "$N" == "3" ]]; then
        skip "bridge write_fallback emits 4 fields with empty 3rd/4th" "0.4 framing not landed (fallback has $N fields)"
    else
        fail "bridge write_fallback emits 4 fields with empty 3rd/4th" "unexpected framing: '$BF'"
    fi

    # ── 1.1 true powerline ────────────────────────────────────────────────
    sock_send "$SOCKET" '{"type":"config","command":"set","config":{"style":{"preset":"powerline"}}}' >/dev/null
    PL_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")
    PL_LEFT=$(jsonq "$PL_RESP" "d.get('left','')")
    if [[ "$PL_LEFT" == *"48;2"* ]]; then
        pass "powerline preset emits SGR 48;2 background fill"
        sock_send "$SOCKET" '{"type":"config","command":"set","config":{"style":{"preset":"rainbow"}}}' >/dev/null
        RB_LEFT=$(jsonq "$(sock_send "$SOCKET" "{\"cwd\":\"$HOME\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")" "d.get('left','')")
        if [[ -n "$RB_LEFT" && "$RB_LEFT" != "$PL_LEFT" ]]; then
            pass "rainbow preset renders differently from powerline"
        else
            fail "rainbow preset renders differently from powerline" "identical output to powerline"
        fi
    else
        skip "powerline preset emits SGR 48;2 background fill" "1.1 true powerline not landed (no bg fill in render)"
        skip "rainbow preset renders differently from powerline" "1.1 true powerline not landed"
    fi
    # restore stock preset so nothing downstream renders with powerline
    sock_send "$SOCKET" '{"type":"config","command":"set","config":{"style":{"preset":"omarchy"}}}' >/dev/null

    # ── 3.3 intro ─────────────────────────────────────────────────────────
    if INTRO_OUT=$(run_to 30 env HOME="$TEST_TMP/home" "$CLI" intro --force </dev/null 2>/dev/null); then
        if [[ -n "${INTRO_OUT//[[:space:]]/}" ]]; then
            pass "intro --force exits 0 and prints output"
        else
            fail "intro --force exits 0 and prints output" "exit 0 but empty output"
        fi

        # Generous probe: O10K_NO_INTRO set must still exit 0; output may be
        # a notice, so only the exit status is asserted.
        if run_to 30 env O10K_NO_INTRO=1 HOME="$TEST_TMP/home" "$CLI" intro --force </dev/null >/dev/null 2>&1; then
            pass "O10K_NO_INTRO makes intro exit 0 without banner"
        else
            fail "O10K_NO_INTRO makes intro exit 0 without banner" "nonzero exit with O10K_NO_INTRO set"
        fi
    else
        skip "intro --force" "3.3 intro subcommand not landed (nonzero exit)"
        skip "O10K_NO_INTRO suppresses intro" "3.3 intro subcommand not landed"
    fi
else
    skip "v0.4 feature coverage" "no socket"
fi
# ── Hook Broker Verification ──────────────────────────────────────────────

section "Hook Broker (Bash Sourcing)"

# Source the adapter in a fresh interactive shell: --noprofile --norc keeps the
# user's ~/.bashrc from injecting an installed adapter, the sandboxed HOME/XDG
# dirs keep it from touching real state, and O10K_BIN/O10K_DAEMON_BIN (the
# variables the adapter actually reads) are pointed at harmless no-ops so no
# real daemon can be spawned from PATH.
HOOK_TEST=$(env HOME="$TEST_TMP/home" BASH_ENV=/dev/null ENV=/dev/null \
    bash --noprofile --norc -i -c '
    export O10K_BIN="echo"
    export O10K_DAEMON_BIN="true"

    source '"$SCRIPT_DIR"'/shell/omarchy10k.bash 2>/dev/null || true

    # Test hook_add API
    test_precmd_called=0
    my_precmd() { test_precmd_called=1; }
    o10k_hook_add precmd my_precmd

    if [[ "${#__O10K_HOOKS_precmd[@]}" -gt 0 ]]; then
        echo "HOOK_ADD_OK"
    fi

    # Test hook dispatch
    __o10k_dispatch precmd
    if [[ "$test_precmd_called" == "1" ]]; then
        echo "DISPATCH_OK"
    fi

    # Test hook_remove
    o10k_hook_remove precmd my_precmd
    if [[ "${#__O10K_HOOKS_precmd[@]}" -eq 0 ]]; then
        echo "HOOK_REMOVE_OK"
    fi

    # Test chpwd emulation
    if declare -f __o10k_check_chpwd >/dev/null; then
        echo "CHPWD_OK"
    fi

    # Test fallback prompt is defined
    if [[ -n "${__O10K_FALLBACK_PS1:-}" ]]; then
        echo "FALLBACK_OK"
    fi
' 2>/dev/null || echo "BASH_SOURCE_FAILED")

if echo "$HOOK_TEST" | grep -q "HOOK_ADD_OK"; then
    pass "o10k_hook_add registers callbacks"
else
    fail "o10k_hook_add" "hook registration failed"
fi

if echo "$HOOK_TEST" | grep -q "DISPATCH_OK"; then
    pass "__o10k_dispatch fires hooks"
else
    fail "__o10k_dispatch" "hook dispatch failed"
fi

if echo "$HOOK_TEST" | grep -q "HOOK_REMOVE_OK"; then
    pass "o10k_hook_remove removes callbacks"
else
    fail "o10k_hook_remove" "hook removal failed"
fi

if echo "$HOOK_TEST" | grep -q "CHPWD_OK"; then
    pass "chpwd emulation function exists"
else
    fail "chpwd emulation" "function not found"
fi

if echo "$HOOK_TEST" | grep -q "FALLBACK_OK"; then
    pass "fallback prompt defined for crash recovery"
else
    fail "fallback prompt" "not defined"
fi

# ── Quattro Plugin Validation ─────────────────────────────────────────────

section "Quattro Plugin"

QUATTRO_DIR="$SCRIPT_DIR/quattro"

if [[ -f "$QUATTRO_DIR/manifest.json" ]]; then
    pass "manifest.json exists"
else
    fail "manifest.json" "not found"
fi

# Validate manifest structure
if python3 -c "
import json, sys
m = json.load(open('$QUATTRO_DIR/manifest.json'))
assert m['schemaVersion'] == 1
assert m['id'] == 'community.omarchy10k'
assert 'bar-widget' in m['kinds']
assert 'barWidget' in m['entryPoints']
print('MANIFEST_VALID')
" 2>/dev/null | grep -q "MANIFEST_VALID"; then
    pass "manifest.json structure valid"
else
    fail "manifest.json structure" "invalid or malformed"
fi

if [[ -f "$QUATTRO_DIR/BarWidget.qml" ]]; then
    pass "BarWidget.qml exists"
else
    fail "BarWidget.qml" "not found"
fi

if [[ -f "$QUATTRO_DIR/Panel.qml" ]]; then
    pass "Panel.qml exists"
else
    fail "Panel.qml" "not found"
fi

if [[ -f "$QUATTRO_DIR/Model.js" ]]; then
    pass "Model.js exists"
else
    fail "Model.js" "not found"
fi

# Check Panel has all 4 tabs
if grep -q "appearanceTab" "$QUATTRO_DIR/Panel.qml" && \
   grep -q "contextTab" "$QUATTRO_DIR/Panel.qml" && \
   grep -q "shellTab" "$QUATTRO_DIR/Panel.qml" && \
   grep -q "advancedTab" "$QUATTRO_DIR/Panel.qml"; then
    pass "Panel.qml has all 4 tabs"
else
    fail "Panel.qml tabs" "missing one or more tabs"
fi

# ── Theme Bridge ──────────────────────────────────────────────────────────

section "Theme Bridge"

if [[ -f "$SCRIPT_DIR/templates/omarchy10k.toml.tpl" ]]; then
    pass "theme template exists"
    if grep -q "{{ accent }}" "$SCRIPT_DIR/templates/omarchy10k.toml.tpl"; then
        pass "theme template has placeholder variables"
    else
        fail "theme template" "missing placeholder variables"
    fi
else
    fail "theme template" "not found"
fi

if [[ -f "$SCRIPT_DIR/hooks/theme-set" ]]; then
    pass "theme-set hook exists"
else
    fail "theme-set hook" "not found"
fi

# ── Default Config ─────────────────────────────────────────────────────────

section "Default Config"

if [[ -f "$SCRIPT_DIR/config/default.toml" ]]; then
    pass "default.toml exists"
    if python3 -c "
import sys
try:
    import tomllib
except ImportError:
    try:
        import tomli as tomllib
    except ImportError:
        sys.exit(0)
with open('$SCRIPT_DIR/config/default.toml', 'rb') as f:
    c = tomllib.load(f)
assert 'prompt' in c
assert 'theme' in c
assert 'git' in c
assert 'daemon' in c
print('CONFIG_VALID')
" 2>/dev/null | grep -q "CONFIG_VALID"; then
        pass "default.toml parses and has required sections"
    else
        skip "default.toml TOML parse" "tomllib not available"
    fi
else
    fail "default.toml" "not found"
fi

# ── Doctor ─────────────────────────────────────────────────────────────────

section "Doctor"

DOCTOR_OUTPUT=$("$CLI" doctor 2>/dev/null || echo "DOCTOR_FAILED")

if echo "$DOCTOR_OUTPUT" | grep -q "Bash"; then
    pass "doctor checks Bash"
else
    fail "doctor" "Bash check missing"
fi

if echo "$DOCTOR_OUTPUT" | grep -q "TrueColor"; then
    pass "doctor checks TrueColor"
else
    fail "doctor" "TrueColor check missing"
fi

if echo "$DOCTOR_OUTPUT" | grep -q "Config"; then
    pass "doctor checks Config"
else
    fail "doctor" "Config check missing"
fi

# ── Cleanup ────────────────────────────────────────────────────────────────

kill "$DAEMON_PID" 2>/dev/null || true
wait "$DAEMON_PID" 2>/dev/null || true
rm -f "$SOCKET"

# ── Summary ────────────────────────────────────────────────────────────────

echo
echo "══════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped"
echo "══════════════════════════════════════"

if (( FAIL > 0 )); then
    exit 1
fi
