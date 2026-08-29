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
TERMINAL_STATIC_GHOSTTY="${CONFIG_DIR}/ghostty.conf"
GHOSTTY_CONFIG="${HOME}/.config/ghostty/config"
FOOT_CONFIG="${HOME}/.config/foot/foot.ini"
GHOSTTY_STATIC_LINE='config-file = ?"~/.config/omarchy10k/ghostty.conf"'
GHOSTTY_THEME_LINE='config-file = ?"~/.local/state/omarchy/current/theme/o10k-ghostty.conf"'
FOOT_THEME_LINE='include=~/.local/state/omarchy/current/theme/o10k-foot.ini'
FOOT_OMARCHY_ANCHOR='include=~/.local/state/omarchy/current/theme/foot.ini'
O10K_FOOT_MARKER='# omarchy10k terminal include'
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
        if command -v omarchy-shell &>/dev/null; then
            # A rescan re-reads the plugin LIST but does not invalidate QML's
        # component cache, so changed .qml files keep serving the old code
        # (verified: an edited PanelLooks.qml still rendered its previous
        # content until the shell restarted). Restart when we can, and say
        # so plainly when we cannot.
        if command -v omarchy >/dev/null 2>&1 && omarchy restart shell >/dev/null 2>&1; then
            ok "Quattro shell restarted (picks up changed QML)"
        elif omarchy-shell shell rescanPlugins 2>/dev/null; then
            warn "Plugin rescanned, but changed QML needs: omarchy restart shell"
        fi
            warn "If the widget was enabled, also remove it from Setup > Plugins (or ~/.config/omarchy/shell.json)."
        fi
        for HOOK_EVENT in theme-set battery-low post-update font-set; do rm -f "${HOME}/.config/omarchy/hooks/${HOOK_EVENT}.d/omarchy10k"; done && ok "Removed hooks" || warn "Hooks not found"
        rm -f "${TEMPLATE_DIR}/omarchy10k.toml.tpl" && ok "Removed theme template" || true
        rm -rf "${DATA_DIR}" && ok "Removed data directory" || true

        if [[ -f "$BASHRC" ]] && grep -qF "$INIT_LINE" "$BASHRC"; then
            sed -i "\|${INIT_LINE}|d" "$BASHRC"
            ok "Removed init line from .bashrc"
        else
            warn "Init line not found in .bashrc"
        fi

        rm -f "${HOME}/.config/omarchy/themed/"o10k-*.tpl && ok "Removed theme rice templates" || true
        rm -f "${CONFIG_DIR}/tools.sh" && ok "Removed modern CLI layer" || true
        # Terminal include layer: static personality file + appended include lines.
        rm -f "${TERMINAL_STATIC_GHOSTTY}" && ok "Removed static ghostty personality file" || true
        if [[ -f "$GHOSTTY_CONFIG" ]] && grep -qF 'omarchy10k' "$GHOSTTY_CONFIG"; then
            sed -i '\|config-file = ?"~/.config/omarchy10k/ghostty\.conf"|d' "$GHOSTTY_CONFIG"
            sed -i '\|config-file = ?"~/.local/state/omarchy/current/theme/o10k-ghostty\.conf"|d' "$GHOSTTY_CONFIG"
            ok "Removed o10k includes from ${GHOSTTY_CONFIG}"
        fi
        if [[ -f "$FOOT_CONFIG" ]] && grep -qF 'o10k-foot.ini' "$FOOT_CONFIG"; then
            sed -i '\|include=.*o10k-foot\.ini|d' "$FOOT_CONFIG"
            sed -i '\|omarchy10k terminal include|d' "$FOOT_CONFIG"
            ok "Removed o10k include from ${FOOT_CONFIG}"
        fi
        rm -f "${HOME}/.local/state/omarchy/current/theme/o10k-ghostty.conf" \
              "${HOME}/.local/state/omarchy/current/theme/o10k-foot.ini" && true
        for link in "${HOME}/.config/yazi/theme.toml" "${HOME}/.config/cava/config"; do
            if [[ -L "$link" ]] && [[ "$(readlink "$link")" == *"/current/theme/o10k-"* ]]; then
                rm -f "$link" && ok "Removed rice link: ${link}"
            fi
        done
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

# Step 1: Dependencies
if ! command -v cargo &>/dev/null; then
    if command -v omarchy-pkg-add &>/dev/null; then
        fail "Rust toolchain missing. Install it with:  omarchy-pkg-add rust  then re-run this installer."
    else
        fail "cargo not found. Install Rust (https://rustup.rs) and re-run this installer."
    fi
fi
if ! command -v git &>/dev/null; then
    if command -v omarchy-pkg-add &>/dev/null; then
        fail "git missing. Install it with:  omarchy-pkg-add git  then re-run this installer."
    else
        fail "git not found. Install git and re-run this installer."
    fi
fi

# Step 1b: Modern CLI tools (interactive replacements for Unix defaults).
# All live in the official repos. Skippable with O10K_SKIP_TOOLS=1.
if [[ "${O10K_SKIP_TOOLS:-0}" == "1" ]]; then
    warn "Skipping modern CLI tools (O10K_SKIP_TOOLS=1)"
else
    info "Ensuring modern CLI tools are installed..."
    TOOL_PKGS=(eza bat zoxide fzf btop fd ripgrep tldr dust duf procs yazi atuin)
    missing=()
    for pkg in "${TOOL_PKGS[@]}"; do
        pacman -Q "$pkg" &>/dev/null || missing+=("$pkg")
    done
    if (( ${#missing[@]} == 0 )); then
        ok "All modern CLI tools present"
    else
        if command -v omarchy-pkg-add &>/dev/null; then
            omarchy-pkg-add "${missing[@]}" && ok "Installed: ${missing[*]}" \
                || warn "Could not install some tools: ${missing[*]} (aliases for them are skipped at runtime)"
        else
            warn "omarchy-pkg-add not available; install manually: pacman -S --needed ${missing[*]}"
        fi
    fi
fi

# Step 2: Build
info "Building from source..."
(cd "$SCRIPT_DIR" && cargo build --release 2>&1) || fail "Cargo build failed"
ok "Build complete"

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
        # A rescan re-reads the plugin LIST but does not invalidate QML's
        # component cache, so changed .qml files keep serving the old code
        # (verified: an edited PanelLooks.qml still rendered its previous
        # content until the shell restarted). Restart when we can, and say
        # so plainly when we cannot.
        if command -v omarchy >/dev/null 2>&1 && omarchy restart shell >/dev/null 2>&1; then
            ok "Quattro shell restarted (picks up changed QML)"
        elif omarchy-shell shell rescanPlugins 2>/dev/null; then
            warn "Plugin rescanned, but changed QML needs: omarchy restart shell"
        fi
    fi
else
    warn "Quattro plugin directory not found; skipping"
fi

# Step 5: Desktop hooks (optional) — theme fan-out + battery-low, post-update,
# and font-set reactions. Each installs as a drop-in under its event dir.
for HOOK_EVENT in theme-set battery-low post-update font-set; do
    if [[ -f "${SCRIPT_DIR}/hooks/${HOOK_EVENT}" ]]; then
        EVENT_DIR="${HOME}/.config/omarchy/hooks/${HOOK_EVENT}.d"
        # Prefer Omarchy's own installer so the hook lands wherever the
        # platform decides hooks live; fall back to the manual drop-in on
        # older Omarchy versions that lack the command.
        if command -v omarchy >/dev/null 2>&1 &&
           omarchy hook install "$HOOK_EVENT" "${SCRIPT_DIR}/hooks/${HOOK_EVENT}" >/dev/null 2>&1
        then
            ok "${HOOK_EVENT} hook installed (omarchy hook install)"
        else
            mkdir -p "$EVENT_DIR"
            cp "${SCRIPT_DIR}/hooks/${HOOK_EVENT}" "${EVENT_DIR}/omarchy10k"
            chmod +x "${EVENT_DIR}/omarchy10k"
            ok "${HOOK_EVENT} hook installed"
        fi
    elif [[ "$HOOK_EVENT" == "theme-set" ]]; then
        warn "Theme hook not found; skipping"
    fi
done

# Step 6: Theme bridge template (optional)
if [[ -f "${SCRIPT_DIR}/templates/omarchy10k.toml.tpl" ]]; then
    info "Installing theme bridge template..."
    mkdir -p "$TEMPLATE_DIR"
    cp "${SCRIPT_DIR}/templates/omarchy10k.toml.tpl" "${TEMPLATE_DIR}/omarchy10k.toml.tpl"
    ok "Template installed to ${TEMPLATE_DIR}"
else
    warn "Theme template not found; skipping"
fi

# Step 7: Rice layer — theme-reacted configs for tools Omarchy does not
# cover (fzf, eza, bat, less, lazygit, yazi, cava). Templates render on
# every theme switch into ~/.local/state/omarchy/current/theme/.

RICE_TEMPLATE_DIR="${HOME}/.config/omarchy/themed"
RICE_STATE_DIR="${HOME}/.local/state/omarchy/current/theme"

if ls "${SCRIPT_DIR}/templates/themed/"o10k-*.tpl &>/dev/null; then
    info "Installing rice templates (theme-reacted tool configs)..."
    mkdir -p "$RICE_TEMPLATE_DIR"
    cp "${SCRIPT_DIR}/templates/themed/"o10k-*.tpl "$RICE_TEMPLATE_DIR/"
    ok "Templates installed to ${RICE_TEMPLATE_DIR}"

    # Render immediately so the symlinks below are never dangling.
    if command -v omarchy-theme-refresh &>/dev/null; then
        omarchy-theme-refresh >/dev/null 2>&1 \
            && ok "Rendered rice configs for the active theme" \
            || warn "omarchy-theme-refresh failed — rice configs will appear on the next theme switch"
    else
        warn "omarchy-theme-refresh not found — rice configs will appear on the next theme switch"
    fi

    # Symlink tool config paths at the rendered files. Never clobber a
    # user file or a foreign symlink.
    o10k_link_rice() {
        local link="$1" target="$2"
        if [[ ! -f "$target" ]]; then
            warn "Rendered file missing, skipping link: ${target}"
            return
        fi
        mkdir -p "$(dirname "$link")"
        if [[ -L "$link" ]]; then
            local cur
            cur="$(readlink "$link")"
            if [[ "$cur" == "$target" ]]; then
                return
            elif [[ "$cur" == *"/current/theme/"* ]]; then
                ln -sfn "$target" "$link" || warn "Could not relink ${link}"
            else
                warn "Not linking ${link} — foreign symlink"
            fi
        elif [[ -e "$link" ]]; then
            warn "Not linking ${link} — regular file exists"
        else
            ln -s "$target" "$link" && ok "Linked ${link}"
        fi
    }

    o10k_link_rice "${HOME}/.config/yazi/theme.toml" "${RICE_STATE_DIR}/o10k-yazi-theme.toml"
    o10k_link_rice "${HOME}/.config/cava/config"     "${RICE_STATE_DIR}/o10k-cava.config"
else
    warn "No rice templates found; skipping"
fi

# ── Step 8: Terminal include layer ───────────────────────────────────────────
# Static personality file (installed once, never regenerated) plus optional
# config-file/include lines appended to the terminal configs. Idempotent:
# lines already present are never duplicated. A timestamped backup is taken
# before any modification, and exactly what was added is printed.
#
# Precedence is positional: terminal emulators apply later values over
# earlier ones, so appending at the end gives o10k precedence over Omarchy's
# static defaults, while any user key placed after these lines still wins.

o10k_backup() {
    local file="$1"
    [[ -f "$file" ]] && cp "$file" "${file}.bak.$(date +%Y%m%d-%H%M%S)"
}

o10k_append_unique() {
    # $1=file $2=line — append only when absent (fixed-string substring check)
    if grep -qF "$2" "$1"; then
        return 0
    fi
    printf '%s\n' "$2" >> "$1"
    ok "Added to $1: $2"
}

info "Terminal include layer..."

# 8a. Static personality file — holds only theme-invariant settings.
if [[ ! -f "$TERMINAL_STATIC_GHOSTTY" ]]; then
    mkdir -p "$CONFIG_DIR"
    cat > "$TERMINAL_STATIC_GHOSTTY" <<'EOF'
# Omarchy10k requires ownership of Bash prompt/pre-exec integration.
# Ghostty's injected Bash hook uses PS0, which Omarchy10k treats as
# literal prompt text.
shell-integration = none
EOF
    ok "Installed ${TERMINAL_STATIC_GHOSTTY}"
else
    ok "Static personality file already present"
fi

# 8b. Ghostty: append the two optional config-file includes.
if [[ -f "$GHOSTTY_CONFIG" ]]; then
    if grep -qF "$GHOSTTY_STATIC_LINE" "$GHOSTTY_CONFIG" && grep -qF "$GHOSTTY_THEME_LINE" "$GHOSTTY_CONFIG"; then
        ok "Ghostty includes already present"
    else
        o10k_backup "$GHOSTTY_CONFIG"
        o10k_append_unique "$GHOSTTY_CONFIG" "$GHOSTTY_STATIC_LINE"
        o10k_append_unique "$GHOSTTY_CONFIG" "$GHOSTTY_THEME_LINE"
    fi
else
    warn "Ghostty config not found; skipping include wiring"
fi

# 8c. foot: multiple include= directives verified against the installed foot
# (man foot.ini: "Multiple include directives are allowed, but only one path
# per directive"). If unverifiable, print the snippet for manual addition
# instead of writing it.
    # Capture first: `man | grep -q` fails under pipefail when grep exits
    # early (SIGPIPE). Verification must depend only on the man page text.
    FOOT_MAN="$(man foot.ini 2>/dev/null || true)"
    if command -v foot &>/dev/null; then
        if ! [[ -f "$FOOT_CONFIG" ]]; then
            warn "foot config not found; skipping include wiring"
        elif [[ "$FOOT_MAN" == *"Multiple include directives are allowed"* ]]; then
            if grep -qF "$FOOT_THEME_LINE" "$FOOT_CONFIG"; then
                ok "foot include already present"
            else
            # Must live under [main] and AFTER Omarchy's theme include (later
            # values win). Insert after the Omarchy include anchor when
            # present, else directly after the [main] header, else append a
            # small [main] block.
            if grep -qF "$FOOT_OMARCHY_ANCHOR" "$FOOT_CONFIG"; then
                anchor="$FOOT_OMARCHY_ANCHOR"
            elif grep -q '^\[main\]' "$FOOT_CONFIG"; then
                anchor='[main]'
            else
                anchor=''
            fi
            if [[ -n "$anchor" ]]; then
                awk -v anchor="$anchor" -v line="$FOOT_THEME_LINE" -v marker="$O10K_FOOT_MARKER" '
                    { print }
                    !done && $0 == anchor {
                        print marker
                        print line
                        done = 1
                    }
                    END {
                        if (!done && NR > 0) {
                            print ""
                            print "[main]"
                            print marker
                            print line
                        }
                    }
                ' "$FOOT_CONFIG" > "${FOOT_CONFIG}.tmp" && mv "${FOOT_CONFIG}.tmp" "$FOOT_CONFIG"
            else
                {
                    printf '\n[main]\n%s\n%s\n' "$O10K_FOOT_MARKER" "$FOOT_THEME_LINE"
                } >> "$FOOT_CONFIG"
            fi
            ok "Added to $FOOT_CONFIG: $FOOT_THEME_LINE"
        fi
    else
        warn "Could not verify multiple-include support for the installed foot; add manually:"
        printf '        (under [main], after Omarchy'\''s theme include)\n'
        printf '        %s\n' "$O10K_FOOT_MARKER"
        printf '        %s\n' "$FOOT_THEME_LINE"
    fi
else
    warn "foot not installed; skipping foot include wiring"
fi

# Modern CLI layer: aliases + tool inits, sourced by the Bash adapter.
if [[ -f "${SCRIPT_DIR}/config/tools.sh" ]]; then
    mkdir -p "$CONFIG_DIR"
    cp "${SCRIPT_DIR}/config/tools.sh" "${CONFIG_DIR}/tools.sh"
    chmod +x "${CONFIG_DIR}/tools.sh"
    ok "Modern CLI layer installed to ${CONFIG_DIR}/tools.sh"
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
