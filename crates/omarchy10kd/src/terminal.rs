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

impl TermCaps {
    pub fn detect() -> Self {
        let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        let terminal = match term_program.to_lowercase().as_str() {
            "ghostty" => TerminalKind::Ghostty,
            "foot" => TerminalKind::Foot,
            _ if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() => TerminalKind::Ghostty,
            _ if std::env::var("KITTY_WINDOW_ID").is_ok() => TerminalKind::Kitty,
            "wezterm" => TerminalKind::WezTerm,
            "alacritty" => TerminalKind::Alacritty,
            _ => TerminalKind::Unknown,
        };

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
