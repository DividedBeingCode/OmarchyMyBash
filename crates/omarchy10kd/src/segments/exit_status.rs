use crate::layout::Segment;
use crate::segments::SegmentContext;
use crate::terminal::TermCaps;
use unicode_width::UnicodeWidthStr;

const UNDERCURL_ON: &str = "\x1b[4:3m";
const UNDERCURL_OFF: &str = "\x1b[4:0m";

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if ctx.exit_code == 0 {
        return None;
    }

    let raw = if ctx.config.segments.exit_status.show_signal_name {
        format_exit_code(ctx.exit_code)
    } else {
        format!("✘ {}", ctx.exit_code)
    };

    let content = if TermCaps::detect().has_undercurl {
        format!("{UNDERCURL_ON}{raw}{UNDERCURL_OFF}")
    } else {
        raw.clone()
    };

    let preferred_width = UnicodeWidthStr::width(raw.as_str()) as u16;

    Some(Segment {
        name: "exit_status",
        content,
        compact_content: Some(format!("✘ {}", ctx.exit_code)),
        priority: 30,
        min_width: 3,
        preferred_width,
        hide_below_cols: 0,
        fg: ctx.palette.red.fg_escape(),
        bg: None,
        bold: true,
        separator: None,
    })
}

fn format_exit_code(code: i32) -> String {
    match code {
        1 => "✘ 1 error".into(),
        2 => "✘ 2 misuse".into(),
        126 => "✘ 126 not executable".into(),
        127 => "✘ 127 command not found".into(),
        128 => "✘ 128 invalid exit".into(),
        130 => "✘ SIGINT".into(),
        131 => "✘ SIGQUIT".into(),
        137 => "✘ SIGKILL".into(),
        139 => "✘ SIGSEGV".into(),
        141 => "✘ SIGPIPE".into(),
        143 => "✘ SIGTERM".into(),
        148 => "✘ SIGTSTP".into(),
        n if n > 128 && n < 165 => {
            let sig = n - 128;
            format!("✘ SIG({sig})")
        }
        n => format!("✘ {n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_names() {
        assert_eq!(format_exit_code(137), "✘ SIGKILL");
        assert_eq!(format_exit_code(130), "✘ SIGINT");
        assert_eq!(format_exit_code(139), "✘ SIGSEGV");
        assert_eq!(format_exit_code(127), "✘ 127 command not found");
    }
}
