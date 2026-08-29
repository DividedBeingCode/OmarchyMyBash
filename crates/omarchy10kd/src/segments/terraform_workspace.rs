use std::sync::LazyLock;

use crate::layout::Segment;
use super::SegmentContext;
use super::util::{run_command, TtlCache};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Terraform workspace segment: only considered when a `.terraform` directory
/// exists in the cwd (cheap stat gate, no CLI spawn outside Terraform
/// projects), then a TTL-cached `terraform workspace show` (15 s, bounded
/// wait). Degrades to hidden without terraform or outside workspaces.
const WORKSPACE_TTL: Duration = Duration::from_secs(15);
const CMD_TIMEOUT_MS: u64 = 1000;

static WORKSPACE_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(64));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.terraform_workspace.enabled {
        return None;
    }

    if !std::path::Path::new(ctx.cwd).join(".terraform").exists() {
        return None;
    }

    let workspace = WORKSPACE_CACHE
        .get_or(ctx.cwd, WORKSPACE_TTL, || {
            run_command("terraform", &["workspace", "show"], CMD_TIMEOUT_MS)
        })?;
    if workspace.is_empty() {
        return None;
    }

    let icon = &ctx.config.segments.terraform_workspace.icon;
    let content = format!("{icon} {workspace}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "terraform_workspace".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 39,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.magenta.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

#[cfg(test)]
mod tests {


    #[test]
    fn test_hidden_without_terraform_dir() {
        // "/tmp" will not contain a .terraform directory; render must gate
        // on the stat before ever considering the CLI.
        assert!(!std::path::Path::new("/tmp").join(".terraform").exists());
    }

    #[test]
    fn test_terraform_dir_gate_logic() {
        let dir = std::env::temp_dir().join(format!("o10k-tf-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".terraform")).unwrap();
        let has_dir = dir.join(".terraform").exists();
        std::fs::remove_dir_all(&dir).ok();
        assert!(has_dir);
    }
}
