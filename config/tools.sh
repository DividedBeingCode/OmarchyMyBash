#!/bin/bash
# Omarchy10k — modern CLI layer
# Installed to: ~/.config/omarchy10k/tools.sh
# Sourced by the omarchy10k Bash adapter (shell/omarchy10k.bash) at init.
#
# Upgrades interactive Unix defaults with modern replacements that take on the
# standard command names. Aliases only expand in interactive shells, so
# scripts keep calling the real Unix binaries. Set O10K_NO_TOOLS=1 (before
# .bashrc runs) to disable this entire file.

[[ -n "${O10K_NO_TOOLS:-}" ]] && return 0

# ── eza: ls / ll / la / tree ────────────────────────────────────────────────
if command -v eza &>/dev/null; then
    alias ls='eza --icons=auto --group-directories-first'
    alias ll='eza -lah --icons=auto --group-directories-first --git'
    alias la='eza -a --icons=auto --group-directories-first'
    alias lt='eza --tree --level=2 --icons=auto'
    alias tree='eza --tree --icons=auto'
fi

# ── bat: cat ────────────────────────────────────────────────────────────────
if command -v bat &>/dev/null; then
    alias cat='bat --paging=never'
    # Themed man pages through bat (falls back to plain less if bat chokes)
    export MANPAGER="sh -c 'col -bx | bat -l man -p 2>/dev/null || less'"
fi

# ── grep: ripgrep (interactive only; scripts use real grep) ─────────────────
command -v rg &>/dev/null && alias grep='rg'

# ── resource monitors / disk / processes ────────────────────────────────────
command -v btop  &>/dev/null && alias top='btop'
command -v dust  &>/dev/null && alias du='dust'
command -v duf   &>/dev/null && alias df='duf'
command -v procs &>/dev/null && alias ps='procs'

# ── zoxide: predictive cd (z / zi also available) ───────────────────────────
if command -v zoxide &>/dev/null; then
    eval "$(zoxide init bash --cmd cd)"
fi

# ── atuin: smart history (Ctrl-R; Up arrow stays vanilla) ───────────────────
if command -v atuin &>/dev/null; then
    eval "$(atuin init bash --disable-up-arrow)"
fi

# ── fzf: key bindings (Ctrl-T files, Ctrl-R history, Alt-C dirs) ────────────
if command -v fzf &>/dev/null; then
    eval "$(fzf --bash 2>/dev/null)" || source /usr/share/fzf/key-bindings.bash 2>/dev/null || true
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
