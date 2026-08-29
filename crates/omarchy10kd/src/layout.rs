use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct Segment {
    /// Registry name. Built-ins are static strings; plugin segments use
    /// `plugin.<plugin>.<segment>` and need owned data, hence `Arc<str>`.
    pub name: std::sync::Arc<str>,
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
    pub separator_width: u16,
}

impl LayoutEngine {
    pub fn new(cols: u16) -> Self {
        Self { cols, separator_width: 1 }
    }

    pub fn new_with_separator_width(cols: u16, separator_width: u16) -> Self {
        Self { cols, separator_width }
    }

    /// Resolve which segments to show and whether to use compact form.
    /// Returns (visible_segments, total_width)
    pub fn resolve(&self, segments: &[Segment]) -> Vec<ResolvedSegment> {
        let mut candidates: Vec<(usize, &Segment)> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.hide_below_cols <= self.cols)
            .collect();

        candidates.sort_by_key(|(_, s)| s.priority);

        let separator_width = self.separator_width;
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
                // The first placed segment consumes no separator.
                let sep = if pending.is_empty() { 0 } else { separator_width };

                let width = if remaining < seg.preferred_width + sep {
                    seg.compact_width()
                } else {
                    seg.preferred_width
                };

                // Honor min_width: hide a segment rather than shrink it
                // below its minimum.
                if width < seg.min_width {
                    continue;
                }

                let needed = width + sep;
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
    pub fn separator(preset: &str) -> &'static str {
        match preset {
            "powerline" => " \u{e0b0} ", // Powerline arrow
            "classic" => " │ ",
            "dense" => " ",
            "pure" => " ",
            "minimal" => " ",
            _ => " ", // omarchy default
        }
    }

    pub fn is_single_line(preset: &str) -> bool {
        matches!(preset, "minimal" | "dense")
    }

    pub fn segment_order(preset: &str) -> &'static [&'static str] {
        match preset {
            "minimal" => &["directory"],
            "powerline" => &[
                "os", "ssh", "container", "directory", "git", "python_env", "toolchain", "nix",
                "k8s", "exit_status", "command_duration", "jobs", "time", "battery",
            ],
            "classic" => &["ssh", "directory", "git", "exit_status"],
            "pure" => &["directory", "git"],
            "dense" => &[
                "os", "ssh", "directory", "git", "exit_status", "command_duration", "jobs",
            ],
            _ => &[
                "os", "ssh", "container", "directory", "git", "python_env", "toolchain", "nix",
                "k8s", "exit_status", "command_duration", "jobs", "time", "battery",
            ],
        }
    }

    pub fn apply_filter(segments: &mut Vec<Segment>, preset: &str) {
        let allowed = Self::segment_order(preset);
        segments.retain(|s| allowed.contains(&&*s.name));
    }
}

/// Segments render_right can draw inline in the right prompt rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightSegment {
    CommandDuration,
    Git,
    Time,
    Battery,
    Jobs,
}

impl RightSegment {
    pub const fn name(self) -> &'static str {
        match self {
            Self::CommandDuration => "command_duration",
            Self::Git => "git",
            Self::Time => "time",
            Self::Battery => "battery",
            Self::Jobs => "jobs",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "command_duration" => Some(Self::CommandDuration),
            "git" => Some(Self::Git),
            "time" => Some(Self::Time),
            "battery" => Some(Self::Battery),
            "jobs" => Some(Self::Jobs),
            _ => None,
        }
    }
}

/// Resolve the configured `[prompt] right_segments` names into rail segments,
/// preserving order and skipping unknown names with a debug log.
pub fn resolve_right_rail(config_right_segments: &[String]) -> Vec<RightSegment> {
    config_right_segments
        .iter()
        .filter_map(|name| match RightSegment::from_name(name) {
            Some(seg) => Some(seg),
            None => {
                tracing::debug!(segment = %name, "unknown right_segments entry skipped");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(segs: &[&str]) -> Vec<String> {
        segs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn valid_set_resolves() {
        assert_eq!(
            resolve_right_rail(&names(&["command_duration", "git"])),
            vec![RightSegment::CommandDuration, RightSegment::Git]
        );
    }

    #[test]
    fn empty_set_resolves_empty() {
        assert!(resolve_right_rail(&[]).is_empty());
    }

    #[test]
    fn unknown_entries_skipped() {
        assert_eq!(
            resolve_right_rail(&names(&["bogus", "time", "weather", "battery"])),
            vec![RightSegment::Time, RightSegment::Battery]
        );
    }

    #[test]
    fn order_preserved() {
        assert_eq!(
            resolve_right_rail(&names(&["git", "jobs", "command_duration", "time"])),
            vec![
                RightSegment::Git,
                RightSegment::Jobs,
                RightSegment::CommandDuration,
                RightSegment::Time
            ]
        );
    }
}
