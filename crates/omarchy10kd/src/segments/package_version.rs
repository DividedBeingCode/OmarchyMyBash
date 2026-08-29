use crate::layout::Segment;
use super::SegmentContext;
use std::sync::LazyLock;
use super::util::TtlCache;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Project version segment: reads the version out of `package.json`,
/// `Cargo.toml`, or `pyproject.toml` in the cwd (first match wins). Parsing
/// is regex-free (we only need one quoted scalar after a `version` key) and
/// results are cached per cwd with a 10 s TTL, negative results included,
/// so warm prompts do zero file reads.
const VERSION_TTL: Duration = Duration::from_secs(10);

static VERSION_CACHE: LazyLock<TtlCache<Option<String>>> = LazyLock::new(|| TtlCache::new(512));

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    if !ctx.config.segments.package_version.enabled {
        return None;
    }

    let version = VERSION_CACHE.get_or(ctx.cwd, VERSION_TTL, || detect_version(ctx.cwd))?;
    let icon = &ctx.config.segments.package_version.icon;
    let content = format!("{icon} {version}");
    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;

    Some(Segment {
        name: "package_version".into(),
        content: content.clone(),
        compact_content: Some(icon.to_string()),
        priority: 33,
        min_width: 2,
        preferred_width,
        hide_below_cols: 50,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold: false,
        separator: None,
    })
}

fn detect_version(cwd: &str) -> Option<String> {
    let root = std::path::Path::new(cwd);
    if let Ok(text) = std::fs::read_to_string(root.join("package.json")) {
        if let Some(v) = extract_json_version(&text) {
            return Some(v);
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
        if let Some(v) = extract_toml_version(&text) {
            return Some(v);
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("pyproject.toml")) {
        if let Some(v) = extract_toml_version(&text) {
            return Some(v);
        }
    }
    None
}

/// Pull the first `"version": "x.y.z"` pair out of JSON text without a
/// regex dependency: locate the key, skip to the next quoted scalar.
fn extract_json_version(text: &str) -> Option<String> {
    let idx = text.find("\"version\"")?;
    let rest = &text[idx + "\"version\"".len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let quote = after.strip_prefix('"')?;
    let end = quote.find('"')?;
    let value = &quote[..end];
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// `version = "x.y.z"` under any table's first occurrence — good enough for
/// the common single-package manifest (`[package]` / `[project]`).
fn extract_toml_version(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("version = \""))
        .and_then(|rest| rest.find('"').map(|end| rest[..end].to_string()))
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_version() {
        let text = r#"{
  "name": "x",
  "version": "1.2.3",
  "scripts": {}
}"#;
        assert_eq!(extract_json_version(text).as_deref(), Some("1.2.3"));
    }

    #[test]
    fn test_json_version_missing() {
        assert_eq!(extract_json_version("{\"name\":\"x\"}"), None);
        assert_eq!(extract_json_version("{\"version\": \"\"}"), None);
    }

    #[test]
    fn test_toml_version() {
        assert_eq!(
            extract_toml_version("[package]\nname = \"x\"\nversion = \"0.4.2\"\n").as_deref(),
            Some("0.4.2")
        );
        assert_eq!(extract_toml_version("[package]\nname = \"x\"\n"), None);
    }

    #[test]
    fn test_detect_version_prefers_package_json() {
        let dir = std::env::temp_dir().join(format!("o10k-pkgver-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("package.json"), "{\"version\":\"9.9.9\"}").unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nversion = \"0.1.0\"\n").unwrap();
        let v = detect_version(dir.to_str().unwrap());
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(v.as_deref(), Some("9.9.9"));
    }
}
