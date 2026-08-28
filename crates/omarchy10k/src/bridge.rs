use std::io::{self, BufRead, Write};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn run(socket_path: &Path) -> anyhow::Result<()> {
    let stream = connect_with_retry(socket_path, 10).await?;
    let (sock_read, sock_write) = stream.into_split();
    let mut sock_reader = BufReader::new(sock_read);
    let mut sock_writer = sock_write;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::task::spawn_blocking(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) if l.is_empty() => continue,
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut stdout = io::stdout();
    let socket_path = socket_path.to_path_buf();

    while let Some(line) = rx.recv().await {
        let request = if line.starts_with('{') {
            format!("{line}\n")
        } else {
            parse_kv_to_json(&line)
        };

        if sock_writer.write_all(request.as_bytes()).await.is_err() {
            match reconnect(&socket_path).await {
                Ok((r, w)) => {
                    sock_reader = r;
                    sock_writer = w;
                    if sock_writer.write_all(request.as_bytes()).await.is_err() {
                        write_fallback(&mut stdout);
                        continue;
                    }
                }
                Err(_) => {
                    write_fallback(&mut stdout);
                    continue;
                }
            }
        }

        let mut response = String::new();
        match sock_reader.read_line(&mut response).await {
            Ok(0) | Err(_) => {
                match reconnect(&socket_path).await {
                    Ok((r, w)) => {
                        sock_reader = r;
                        sock_writer = w;
                        if sock_writer.write_all(request.as_bytes()).await.is_ok() {
                            response.clear();
                            if sock_reader.read_line(&mut response).await.unwrap_or(0) > 0 {
                                write_prompt(&mut stdout, &response);
                                continue;
                            }
                        }
                        write_fallback(&mut stdout);
                    }
                    Err(_) => write_fallback(&mut stdout),
                }
            }
            Ok(_) => write_prompt(&mut stdout, &response),
        }
    }

    Ok(())
}

fn write_prompt(stdout: &mut io::Stdout, response: &str) {
    match extract_prompt_parts(response) {
        Some((left, right, threshold)) => {
            let _ = stdout.write_all(left.as_bytes());
            let _ = stdout.write_all(&[0]);
            let _ = stdout.write_all(right.as_bytes());
            let _ = stdout.write_all(&[0]);
            let _ = stdout.write_all(threshold.as_bytes());
            let _ = stdout.write_all(&[0]);
        }
        None => {
            write_fallback(stdout);
            return;
        }
    }
    let _ = stdout.flush();
}

fn write_fallback(stdout: &mut io::Stdout) {
    // PS1 escapes (\[ and \e) stay literal text; the ❯ glyph is the real
    // UTF-8 byte sequence (U+276F).
    let _ = stdout.write_all(b"\\[\\e[1;34m\\]\\w\\[\\e[0m\\] \\[\\e[1;32m\\]\xe2\x9d\xaf\\[\\e[0m\\] ");
    let _ = stdout.write_all(&[0]);
    let _ = stdout.write_all(&[0]); // empty right prompt
    let _ = stdout.write_all(&[0]); // empty notify_threshold_ms
    let _ = stdout.flush();
}

async fn reconnect(
    socket_path: &Path,
) -> anyhow::Result<(
    BufReader<tokio::net::unix::OwnedReadHalf>,
    tokio::net::unix::OwnedWriteHalf,
)> {
    let stream = connect_with_retry(socket_path, 3).await?;
    let (r, w) = stream.into_split();
    Ok((BufReader::new(r), w))
}

async fn connect_with_retry(socket_path: &Path, max_retries: u32) -> anyhow::Result<UnixStream> {
    for attempt in 0..max_retries {
        match UnixStream::connect(socket_path).await {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                if attempt + 1 >= max_retries {
                    return Err(e.into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1)))
                    .await;
            }
        }
    }
    anyhow::bail!("failed to connect to daemon socket")
}

fn extract_prompt_parts(response: &str) -> Option<(String, String, String)> {
    let trimmed = response.trim();
    // Try a full JSON parse first, then a parse starting at the first '{' to
    // catch a JSON object embedded in surrounding noise.
    let mut parsed = serde_json::from_str::<serde_json::Value>(trimmed).ok();
    if parsed.is_none() {
        if let Some(start) = trimmed.find('{') {
            parsed = serde_json::from_str::<serde_json::Value>(&trimmed[start..]).ok();
        }
    }
    let v = parsed?;
    let left = v
        .get("left")
        .and_then(|l| l.as_str())
        .unwrap_or("")
        .to_string();
    if left.is_empty() {
        // JSON error/control payloads must never become PS1
        return None;
    }
    let right = v
        .get("right")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    let threshold = v
        .get("notify_threshold_ms")
        .and_then(|t| t.as_u64())
        .map(|t| t.to_string())
        .unwrap_or_default();
    Some((left, right, threshold))
}

fn parse_kv_to_json(line: &str) -> String {
    let mut map = serde_json::Map::new();
    for part in line.split('\t') {
        if let Some((key, value)) = part.split_once('=') {
            let v = if let Ok(n) = value.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else {
                serde_json::Value::String(value.to_string())
            };
            map.insert(key.to_string(), v);
        }
    }
    let obj = serde_json::Value::Object(map);
    format!("{obj}\n")
}
