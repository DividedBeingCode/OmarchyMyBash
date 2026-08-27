use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn render(
    socket_path: &Path,
    cwd: &str,
    exit_code: i32,
    cmd_duration_ms: u64,
    cols: u16,
    jobs: u32,
) -> anyhow::Result<()> {
    let request = serde_json::json!({
        "cwd": cwd,
        "exit_code": exit_code,
        "cmd_duration_ms": cmd_duration_ms,
        "cols": cols,
        "jobs": jobs,
    });

    match send_request(socket_path, &request.to_string()).await {
        Ok(response) => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&response) {
                if let Some(left) = v.get("left").and_then(|l| l.as_str()) {
                    print!("{left}");
                    return Ok(());
                }
            }
            // Raw fallback
            print!("{response}");
        }
        Err(_) => {
            // Daemon unreachable — print fallback prompt
            print!("\\[\\e[1;34m\\]\\w\\[\\e[0m\\] \\[\\e[1;32m\\]❯\\[\\e[0m\\] ");
        }
    }

    Ok(())
}

pub async fn send_command(socket_path: &Path, command: &str) -> anyhow::Result<()> {
    let request = serde_json::json!({ "command": command });
    let response = send_request(socket_path, &request.to_string()).await?;
    println!("{response}");
    Ok(())
}

pub async fn benchmark(socket_path: &Path, iterations: u32) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/tmp".into());

    let request = serde_json::json!({
        "cwd": cwd,
        "exit_code": 0,
        "cmd_duration_ms": 0,
        "cols": 120,
        "jobs": 0,
    });

    let mut durations = Vec::with_capacity(iterations as usize);

    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let _ = send_request(socket_path, &request.to_string()).await;
        durations.push(start.elapsed());
    }

    durations.sort();

    let total: std::time::Duration = durations.iter().sum();
    let avg = total / iterations;
    let p50 = durations[durations.len() / 2];
    let p95 = durations[(durations.len() as f64 * 0.95) as usize];
    let p99 = durations[(durations.len() as f64 * 0.99) as usize];

    println!("Omarchy10k Benchmark ({iterations} iterations)");
    println!("────────────────────────────────────");
    println!("  avg:  {:>8.2}ms", avg.as_secs_f64() * 1000.0);
    println!("  p50:  {:>8.2}ms", p50.as_secs_f64() * 1000.0);
    println!("  p95:  {:>8.2}ms", p95.as_secs_f64() * 1000.0);
    println!("  p99:  {:>8.2}ms", p99.as_secs_f64() * 1000.0);

    if p50.as_secs_f64() * 1000.0 < 5.0 {
        println!("  result: ✓ sub-5ms target met");
    } else {
        println!("  result: ✘ above 5ms target");
    }

    Ok(())
}

async fn send_request(socket_path: &Path, request: &str) -> anyhow::Result<String> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    writer.write_all(request.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    let mut reader = BufReader::new(reader);
    let mut response = String::new();
    reader.read_line(&mut response).await?;

    Ok(response.trim().to_string())
}
