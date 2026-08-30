# Ghostty & foot Integration — Design

**Date:** 2026-08-30
**Status:** Approved for implementation

## Goal

Make Omarchy10k's terminal integration correct and verifiable on the two
terminals Omarchy ships. Ghostty is the primary target; foot must be a
first-class peer.

## Diagnosis

Three faults, each confirmed by probing the real terminals rather than by
reading the code.

### D1 — foot is undetectable, so it always gets the degraded profile

`TermCaps::detect()` identifies foot by `TERM_PROGRAM == "foot"`. foot's own
man page lists `TERM_PROGRAM` under **"Variables *unset* in the child
process"** — it clears the variable deliberately. Omarchy additionally sets
`term=xterm-256color` in `~/.config/foot/foot.ini`, so `TERM` carries no
signal either. Captured from a real foot session:

```
TERM=xterm-256color   COLORTERM=truecolor   (no TERM_PROGRAM, no foot marker)
```

foot therefore resolves to `TerminalKind::Unknown`, whose profile reports no
OSC 8, no OSC 52, no sixel, no undercurl and no synchronised output. foot
supports every one of those.

### D2 — `GHOSTTY_SHELL_FEATURES` is the wrong signal

`shell/omarchy10k.bash:__o10k_update_133cd` abandons OSC 133;C/D when
`GHOSTTY_SHELL_FEATURES` is set. Captured from real Ghostty sessions:

```
--shell-integration=none    → GHOSTTY_SHELL_FEATURES=cursor:steady,path,title
--shell-integration=detect  → GHOSTTY_SHELL_FEATURES=cursor:steady,path,title
```

Identical. The variable is populated from **`shell-integration-features`**, a
different configuration key: it lists which features are configured, not
whether Ghostty injected its integration. Ghostty sets it unconditionally, so
the guard always returns.

### D3 — the same `TERM_PROGRAM` fault in the adapter

`__o10k_update_133cd` also gates on `case "${TERM_PROGRAM:-}" in ghostty|foot`,
which foot can never satisfy.

**Net effect:** OSC 133;C/D semantic prompts cannot fire on either terminal
Omarchy ships. That is the feature behind jump-to-previous-prompt and
scrollback marks, and Omarchy's Ghostty config sets `shell-integration = none`,
so nothing else provides them.

### Not a fault

The capability table itself is correct and stays as it is. Ghostty has kitty
graphics and has explicitly declined sixel; foot has sixel and no kitty
graphics; both have OSC 7, OSC 8 and OSC 52. Only detection is broken.

## Architecture

### R1 — `TerminalKind::from_xtversion()`

A pure parser in `crates/omarchy10kd/src/terminal.rs`, no I/O. Input is the
payload of an XTVERSION (`CSI > q`) reply; output is a `TerminalKind` and a
version string.

Captured from the real terminals:

```
ghostty  ESC P > | ghostty 1.3.1-arch2 ESC \
foot     ESC P > | foot(1.27.0)        ESC \
```

The parser accepts the `ESC P > |` … `ESC \` envelope, tolerates a bare
payload, and matches case-insensitively on the leading name token, so
`foot(1.27.0)` and `ghostty 1.3.1-arch2` both resolve. Unrecognised names
return `Unknown` with the raw string preserved for `doctor` to display.

### S1 — the probe, in the adapter

The probe lives in the bash adapter, not the daemon: the daemon is a
background process with no controlling terminal, and only the shell can talk
to `/dev/tty`.

Resolution order at shell start:

1. **`O10K_TERM` already set** — honour it. This is the documented manual
   override and the escape hatch for terminals that answer nothing.
2. **Unambiguous environment** — `GHOSTTY_RESOURCES_DIR` ⇒ ghostty;
   `TERM=xterm-ghostty` ⇒ ghostty; `TERM` matching `foot*` ⇒ foot;
   `KITTY_WINDOW_ID` ⇒ kitty. No probe, zero cost. Ghostty — the common case
   — always lands here.
3. **XTVERSION probe** — only when the above are inconclusive, which in
   practice means foot. Written to `/dev/tty` with the terminal in raw mode,
   read back under an **80 ms** timeout, terminal settings restored via a
   trap so an interrupted probe cannot leave the tty raw.
4. **`TERM_PROGRAM`** — as today.
5. **`unknown`** — today's degraded profile.

Skipped entirely when stdin is not a TTY, and when `TMUX` or `SSH_TTY` is set:
inside a multiplexer or a remote session the reply describes the wrong
terminal, and a stale answer is worse than no answer.

The result is exported as `O10K_TERM` and `O10K_TERM_VERSION` and reaches the
daemon over the **existing env channel** (`__o10k_env_json`, which already
forwards a fixed key list). No protocol change.

### S2 — the gating fixes

- Delete the `GHOSTTY_SHELL_FEATURES` / `GHOSTTY_SHELL_INTEGRATION_FEATURES`
  check. The line immediately below it — `declare -F __ghostty_precmd` — is
  the correct and sufficient signal that Ghostty actually injected its
  integration, and it already exists.
- Replace `TERM_PROGRAM` matching with the resolved `O10K_TERM`, in both
  `terminal.rs` and `__o10k_update_133cd`.

### C1 — `omarchy10k doctor` Terminal section

`check_terminal()` currently prints `TERM_PROGRAM` or `TERM` and an
unconditional `✓`, which would have reported "healthy" throughout every fault
above. It is replaced with:

```
  Terminal          foot 1.27.0   ✓ detected by XTVERSION probe
    capabilities    OSC 7 ✓  OSC 8 ✓  OSC 52 ✓  sixel ✓  kitty-gfx ✘
    emitting        OSC 133 A/B ✓   OSC 133 C/D ✓   OSC 7 ✓
    theme include   foot.ini → o10k-foot.ini ✓
```

The detection *method* is printed because "which signal identified this
terminal" is the question every fault above turned on. The theme-include
check reads the terminal's own config and looks for our include line —
`config-file = ?"…o10k-ghostty.conf"` for Ghostty, `include=…o10k-foot.ini`
under `[main]` for foot — catching the case where a config reset silently
drops it.

### F1 — Ghostty-specific features

**Cursor shape per vi mode** (DECSCUSR, `CSI Ps SP q`). Universal across both
terminals and every other terminal in the catalog. The adapter already
receives `KEYMAP` for the vi-mode prompt character; the same signal drives the
cursor. Emitted only when `[terminal.cursor_shape].enabled`, default off.

**Kitty-graphics prompt sprite.** Where `has_kitty_graphics` (Ghostty, kitty,
wezterm), the existing half-block sprite renderer is upgraded to the kitty
graphics protocol for a full-resolution image. foot keeps half-blocks — it has
sixel, not kitty graphics, and one high-quality path plus one universal
fallback is better than three partial ones.

**OSC 777 long-command notification — gated on verification.** Omarchy runs
Quickshell's own notification server rather than mako or dunst, and whether it
surfaces OSC 777 from a terminal could not be confirmed during design. The
first implementation step is a probe: emit OSC 777 from each terminal and
check whether a notification appears. **If it does not, this feature is
dropped and the spec is amended** — shipping a notification path that silently
does nothing is worse than not having one.

## Error handling

| Condition | Behaviour |
|---|---|
| Terminal does not answer XTVERSION | 80 ms timeout, fall through to `TERM_PROGRAM`, then `unknown`. One-time cost per shell |
| Probe interrupted mid-read | `trap` restores the saved `stty` state; a raw tty is the one unacceptable outcome |
| Not a TTY (script, pipe, CI) | Probe skipped entirely |
| Inside tmux or ssh | Probe skipped; the reply would describe the wrong terminal |
| Reply is garbage or truncated | Parser returns `Unknown`, raw string kept for `doctor` |
| `O10K_TERM` set to nonsense | Honoured as `unknown`; `doctor` shows it came from the override |
| Theme include missing from terminal config | `doctor` reports it; nothing is auto-written — this project never edits terminal configs |

## Testing

**Rust**
- `from_xtversion` against the two **real captured strings**, plus kitty and
  wezterm forms, a bare payload with no envelope, an empty reply and garbage.
- Capability table per kind; `foot` must report OSC 8, OSC 52, sixel,
  undercurl and sync output — the exact set D1 was suppressing.
- Fallback ordering: `O10K_TERM` beats env beats `TERM_PROGRAM`.

**Bash**
- Probe returns promptly with no TTY, under `TMUX`, and under `SSH_TTY`.
- Timeout path leaves `stty` unchanged (assert the saved state is restored).
- `__o10k_update_133cd` emits for foot; and does **not** emit when
  `__ghostty_precmd` is defined, while `GHOSTTY_SHELL_FEATURES` alone no
  longer suppresses it.

**End-to-end** — the tests that would have caught all three faults. Launch
`ghostty -e` and `foot` running a real prompt render, capture the bytes, and
assert OSC 133 markers are present for both. Skipped with a clear message when
the binaries are absent, so the suite still runs elsewhere.

## Increments

1. `from_xtversion` + capability tests. No behaviour change.
2. The adapter probe and `O10K_TERM` resolution.
3. The two gating fixes (D2, D3).
4. `doctor` Terminal section, including the theme-include check.
5. End-to-end ghostty/foot tests.
6. OSC 777 verification probe → build the notification feature or drop it and
   amend this spec.
7. Cursor shape per vi mode.
8. Kitty-graphics sprite upgrade.
9. Wiki: `bash-adapter.md`, `daemon.md`, `cli.md`, `config.md`, `glossary.md`.
