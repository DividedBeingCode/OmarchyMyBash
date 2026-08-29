use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub is_repo: bool,
    pub branch: String,
    pub commit: String,
    pub tag: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub stashes: u32,
    pub action: Option<GitAction>,
    pub is_detached: bool,
    pub repo_root: PathBuf,
    pub worktree: Option<String>,
    pub stale: bool,
    /// URL of the `origin` remote, captured once per cache refresh. None
    /// when the repo has no origin.
    pub remote: Option<String>,
}

impl GitStatus {
    /// True when the worktree has any staged, unstaged, untracked, or
    /// conflicted change.
    pub fn is_dirty(&self) -> bool {
        self.staged > 0 || self.unstaged > 0 || self.untracked > 0 || self.conflicted > 0
    }
}

#[derive(Debug, Clone)]
pub enum GitAction {
    Merge,
    Rebase(String),
    CherryPick,
    Bisect,
    Revert,
}

impl std::fmt::Display for GitAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitAction::Merge => write!(f, "MERGE"),
            GitAction::Rebase(step) => write!(f, "REBASE {step}"),
            GitAction::CherryPick => write!(f, "CHERRY-PICK"),
            GitAction::Bisect => write!(f, "BISECT"),
            GitAction::Revert => write!(f, "REVERT"),
        }
    }
}

#[derive(Debug)]
struct CachedStatus {
    status: GitStatus,
    fetched_at: Instant,
}

#[derive(Debug)]
pub struct GitCache {
    cache: Arc<RwLock<HashMap<PathBuf, CachedStatus>>>,
    in_flight: Arc<RwLock<HashSet<PathBuf>>>,
    // Bumped on every invalidate; refresh tasks compare their start snapshot
    // against this at insert time so a pre-invalidation snapshot is dropped.
    generation: Arc<AtomicU64>,
    ttl_ms: AtomicU64,
}

impl GitCache {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashSet::new())),
            generation: Arc::new(AtomicU64::new(0)),
            ttl_ms: AtomicU64::new(ttl_ms),
        }
    }

    pub fn set_ttl(&self, ttl_ms: u64) {
        self.ttl_ms.store(ttl_ms, Ordering::Relaxed);
    }

    fn ttl(&self) -> Duration {
        Duration::from_millis(self.ttl_ms.load(Ordering::Relaxed))
    }

    pub async fn get_status(&self, cwd: &Path) -> GitStatus {
        let repo_root = match find_repo_root(cwd) {
            Some(root) => root,
            None => return GitStatus::default(),
        };

        let cache = self.cache.read().await;
        if let Some(cached) = cache.get(&repo_root) {
            if cached.fetched_at.elapsed() < self.ttl() {
                debug!("git cache fresh hit for {}", repo_root.display());
                return cached.status.clone();
            }

            debug!("git cache stale hit for {}", repo_root.display());
            let mut stale = cached.status.clone();
            stale.stale = true;
            drop(cache);
            self.schedule_refresh(repo_root);
            return stale;
        }
        drop(cache);

        debug!("git cache cold miss for {}", repo_root.display());
        self.schedule_refresh(repo_root.clone());
        GitStatus {
            is_repo: true,
            repo_root,
            stale: true,
            ..Default::default()
        }
    }

    fn schedule_refresh(&self, repo_root: PathBuf) {
        let cache = Arc::clone(&self.cache);
        let in_flight = Arc::clone(&self.in_flight);
        let generation = Arc::clone(&self.generation);
        let gen_at_start = generation.load(Ordering::SeqCst);
        let ttl = self.ttl();

        tokio::spawn(async move {
            {
                let mut flights = in_flight.write().await;
                if flights.contains(&repo_root) {
                    return;
                }
                flights.insert(repo_root.clone());
            }

            let status = fetch_git_status(&repo_root).await;

            if generation.load(Ordering::SeqCst) != gen_at_start {
                // Cache was invalidated while this refresh ran in flight;
                // re-inserting the pre-invalidation snapshot would resurrect
                // stale data. Drop the result — the next get_status schedules
                // a fresh refresh.
                debug!(
                    "dropped in-flight git refresh for {} (invalidated mid-flight)",
                    repo_root.display()
                );
            } else {
                match status {
                    Some(status) => {
                        cache.write().await.insert(repo_root.clone(), CachedStatus {
                            status,
                            fetched_at: Instant::now(),
                        });
                    }
                    None => {
                        // Fetch failed: keep any previous cache entry in place;
                        // only mark is_repo:false when we have nothing better,
                        // so the segment renders None instead of a fake repo.
                        let mut c = cache.write().await;
                        if !c.contains_key(&repo_root) {
                            c.insert(repo_root.clone(), CachedStatus {
                                status: GitStatus {
                                    is_repo: false,
                                    repo_root: repo_root.clone(),
                                    ..Default::default()
                                },
                                fetched_at: Instant::now(),
                            });
                        }
                    }
                }
            }

            // Bound memory: a long-lived daemon can touch hundreds of repos
            // over weeks. Expired entries are dead weight (get_status treats
            // them as stale anyway); drop them when the map overflows the
            // cap, evicting least-recently-fetched first if still over.
            const MAX_CACHE_ENTRIES: usize = 256;
            {
                let mut c = cache.write().await;
                if c.len() > MAX_CACHE_ENTRIES {
                    let now = Instant::now();
                    c.retain(|_, v| now.duration_since(v.fetched_at) <= ttl);
                    while c.len() > MAX_CACHE_ENTRIES {
                        let oldest = c.iter()
                            .min_by_key(|(_, v)| v.fetched_at)
                            .map(|(k, _)| k.clone());
                        match oldest {
                            Some(k) => { c.remove(&k); }
                            None => break,
                        }
                    }
                }
            }

            {
                let mut flights = in_flight.write().await;
                flights.remove(&repo_root);
            }
        });
    }

    pub async fn invalidate(&self, repo_root: &Path) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut cache = self.cache.write().await;
        cache.remove(repo_root);
        debug!("invalidated git cache for {}", repo_root.display());
    }

    pub async fn invalidate_all(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}


fn find_repo_root(mut dir: &Path) -> Option<PathBuf> {
    loop {
        let git_path = dir.join(".git");
        if git_path.exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn detect_worktree(repo_root: &Path) -> Option<String> {
    let git_path = repo_root.join(".git");
    if git_path.is_file() {
        let content = std::fs::read_to_string(&git_path).ok()?;
        let gitdir = content.strip_prefix("gitdir: ")?.trim();
        // Only treat as worktree if gitdir points into a worktrees/ directory
        if gitdir.contains("/worktrees/") {
            repo_root
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    } else {
        // Check if there's a commondir file (alternate worktree detection)
        let commondir = git_path.join("commondir");
        if commondir.exists() {
            repo_root
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    }
}

async fn fetch_git_status(repo_root: &Path) -> Option<GitStatus> {
    let start = Instant::now();

    let output = match tokio::process::Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain=v2", "--branch"])
        .current_dir(repo_root)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!("git status failed: {e}");
            return None;
        }
    };


    if !output.status.success() {
        warn!(
            "git status exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut status = parse_porcelain_v2(&stdout);
    status.is_repo = true;
    status.repo_root = repo_root.to_path_buf();
    status.worktree = detect_worktree(repo_root);

    // Origin URL: one extra git call per refresh; the cache makes it free
    // for warm renders. `remote get-url` resolves insteadOf rewrites and
    // exits non-zero with empty stdout when no origin is configured.
    if let Ok(remote_out) = tokio::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_root)
        .output()
        .await
    {
        let url = String::from_utf8_lossy(&remote_out.stdout).trim().to_string();
        if remote_out.status.success() && !url.is_empty() {
            status.remote = Some(url);
        }
    }

    if let Ok(stash_out) = tokio::process::Command::new("git")
        .args(["stash", "list"])
        .current_dir(repo_root)
        .output()
        .await
    {
        status.stashes = String::from_utf8_lossy(&stash_out.stdout)
            .lines()
            .count() as u32;
    }

    status.action = detect_git_action(repo_root);

    debug!(
        "git status for {} in {:?}",
        repo_root.display(),
        start.elapsed()
    );
    Some(status)
}

fn parse_porcelain_v2(output: &str) -> GitStatus {
    let mut status = GitStatus::default();

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            if rest == "(detached)" {
                status.is_detached = true;
                status.branch = "HEAD".into();
            } else {
                status.branch = rest.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            status.upstream = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() == 2 {
                status.ahead = parts[0]
                    .trim_start_matches('+')
                    .parse()
                    .unwrap_or(0);
                status.behind = parts[1]
                    .trim_start_matches('-')
                    .parse()
                    .unwrap_or(0);
            }
        } else if let Some(rest) = line.strip_prefix("# branch.oid ") {
            status.commit = rest.chars().take(8).collect();
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // Changed entry: index XY
            let xy: Vec<char> = line.chars().skip(2).take(2).collect();
            if xy.len() == 2 {
                if xy[0] != '.' {
                    status.staged += 1;
                }
                if xy[1] != '.' {
                    status.unstaged += 1;
                }
            }
        } else if line.starts_with("u ") {
            status.conflicted += 1;
        } else if line.starts_with("? ") {
            status.untracked += 1;
        }
    }

    status
}

fn detect_git_action(repo_root: &Path) -> Option<GitAction> {
    let git_dir = repo_root.join(".git");
    let git_dir = if git_dir.is_file() {
        // Worktree: read the gitdir pointer
        std::fs::read_to_string(&git_dir)
            .ok()
            .and_then(|content| {
                content
                    .strip_prefix("gitdir: ")
                    .map(|p| PathBuf::from(p.trim()))
            })
            .unwrap_or(git_dir)
    } else {
        git_dir
    };

    if git_dir.join("MERGE_HEAD").exists() {
        Some(GitAction::Merge)
    } else if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        let step = read_rebase_progress(&git_dir);
        Some(GitAction::Rebase(step))
    } else if git_dir.join("CHERRY_PICK_HEAD").exists() {
        Some(GitAction::CherryPick)
    } else if git_dir.join("BISECT_LOG").exists() {
        Some(GitAction::Bisect)
    } else if git_dir.join("REVERT_HEAD").exists() {
        Some(GitAction::Revert)
    } else {
        None
    }
}

fn read_rebase_progress(git_dir: &Path) -> String {
    let dir = if git_dir.join("rebase-merge").exists() {
        git_dir.join("rebase-merge")
    } else {
        git_dir.join("rebase-apply")
    };

    let current = std::fs::read_to_string(dir.join("msgnum"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let total = std::fs::read_to_string(dir.join("end"))
        .unwrap_or_default()
        .trim()
        .to_string();

    if !current.is_empty() && !total.is_empty() {
        format!("{current}/{total}")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clean_repo() {
        let output = "# branch.oid abc1234def567890\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, "main");
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
        assert_eq!(status.staged, 0);
        assert_eq!(status.unstaged, 0);
    }


    #[test]
    fn test_parse_leaves_remote_unset_for_default_construction() {
        // The remote field is additive: porcelain parsing never sets it
        // (fetch_git_status fills it from `git remote get-url origin`), and
        // struct constructions elsewhere keep compiling via ..Default::default().
        let output = "# branch.oid abc1234def567890\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, "main");
        assert_eq!(status.remote, None);
    }
    #[test]
    fn test_parse_dirty_repo() {
        let output = "# branch.oid abc1234def567890\n# branch.head feature/test\n# branch.upstream origin/feature/test\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 abc def file.rs\n? newfile.txt\n";
        let status = parse_porcelain_v2(output);
        assert_eq!(status.branch, "feature/test");
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.unstaged, 1);
        assert_eq!(status.untracked, 1);
    }

    #[test]
    fn test_parse_detached_head() {
        let output = "# branch.oid abc1234def567890\n# branch.head (detached)\n";
        let status = parse_porcelain_v2(output);
        assert!(status.is_detached);
        assert_eq!(status.branch, "HEAD");
    }
}
