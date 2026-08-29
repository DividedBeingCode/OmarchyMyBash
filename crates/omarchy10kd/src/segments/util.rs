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
use std::sync::Mutex;
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

/// How often the timeout loop re-checks a still-running child. Small enough
/// that a fast command is not noticeably delayed, large enough that a full
/// timeout budget costs a few hundred wakeups at most.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Run `bin args` synchronously, trimmed stdout on success, `None` on spawn
/// failure, non-zero exit, or timeout. The child is killed on timeout so a
/// hung CLI cannot stall the prompt renderer.
pub fn run_command(bin: &str, args: &[&str], timeout_ms: u64) -> Option<String> {
    if !on_path(bin) {
        return None;
    }
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Drain stdout on a helper thread: a command that writes more than the
    // pipe buffer blocks until someone reads, so reading only after the wait
    // would deadlock a chatty child against its own timeout.
    let mut stdout_pipe = child.stdout.take();
    let (out_tx, out_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = std::io::Read::read_to_string(pipe, &mut buf);
        }
        let _ = out_tx.send(buf);
    });

    // The child stays owned by this thread and is polled with `try_wait`, so
    // `kill` is always reachable. Parking it behind a mutex that a waiter
    // thread holds across a blocking `wait()` makes the timeout
    // unenforceable: the kill then blocks on that same mutex until the child
    // exits on its own, which is exactly the stall this bound exists to
    // prevent.
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    // Reap the zombie, then let the reader observe the pipe
                    // close so its thread does not leak.
                    let _ = child.wait();
                    let _ = out_rx.recv_timeout(Duration::from_millis(250));
                    return None;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => return None,
        }
    };

    // A failing CLI often writes its error or usage text to stdout. Without
    // this check that text renders verbatim as the segment value.
    if !status.success() {
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
        let started = Instant::now();
        let out = run_command("sleep", &["5"], 200);
        let elapsed = started.elapsed();
        assert!(out.is_none(), "sleep must be killed at the 200ms timeout");
        // The bound must be enforced, not merely observed after the child
        // exits on its own: a regression that makes `kill` unreachable still
        // returns `None`, just five seconds late.
        assert!(
            elapsed < Duration::from_secs(1),
            "timeout must kill the child, not wait it out (took {elapsed:?})"
        );
    }

    #[test]
    fn test_run_command_rejects_nonzero_exit() {
        // Writes to stdout *and* fails: the output must not be adopted as a
        // segment value.
        let out = run_command("sh", &["-c", "echo broken-context; exit 1"], 2000);
        assert!(out.is_none(), "non-zero exit must not yield stdout");
    }
}
