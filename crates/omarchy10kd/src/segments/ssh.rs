use crate::layout::Segment;
use super::SegmentContext;
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.ssh.enabled {
        return None;
    }

    let show = match ctx.config.segments.ssh.show.as_str() {
        "always" => true,
        "never" => false,
        _ => ctx.in_ssh,
    };

    if !show {
        return None;
    }

    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_default();

    if hostname.is_empty() {
        return None;
    }

    let short_host = hostname.split('.').next().unwrap_or(&hostname);

    let content = format!("\u{f489} {short_host}");
    let compact = format!("\u{f489}");
    let width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "ssh".into(),
        content,
        compact_content: Some(compact),
        priority: 8,
        min_width: 2,
        preferred_width: width,
        hide_below_cols: 50,
        fg: ctx.palette.yellow.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

mod hostname {
    use std::ffi::OsString;

    pub fn get() -> std::io::Result<OsString> {
        let mut buf = vec![0u8; 256];
        let ret = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        buf.truncate(len);
        Ok(OsString::from(String::from_utf8_lossy(&buf).into_owned()))
    }

    mod libc {
        unsafe extern "C" {
            pub fn gethostname(name: *mut std::ffi::c_char, len: usize) -> std::ffi::c_int;
        }
    }
}
