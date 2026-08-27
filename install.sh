#!/usr/bin/env bash
set -euo pipefail

# Omarchy10k Installer
# Usage: ./install.sh [--uninstall]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/omarchy10k"
PLUGIN_DIR="${HOME}/.config/omarchy/plugins/community.omarchy10k"
HOOK_DIR="${HOME}/.config/omarchy/hooks/theme-set.d"
BASHRC="${HOME}/.bashrc"
INIT_LINE='eval "$(omarchy10k init bash)"'

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

# ── Uninstall ────────────────────────────────────────────────────────────────

if [[ "${1:-}" == "--uninstall" ]]; then
    info "Uninstalling Omarchy10k..."

    rm -f "${BIN_DIR}/omarchy10k" "${BIN_DIR}/omarchy10kd" && ok "Removed binaries" || warn "Binaries not found"
    rm -rf "${PLUGIN_DIR}" && ok "Removed Quattro plugin" || warn "Plugin not found"
    rm -f "${HOOK_DIR}/omarchy10k" && ok "Removed theme hook" || warn "Hook not found"

    if [[ -f "$BASHRC" ]] && grep -qF "$INIT_LINE" "$BASHRC"; then
        sed -i "\|${INIT_LINE}|d" "$BASHRC"
        ok "Removed init line from .bashrc"
    else
        warn "Init line not found in .bashrc"
    fi

    info "Done. Open a new terminal to complete removal."
    exit 0
fi

# ── Install ──────────────────────────────────────────────────────────────────

printf "\n${C_BOLD}  OMARCHY10K INSTALLER${C_RESET}\n\n"

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
        cp "$src" "${BIN_DIR}/${bin}"
        chmod +x "${BIN_DIR}/${bin}"
        ok "${bin}"
    else
        fail "${bin} not found at ${src}"
    fi
done

# Ensure ~/.local/bin is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
    warn "${BIN_DIR} is not in your PATH."
    warn "Add this to your .bashrc:  export PATH=\"\${HOME}/.local/bin:\${PATH}\""
fi

# Step 3: Shell init
info "Configuring shell..."
if [[ -f "$BASHRC" ]] && grep -qF "$INIT_LINE" "$BASHRC"; then
    ok ".bashrc already configured"
else
    echo "" >> "$BASHRC"
    echo "# Omarchy10k shell prompt" >> "$BASHRC"
    echo "$INIT_LINE" >> "$BASHRC"
    ok "Added init line to .bashrc"
fi

# Step 4: Quattro plugin (optional)
if [[ -d "${SCRIPT_DIR}/quattro" ]]; then
    info "Installing Quattro Control Center plugin..."
    mkdir -p "$PLUGIN_DIR"
    cp -r "${SCRIPT_DIR}/quattro/"* "$PLUGIN_DIR/"
    ok "Plugin installed to ${PLUGIN_DIR}"
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

# Summary
printf "\n${C_GREEN}${C_BOLD}  Installation complete!${C_RESET}\n\n"
printf "  ${C_BOLD}Next steps:${C_RESET}\n"
printf "    1. Open a new terminal (or run: source ~/.bashrc)\n"
printf "    2. Run: ${C_BLUE}omarchy10k doctor${C_RESET} to verify\n"
printf "    3. Add the Omarchy10k widget to your Quattro bar\n"
printf "\n  To uninstall: ${C_BLUE}./install.sh --uninstall${C_RESET}\n\n"
