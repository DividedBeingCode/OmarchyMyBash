use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum TerminalKind {
    Ghostty,
    Foot,
    Kitty,
    WezTerm,
    Alacritty,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct TermCaps {
    pub terminal: TerminalKind,
    pub has_osc7: bool,
    pub has_osc8: bool,
    pub has_osc52: bool,
    pub has_osc777: bool,
    pub has_kitty_graphics: bool,
    pub has_sixel: bool,
    pub has_undercurl: bool,
    pub has_sync_output: bool,
}

impl Default for TermCaps {
    fn default() -> Self {
        Self {
            terminal: TerminalKind::Unknown,
            has_osc7: false,
            has_osc8: false,
            has_osc52: false,
            has_osc777: false,
            has_kitty_graphics: false,
            has_sixel: false,
            has_undercurl: false,
            has_sync_output: false,
        }
    }
}

/// Parse an XTVERSION (`CSI > q`) reply into a terminal kind and version.
///
/// This exists because environment variables are not a reliable way to
/// identify a terminal. foot deliberately UNSETS `TERM_PROGRAM` -- its man
/// page lists the variable under "Variables unset in the child process" --
/// and Omarchy additionally sets `term=xterm-256color` in foot.ini, so a real
/// foot session carries no identifying signal whatsoever. It answers
/// XTVERSION correctly regardless, which is why the shell probes for it.
///
/// Accepts the full DCS envelope or a bare payload. Captured from the real
/// terminals (ESC shown as \e):
///
/// ```text
/// ghostty   \eP>|ghostty 1.3.1-arch2\e\\
/// foot      \eP>|foot(1.27.0)\e\\
/// ```
///
/// Returns the kind plus the version substring, which may be empty. An
/// unrecognised name yields `Unknown`; the caller keeps the raw reply so
/// `doctor` can show what actually came back.
pub fn from_xtversion(reply: &str) -> (TerminalKind, String) {
    const ESC: char = '\x1b';
    const BEL: char = '\x07';

    // Strip the DCS envelope if present: ESC P > | payload ST.
    let mut body = reply.trim();
    if let Some(rest) = body.strip_prefix(ESC) {
        body = rest.strip_prefix('P').unwrap_or(rest);
    }
    body = body.trim_start_matches('>').trim_start_matches('|').trim();
    // Terminator: ST (ESC backslash) or BEL.
    if let Some(i) = body.find(ESC) {
        body = &body[..i];
    }
    if let Some(i) = body.find(BEL) {
        body = &body[..i];
    }
    let body = body.trim();
    if body.is_empty() {
        return (TerminalKind::Unknown, String::new());
    }

    // The name is the leading token, delimited by whitespace or `(`:
    // ghostty reports "ghostty 1.3.1-arch2", foot reports "foot(1.27.0)".
    let split = body
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(body.len());
    let (name, rest) = body.split_at(split);

    let version = rest
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_string();

    let kind = match name.to_lowercase().as_str() {
        "ghostty" => TerminalKind::Ghostty,
        "foot" => TerminalKind::Foot,
        "kitty" | "xterm-kitty" => TerminalKind::Kitty,
        "wezterm" => TerminalKind::WezTerm,
        "alacritty" => TerminalKind::Alacritty,
        _ => TerminalKind::Unknown,
    };
    (kind, version)
}

/// Resolve the terminal from the environment alone, in the same order the
/// shell adapter uses.
///
/// `O10K_TERM` wins: it is what the adapter's XTVERSION probe writes, and is
/// also the documented manual override for terminals that answer nothing.
pub fn kind_from_env() -> TerminalKind {
    if let Ok(explicit) = std::env::var("O10K_TERM") {
        let k = match explicit.trim().to_lowercase().as_str() {
            "ghostty" => Some(TerminalKind::Ghostty),
            "foot" => Some(TerminalKind::Foot),
            "kitty" => Some(TerminalKind::Kitty),
            "wezterm" => Some(TerminalKind::WezTerm),
            "alacritty" => Some(TerminalKind::Alacritty),
            "" => None,
            // An override naming something we have no profile for is honoured
            // as Unknown rather than ignored: the user said what it is.
            _ => Some(TerminalKind::Unknown),
        };
        if let Some(k) = k {
            return k;
        }
    }

    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() || term == "xterm-ghostty" {
        return TerminalKind::Ghostty;
    }
    // foot's own terminfo names, for the case where Omarchy has not
    // overridden `term` in foot.ini.
    if term == "foot" || term.starts_with("foot-") {
        return TerminalKind::Foot;
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok() || term == "xterm-kitty" {
        return TerminalKind::Kitty;
    }

    match std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "ghostty" => TerminalKind::Ghostty,
        // Kept for completeness even though foot unsets it, so a future foot
        // that starts setting it is handled.
        "foot" => TerminalKind::Foot,
        "wezterm" => TerminalKind::WezTerm,
        "alacritty" => TerminalKind::Alacritty,
        _ => TerminalKind::Unknown,
    }
}

/// The terminal the SHELL is in, as resolved by the adapter and sent over the
/// env channel as `O10K_TERM`.
///
/// This is the authoritative answer and `kind_from_env` is only a fallback.
/// The daemon has no controlling terminal, so it cannot probe; worse, a
/// daemon outlives the shell that spawned it and a headless one serves many
/// shells at once, so its own environment describes whichever terminal
/// happened to start it first.
///
/// That matters because `TermCaps` gates visible behaviour: OSC 8 hyperlinks
/// on the branch and path, and undercurl on errors.
pub fn kind_from_channel(
    env: Option<&std::collections::HashMap<String, String>>,
) -> TerminalKind {
    if let Some(name) = env.and_then(|e| e.get("O10K_TERM")) {
        return match name.trim().to_lowercase().as_str() {
            "ghostty" => TerminalKind::Ghostty,
            "foot" => TerminalKind::Foot,
            "kitty" => TerminalKind::Kitty,
            "wezterm" => TerminalKind::WezTerm,
            "alacritty" => TerminalKind::Alacritty,
            // An honest `unknown` from the shell is carried through rather
            // than falling back: the fallback is exactly what is untrusted.
            _ => TerminalKind::Unknown,
        };
    }
    kind_from_env()
}

impl TermCaps {
    pub fn detect() -> Self {
        Self::for_kind(kind_from_env())
    }

    /// The capability profile for a known kind. Split out from `detect` so
    /// the table is testable without touching the process environment.
    pub fn for_kind(terminal: TerminalKind) -> Self {
        match terminal {
            TerminalKind::Ghostty => Self {
                terminal,
                has_osc7: true,
                has_osc8: true,
                has_osc52: true,
                has_osc777: true,
                has_kitty_graphics: true,
                has_sixel: false,
                has_undercurl: true,
                has_sync_output: true,
            },
            TerminalKind::Foot => Self {
                terminal,
                has_osc7: true,
                has_osc8: true,
                has_osc52: true,
                has_osc777: true,
                has_kitty_graphics: false,
                has_sixel: true,
                has_undercurl: true,
                has_sync_output: true,
            },
            TerminalKind::Kitty => Self {
                terminal,
                has_osc7: true,
                has_osc8: true,
                has_osc52: true,
                has_osc777: false,
                has_kitty_graphics: true,
                has_sixel: false,
                has_undercurl: true,
                has_sync_output: true,
            },
            TerminalKind::WezTerm => Self {
                terminal,
                has_osc7: true,
                has_osc8: true,
                has_osc52: true,
                has_osc777: true,
                has_kitty_graphics: true,
                has_sixel: true,
                has_undercurl: true,
                has_sync_output: true,
            },
            TerminalKind::Alacritty => Self {
                terminal,
                has_osc7: true,
                has_osc8: false,
                has_osc52: true,
                has_osc777: false,
                has_kitty_graphics: false,
                has_sixel: false,
                has_undercurl: false,
                has_sync_output: false,
            },
            TerminalKind::Unknown => Self {
                terminal,
                has_osc7: true,
                has_osc8: false,
                has_osc52: false,
                has_osc777: false,
                has_kitty_graphics: false,
                has_sixel: false,
                has_undercurl: false,
                has_sync_output: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact bytes these terminals sent when probed on 2026-08-30
    // (Ghostty 1.3.1-arch2, foot 1.27.0). Anything that breaks these breaks
    // detection on the two terminals Omarchy ships.
    const GHOSTTY_REPLY: &str = "\x1bP>|ghostty 1.3.1-arch2\x1b\\";
    const FOOT_REPLY: &str = "\x1bP>|foot(1.27.0)\x1b\\";

    #[test]
    fn parses_the_real_ghostty_reply() {
        let (kind, ver) = from_xtversion(GHOSTTY_REPLY);
        assert_eq!(kind, TerminalKind::Ghostty);
        assert_eq!(ver, "1.3.1-arch2");
    }

    #[test]
    fn parses_the_real_foot_reply() {
        // foot wraps its version in parentheses with no space, which is why
        // `(` is a name delimiter and not just whitespace.
        let (kind, ver) = from_xtversion(FOOT_REPLY);
        assert_eq!(kind, TerminalKind::Foot);
        assert_eq!(ver, "1.27.0");
    }

    #[test]
    fn parses_other_terminals() {
        assert_eq!(from_xtversion("\x1bP>|kitty(0.32.2)\x1b\\").0, TerminalKind::Kitty);
        assert_eq!(from_xtversion("\x1bP>|WezTerm 20240203\x1b\\").0, TerminalKind::WezTerm);
    }

    #[test]
    fn name_matching_is_case_insensitive() {
        assert_eq!(from_xtversion("\x1bP>|Ghostty 1.0\x1b\\").0, TerminalKind::Ghostty);
        assert_eq!(from_xtversion("\x1bP>|FOOT(1.0)\x1b\\").0, TerminalKind::Foot);
    }

    #[test]
    fn accepts_a_bare_payload_without_the_envelope() {
        // Some readers strip the DCS wrapper before handing it over.
        assert_eq!(from_xtversion("foot(1.27.0)").0, TerminalKind::Foot);
        assert_eq!(from_xtversion("ghostty 1.3.1").0, TerminalKind::Ghostty);
    }

    #[test]
    fn accepts_a_bel_terminator() {
        assert_eq!(from_xtversion("\x1bP>|foot(1.27.0)\x07").0, TerminalKind::Foot);
    }

    #[test]
    fn garbage_and_silence_are_unknown_not_a_panic() {
        // A terminal that answers nothing, or answers something else
        // entirely, must degrade rather than crash the prompt path.
        for reply in ["", "   ", "\x1bP>|\x1b\\", "not a terminal", "\x1b[?62;c"] {
            let (kind, _) = from_xtversion(reply);
            assert_eq!(kind, TerminalKind::Unknown, "reply {reply:?}");
        }
    }

    #[test]
    fn a_version_is_optional() {
        let (kind, ver) = from_xtversion("\x1bP>|ghostty\x1b\\");
        assert_eq!(kind, TerminalKind::Ghostty);
        assert_eq!(ver, "");
    }

    // ── Capability table ───────────────────────────────────────────────────

    /// The regression this whole change exists for: foot resolved to
    /// `Unknown`, whose profile denies OSC 8, OSC 52, sixel, undercurl and
    /// synchronised output. foot supports every one of them.
    #[test]
    fn foot_gets_its_real_capabilities() {
        let c = TermCaps::for_kind(TerminalKind::Foot);
        assert!(c.has_osc8, "foot supports OSC 8 hyperlinks");
        assert!(c.has_osc52, "foot supports OSC 52 clipboard");
        assert!(c.has_sixel, "sixel is foot's headline graphics feature");
        assert!(c.has_undercurl);
        assert!(c.has_sync_output);
        // foot has never implemented the kitty graphics protocol.
        assert!(!c.has_kitty_graphics);
    }

    #[test]
    fn ghostty_gets_kitty_graphics_and_no_sixel() {
        let c = TermCaps::for_kind(TerminalKind::Ghostty);
        assert!(c.has_kitty_graphics);
        // Ghostty has explicitly declined sixel in favour of kitty graphics.
        assert!(!c.has_sixel);
        assert!(c.has_osc8 && c.has_osc52 && c.has_osc7);
    }

    #[test]
    fn unknown_stays_conservative() {
        let c = TermCaps::for_kind(TerminalKind::Unknown);
        assert!(!c.has_osc8 && !c.has_osc52 && !c.has_kitty_graphics);
        // OSC 7 is safe everywhere -- an unsupporting terminal ignores it.
        assert!(c.has_osc7);
    }
}
