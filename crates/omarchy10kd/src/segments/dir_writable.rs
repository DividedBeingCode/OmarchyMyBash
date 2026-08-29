use crate::layout::Segment;
use super::SegmentContext;
use std::sync::LazyLock;
use super::util::TtlCache;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Write-permission warning segment: renders a lock glyph only when the cwd
/// is NOT writable. The check is a real create/delete probe (metadata
/// permission bits miss ACLs, read-only mounts, and other-user ownership),
/// cached per cwd with a 10 s TTL so warm prompts do no syscalls at all —
/// the probe runs once per cwd per TTL window.
const WRITABLE_TTL: Duration = Duration::from_secs(10);

static WRITABLE_CACHE: LazyLock<TtlCache<bool>> = LazyLock::new(|| TtlCache::new(512));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.dir_writable.enabled {
        return None;
    }

    let writable = WRITABLE_CACHE.get_or(ctx.cwd, WRITABLE_TTL, || probe_writable(ctx.cwd));
    if writable {
        // Writable is the expected state: stay silent, no noise in the prompt.
        return None;
    }

    let icon = &ctx.config.segments.dir_writable.icon;
    let preferred_width = UnicodeWidthStr::width(icon.as_str()) as u16;

    Some(Segment {
        name: "dir_writable".into(),
        content: icon.clone(),
        compact_content: Some(icon.to_string()),
        priority: 6,
        min_width: 1,
        preferred_width,
        hide_below_cols: 40,
        fg: ctx.palette.red.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

/// Real probe: a unique temp file created (and immediately removed) inside
/// the directory is the only honest write test.
fn probe_writable(cwd: &str) -> bool {
    let path = std::path::Path::new(cwd);
    let probe = path.join(format!(".o10k-writetest-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(file) => {
            drop(file);
            std::fs::remove_file(&probe).ok();
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writable_dir_probes_true() {
        let dir = std::env::temp_dir().join(format!("o10k-wr-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = probe_writable(dir.to_str().unwrap());
        std::fs::remove_dir_all(&dir).ok();
        assert!(ok);
        // No probe file left behind.
        assert!(!dir.join(format!(".o10k-writetest-{}", std::process::id())).exists());
    }

    #[test]
    fn test_unwritable_dir_probes_false() {
        let base = std::env::temp_dir().join(format!("o10k-wr-ro-{}", std::process::id()));
        let dir = base.join("ro");
        std::fs::create_dir_all(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        }
        let ok = probe_writable(dir.to_str().unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        std::fs::remove_dir_all(&base).ok();
        #[cfg(unix)]
        assert!(!ok, "read-only dir must probe false on unix");
        let _ = ok;
    }

    #[test]
    fn test_cache_roundtrip_and_expiry() {
        // Drive the shared cache directly with a synthetic key: the value
        // must stick until expired.
        WRITABLE_CACHE.get_or("test-key", WRITABLE_TTL, || true);
        assert!(WRITABLE_CACHE.has_fresh("test-key", WRITABLE_TTL));
        WRITABLE_CACHE.expire("test-key");
        let recomputed = WRITABLE_CACHE.get_or("test-key", WRITABLE_TTL, || false);
        assert!(!recomputed);
    }
}
