//! Project profiles (Tier C): a repo can carry its own prompt in a
//! `.o10k.toml` file at its root. Detection walks from the prompt cwd upward
//! and stops at project boundaries (`.git`) and at `$HOME` (exclusive), so a
//! profile applies within its project only.
//!
//! SECURITY: `.o10k.toml` comes from cloned repositories — untrusted input.
//! `load_profile_patch` enforces a strict display-key allowlist; state keys
//! (daemon, env, notifications, git, ...) are rejected with an error naming
//! the offending key. Every profile failure is warn-once and swallowed: a
//! broken repo profile must never fail the prompt.
use crate::config::Config;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _, Result};
use tracing::warn;

/// The repo-carried profile file.
pub const PROFILE_FILE: &str = ".o10k.toml";

/// Top-level keys a project profile may set. Display/render keys only —
/// `segments` sub-keys stay display-shaped (enabled flags, icons, glyph
/// names resolved via the standard glyph catalog at render time).
const ALLOWED_KEYS: &[&str] = &["style", "prompt", "segments", "theme", "frame"];

#[derive(Clone)]
struct CachedDetection {
    profile_dir: Option<PathBuf>,
    stamp: Instant,
}

/// Process-local cache of profile detection, keyed by the prompt cwd. Each
/// entry caches whether that cwd resolves to a profile directory (including
/// a negative result). Entries older than 30 s are recomputed on the next
/// render, so warm renders skip the directory walk; the map is bounded with
/// expired-then-oldest eviction (same pattern as the sibling-table cache).
static DETECTION_CACHE: LazyLock<Mutex<HashMap<PathBuf, CachedDetection>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const DETECTION_TTL: Duration = Duration::from_secs(30);
const MAX_DETECTION_ENTRIES: usize = 512;

/// One-shot warnings for broken profile files, keyed by profile path.
static WARNED_PROFILES: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Resolve the user's home directory (the exclusive upper bound of the
/// profile walk).
pub fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Detect the project profile directory for `cwd`: walk upward from `cwd`
/// and return the first directory containing a `.o10k.toml`. The walk stops
/// at `home` (exclusive — `$HOME` itself is never considered) and at any
/// directory containing a `.git` entry (a profile does not leak across the
/// project boundary). Results are cached per cwd with a 30 s TTL.
pub fn detect_profile(cwd: &Path, home: &Path) -> Option<PathBuf> {
    // Lock is held across the walk — same pattern as the sibling-table
    // cache in segments/directory.rs; the walk is short and its result
    // cached for 30 s.
    let mut cache = DETECTION_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    detect_profile_cached(&mut cache, cwd, home)
}

/// Cache-keyed core of [`detect_profile`], split out so tests can drive an
/// isolated map instead of the process-wide global.
fn detect_profile_cached(
    cache: &mut HashMap<PathBuf, CachedDetection>,
    cwd: &Path,
    home: &Path,
) -> Option<PathBuf> {
    if let Some(entry) = cache.get(cwd) {
        if entry.stamp.elapsed() < DETECTION_TTL {
            return entry.profile_dir.clone();
        }
    }
    let found = detect_profile_walk(cwd, home);
    insert_detection(cache, cwd, found.clone());
    found
}

/// Bounded insert: expired-then-oldest eviction (sibling-cache pattern).
fn insert_detection(
    cache: &mut HashMap<PathBuf, CachedDetection>,
    cwd: &Path,
    found: Option<PathBuf>,
) {
    if cache.len() >= MAX_DETECTION_ENTRIES {
        let now = Instant::now();
        cache.retain(|_, v| now.duration_since(v.stamp) < DETECTION_TTL);
        while cache.len() >= MAX_DETECTION_ENTRIES {
            let oldest = cache
                .iter()
                .min_by_key(|(_, v)| v.stamp)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    cache.remove(&k);
                }
                None => break,
            }
        }
    }
    cache.insert(
        cwd.to_path_buf(),
        CachedDetection {
            profile_dir: found,
            stamp: Instant::now(),
        },
    );
}

/// Drop all cached detection results (test helper: detection tests seed the
/// global cache and must not observe each other's cwd entries).
#[cfg(test)]
pub(crate) fn clear_detection_cache() {
    DETECTION_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

fn detect_profile_walk(cwd: &Path, home: &Path) -> Option<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        // `home` is exclusive: never consider $HOME itself or ascend past it.
        if dir == home {
            return None;
        }
        if dir.join(PROFILE_FILE).is_file() {
            return Some(dir);
        }
        // Project boundary: a repo root without a profile ends the walk —
        // a profile applies within its project only.
        if dir.join(".git").exists() {
            return None;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return None,
        }
    }
}

/// Load and validate the profile patch from a `.o10k.toml` file.
///
/// Accepts either a bare `config_set`-shaped patch (top-level `style` /
/// `prompt` / `segments` / `theme` / `frame` keys) or the wrapper form
/// `{ patch = { ... }, name = "..." }`. Anything else — state keys, unknown
/// keys, non-table documents — is rejected with an error naming the key.
/// An empty file yields an empty table (a no-op patch).
pub fn load_profile_patch(path: &Path) -> Result<toml::Value> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        // Lost the race with a delete after detection: an empty no-op patch.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(toml::Value::Table(toml::Table::new()))
        }
        Err(e) => {
            return Err(anyhow::Error::new(e))
                .with_context(|| format!("cannot read {}", path.display()))
        }
    };
    let value: toml::Value = toml::from_str(&text)
        .with_context(|| format!("cannot parse {}", path.display()))?;
    let table = match value.as_table() {
        Some(table) => table,
        None => bail!("{} must be a TOML table", path.display()),
    };
    if table.is_empty() {
        return Ok(toml::Value::Table(toml::Table::new()));
    }

    // Wrapper form: { patch = { ... }, name = "..." }.
    let patch_table: &toml::Table = if table.contains_key("patch") {
        for key in table.keys() {
            if key != "patch" && key != "name" {
                bail!("profile wrapper may only contain `patch` and `name`; rejected key `{key}`");
            }
        }
        if let Some(name) = table.get("name") {
            if name.as_str().is_none() {
                bail!("profile wrapper `name` must be a string");
            }
        }
        match table.get("patch").and_then(|p| p.as_table()) {
            Some(patch) => patch,
            None => bail!("profile wrapper `patch` must be a table"),
        }
    } else {
        table
    };

    for key in patch_table.keys() {
        if !ALLOWED_KEYS.contains(&key.as_str()) {
            bail!(
                "project profiles may only set display keys ({}); rejected key `{key}` — \
                 .o10k.toml comes from an untrusted repo",
                ALLOWED_KEYS.join(", ")
            );
        }
    }
    Ok(toml::Value::Table(patch_table.clone()))
}

/// Detect, load, and validate the profile patch for `cwd`. Returns `None`
/// when there is no profile, the file is empty, or anything fails — failures
/// are warn-once per profile path and never propagate (a broken repo profile
/// must never fail the prompt).
pub fn profile_patch_for(cwd: &Path, home: &Path) -> Option<toml::Value> {
    let dir = detect_profile(cwd, home)?;
    let path = dir.join(PROFILE_FILE);
    match load_profile_patch(&path) {
        Ok(patch) => {
            if patch.as_table().is_some_and(|t| t.is_empty()) {
                None
            } else {
                Some(patch)
            }
        }
        Err(e) => {
            warn_once(&path, format!("ignoring project profile: {e}"));
            None
        }
    }
}

/// Merge a validated profile patch over `base` (profile wins) and parse the
/// result back into a [`Config`]. Fails only if the merged document is no
/// longer representable — callers fall back to `base`.
pub fn apply_profile(base: &Config, patch: &toml::Value) -> Result<Config> {
    let mut doc =
        toml::Value::try_from(base).context("config is not TOML-representable")?;
    let table = doc.as_table_mut().context("config is not a table")?;
    if let Some(patch_table) = patch.as_table() {
        for (key, value) in patch_table {
            crate::server::merge_toml_value(
                table
                    .entry(key.clone())
                    .or_insert(toml::Value::Table(toml::Table::new())),
                value.clone(),
            );
        }
    }
    Ok(doc.try_into()?)
}

fn warn_once(path: &Path, message: String) {
    let mut warned = WARNED_PROFILES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if warned.insert(path.to_path_buf()) {
        warn!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_root(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "o10kd-profiles-test-{label}-{n}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        clear_detection_cache();
        dir
    }

    fn write_profile(dir: &Path, content: &str) {
        std::fs::write(dir.join(PROFILE_FILE), content).unwrap();
    }

    const DISPLAY_PATCH: &str = "\
[style]
preset = \"lean\"

[prompt]
blank_line = false

[segments.character]
success = \"λ\"

[theme]
source = \"omarchy\"

[frame]
enabled = false
";

    #[test]
    fn detect_finds_nearest_profile() {
        let root = temp_root("nearest");
        let a = root.join("a");
        let b = a.join("b");
        std::fs::create_dir_all(&b).unwrap();
        let nowhere = root.join("nowhere-home");

        write_profile(&root, DISPLAY_PATCH);
        assert_eq!(
            detect_profile(&b, &nowhere),
            Some(root.clone()),
            "walks up past dirs without a profile"
        );

        clear_detection_cache();
        write_profile(&a, DISPLAY_PATCH);
        assert_eq!(
            detect_profile(&b, &nowhere),
            Some(a.clone()),
            "nearest profile wins over an ancestor's"
        );

        clear_detection_cache();
        write_profile(&b, DISPLAY_PATCH);
        assert_eq!(detect_profile(&b, &nowhere), Some(b.clone()));
    }

    #[test]
    fn detect_stops_at_git_boundary() {
        let root = temp_root("git-boundary");
        write_profile(&root, DISPLAY_PATCH);
        let proj = root.join("proj");
        let src = proj.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(proj.join(".git")).unwrap();
        let nowhere = root.join("nowhere-home");

        assert_eq!(
            detect_profile(&src, &nowhere),
            None,
            "a profile above the repo root must not leak into the repo"
        );
        assert_eq!(
            detect_profile(&proj, &nowhere),
            None,
            "repo root without a profile ends the walk"
        );

        // A repo root WITH a profile: the file is checked before the boundary.
        clear_detection_cache();
        write_profile(&proj, DISPLAY_PATCH);
        assert_eq!(detect_profile(&src, &nowhere), Some(proj.clone()));
    }

    #[test]
    fn detect_home_is_exclusive_boundary() {
        let root = temp_root("home");
        let home = root.join("home");
        let proj = home.join("proj");
        let sub = proj.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        write_profile(&home, DISPLAY_PATCH);
        assert_eq!(
            detect_profile(&home, &home),
            None,
            "$HOME itself is never considered"
        );
        assert_eq!(
            detect_profile(&sub, &home),
            None,
            "the walk stops before $HOME even without a .git boundary"
        );

        clear_detection_cache();
        write_profile(&proj, DISPLAY_PATCH);
        assert_eq!(
            detect_profile(&sub, &home),
            Some(proj.clone()),
            "profiles inside $HOME's tree still apply"
        );

        // A cwd entirely outside $HOME walks up to the filesystem root.
        clear_detection_cache();
        write_profile(&root, DISPLAY_PATCH);
        assert_eq!(detect_profile(&proj, &home), Some(proj.clone()));
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        assert_eq!(detect_profile(&outside, &home), Some(root.clone()));
    }

    #[test]
    fn cache_reuses_result_until_expiry() {
        let root = temp_root("cache");
        let cwd = root.join("deep").join("deeper");
        std::fs::create_dir_all(&cwd).unwrap();
        let nowhere = root.join("nowhere-home");
        let mut cache: HashMap<PathBuf, CachedDetection> = HashMap::new();

        write_profile(&root, DISPLAY_PATCH);
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            Some(root.clone())
        );

        // Warm hit: the result survives the file disappearing.
        std::fs::remove_file(root.join(PROFILE_FILE)).unwrap();
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            Some(root.clone()),
            "warm render must hit the cache"
        );

        // Expire the entry: the next call re-walks and sees the deletion.
        cache.get_mut(&cwd).unwrap().stamp =
            Instant::now() - DETECTION_TTL - Duration::from_secs(1);
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            None,
            "expired entries recompute"
        );

        // Negative results are cached too: recreate the file, expect a hit.
        write_profile(&root, DISPLAY_PATCH);
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            None,
            "cached negative result is reused"
        );
        cache.get_mut(&cwd).unwrap().stamp =
            Instant::now() - DETECTION_TTL - Duration::from_secs(1);
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            Some(root.clone())
        );
    }

    #[test]
    fn cache_evicts_expired_then_oldest() {
        let root = temp_root("bound");
        let cwd = root.join("real");
        std::fs::create_dir_all(&cwd).unwrap();
        let nowhere = root.join("nowhere-home");
        write_profile(&root, DISPLAY_PATCH);
        let mut cache: HashMap<PathBuf, CachedDetection> = HashMap::new();
        assert_eq!(
            detect_profile_cached(&mut cache, &cwd, &nowhere),
            Some(root.clone())
        );

        // Phase 1: entries past the TTL are purged on the next insert
        // before any oldest-first eviction runs.
        for i in 0..MAX_DETECTION_ENTRIES {
            cache.insert(
                PathBuf::from(format!("/expired/{i}")),
                CachedDetection {
                    profile_dir: None,
                    stamp: Instant::now() - DETECTION_TTL - Duration::from_secs(1),
                },
            );
        }
        let probe = root.join("probe");
        std::fs::create_dir_all(&probe).unwrap();
        detect_profile_cached(&mut cache, &probe, &nowhere);
        assert!(
            !cache.keys().any(|k| k.starts_with("/expired/")),
            "expired entries must be purged, got {}",
            cache.len()
        );
        assert!(cache.contains_key(&cwd), "fresh real entry survives");
        assert!(cache.contains_key(&probe), "newly inserted entry survives");

        // Phase 2: at the cap, eviction drops the oldest stamps first and
        // keeps the map bounded.
        cache.clear();
        for i in 0..MAX_DETECTION_ENTRIES {
            cache.insert(
                PathBuf::from(format!("/fresh/{i}")),
                CachedDetection {
                    profile_dir: None,
                    // Distinct millisecond stamps, all within the TTL;
                    // i = MAX-1 is uniquely the oldest.
                    stamp: Instant::now() - Duration::from_millis(i as u64 + 1),
                },
            );
        }
        let probe2 = root.join("probe2");
        std::fs::create_dir_all(&probe2).unwrap();
        detect_profile_cached(&mut cache, &probe2, &nowhere);
        assert!(
            cache.len() <= MAX_DETECTION_ENTRIES,
            "cache must stay bounded, got {}",
            cache.len()
        );
        assert!(
            !cache.contains_key(&PathBuf::from(format!(
                "/fresh/{}",
                MAX_DETECTION_ENTRIES - 1
            ))),
            "the oldest entry must be evicted"
        );
        assert!(
            cache.contains_key(&PathBuf::from("/fresh/0")),
            "the newest synthetic entry survives"
        );
        assert!(cache.contains_key(&probe2), "the new entry survives");
    }

    #[test]
    fn allowlist_accepts_display_keys_and_wrapper() {
        let root = temp_root("allow");
        write_profile(&root, DISPLAY_PATCH);
        let patch = load_profile_patch(&root.join(PROFILE_FILE)).expect("display keys accepted");
        assert_eq!(
            patch["segments"]["character"]["success"].as_str(),
            Some("λ"),
            "glyph shortcut reaches the patch verbatim"
        );
        assert_eq!(patch["style"]["preset"].as_str(), Some("lean"));
        assert_eq!(patch["prompt"]["blank_line"].as_bool(), Some(false));

        // `name` as a table must fail; as a string must pass.
        write_profile(
            &root,
            "[patch.style]\npreset = \"lean\"\n\nname = \"my-repo\"\n",
        );
        let patch = load_profile_patch(&root.join(PROFILE_FILE)).expect("wrapper accepted");
        assert_eq!(patch["style"]["preset"].as_str(), Some("lean"));
        assert_eq!(patch.as_table().unwrap().len(), 1, "wrapper unwrapped");
    }

    #[test]
    fn allowlist_rejects_state_and_unknown_keys() {
        let root = temp_root("reject");
        let rejected = [
            "daemon",
            "env",
            "notifications",
            "git",
            "socket",
            "log_level",
            "looks",
            "terminal",
            "directory",
            "mystery_key",
        ];
        for key in rejected {
            write_profile(&root, &format!("[{key}]\nfoo = 1\n"));
            let err = load_profile_patch(&root.join(PROFILE_FILE))
                .expect_err("state/unknown keys rejected");
            assert!(
                err.to_string().contains(key),
                "error must name the rejected key `{key}`, got: {err}"
            );
        }

        // Rejection also applies inside the wrapper form.
        write_profile(&root, "[patch.daemon]\nfoo = 1\n");
        let err = load_profile_patch(&root.join(PROFILE_FILE)).expect_err("wrapper reject");
        assert!(err.to_string().contains("daemon"), "got: {err}");

        // `env.watch` reaches the daemon through `[env]` — rejected at the
        // top level regardless of nesting.
        write_profile(&root, "[env]\nwatch = true\n");
        assert!(load_profile_patch(&root.join(PROFILE_FILE)).is_err());
    }

    #[test]
    fn rejects_non_table_and_malformed_files() {
        let root = temp_root("malformed");

        write_profile(&root, "42\n");
        assert!(load_profile_patch(&root.join(PROFILE_FILE)).is_err(), "scalar");

        write_profile(&root, "[1, 2, 3]\n");
        assert!(load_profile_patch(&root.join(PROFILE_FILE)).is_err(), "array");

        write_profile(&root, "[style\npreset = \"lean\"\n");
        assert!(
            load_profile_patch(&root.join(PROFILE_FILE)).is_err(),
            "parse error"
        );

        // Wrapper with an unexpected extra key.
        write_profile(
            &root,
            "name = \"x\"\n[patch.style]\npreset = \"lean\"\n\n[patch.extra]\nfoo = 1\n",
        );
        let err = load_profile_patch(&root.join(PROFILE_FILE)).expect_err("extra wrapper key");
        assert!(err.to_string().contains("extra"), "got: {err}");

        // Wrapper with a non-string name.
        write_profile(&root, "name = 3\n[patch.style]\npreset = \"lean\"\n");
        assert!(load_profile_patch(&root.join(PROFILE_FILE)).is_err());
    }

    #[test]
    fn empty_file_is_a_noop() {
        let root = temp_root("empty");
        write_profile(&root, "");
        let patch =
            load_profile_patch(&root.join(PROFILE_FILE)).expect("empty file is not an error");
        assert!(patch.as_table().unwrap().is_empty());
        assert_eq!(
            profile_patch_for(&root, &root.join("nowhere-home")),
            None,
            "empty profile resolves to no patch"
        );
    }

    #[test]
    fn profile_patch_for_never_fails_the_prompt() {
        let root = temp_root("never-fail");
        let cwd = root.join("repo");
        std::fs::create_dir_all(&cwd).unwrap();

        // Malformed profile: swallowed, None.
        write_profile(&cwd, "[style\npreset = \"lean\"\n");
        assert_eq!(profile_patch_for(&cwd, &root.join("nohome")), None);

        // Security rejection: swallowed, None.
        write_profile(&cwd, "[daemon]\nfoo = 1\n");
        assert_eq!(profile_patch_for(&cwd, &root.join("nohome")), None);

        // Valid profile: resolved.
        write_profile(&cwd, "[segments.character]\nsuccess = \"λ\"\n");
        let patch = profile_patch_for(&cwd, &root.join("nohome")).expect("valid profile");
        assert_eq!(patch["segments"]["character"]["success"].as_str(), Some("λ"));
    }

    #[test]
    fn apply_profile_merges_over_base() {
        let base = Config::default();
        assert!(base.prompt.blank_line, "sanity: default is on");
        let patch: toml::Value =
            toml::from_str("[prompt]\nblank_line = false\n\n[segments.character]\nsuccess = \"λ\"\n")
                .unwrap();
        let merged = apply_profile(&base, &patch).expect("merge ok");
        assert!(!merged.prompt.blank_line, "profile wins over base");
        assert_eq!(merged.segments.character.success, "λ");
        assert_eq!(
            merged.style.preset, base.style.preset,
            "untouched keys keep the base value"
        );
    }
}
