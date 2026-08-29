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

/// `version = "x.y.z"` from the `[package]` (Cargo) or `[project]`
/// (PEP 621) table only.
///
/// Scoping to those tables matters: taking the first `version` key anywhere
/// in the file makes a manifest that inherits its version
/// (`version.workspace = true`) report the first dependency's version
/// requirement instead — `[dependencies.serde] version = "1.0"` displayed as
/// the project version. A key outside those tables is never the project's
/// own version, so the segment stays hidden instead of showing a wrong one.
fn extract_toml_version(text: &str) -> Option<String> {
    let mut in_version_table = false;
    for line in text.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            // `[package]` / `[project]`, but not `[package.metadata…]` —
            // and any other table ends the scope.
            let name = header.split(']').next().unwrap_or("").trim();
            in_version_table = name == "package" || name == "project";
            continue;
        }
        if !in_version_table {
            continue;
        }
        // Tolerate `version="x"` and `version  =  "x"`; reject
        // `version.workspace = true`, whose remainder does not start with
        // the assignment.
        let Some(rest) = line.strip_prefix("version") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix('"') else {
            continue;
        };
        let Some(end) = value.find('"') else {
            continue;
        };
        if end > 0 {
            return Some(value[..end].to_string());
        }
    }
    None
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
    fn test_toml_version_ignores_dependency_versions() {
        // A workspace-inheriting manifest must not report serde's version
        // requirement as the project version.
        let text = "[package]\nname = \"x\"\nversion.workspace = true\n\n\
                    [dependencies.serde]\nversion = \"1.0\"\n";
        assert_eq!(extract_toml_version(text), None);

        // ...and a real project version still wins over a later dependency.
        let text = "[package]\nname = \"x\"\nversion = \"0.4.2\"\n\n\
                    [dependencies]\nserde = { version = \"1.0\" }\n";
        assert_eq!(extract_toml_version(text).as_deref(), Some("0.4.2"));
    }

    #[test]
    fn test_toml_version_pep621_and_spacing() {
        assert_eq!(
            extract_toml_version("[project]\nversion=\"2.0.0\"\n").as_deref(),
            Some("2.0.0")
        );
        assert_eq!(
            extract_toml_version("[project]\nversion   =   \"2.1\"\n").as_deref(),
            Some("2.1")
        );
        // Sub-tables of [package] are not the package table.
        assert_eq!(
            extract_toml_version("[package.metadata.x]\nversion = \"9.9\"\n"),
            None
        );
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
