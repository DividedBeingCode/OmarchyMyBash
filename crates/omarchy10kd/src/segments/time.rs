use crate::layout::Segment;
use super::SegmentContext;
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthStr;

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.time.enabled {
        return None;
    }

    let (hour, minute, second) = local_time()?;
    let content = format_time(&ctx.config.segments.time.format, hour, minute, second);
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "time".into(),
        content: content.clone(),
        compact_content: Some(content),
        priority: 55,
        min_width: 4,
        preferred_width,
        hide_below_cols: 40,
        fg: ctx.palette.muted.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn local_time() -> Option<(u32, u32, u32)> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    let mut tm = std::mem::MaybeUninit::<Tm>::uninit();
    let ret = unsafe { localtime::localtime_r(&secs, tm.as_mut_ptr()) };
    if ret.is_null() {
        return None;
    }
    let tm = unsafe { tm.assume_init() };
    Some((tm.tm_hour as u32, tm.tm_min as u32, tm.tm_sec as u32))
}

fn format_time(fmt: &str, hour: u32, minute: u32, second: u32) -> String {
    match fmt {
        "%H:%M" => format!("{hour:02}:{minute:02}"),
        "%H:%M:%S" => format!("{hour:02}:{minute:02}:{second:02}"),
        "%I:%M %p" => {
            let (h12, ampm) = if hour == 0 {
                (12, "AM")
            } else if hour < 12 {
                (hour, "AM")
            } else if hour == 12 {
                (12, "PM")
            } else {
                (hour - 12, "PM")
            };
            format!("{h12:02}:{minute:02} {ampm}")
        }
        other => other.to_string(),
    }
}

#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: std::ffi::c_long,
    tm_zone: *const std::ffi::c_char,
}

mod localtime {
    use super::Tm;

    unsafe extern "C" {
        pub fn localtime_r(time: *const i64, result: *mut Tm) -> *mut Tm;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time("%H:%M", 14, 5, 30), "14:05");
        assert_eq!(format_time("%H:%M:%S", 14, 5, 30), "14:05:30");
        assert_eq!(format_time("%I:%M %p", 0, 30, 0), "12:30 AM");
        assert_eq!(format_time("%I:%M %p", 14, 5, 0), "02:05 PM");
    }
}
