#!/bin/bash
# Omarchy10k — modern CLI layer (uncontested, own-class definitions)
# Installed to: ~/.config/omarchy10k/tools.sh
# Sourced by the omarchy10k Bash adapter (shell/omarchy10k.bash) at init.
#
# Contested claims — ls, lt, cd/zoxide, fzf keys, MANPAGER — are NOT defined
# here anymore. The adapter's Shell Layer section resolves them with baked
# policy and per-item platform detection (platform-coexistence wave C2);
# Omarchy ships richer variants and they defer/extend there. Everything in
# this file is uncontested: Omarchy has no opinion on these names.
#
# Policy seam (set by the adapter before this file sources):
#   __O10K_LAYER_POLICY             global layer policy (default "extend")
#   __O10K_LAYER_OVERRIDES_<name>   per-item override of the global policy
# "off" as the global policy suppresses this entire file; "off"/"defer" on
# an item suppresses that definition. Set O10K_NO_TOOLS=1 (before .bashrc
# runs) for the same effect. User definitions always win — the adapter owns
# that guarantee; the only local detection left here is `help` below, since
# alias/function presence can only be checked at shell runtime.

[[ -n "${O10K_NO_TOOLS:-}" ]] && return 0
[[ "${__O10K_LAYER_POLICY:-}" == "off" ]] && return 0

# Per-item policy check: "off"/"defer" suppress the definition, anything
# else (or unset) allows it. Reads __O10K_LAYER_OVERRIDES_<name> indirectly.
__o10k_tools_allowed() {
    local ref="__O10K_LAYER_OVERRIDES_${1}" policy="${__O10K_LAYER_POLICY:-extend}" override
    override="${!ref:-}"
    [[ -n "$override" ]] && policy="$override"
    ! [[ "$policy" == "off" || "$policy" == "defer" ]]
}

# ── eza: ll / la / tree ─────────────────────────────────────────────────────
# (ls and lt are Shell-Layer claims now — Omarchy's long-format and tree
# variants are strictly richer and stay in force.)
if command -v eza &>/dev/null; then
    __o10k_tools_allowed ll   && alias ll='eza -lah --icons=auto --group-directories-first --git'
    __o10k_tools_allowed la   && alias la='eza -a --icons=auto --group-directories-first'
    __o10k_tools_allowed tree && alias tree='eza --tree --icons=auto'
fi

# ── bat: cat ────────────────────────────────────────────────────────────────
# (MANPAGER is a Shell-Layer claim — defer: Omarchy already sets it.)
command -v bat &>/dev/null && __o10k_tools_allowed cat && alias cat='bat --paging=never'

# ── grep: ripgrep (interactive only; scripts use real grep) ─────────────────
# Compatibility-sensitive: ripgrep's flags are not grep's. This is existing
# shipped behavior, retained; doctor labels it, and
# [shell.layer.overrides] grep = "defer" (surfaced here as
# __O10K_LAYER_OVERRIDES_grep by the adapter) suppresses it cleanly.
command -v rg &>/dev/null && __o10k_tools_allowed grep && alias grep='rg'

# ── resource monitors / disk / processes ────────────────────────────────────
command -v btop  &>/dev/null && __o10k_tools_allowed top && alias top='btop'
command -v dust  &>/dev/null && __o10k_tools_allowed du  && alias du='dust'
command -v duf   &>/dev/null && __o10k_tools_allowed df  && alias df='duf'
command -v procs &>/dev/null && __o10k_tools_allowed ps  && alias ps='procs'

# ── atuin: smart history (Ctrl-R; Up arrow stays vanilla) ───────────────────
# Omarchy does not init atuin; uncontested.
if command -v atuin &>/dev/null; then
    eval "$(atuin init bash --disable-up-arrow)"
fi

# ── yazi: file manager with exit-directory follow ───────────────────────────
if command -v yazi &>/dev/null; then
    y() {
        local tmp="$(mktemp -t "yazi-cwd.XXXXXX")" cwd
        yazi "$@" --cwd-file="$tmp"
        if [[ -s "$tmp" ]] && IFS= read -r cwd < "$tmp" && [[ -n "$cwd" && "$cwd" != "$PWD" ]]; then
            builtin cd -- "$cwd"
        fi
        rm -f -- "$tmp"
    }
fi

# ── gap fills (own-class; a missing binary silently defines nothing) ────────
command -v lazygit    &>/dev/null && __o10k_tools_allowed lg  && alias lg='lazygit'
command -v lazydocker &>/dev/null && __o10k_tools_allowed lzd && alias lzd='lazydocker'

# help → tldr, only when nothing owns `help`: a user alias/function defined
# before this file sourced must always win, and that presence check needs
# shell runtime (shell-side detection, per the coexistence spec §4.3).
if command -v tldr &>/dev/null && __o10k_tools_allowed help; then
    if ! alias help &>/dev/null && ! declare -F help &>/dev/null; then
        alias help='tldr'
    fi
fi

# erdtree: the upstream binary is `erd` on most distros. Where a distro
# ships it as `et`, the name already invokes erdtree and no alias is needed.
command -v erd &>/dev/null && __o10k_tools_allowed et && alias et='erd'

# ── presence-only stack (doctor verifies; no aliases warranted) ─────────────
# fd, tldr, jq, xh are their own commands — nothing to alias. fd backs
# FZF_DEFAULT_COMMAND in the theme env file, tldr backs the help gap-fill
# above; doctor reports any of them missing (coexistence spec §4.3).
