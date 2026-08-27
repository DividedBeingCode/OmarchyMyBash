use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct Segment {
    pub name: &'static str,
    pub content: String,
    pub compact_content: Option<String>,
    pub priority: u8,
    pub min_width: u16,
    pub preferred_width: u16,
    pub hide_below_cols: u16,
    pub fg: String,
    pub bg: Option<String>,
    pub bold: bool,
    pub separator: Option<String>,
}

impl Segment {
    pub fn display_width(&self) -> u16 {
        UnicodeWidthStr::width(self.content.as_str()) as u16
    }

    pub fn compact_width(&self) -> u16 {
        self.compact_content
            .as_ref()
            .map(|c| UnicodeWidthStr::width(c.as_str()) as u16)
            .unwrap_or(self.display_width())
    }
}

#[derive(Debug)]
pub struct LayoutEngine {
    pub cols: u16,
}

impl LayoutEngine {
    pub fn new(cols: u16) -> Self {
        Self { cols }
    }

    /// Resolve which segments to show and whether to use compact form.
    /// Returns (visible_segments, total_width)
    pub fn resolve(&self, segments: &[Segment]) -> Vec<ResolvedSegment> {
        // First pass: filter segments below column threshold
        let mut candidates: Vec<(usize, &Segment)> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.hide_below_cols <= self.cols)
            .collect();

        // Sort by priority (lower = more important, kept first)
        candidates.sort_by_key(|(_, s)| s.priority);

        // Calculate total preferred width including separators
        let separator_width = 1u16; // space between segments
        let mut total_preferred: u16 = 0;
        for (i, (_, seg)) in candidates.iter().enumerate() {
            total_preferred += seg.preferred_width;
            if i > 0 {
                total_preferred += separator_width;
            }
        }

        let mut result: Vec<ResolvedSegment> = Vec::new();

        if total_preferred <= self.cols {
            // Everything fits at preferred width
            for (idx, seg) in &candidates {
                result.push(ResolvedSegment {
                    original_index: *idx,
                    content: seg.content.clone(),
                    fg: seg.fg.clone(),
                    bg: seg.bg.clone(),
                    bold: seg.bold,
                    is_compact: false,
                });
            }
        } else {
            // Try compact forms for lower-priority segments (higher priority number)
            let mut remaining = self.cols;
            let mut pending: Vec<(usize, &Segment, bool)> = Vec::new();

            for (idx, seg) in &candidates {
                let width = if remaining < seg.preferred_width + separator_width {
                    seg.compact_width()
                } else {
                    seg.preferred_width
                };

                let needed = width + if pending.is_empty() { 0 } else { separator_width };
                if remaining >= needed {
                    let is_compact = width == seg.compact_width() && seg.compact_content.is_some();
                    remaining -= needed;
                    pending.push((*idx, seg, is_compact));
                }
                // If it doesn't fit even compact, skip it
            }

            // Restore original ordering
            pending.sort_by_key(|(idx, _, _)| *idx);

            for (idx, seg, is_compact) in pending {
                let content = if is_compact {
                    seg.compact_content.clone().unwrap_or_else(|| seg.content.clone())
                } else {
                    seg.content.clone()
                };
                result.push(ResolvedSegment {
                    original_index: idx,
                    content,
                    fg: seg.fg.clone(),
                    bg: seg.bg.clone(),
                    bold: seg.bold,
                    is_compact,
                });
            }
        }

        // Restore original order for display
        result.sort_by_key(|r| r.original_index);
        result
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSegment {
    pub original_index: usize,
    pub content: String,
    pub fg: String,
    pub bg: Option<String>,
    pub bold: bool,
    pub is_compact: bool,
}

pub struct LayoutPreset;

impl LayoutPreset {
    pub fn segment_order(preset: &str) -> &'static [&'static str] {
        match preset {
            "minimal" => &["directory", "character"],
            "powerline" => &["os", "ssh", "directory", "git", "exit_status", "command_duration", "jobs"],
            "classic" => &["ssh", "directory", "git", "exit_status", "character"],
            "pure" => &["directory", "git"],
            "dense" => &["os", "ssh", "directory", "git", "exit_status", "command_duration", "jobs"],
            _ => &["os", "directory", "git", "exit_status", "command_duration", "jobs", "ssh"],
        }
    }

    pub fn apply_filter(segments: &mut Vec<Segment>, preset: &str) {
        let allowed = Self::segment_order(preset);
        segments.retain(|s| allowed.contains(&s.name));
    }
}
