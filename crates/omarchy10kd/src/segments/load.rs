use crate::layout::Segment;
use super::SegmentContext;
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use unicode_width::UnicodeWidthStr;

/// Process-local 16-slot ring of recent 1-minute load samples. The daemon is
/// per-shell and warm, so this persists across renders and only advances
/// while the shell actually renders prompts — an idle shell freezes history.
const RING_CAPACITY: usize = 16;

static RING: LazyLock<Mutex<VecDeque<f32>>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(RING_CAPACITY)));

fn ring() -> &'static Mutex<VecDeque<f32>> {
    &RING
}
fn push_sample(value: f32, width: usize) -> Vec<f32> {
    let mut ring = ring().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    ring.push_back(value);
    while ring.len() > RING_CAPACITY {
        ring.pop_front();
    }
    let take = width.min(ring.len());
    ring.iter()
        .skip(ring.len() - take)
        .copied()
        .collect()
}

/// 1-minute load from /proc/loadavg, first whitespace-separated field.
fn read_load1() -> Option<f32> {
    let raw = std::fs::read_to_string("/proc/loadavg").ok()?;
    raw.split_whitespace().next()?.parse().ok()
}

/// Map a load value onto one of the eight braille bar heights, autoscaled to
/// the ring maximum with a floor of 1.0 so an idle box still shows low bars.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn braille_char(value: f32, max: f32) -> char {
    let max = max.max(1.0);
    let scaled = (value / max).clamp(0.0, 1.0);
    // Round to nearest bar; value 0 → first bar, value == max → last bar.
    let idx = (scaled * (BARS.len() - 1) as f32).round() as usize;
    BARS[idx]
}

fn sparkline(samples: &[f32]) -> String {
    let max = samples.iter().copied().fold(0.0f32, f32::max);
    samples.iter().map(|v| braille_char(*v, max)).collect()
}

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.load.enabled {
        return None;
    }

    let load = read_load1()?;
    let width = ctx.config.segments.load.width.max(1);
    let samples = push_sample(load, width);
    let content = sparkline(&samples);
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "load",
        content: content.clone(),
        compact_content: Some(content),
        priority: 55,
        min_width: 2,
        preferred_width,
        hide_below_cols: 40,
        fg: ctx.palette.muted.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    #[test]
    fn ring_push_evicts_oldest() {
        let mut ring = VecDeque::new();
        for v in 0..20 {
            let v = v as f32;
            ring.push_back(v);
            if ring.len() > RING_CAPACITY {
                ring.pop_front();
            }
        }
        assert_eq!(ring.len(), RING_CAPACITY);
        assert_eq!(ring.front(), Some(&4.0)); // 0..=19, oldest 16 evicted
        assert_eq!(ring.back(), Some(&19.0));
    }

    #[test]
    fn push_sample_truncates_to_width() {
        for v in [0.5f32, 1.0, 2.0] {
            push_sample(v, 16);
        }
        let samples = push_sample(4.0, 3);
        assert_eq!(samples, vec![1.0, 2.0, 4.0]);
    }

    #[test]
    fn autoscale_floors_at_one() {
        // All-zero ring: max floor 1.0 keeps bars on the bottom rung, not NaN/0-div.
        let chars = sparkline(&[0.0, 0.0, 0.0]);
        assert_eq!(chars, "▁▁▁");
    }

    #[test]
    fn braille_map_boundaries() {
        // Autoscale floor: values scale against max(1.0), so 0.5 is mid-bar.
        assert_eq!(braille_char(0.5, 0.5), '▅');
        assert_eq!(braille_char(0.25, 0.5), '▃');
        // Between maps to a middle bar.
        let mid = braille_char(2.0, 4.0);
        assert!(BARS.contains(&mid));
        assert_ne!(mid, '▁');
        assert_ne!(mid, '█');
    }
}
