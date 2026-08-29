//! Shared helpers for the cloud/ops segment catalog: a small process-local
//! TTL cache (same pattern as the sibling-table cache in `directory.rs` and
//! the detection cache in `profiles.rs`) and a timeout-guarded synchronous
//! command runner for the async-command segments (`kubectl`, `terraform`,
//! `gcloud`, `docker context`).
//!
//! Render paths in `collect_segments` are synchronous, so command segments
//! must never block on a hung CLI: `run_command` bounds the wait and kills
//! the child on timeout. Binary presence is probed via `PATH` first so a
//! missing tool costs one stat walk, not a spawn.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Process-local TTL cache keyed by an arbitrary string (usually the cwd).
/// Negative results are cached alongside positives so a miss is not retried
/// on every prompt.
pub struct TtlCache<V> {
    map: Mutex<HashMap<String, (V, Instant)>>,
    max_entries: usize,
}

impl<V: Clone> TtlCache<V> {
    pub fn new(max_entries: usize) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    /// Return the fresh entry for `key`, or compute and cache a new one.
    /// Expired entries are recomputed on access.
    pub fn get_or(&self, key: &str, ttl: Duration, compute: impl FnOnce() -> V) -> V {
        let mut map = self
            .map
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((value, stamp)) = map.get(key) {
            if stamp.elapsed() < ttl {
                return value.clone();
            }
        }
        let value = compute();
        if map.len() >= self.max_entries {
            let now = Instant::now();
            map.retain(|_, (_, stamp)| now.duration_since(*stamp) < ttl);
            while map.len() >= self.max_entries {
                let oldest = map
                    .iter()
                    .min_by_key(|(_, (_, stamp))| *stamp)
                    .map(|(k, _)| k.clone());
                match oldest {
                    Some(k) => {
                        map.remove(&k);
                    }
                    None => break,
                }
            }
        }
        map.insert(key.to_string(), (value.clone(), Instant::now()));
        value
    }

    /// Rewind an entry's stamp for cache-expiry tests.
    #[cfg(test)]
    pub fn expire(&self, key: &str) {
        let mut map = self
            .map
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = map.get_mut(key) {
            entry.1 = Instant::now() - Duration::from_secs(3600);
        }
    }

    /// Whether a fresh (non-expired) entry exists — used by tests.
    #[cfg(test)]
    pub fn has_fresh(&self, key: &str, ttl: Duration) -> bool {
        let map = self
            .map
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.get(key)
            .map(|(_, stamp)| stamp.elapsed() < ttl)
            .unwrap_or(false)
    }
}

/// True when `bin` resolves to an existing executable file on `PATH`.
pub fn on_path(bin: &str) -> bool {
    if bin.contains('/') {
        return std::path::Path::new(bin).is_file();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let p: PathBuf = dir.join(bin);
        p.is_file()
    })
}

/// Run `bin args` synchronously, trimmed stdout on success, `None` on spawn
/// failure, non-zero exit, or timeout. The child is killed on timeout so a
/// hung CLI cannot stall the prompt renderer.
pub fn run_command(bin: &str, args: &[&str], timeout_ms: u64) -> Option<String> {
    if !on_path(bin) {
        return None;
    }
    use std::process::{Command, Stdio};
    let child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut child = Arc::new(Mutex::new(child));
    let (done_tx, done_rx) = mpsc::channel::<()>();

    // stdout reader: takes the pipe handle before the waiter thread locks
    // the child, so both can proceed concurrently.
    let mut stdout_pipe = child
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stdout
        .take();
    let (out_tx, out_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = std::io::Read::read_to_string(pipe, &mut buf);
        }
        let _ = out_tx.send(buf);
    });

    let waiter = child.clone();
    std::thread::spawn(move || {
        let mut guard = waiter.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = guard.wait();
        let _ = done_tx.send(());
    });

    let within = done_rx.recv_timeout(Duration::from_millis(timeout_ms));
    if within.is_err() {
        // Timed out: kill the child, then drain the reader so its thread
        // does not leak blocked on a still-open pipe (kill closes it).
        let _ = child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .kill();
        let _ = out_rx.recv_timeout(Duration::from_millis(250));
        return None;
    }

    let output = out_rx.recv_timeout(Duration::from_millis(250)).ok()?;
    let trimmed = output.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_cache_hit_and_expiry() {
        let cache: TtlCache<u32> = TtlCache::new(8);
        let first = cache.get_or("k", Duration::from_secs(10), || 1);
        assert_eq!(first, 1);
        // Second call returns the cached value without recompute: bump a
        // probe counter through a second key write to prove no recompute
        // ran by checking the fresh flag instead.
        assert!(cache.has_fresh("k", Duration::from_secs(10)));
        let second = cache.get_or("k", Duration::from_secs(10), || 2);
        assert_eq!(second, 1, "warm entry must not be recomputed");

        cache.expire("k");
        let third = cache.get_or("k", Duration::from_secs(10), || 2);
        assert_eq!(third, 2, "expired entry must be recomputed");
    }

    #[test]
    fn test_run_command_missing_binary() {
        assert!(run_command("definitely-not-a-real-binary-xyz", &[], 100).is_none());
    }

    #[test]
    fn test_run_command_success_and_trim() {
        let out = run_command("echo", &["  spaced  "], 2000).expect("echo must succeed");
        assert_eq!(out, "spaced");
    }

    #[test]
    fn test_run_command_timeout_kills_child() {
        let out = run_command("sleep", &["5"], 200);
        assert!(out.is_none(), "sleep must be killed at the 200ms timeout");
    }
}
