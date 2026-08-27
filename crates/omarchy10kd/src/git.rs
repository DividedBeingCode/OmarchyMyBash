use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
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
    pub stale: bool,
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
    ttl: Duration,
}

impl GitCache {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    pub async fn get_status(&self, cwd: &Path) -> GitStatus {
        let repo_root = match find_repo_root(cwd) {
            Some(root) => root,
            None => return GitStatus::default(),
        };

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&repo_root) {
                if cached.fetched_at.elapsed() < self.ttl {
                    debug!("git cache hit for {}", repo_root.display());
                    return cached.status.clone();
                }
            }
        }

        // Cache miss — fetch fresh status
        let status = fetch_git_status(&repo_root).await;
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                repo_root,
                CachedStatus {
                    status: status.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }
        status
    }

    pub async fn invalidate(&self, repo_root: &Path) {
        let mut cache = self.cache.write().await;
        cache.remove(repo_root);
        debug!("invalidated git cache for {}", repo_root.display());
    }

    pub async fn invalidate_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

fn find_repo_root(mut dir: &Path) -> Option<PathBuf> {
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

async fn fetch_git_status(repo_root: &Path) -> GitStatus {
    let start = Instant::now();

    let output = match Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain=v2", "--branch"])
        .current_dir(repo_root)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            warn!("git status failed: {e}");
            return GitStatus {
                is_repo: true,
                repo_root: repo_root.to_path_buf(),
                ..Default::default()
            };
        }
    };

    if !output.status.success() {
        return GitStatus {
            is_repo: true,
            repo_root: repo_root.to_path_buf(),
            ..Default::default()
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut status = parse_porcelain_v2(&stdout);
    status.is_repo = true;
    status.repo_root = repo_root.to_path_buf();

    // Check for stashes
    if let Ok(stash_out) = Command::new("git")
        .args(["stash", "list"])
        .current_dir(repo_root)
        .output()
    {
        status.stashes = String::from_utf8_lossy(&stash_out.stdout)
            .lines()
            .count() as u32;
    }

    // Detect ongoing operations
    status.action = detect_git_action(repo_root);

    debug!(
        "git status for {} in {:?}",
        repo_root.display(),
        start.elapsed()
    );
    status
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
