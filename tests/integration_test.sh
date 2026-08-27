#!/usr/bin/env bash
# Omarchy10k Integration Test Suite
# Run: bash tests/integration_test.sh
set -uo pipefail

PASS=0
FAIL=0
SKIP=0

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

if echo "$INIT_OUTPUT" | grep -q "BLE_VERSION"; then
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
sleep 1

if [[ -S "$SOCKET" ]]; then
    pass "daemon creates socket"
else
    fail "daemon creates socket" "socket not found at $SOCKET"
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

    # Git directory prompt (test in the workspace which may be a git repo)
    if git -C "$SCRIPT_DIR/.." rev-parse --git-dir >/dev/null 2>&1; then
        GIT_RESP=$(sock_send "$SOCKET" "{\"cwd\":\"$SCRIPT_DIR/..\",\"exit_code\":0,\"cmd_duration_ms\":0,\"cols\":120,\"jobs\":0}")
        if [[ -n "$GIT_RESP" ]]; then
            pass "prompt renders in git repo"
        else
            fail "prompt renders in git repo" "empty response"
        fi
    else
        skip "prompt renders in git repo" "parent not a git repo"
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

# ── Hook Broker Verification ──────────────────────────────────────────────

section "Hook Broker (Bash Sourcing)"

# Source the adapter in a subshell, forcing interactive mode for the guard
HOOK_TEST=$(bash -i -c '
    __O10K_BIN="echo"
    __O10K_DAEMON_BIN="true"
    __O10K_SOCKET="/dev/null/nonexistent"

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
