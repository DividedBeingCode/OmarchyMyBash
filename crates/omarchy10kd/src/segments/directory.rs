use crate::layout::Segment;
use crate::render::wrap_np;
use crate::segments::SegmentContext;
use unicode_width::UnicodeWidthStr;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub fn render(ctx: &SegmentContext<'_>) -> Option<Segment> {
    let path = ctx.cwd;
    let home = ctx.home;

    let display_path = if !home.is_empty()
        && std::path::Path::new(path).starts_with(std::path::Path::new(home))
    {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };

    let display_path = if ctx.config.directory.unique {
        shorten_unique(path, home, &ctx.config.directory.anchors)
    } else {
        display_path
    };

    let strategy = ctx.config.directory.strategy.as_str();
    let max_len = ctx.config.directory.max_length;

    let (content, compact) = match strategy {
        "full" => (display_path.clone(), display_path.clone()),
        "truncate" => {
            let truncated = truncate_path(&display_path, max_len);
            (truncated.clone(), truncated)
        }
        _ => {
            let compact = smart_truncate(&display_path, max_len, ctx.config.directory.repo_root_style.as_str());
            (display_path.clone(), compact)
        }
    };

    let bold = ctx.config.directory.repo_root_style == "bold";

    let display_content = if ctx.term_caps.has_osc8 {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_default();
        let abs_path = ctx.cwd;
        let osc_open = format!("\x1b]8;;file://{hostname}{}\x1b\\", percent_encode_path(abs_path));
        let osc_close = "\x1b]8;;\x1b\\";
        format!(
            "{}{}{}",
            wrap_np(&osc_open),
            content,
            wrap_np(osc_close)
        )
    } else {
        content.clone()
    };

    let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
    let compact_width = UnicodeWidthStr::width(compact.as_str()) as u16;

    Some(Segment {
        name: "directory",
        content: display_content,
        compact_content: Some(compact),
        priority: 10,
        min_width: compact_width.min(10),
        preferred_width,
        hide_below_cols: 0,
        fg: ctx.palette.accent.fg_escape(),
        bg: None,
        bold,
        separator: None,
    })
}

// ---------------------------------------------------------------------------
// truncate_to_unique shortening
// ---------------------------------------------------------------------------

/// Process-local cache of sibling tables, keyed by cwd. Each entry holds, per
/// component of the cwd, that component's sibling directory names and whether
/// the component is an anchor (its directory contains an anchor file). Entries
/// older than 30 s are recomputed on the next render, so warm renders do zero
/// filesystem reads.
#[derive(Clone)]
struct SiblingTables {
    /// `tables[i]` describes component `i` of the cwd (root excluded): the
    /// directory names of the component's siblings, and whether the component
    /// itself is an anchor. `None` = a read error occurred; the component
    /// falls back to its full name.
    tables: Vec<Option<(Vec<String>, bool)>>,
    stamp: Instant,
}

static SIBLING_CACHE: LazyLock<Mutex<HashMap<PathBuf, SiblingTables>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const SIBLING_TTL: Duration = Duration::from_secs(30);

/// Shorten `cwd` to unique prefixes: anchor directories and the last
/// component keep their full names, everything else shrinks to the fewest
/// leading characters that stay unambiguous among its sibling directories.
/// The home prefix renders as `~`, exactly as the non-unique path does.
fn shorten_unique(cwd: &str, home: &str, anchors: &[String]) -> String {
    let comps: Vec<String> = Path::new(cwd)
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if comps.is_empty() {
        return cwd.to_string();
    }
    let home_comps = if !home.is_empty() && Path::new(cwd).starts_with(Path::new(home)) {
        Path::new(home)
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count()
    } else {
        0
    };
    let absolute = Path::new(cwd).is_absolute();
    let tables = sibling_tables(Path::new(cwd), anchors);
    shorten_components(&comps, home_comps, absolute, &tables.tables)
}

fn shorten_components(
    comps: &[String],
    home_comps: usize,
    absolute: bool,
    tables: &[Option<(Vec<String>, bool)>],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if home_comps > 0 {
        parts.push("~".to_string());
    } else if absolute {
        // Empty head element makes `join` emit the leading slash.
        parts.push(String::new());
    }
    let last = comps.len().saturating_sub(1);
    for (i, comp) in comps.iter().enumerate() {
        if i < home_comps {
            continue; // covered by the `~` prefix
        }
        if i == last {
            parts.push(comp.clone());
            break;
        }
        let shortened = match tables.get(i).and_then(|t| t.as_ref()) {
            Some((siblings, false)) => {
                if siblings.len() <= 1 {
                    // Only child: one character is already unambiguous.
                    comp.chars().next().map(String::from).unwrap_or_else(|| comp.clone())
                } else {
                    unique_prefix_among(comp, siblings)
                }
            }
            // Anchor directory, unknown component, or read error: full name.
            _ => comp.clone(),
        };
        parts.push(shortened);
    }
    parts.join("/")
}

fn sibling_tables(cwd: &Path, anchors: &[String]) -> SiblingTables {
    let mut cache = SIBLING_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(entry) = cache.get(cwd) {
        if entry.stamp.elapsed() < SIBLING_TTL {
            return entry.clone();
        }
    }
    let tables = compute_tables(cwd, anchors);
    // Bound memory: drop expired entries when the map overflows the cap,
    // evicting least-recently-stamped first if still over. Entries are
    // cheap but a weeks-long daemon visiting hundreds of directories adds up.
    const MAX_SIBLING_ENTRIES: usize = 512;
    if cache.len() >= MAX_SIBLING_ENTRIES {
        let now = Instant::now();
        cache.retain(|_, v| now.duration_since(v.stamp) < SIBLING_TTL);
        while cache.len() >= MAX_SIBLING_ENTRIES {
            let oldest = cache.iter()
                .min_by_key(|(_, v)| v.stamp)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => { cache.remove(&k); }
                None => break,
            }
        }
    }
    cache.insert(cwd.to_path_buf(), tables.clone());
    tables
}

fn compute_tables(cwd: &Path, anchors: &[String]) -> SiblingTables {
    let comps: Vec<&OsStr> = cwd
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .map(|c| c.as_os_str())
        .collect();
    let mut dir = PathBuf::new();
    if cwd.is_absolute() {
        dir.push("/");
    }
    let mut tables = Vec::with_capacity(comps.len());
    for (i, comp) in comps.iter().enumerate() {
        if i + 1 == comps.len() {
            break; // the last component is never shortened
        }
        dir.push(comp);
        // Sibling directories of the component, in its parent.
        let siblings = dir.parent().and_then(list_dirs);
        // Anchor check: does the component directory contain an anchor file?
        let is_anchor = read_dir_contains_anchor(&dir, anchors);
        tables.push(match (siblings, is_anchor) {
            (Some(siblings), Some(is_anchor)) => Some((siblings, is_anchor)),
            _ => None,
        });
    }
    SiblingTables { tables, stamp: Instant::now() }
}

/// Directory entries of `dir` that are themselves directories. Entries whose
/// type cannot be checked count as directories, so ambiguity is never
/// silently hidden. `None` when the directory cannot be read.
fn list_dirs(dir: &Path) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(true);
        if is_dir {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Some(names)
}

fn read_dir_contains_anchor(dir: &Path, anchors: &[String]) -> Option<bool> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if anchors.iter().any(|a| name.as_os_str() == OsStr::new(a.as_str())) {
            return Some(true);
        }
    }
    Some(false)
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if UnicodeWidthStr::width(path) <= max_len {
        return path.to_string();
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    let first = parts[0];
    let last = parts[parts.len() - 1];
    format!("{first}/\u{2026}/{last}")
}

fn smart_truncate(path: &str, max_len: usize, _repo_root_style: &str) -> String {
    if UnicodeWidthStr::width(path) <= max_len {
        return path.to_string();
    }

    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return path.to_string();
    }

    // Keep first component (~ or /) and last component, truncate middle
    let first = parts[0]; // "~" or ""
    let last = parts[parts.len() - 1];

    let mut result_parts = vec![first.to_string()];

    // Check if any middle component is a repo root
    let mut current_path = if first == "~" {
        std::env::var("HOME").unwrap_or_default()
    } else {
        String::new()
    };

    for (i, part) in parts.iter().enumerate().skip(1) {
        if i == parts.len() - 1 {
            result_parts.push(part.to_string());
            break;
        }

        current_path = format!("{current_path}/{part}");
        let is_repo_root = Path::new(&current_path).join(".git").exists();

        if is_repo_root {
            result_parts.push(part.to_string());
        } else {
            // Truncate to first unique character
            let truncated = unique_prefix(part, &parts[1..i], &parts[i + 1..parts.len() - 1]);
            result_parts.push(truncated);
        }
    }

    let result = result_parts.join("/");
    if UnicodeWidthStr::width(result.as_str()) <= max_len {
        result
    } else {
        // Last resort: keep just first + last
        format!("{first}/…/{last}")
    }
}

fn unique_prefix(target: &str, before: &[&str], after: &[&str]) -> String {
    let siblings: Vec<&&str> = before.iter().chain(after.iter()).collect();
    unique_prefix_among(target, &siblings)
}

/// Fewest leading characters of `target` that no sibling shares as a prefix.
fn unique_prefix_among<S: AsRef<str>>(target: &str, siblings: &[S]) -> String {
    let chars: Vec<char> = target.chars().collect();

    for len in 1..=chars.len() {
        let prefix: String = chars[..len].iter().collect();
        let is_unique = siblings
            .iter()
            .all(|s| s.as_ref() == target || !s.as_ref().starts_with(&prefix));
        if is_unique {
            return prefix;
        }
    }

    target.to_string()
}

/// Percent-encode a path for use in a file:// URI: everything outside the
/// RFC 3986 unreserved set plus `/` is escaped, so spaces, `#`, `%`, `?`,
/// and control characters cannot corrupt the hyperlink target.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b'/' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_substitution() {
        let path = "/home/ian/Code/omarchy10k";
        let home = "/home/ian";
        let display = if std::path::Path::new(path).starts_with(std::path::Path::new(home)) {
            format!("~{}", &path[home.len()..])
        } else {
            path.to_string()
        };
        assert_eq!(display, "~/Code/omarchy10k");
    }

    #[test]
    fn test_short_path_no_truncation() {
        assert_eq!(smart_truncate("~/Code", 40, "bold"), "~/Code");
    }

    #[test]
    fn test_home_prefix_not_path_aware_false_positive() {
        let path = "/home/ian2/projects";
        let home = "/home/ian";
        let matches = std::path::Path::new(path).starts_with(std::path::Path::new(home));
        assert!(!matches, "/home/ian2 should NOT match /home/ian");
    }

    #[test]
    fn test_unique_prefix_multibyte() {
        let result = unique_prefix("données", &["docs"], &[]);
        assert!(result.is_char_boundary(result.len()), "prefix must be valid UTF-8");
        assert!(!result.is_empty());
    }

    fn tbl(siblings: &[&str], anchor: bool) -> Option<(Vec<String>, bool)> {
        Some((siblings.iter().map(|s| s.to_string()).collect(), anchor))
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("o10k-unique-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_unique_collision_extends_prefix() {
        // "Code" collides with "Cool"; "project" collides with "project2".
        let comps = vec![
            "ian".to_string(),
            "Code".to_string(),
            "project".to_string(),
            "src".to_string(),
        ];
        let tables = vec![
            None,
            tbl(&["Code", "Cool"], false),        // C → Co → Cod
            tbl(&["project", "project2"], false), // extends to the full name
            tbl(&["src", "other"], false),        // last component is ignored
        ];
        assert_eq!(shorten_components(&comps, 1, false, &tables), "~/Cod/project/src");
    }

    #[test]
    fn test_unique_anchor_never_shortened() {
        let comps = vec![
            "ian".to_string(),
            "work".to_string(),
            "src".to_string(),
        ];
        let tables = vec![None, tbl(&["work", "workflow"], true), None];
        assert_eq!(shorten_components(&comps, 1, false, &tables), "~/work/src");
    }

    #[test]
    fn test_unique_single_child_one_char() {
        let comps = vec![
            "ian".to_string(),
            "Work".to_string(),
            "src".to_string(),
        ];
        let tables = vec![None, tbl(&["Work"], false), None];
        assert_eq!(shorten_components(&comps, 1, false, &tables), "~/W/src");
    }

    #[test]
    fn test_unique_unicode_components() {
        let comps = vec![
            "ian".to_string(),
            "données".to_string(),
            "日本語".to_string(),
            "src".to_string(),
        ];
        let tables = vec![
            None,
            tbl(&["données", "docs"], false), // d → do → don
            tbl(&["日本語"], false),          // only child → first char
            None,
        ];
        let out = shorten_components(&comps, 1, false, &tables);
        assert_eq!(out, "~/don/日/src");
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn test_unique_deep_path_without_home() {
        let comps = vec![
            "usr".to_string(),
            "local".to_string(),
            "share".to_string(),
            "doc".to_string(),
        ];
        let tables = vec![
            tbl(&["usr", "var"], false),   // u
            tbl(&["local", "lib"], false), // l → lo
            tbl(&["share", "man"], false), // s
            None,
        ];
        assert_eq!(shorten_components(&comps, 0, true, &tables), "/u/lo/s/doc");
    }

    #[test]
    fn test_unique_read_error_falls_back_full() {
        let comps = vec![
            "ian".to_string(),
            "Code".to_string(),
            "src".to_string(),
        ];
        let tables = vec![None, None, None];
        assert_eq!(shorten_components(&comps, 1, false, &tables), "~/Code/src");
    }

    #[test]
    fn test_unique_home_root_renders_tilde() {
        let home = temp_root("eq");
        let out = shorten_unique(home.to_str().unwrap(), home.to_str().unwrap(), &[]);
        assert_eq!(out, "~");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_shorten_unique_with_home() {
        let home = temp_root("home");
        std::fs::create_dir_all(home.join("Work/project/src")).unwrap();
        std::fs::create_dir_all(home.join("Work/prelude")).unwrap();
        // project is an anchor directory.
        std::fs::write(home.join("Work/project/Cargo.toml"), "").unwrap();
        let cwd = home.join("Work/project/src");
        let anchors = vec![".git".to_string(), "Cargo.toml".to_string()];
        let out = shorten_unique(cwd.to_str().unwrap(), home.to_str().unwrap(), &anchors);
        assert_eq!(out, "~/W/project/src");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_shorten_unique_outside_home() {
        let root = temp_root("nohome");
        std::fs::create_dir_all(root.join("Work/project/src")).unwrap();
        std::fs::create_dir_all(root.join("Workflow/other")).unwrap();
        let cwd = root.join("Work/project/src");
        let out = shorten_unique(cwd.to_str().unwrap(), "", &[".git".to_string()]);
        // "Work" collides with "Workflow" down to the full name; "project" is
        // an only child; "src" is the last component and stays full.
        assert!(out.starts_with('/'), "absolute display expected: {out}");
        assert!(out.ends_with("/Work/p/src"), "unexpected shortening: {out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sibling_cache_reuse_and_expiry() {
        let root = temp_root("cache");
        let cwd = root.join("a/b");
        std::fs::create_dir_all(&cwd).unwrap();
        let anchors = vec![".git".to_string()];

        let first = sibling_tables(&cwd, &anchors);
        // Warm render: a new sibling appears in the temp root but must not be
        // seen yet.
        std::fs::create_dir_all(root.join("a2")).unwrap();
        let second = sibling_tables(&cwd, &anchors);
        assert_eq!(second.tables, first.tables, "warm render must hit the cache");

        // Expire the entry: the next render must recompute.
        let mut cache = SIBLING_CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get_mut(&cwd).unwrap().stamp = Instant::now() - SIBLING_TTL - Duration::from_secs(1);
        drop(cache);
        let third = sibling_tables(&cwd, &anchors);
        let entry = third.tables[2].as_ref().unwrap();
        assert!(
            entry.0.iter().any(|s| s == "a2"),
            "expired entry must recompute: {:?}",
            entry.0
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    static TEST_THEME: LazyLock<crate::theme::ThemePalette> =
        LazyLock::new(crate::theme::ThemePalette::default);
    static TEST_CAPS: LazyLock<crate::terminal::TermCaps> =
        LazyLock::new(crate::terminal::TermCaps::detect);

    #[test]
    fn test_render_unique_end_to_end() {
        let home = temp_root("render");
        std::fs::create_dir_all(home.join("Work/project/src")).unwrap();
        std::fs::create_dir_all(home.join("Work/prelude")).unwrap();
        // project is an anchor directory: never shortened.
        std::fs::write(home.join("Work/project/Cargo.toml"), "").unwrap();
        let cwd = home.join("Work/project/src");

        let mut config = crate::config::Config::default();
        config.directory.unique = true;
        let git = crate::git::GitStatus::default();
        let ctx = SegmentContext {
            cwd: cwd.to_str().unwrap(),
            home: home.to_str().unwrap(),
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: &git,
            config: &config,
            palette: &TEST_THEME,
            term_caps: &TEST_CAPS,
            env: None,
        };
        let seg = render(&ctx).unwrap();
        // Anchor `project` and last component `src` stay full; `Work` collides
        // with nothing under home here, so it is unique to one character… but
        // `prelude` does not share a prefix, so `Work` → `W`.
        assert!(seg.content.contains("~/W/project/src"), "got: {:?}", seg.content);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn test_render_default_path_unchanged() {
        let home = temp_root("render-default");
        std::fs::create_dir_all(home.join("Work/project/src")).unwrap();
        std::fs::create_dir_all(home.join("Work/prelude")).unwrap();
        let cwd = home.join("Work/project/src");

        let config = crate::config::Config::default();
        let git = crate::git::GitStatus::default();
        let ctx = SegmentContext {
            cwd: cwd.to_str().unwrap(),
            home: home.to_str().unwrap(),
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: &git,
            config: &config,
            palette: &TEST_THEME,
            term_caps: &TEST_CAPS,
            env: None,
        };
        let seg = render(&ctx).unwrap();
        // unique=false (default): the full path, byte-identical to before.
        assert!(seg.content.contains("~/Work/project/src"), "got: {:?}", seg.content);
        let _ = std::fs::remove_dir_all(&home);
    }
}
