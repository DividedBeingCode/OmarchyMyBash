#!/bin/bash
set -euo pipefail

# Omarchy10k Installer
# Usage: ./install.sh [--uninstall] [--update]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/omarchy10k"
DATA_DIR="${HOME}/.local/share/omarchy10k"
PLUGIN_DIR="${HOME}/.config/omarchy/plugins/community.omarchy10k"
HOOK_DIR="${HOME}/.config/omarchy/hooks/theme-set.d"
TEMPLATE_DIR="${HOME}/.local/share/omarchy/templates"
BASHRC="${HOME}/.bashrc"
INIT_LINE='eval "$(omarchy10k init bash)"'
UPDATE_MODE=false

C_GREEN='\033[1;32m'
C_RED='\033[1;31m'
C_BLUE='\033[1;34m'
C_YELLOW='\033[1;33m'
C_BOLD='\033[1m'
C_RESET='\033[0m'

info()  { printf "${C_BLUE}[omarchy10k]${C_RESET} %s\n" "$*"; }
ok()    { printf "${C_GREEN}      ✓${C_RESET} %s\n" "$*"; }
warn()  { printf "${C_YELLOW}      ⚠${C_RESET} %s\n" "$*"; }
fail()  { printf "${C_RED}      ✘${C_RESET} %s\n" "$*"; exit 1; }

# ── Flag parsing ─────────────────────────────────────────────────────────────

case "${1:-}" in
    --uninstall)
        info "Uninstalling Omarchy10k..."

        rm -f "${BIN_DIR}/omarchy10k" "${BIN_DIR}/omarchy10kd" && ok "Removed binaries" || warn "Binaries not found"
        rm -rf "${PLUGIN_DIR}" && ok "Removed Quattro plugin" || warn "Plugin not found"
        rm -f "${HOOK_DIR}/omarchy10k" && ok "Removed theme hook" || warn "Hook not found"
        rm -f "${TEMPLATE_DIR}/omarchy10k.toml.tpl" && ok "Removed theme template" || true
        rm -rf "${DATA_DIR}" && ok "Removed data directory" || true

        if [[ -f "$BASHRC" ]] && grep -qF "$INIT_LINE" "$BASHRC"; then
            sed -i "\|${INIT_LINE}|d" "$BASHRC"
            ok "Removed init line from .bashrc"
        else
            warn "Init line not found in .bashrc"
        fi

        info "Done. Open a new terminal to complete removal."
        exit 0
        ;;
    --update)
        UPDATE_MODE=true
        ;;
esac

# ── Install ──────────────────────────────────────────────────────────────────

if [[ "$UPDATE_MODE" == true ]]; then
    printf "\n${C_BOLD}  OMARCHY10K UPDATE${C_RESET}\n\n"
else
    printf "\n${C_BOLD}  OMARCHY10K INSTALLER${C_RESET}\n\n"
fi

# Step 1: Build
if [[ -f "${SCRIPT_DIR}/Cargo.toml" ]]; then
    info "Building from source..."
    (cd "$SCRIPT_DIR" && cargo build --release 2>&1) || fail "Cargo build failed"
    ok "Build complete"
else
    fail "Cargo.toml not found in ${SCRIPT_DIR}. Run this script from the omarchy10k directory."
fi

# Step 2: Install binaries
info "Installing binaries to ${BIN_DIR}..."
mkdir -p "$BIN_DIR"

for bin in omarchy10k omarchy10kd; do
    src="${SCRIPT_DIR}/target/release/${bin}"
    if [[ -f "$src" ]]; then
        cp "$src" "${BIN_DIR}/.${bin}.tmp"
        chmod +x "${BIN_DIR}/.${bin}.tmp"
        mv "${BIN_DIR}/.${bin}.tmp" "${BIN_DIR}/${bin}"
        ok "${bin}"
    else
        fail "${bin} not found at ${src}"
    fi
done

# Record source directory breadcrumb
mkdir -p "${DATA_DIR}"
echo "$SCRIPT_DIR" > "${DATA_DIR}/source-dir"
ok "Source directory breadcrumb saved"

# Ensure ~/.local/bin is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
    warn "${BIN_DIR} is not in your PATH."
    warn "Add this to your .bashrc:  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi

# Step 2b: Claude Code statusline (merge, never overwrite)
CLAUDE_SETTINGS="${HOME}/.claude/settings.json"
STATUSLINE_JSON='{"type":"command","command":"omarchy10k statusline"}'

if [[ ! -d "${HOME}/.claude" ]]; then
    warn "Claude Code not detected (~/.claude absent) — skipping statusLine setup."
    warn "To enable later, merge into ~/.claude/settings.json:  \"statusLine\": ${STATUSLINE_JSON}"
else
    if [[ ! -f "$CLAUDE_SETTINGS" ]]; then
        printf '{\n  "statusLine": %s\n}\n' "$STATUSLINE_JSON" > "$CLAUDE_SETTINGS" \
            && ok "Created Claude Code settings.json with statusLine" \
            || warn "Could not write ${CLAUDE_SETTINGS}"
    elif command -v jq &>/dev/null; then
        if jq -e '.statusLine' "$CLAUDE_SETTINGS" &>/dev/null; then
            if jq -e '.statusLine.command == "omarchy10k statusline"' "$CLAUDE_SETTINGS" &>/dev/null; then
                ok "Claude Code statusLine already configured"
            else
                warn "Claude Code settings.json already has a different statusLine — left untouched"
            fi
        else
            if jq --argjson sl "$STATUSLINE_JSON" '.statusLine = $sl' \
                "$CLAUDE_SETTINGS" > "${CLAUDE_SETTINGS}.o10k.tmp" 2>/dev/null; then
                cp "$CLAUDE_SETTINGS" "${CLAUDE_SETTINGS}.o10k.bak"
                mv "${CLAUDE_SETTINGS}.o10k.tmp" "$CLAUDE_SETTINGS"
                ok "Merged statusLine into Claude Code settings.json (backup: settings.json.o10k.bak)"
            else
                warn "settings.json is not valid JSON — left untouched"
            fi
        fi
    else
        # jq-less best-effort: only append when the file's outermost braces
        # parse as a bare top-level object and statusLine is absent
        if grep -q '"statusLine"' "$CLAUDE_SETTINGS"; then
            ok "Claude Code settings.json already mentions statusLine — left untouched"
        elif [[ "$(head -c 1 "$CLAUDE_SETTINGS" | tr -d '[:space:]')" == "{" \
            && "$(tail -c 1 "$CLAUDE_SETTINGS" | tr -d '[:space:]')" == "}" ]]; then
            cp "$CLAUDE_SETTINGS" "${CLAUDE_SETTINGS}.o10k.bak"
            tmp="${CLAUDE_SETTINGS}.o10k.tmp"
            {
                printf '{\n  "statusLine": %s,' "$STATUSLINE_JSON"
                command sed '1s/^[[:space:]]*{//' "$CLAUDE_SETTINGS"
            } > "$tmp" 2>/dev/null \
                && python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$tmp" &>/dev/null \
                && mv "$tmp" "$CLAUDE_SETTINGS" \
                && ok "Merged statusLine into Claude Code settings.json (backup: settings.json.o10k.bak)" \
                || { rm -f "$tmp"; warn "Could not merge statusLine safely — left untouched"; }
        else
            warn "settings.json format unrecognized — left untouched"
        fi
    fi
fi

# Step 3: Shell init (skip on update -- already configured)
if [[ "$UPDATE_MODE" == false ]]; then
    info "Configuring shell..."
    if [[ -f "$BASHRC" ]] && grep -qF "$INIT_LINE" "$BASHRC"; then
        ok ".bashrc already configured"
    else
        echo "" >> "$BASHRC"
        echo "# Omarchy10k shell prompt" >> "$BASHRC"
        echo "$INIT_LINE" >> "$BASHRC"
        ok "Added init line to .bashrc"
    fi
fi

# Step 4: Quattro plugin (optional)
if [[ -d "${SCRIPT_DIR}/quattro" ]]; then
    info "Installing Quattro Control Center plugin..."
    mkdir -p "$PLUGIN_DIR"
    cp -r "${SCRIPT_DIR}/quattro/"* "$PLUGIN_DIR/"
    # Sync manifest.json version with Cargo.toml
    cargo_version=$(grep -m1 '^version' "${SCRIPT_DIR}/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
    if [[ -n "$cargo_version" && -f "${PLUGIN_DIR}/manifest.json" ]]; then
        # Patch only the FIRST "version" occurrence (the manifest's own, not a
        # later dependency's). GNU sed range form; degrade to a warning on error.
        if ! sed -i "0,/\"version\": *\"[^\"]*\"/s//\"version\": \"${cargo_version}\"/" "${PLUGIN_DIR}/manifest.json"; then
            warn "Could not sync manifest.json version with Cargo.toml"
        fi
    else
        warn "manifest.json not found; skipping version sync"
    fi
    ok "Plugin installed to ${PLUGIN_DIR}"
    if command -v omarchy-shell &>/dev/null; then
        omarchy-shell shell rescanPlugins 2>/dev/null && ok "Quattro plugin rescan triggered" || true
    fi
else
    warn "Quattro plugin directory not found; skipping"
fi

# Step 5: Theme hook (optional)
if [[ -f "${SCRIPT_DIR}/hooks/theme-set" ]]; then
    info "Installing theme-set hook..."
    mkdir -p "$HOOK_DIR"
    cp "${SCRIPT_DIR}/hooks/theme-set" "${HOOK_DIR}/omarchy10k"
    chmod +x "${HOOK_DIR}/omarchy10k"
    ok "Theme hook installed"
else
    warn "Theme hook not found; skipping"
fi

# Step 6: Theme bridge template (optional)
if [[ -f "${SCRIPT_DIR}/templates/omarchy10k.toml.tpl" ]]; then
    info "Installing theme bridge template..."
    mkdir -p "$TEMPLATE_DIR"
    cp "${SCRIPT_DIR}/templates/omarchy10k.toml.tpl" "${TEMPLATE_DIR}/omarchy10k.toml.tpl"
    ok "Template installed to ${TEMPLATE_DIR}"
else
    warn "Theme template not found; skipping"
fi

# Summary
if [[ "$UPDATE_MODE" == true ]]; then
    printf "\n${C_GREEN}${C_BOLD}  Update complete!${C_RESET}\n\n"
    printf "  New terminals will use the updated prompt automatically.\n"
    printf "  Running terminals will restart their daemon on next command.\n\n"
else
    printf "\n${C_GREEN}${C_BOLD}  Installation complete!${C_RESET}\n\n"
    printf "  ${C_BOLD}Next steps:${C_RESET}\n"
    printf "    1. Open a new terminal (or run: source ~/.bashrc)\n"
    printf "    2. Run: ${C_BLUE}omarchy10k doctor${C_RESET} to verify\n"
    if command -v omarchy-shell &>/dev/null; then
        printf "    3. Enable the bar widget: ${C_BLUE}omarchy plugin enable community.omarchy10k${C_RESET}\n"
    else
        printf "    3. (Optional) If using Omarchy Quattro, add the widget to your bar\n"
    fi
    printf "\n  To uninstall: ${C_BLUE}./install.sh --uninstall${C_RESET}\n"
    printf "  To update:    ${C_BLUE}omarchy10k update${C_RESET}  or  ${C_BLUE}./install.sh --update${C_RESET}\n\n"
fi
