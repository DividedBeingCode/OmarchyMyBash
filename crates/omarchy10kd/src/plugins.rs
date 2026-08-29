//! Declarative segment plugins.
//!
//! A plugin is a directory under `~/.config/omarchy10k/plugins/<name>/`
//! containing a `plugin.toml` manifest. Plugins declare *data*, not code:
//! each `[[segments]]` entry is either
//!
//! * tier `"env"`     — renders the first set env key's value, zero forks; or
//! * tier `"command"` — runs a command in the prompt's cwd, async, TTL-cached,
//!   with a hard 500 ms timeout (the render path never awaits it).
//!
//! Presence on disk means *available*; presence in `[plugins] enabled` means
//! *active*. Dropping a directory in never activates it.
//!
//! Plugin segments join the built-in segment pipeline with the registry name
//! `plugin.<plugin>.<segment>` so they can never collide with (or shadow) a
//! built-in segment. The registry rebuilds on every `reload_config`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use unicode_width::UnicodeWidthStr;

use crate::layout::Segment;
use crate::segments::SegmentContext;

/// Prefix for every plugin-provided segment registry name
/// (`plugin.<plugin>.<segment>`); the render-path preset filter lets
/// names with this prefix through where built-in allowlists apply.
pub const PLUGIN_SEGMENT_PREFIX: &str = "plugin";

/// Hard cap on a command-tier plugin subprocess. The sub-5ms prompt
/// guarantee is absolute: the render path returns stale/absent and this
/// timeout bounds the background refresh.
pub const COMMAND_TIMEOUT_MS: u64 = 500;

/// Command output is capped to its first line, 256 bytes.
const MAX_OUTPUT_BYTES: usize = 256;

// ── Manifest ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginTier {
    Env,
    Command,
}

/// One `[[segments]]` entry of a `plugin.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PluginSegmentDef {
    pub name: String,
    pub icon: String,
    pub tier: PluginTier,
    /// env tier: keys tried in order; the first set (non-empty) key's value
    /// is rendered.
    pub env_keys: Vec<String>,
    /// command tier: the command line to run in the prompt's cwd.
    pub command: Option<String>,
    /// command tier: cache lifetime in seconds.
    pub ttl_secs: u64,
    /// Show only when this file name exists in the prompt's cwd
    /// (e.g. `".terraform"`). Evaluated before any fork.
    pub detect_cwd_file: Option<String>,
}

impl Default for PluginSegmentDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            icon: String::new(),
            tier: PluginTier::Env,
            env_keys: Vec::new(),
            command: None,
            ttl_secs: 10,
            detect_cwd_file: None,
        }
    }
}

/// A validated plugin: manifest + install directory + registry names.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub dir: PathBuf,
    pub segments: Vec<LoadedSegment>,
}

/// A manifest segment joined with its full registry name
/// (`plugin.<plugin>.<segment>`). Owned (`Arc<str>`) because
/// `Segment.name` carries dynamic plugin registry names that cannot be
/// static string literals.
#[derive(Debug, Clone)]
pub struct LoadedSegment {
    pub def: PluginSegmentDef,
    pub full_name: Arc<str>,
}

/// `[A-Za-z0-9_-]+`, short. Also the traversal guard: a name that fails this
/// can never be a single safe path component.
pub fn valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse and validate a `plugin.toml` document. `dir` is the plugin's
/// install directory; the manifest `name` must match it, so a hostile
/// manifest cannot direct writes/reads at another path.
pub fn parse_manifest(text: &str, dir: &Path) -> Result<Plugin, String> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| format!("invalid TOML: {e}"))?;

    if !valid_plugin_name(&manifest.name) {
        return Err(format!(
            "invalid plugin name {:?}: use only ASCII letters, digits, '-' and '_'",
            manifest.name
        ));
    }
    let dir_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if manifest.name != dir_name {
        return Err(format!(
            "plugin name {:?} does not match directory name {:?}",
            manifest.name, dir_name
        ));
    }

    let plugin_name = manifest.name.clone();
    let mut segments = Vec::new();
    for (i, def) in manifest.segments.into_iter().enumerate() {
        let what = format!("segment #{i} ({:?})", def.name);
        if !valid_plugin_name(&def.name) {
            return Err(format!(
                "{what}: invalid segment name, use only ASCII letters, digits, '-' and '_'"
            ));
        }
        match def.tier {
            PluginTier::Env => {
                if def.env_keys.is_empty() {
                    return Err(format!("{what}: env tier requires at least one env_keys entry"));
                }
                if def.command.is_some() {
                    return Err(format!("{what}: env tier must not set command"));
                }
            }
            PluginTier::Command => {
                let cmd = def.command.as_deref().unwrap_or_default();
                if cmd.trim().is_empty() {
                    return Err(format!("{what}: command tier requires a non-empty command"));
                }
                if !def.env_keys.is_empty() {
                    return Err(format!("{what}: command tier must not set env_keys"));
                }
            }
        }
        let full_name: Arc<str> = Arc::from(format!(
            "{PLUGIN_SEGMENT_PREFIX}.{plugin_name}.{}",
            def.name
        ));
        segments.push(LoadedSegment { def, full_name });
    }

    Ok(Plugin {
        name: manifest.name,
        description: manifest.description,
        version: manifest.version,
        author: manifest.author,
        dir: dir.to_path_buf(),
        segments,
    })
}

/// Raw manifest shape. `name` and per-segment `name`/`tier` are required
/// (missing fields fail the parse); everything else defaults.
#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    segments: Vec<PluginSegmentDef>,
}

/// Read every plugin under `dir`. A malformed or invalid plugin is skipped
/// with a `warn!` — a broken plugin must never fail config load or take
/// down a shell.
pub fn load_plugins(dir: &Path) -> Vec<Plugin> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            if dir.exists() {
                warn!("plugins dir {} unreadable: {e}", dir.display());
            }
            return Vec::new();
        }
    };

    let mut plugins = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("plugin.toml");
        let text = match std::fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(e) => {
                warn!(
                    "skipping plugin dir {}: no readable plugin.toml ({e})",
                    path.display()
                );
                continue;
            }
        };
        match parse_manifest(&text, &path) {
            Ok(plugin) => {
                debug!(
                    "loaded plugin {} ({} segment{})",
                    plugin.name,
                    plugin.segments.len(),
                    if plugin.segments.len() == 1 { "" } else { "s" }
                );
                plugins.push(plugin);
            }
            Err(e) => warn!("skipping plugin at {}: {e}", path.display()),
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

/// The daemon's plugin root: `<config dir>/plugins/<name>/`.
pub fn plugins_dir_for(config_dir: &Path) -> PathBuf {
    config_dir.join("plugins")
}

// ── Command-tier cache ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CachedOutput {
    /// `None` records a *negative* result — the command failed, timed out,
    /// or produced nothing. Caching that alongside successes is what keeps a
    /// broken plugin from re-forking on every single prompt.
    value: Option<String>,
    computed_at: Instant,
}

/// TTL cache for command-tier plugin segments, modelled on `GitCache`:
/// keyed by (segment registry name, cwd), in-flight deduped so a fast
/// prompt loop cannot spawn a process storm, generation-guarded against
/// resurrecting pre-invalidation snapshots.
#[derive(Debug, Default)]
pub struct PluginCache {
    cache: Arc<RwLock<HashMap<(String, PathBuf), CachedOutput>>>,
    in_flight: Arc<RwLock<HashSet<(String, PathBuf)>>>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl PluginCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fresh entry → its value immediately, **including a cached negative**
    /// (no reschedule: that is what bounds a failing command to one run per
    /// TTL rather than one per prompt). Stale entry → recompute in the
    /// background and return the stale value (the `git_stale` trade:
    /// blank-on-first-appearance in a directory, correct after).
    /// Missing → schedule and return `None`.
    pub async fn get(&self, key: (String, PathBuf), ttl: Duration, cwd: PathBuf, command: String) -> Option<String> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(&key) {
            if entry.computed_at.elapsed() < ttl {
                return entry.value.clone();
            }
            let stale = entry.value.clone();
            drop(cache);
            self.schedule(key, ttl, cwd, command);
            return stale;
        }
        drop(cache);
        self.schedule(key, ttl, cwd, command);
        None
    }

    fn schedule(&self, key: (String, PathBuf), ttl: Duration, cwd: PathBuf, command: String) {
        let cache = Arc::clone(&self.cache);
        let in_flight = Arc::clone(&self.in_flight);
        let generation = Arc::clone(&self.generation);
        let gen_at_start = generation.load(std::sync::atomic::Ordering::SeqCst);

        tokio::spawn(async move {
            {
                let mut flights = in_flight.write().await;
                if flights.contains(&key) {
                    return;
                }
                flights.insert(key.clone());
            }

            let output = run_command(&cwd, &command).await;

            if generation.load(std::sync::atomic::Ordering::SeqCst) == gen_at_start {
                // Record the outcome either way. Writing only on success
                // left a failing command permanently absent from the cache,
                // so every subsequent prompt saw a miss and scheduled again
                // — one subprocess per prompt, forever, with no backoff
                // (`in_flight` only dedupes *concurrent* runs). A cached
                // negative retries once per TTL instead.
                cache.write().await.insert(
                    key.clone(),
                    CachedOutput { value: output, computed_at: Instant::now() },
                );
            }

            // Bound memory: evict entries that are well past their TTL.
            {
                let mut c = cache.write().await;
                if c.len() > 256 {
                    c.retain(|_, v| v.computed_at.elapsed() < Duration::from_secs(600));
                }
            }

            in_flight.write().await.remove(&key);
        });
    }

    pub async fn invalidate_all(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.cache.write().await.clear();
        self.in_flight.write().await.clear();
    }
}

/// Quote-aware split of a command string into argv (double and single
/// quotes, `\"`/`\\` escapes inside double quotes). Never a shell string:
/// no `sh -c`, no interpolation, no injection surface.
fn split_command(s: &str) -> Option<Vec<String>> {
    let mut argv: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_token = true;
                while let Some(sc) = chars.next() {
                    if sc == '\'' {
                        break;
                    }
                    cur.push(sc);
                }
            }
            '"' => {
                in_token = true;
                while let Some(sc) = chars.next() {
                    match sc {
                        '"' => break,
                        '\\' => {
                            if let Some(&n) = chars.peek() {
                                if n == '"' || n == '\\' {
                                    cur.push(chars.next().unwrap());
                                    continue;
                                }
                            }
                            cur.push('\\');
                        }
                        _ => cur.push(sc),
                    }
                }
            }
            c if c.is_whitespace() => {
                if in_token {
                    argv.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            c => {
                in_token = true;
                cur.push(c);
            }
        }
    }
    if in_token {
        argv.push(cur);
    }
    if argv.is_empty() {
        None
    } else {
        Some(argv)
    }
}

/// Run a command-tier plugin command in the prompt's cwd. Returns the
/// first line of stdout (≤ 256 bytes) on success; `None` on failure,
/// non-zero exit, or timeout (the child is killed on timeout via
/// `kill_on_drop`).
async fn run_command(cwd: &Path, command: &str) -> Option<String> {
    let argv = split_command(command)?;
    let output = tokio::process::Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = match tokio::time::timeout(Duration::from_millis(COMMAND_TIMEOUT_MS), output).await
    {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            debug!("plugin command {command:?} failed to spawn: {e}");
            return None;
        }
        Err(_) => {
            debug!("plugin command {command:?} timed out after {COMMAND_TIMEOUT_MS}ms");
            return None;
        }
    };
    if !output.status.success() {
        debug!(
            "plugin command {command:?} exited {:?}; segment absent",
            output.status.code()
        );
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().next().unwrap_or_default();
    Some(line.chars().take(MAX_OUTPUT_BYTES).collect())
}

// ── Rendering ───────────────────────────────────────────────────────────────

/// Render every enabled plugin's segments into the built-in segment
/// pipeline. Called from the prompt path with the daemon's current plugin
/// registry; results flow through layout/priority filtering like built-ins.
pub async fn render_plugin_segments(
    ctx: &SegmentContext<'_>,
    cache: &PluginCache,
    plugins: &[Plugin],
) -> Vec<Segment> {
    let enabled = &ctx.config.plugins.enabled;
    if enabled.is_empty() || plugins.is_empty() {
        return Vec::new();
    }

    let cwd = PathBuf::from(ctx.cwd);
    let mut out = Vec::new();
    // Iterate in `[plugins] enabled` order; a plugin listed twice would
    // render twice, so dedupe by name (first listing wins).
    let mut seen = HashSet::new();
    for name in enabled {
        if !seen.insert(name.as_str()) {
            continue;
        }
        let Some(plugin) = plugins.iter().find(|p| &p.name == name) else {
            continue;
        };
        for seg in &plugin.segments {
            // Declarative visibility gate: evaluated before any fork.
            if let Some(file) = &seg.def.detect_cwd_file {
                if !file.is_empty() && !cwd.join(file).exists() {
                    continue;
                }
            }

            let content = match seg.def.tier {
                PluginTier::Env => {
                    let value = seg
                        .def
                        .env_keys
                        .iter()
                        .find_map(|key| ctx.env_get(key))
                        .filter(|v| !v.is_empty());
                    match value {
                        Some(v) => format_value(&seg.def.icon, &v),
                        None => continue,
                    }
                }
                PluginTier::Command => {
                    let cmd = seg.def.command.clone().unwrap_or_default();
                    let ttl = Duration::from_secs(seg.def.ttl_secs.max(1));
                    match cache
                        .get(
                            (seg.full_name.to_string(), cwd.clone()),
                            ttl,
                            cwd.clone(),
                            cmd,
                        )
                        .await
                    {
                        Some(v) => format_value(&seg.def.icon, &v),
                        // Cold miss: absent this prompt, cached next one.
                        None => continue,
                    }
                }
            };

            let preferred_width = UnicodeWidthStr::width(content.as_str()) as u16;
            out.push(Segment {
                name: Arc::clone(&seg.full_name),
                content: content.clone(),
                compact_content: Some(content),
                priority: 45,
                min_width: 1,
                preferred_width,
                hide_below_cols: 40,
                fg: ctx.palette.muted.fg_escape(),
                bg: None,
                bold: false,
                separator: None,
            });
        }
    }
    out
}

fn format_value(icon: &str, value: &str) -> String {
    if icon.is_empty() {
        value.to_string()
    } else {
        format!("{icon} {value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::git::GitStatus;
    use crate::terminal::TermCaps;
    use crate::theme::ThemePalette;
    use std::collections::HashMap;

    fn write_plugin(root: &Path, name: &str, manifest: &str) -> PathBuf {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), manifest).unwrap();
        dir
    }

    const GOOD_MANIFEST: &str = r#"
name = "aws"
description = "AWS profile and region"
version = "1.0.0"
author = "someone"

[[segments]]
name = "profile"
icon = "☁"
tier = "env"
env_keys = ["AWS_PROFILE", "AWS_REGION"]

[[segments]]
name = "package"
tier = "command"
command = "cat package.version"
ttl_secs = 30
detect_cwd_file = "package.json"
"#;

    #[test]
    fn valid_plugin_name_rules() {
        assert!(valid_plugin_name("aws"));
        assert!(valid_plugin_name("git-aliases_2"));
        assert!(!valid_plugin_name(""));
        assert!(!valid_plugin_name("../etc"));
        assert!(!valid_plugin_name("a/b"));
        assert!(!valid_plugin_name("has space"));
        assert!(!valid_plugin_name(&"x".repeat(65)));
    }

    #[test]
    fn manifest_parses_and_prefixes_registry_names() {
        let dir = std::env::temp_dir().join("aws");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let plugin = parse_manifest(GOOD_MANIFEST, &dir).expect("valid manifest");
        assert_eq!(plugin.name, "aws");
        assert_eq!(plugin.version, "1.0.0");
        assert_eq!(plugin.segments.len(), 2);
        assert_eq!(&*plugin.segments[0].full_name, "plugin.aws.profile");
        assert_eq!(&*plugin.segments[1].full_name, "plugin.aws.package");
        assert_eq!(plugin.segments[1].def.ttl_secs, 30);
        assert_eq!(plugin.segments[1].def.detect_cwd_file.as_deref(), Some("package.json"));
    }

    #[test]
    fn manifest_rejects_name_mismatch() {
        let dir = std::env::temp_dir().join("o10k-plugin-parse-mismatch");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = parse_manifest(GOOD_MANIFEST, &dir).unwrap_err();
        assert!(err.contains("does not match directory name"), "{err}");
    }

    #[test]
    fn manifest_rejects_traversal_name() {
        let dir = std::env::temp_dir().join("o10k-plugin-parse-traversal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = "name = \"../escape\"\ndescription = \"evil\"\n";
        // Even if the directory were named "../escape", the ident rule
        // rejects traversal characters first.
        let err = parse_manifest(text, &dir).unwrap_err();
        assert!(err.contains("invalid plugin name"), "{err}");
    }

    #[test]
    fn manifest_rejects_missing_name() {
        let dir = std::env::temp_dir().join("noname");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = parse_manifest("description = \"x\"\n", &dir).unwrap_err();
        assert!(err.contains("invalid TOML"), "{err}");
    }

    #[test]
    fn manifest_rejects_missing_segment_name() {
        let dir = std::env::temp_dir().join("p");
        let _ = std::fs::create_dir_all(&dir);
        let text = "name = \"p\"\n[[segments]]\ntier = \"env\"\nenv_keys = [\"A\"]\n";
        let err = parse_manifest(text, &dir).unwrap_err();
        // Container-level serde(default) makes a missing segment `name`
        // parse as empty, which the ident validation rejects.
        assert!(err.contains("invalid segment name"), "{err}");
    }

    #[test]
    fn manifest_rejects_tier_field_mismatches() {
        let dir = std::env::temp_dir().join("p");
        let _ = std::fs::create_dir_all(&dir);

        let env_no_keys = "name = \"p\"\n[[segments]]\nname = \"s\"\ntier = \"env\"\n";
        assert!(parse_manifest(env_no_keys, &dir)
            .unwrap_err()
            .contains("requires at least one env_keys entry"));

        let env_with_cmd =
            "name = \"p\"\n[[segments]]\nname = \"s\"\ntier = \"env\"\nenv_keys = [\"A\"]\ncommand = \"ls\"\n";
        assert!(parse_manifest(env_with_cmd, &dir).unwrap_err().contains("must not set command"));

        let cmd_no_cmd =
            "name = \"p\"\n[[segments]]\nname = \"s\"\ntier = \"command\"\n";
        assert!(parse_manifest(cmd_no_cmd, &dir).unwrap_err().contains("requires a non-empty command"));

        let cmd_with_keys =
            "name = \"p\"\n[[segments]]\nname = \"s\"\ntier = \"command\"\ncommand = \"ls\"\nenv_keys = [\"A\"]\n";
        assert!(parse_manifest(cmd_with_keys, &dir).unwrap_err().contains("must not set env_keys"));
    }

    #[test]
    fn load_plugins_skips_broken_and_keeps_good() {
        let root = std::env::temp_dir().join(format!("o10k-plugins-load-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        write_plugin(&root, "aws", GOOD_MANIFEST);
        write_plugin(&root, "badname", "name = \"not_this_dir\"\n");
        write_plugin(&root, "broken", "name = \"broken\n[[seg");
        std::fs::write(root.join("stray-file"), "not a plugin dir").unwrap();

        let plugins = load_plugins(&root);
        assert_eq!(plugins.len(), 1, "exactly the valid plugin loads");
        assert_eq!(plugins[0].name, "aws");
    }

    #[test]
    fn load_plugins_missing_dir_is_empty() {
        assert!(load_plugins(Path::new("/nonexistent/o10k/plugins")).is_empty());
    }

    fn make_ctx<'a>(
        env: Option<&'a HashMap<String, String>>,
        config: &'a Config,
        cwd: &'a str,
    ) -> SegmentContext<'a> {
        static THEME: std::sync::LazyLock<ThemePalette> =
            std::sync::LazyLock::new(ThemePalette::default);
        static CAPS: std::sync::LazyLock<TermCaps> = std::sync::LazyLock::new(TermCaps::detect);
        SegmentContext {
            cwd,
            home: "/home/u",
            exit_code: 0,
            cmd_duration_ms: 0,
            cols: 120,
            jobs: 0,
            in_ssh: false,
            git_status: &GIT,
            config,
            palette: &THEME,
            term_caps: &CAPS,
            env,
        }
    }

    static GIT: std::sync::LazyLock<GitStatus> = std::sync::LazyLock::new(GitStatus::default);

    #[tokio::test]
    async fn env_tier_renders_first_set_key_when_enabled() {
        let root = std::env::temp_dir().join(format!("o10k-plugins-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = write_plugin(&root, "aws", GOOD_MANIFEST);
        let plugins = load_plugins(&root);
        assert_eq!(plugins.len(), 1);

        let mut map = HashMap::new();
        map.insert("AWS_REGION".to_string(), "eu-west-1".to_string());

        // Disabled by default: nothing renders.
        let config = Config::default();
        let ctx = make_ctx(Some(&map), &config, "/tmp");
        let cache = PluginCache::new();
        let segments = render_plugin_segments(&ctx, &cache, &plugins).await;
        assert!(segments.is_empty(), "plugins land disabled");

        // Enabled: the first *set* key (AWS_REGION, second in the list) wins.
        let mut config = Config::default();
        config.plugins.enabled = vec!["aws".into()];
        let ctx = make_ctx(Some(&map), &config, "/tmp");
        let segments = render_plugin_segments(&ctx, &cache, &plugins).await;
        assert_eq!(&*segments[0].name, "plugin.aws.profile");
        assert!(segments[0].content.contains("eu-west-1"), "{}", segments[0].content);
        assert!(segments[0].content.starts_with("☁ "), "{}", segments[0].content);

        // Detect gate: package.json absent in /tmp, present in a fixture dir.
        let proj = root.join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::write(proj.join("package.json"), "{}").unwrap();
        std::fs::write(proj.join("package.version"), "9.9.9").unwrap();
        let proj_str = proj.display().to_string();
        let ctx = make_ctx(Some(&map), &config, &proj_str);
        // Cold miss: the command-tier segment is absent on the first render
        // while the refresh runs in the background.
        let first = render_plugin_segments(&ctx, &cache, &plugins).await;
        assert!(
            !first.iter().any(|s| s.name.as_ref() == "plugin.aws.package"),
            "cold cache renders nothing for command tier"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let segments = render_plugin_segments(&ctx, &cache, &plugins).await;
        let names: Vec<&str> = segments.iter().map(|s| &*s.name).collect();
        assert!(
            names.contains(&"plugin.aws.package"),
            "command segment appears when detect file present: {names:?}"
        );
        assert!(
            segments.iter().any(|s| s.name.as_ref() == "plugin.aws.package" && s.content.contains("9.9.9")),
            "command output reaches the segment (second prompt after cache fill)"
        );
        std::fs::remove_dir_all(&root).unwrap();
        let _ = dir;
    }

    #[test]
    fn split_command_handles_quotes() {
        assert_eq!(
            split_command("jq -r '.version' package.json"),
            Some(vec!["jq".into(), "-r".into(), ".version".into(), "package.json".into()])
        );
        assert_eq!(
            split_command("echo \"a b\""),
            Some(vec!["echo".into(), "a b".into()])
        );
        assert_eq!(split_command("   "), None);
        assert_eq!(split_command("''"), Some(vec![String::new()]));
    }
}
