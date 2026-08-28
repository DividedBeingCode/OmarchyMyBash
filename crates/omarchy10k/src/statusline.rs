//! `omarchy10k statusline` — daemon-rendered Claude Code statusline (1.2).
//!
//! Reads the Claude Code statusLine JSON payload from stdin, forwards it to
//! the daemon as a `statusline` message, and prints the rendered `left`
//! string. Falls back to a pure-Rust builtin render when the daemon is
//! unreachable; exits non-zero when stdin is not a JSON object.

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const DAEMON_TIMEOUT: Duration = Duration::from_millis(300);

pub async fn run(socket_path: &Path) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;

    let payload: serde_json::Value = match serde_json::from_str(input.trim()) {
        Ok(v @ serde_json::Value::Object(_)) => v,
        _ => {
            eprintln!("omarchy10k statusline: stdin is not a JSON object");
            std::process::exit(2);
        }
    };

    let request = serde_json::json!({
        "type": "statusline",
        "id": format!("cli-{}", std::process::id()),
        "payload": payload,
    });

    match send_request(socket_path, &request.to_string()).await {
        Ok(response) => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&response) {
                if v.get("status").and_then(|s| s.as_str()) == Some("ok") {
                    if let Some(left) = v.get("left").and_then(|l| l.as_str()) {
                        println!("{left}");
                        return Ok(());
                    }
                }
            }
            // Unusable response — builtin fallback render.
            println!("{}", fallback_render(&payload));
            Ok(())
        }
        // Daemon down — builtin fallback render.
        Err(_) => {
            println!("{}", fallback_render(&payload));
            Ok(())
        }
    }
}

async fn send_request(socket_path: &Path, request: &str) -> anyhow::Result<String> {
    let fut = async {
        let stream = UnixStream::connect(socket_path).await?;
        let (reader, mut writer) = stream.into_split();
        writer.write_all(request.as_bytes()).await?;
        writer.write_all(b"\n").await?;

        let mut reader = BufReader::new(reader);
        let mut response = String::new();
        reader.read_line(&mut response).await?;

        Ok(response.trim().to_string())
    };
    tokio::time::timeout(DAEMON_TIMEOUT, fut).await?
}

/// Pure-Rust fallback: model display name + context percentage with the
/// conventional statusline threshold colors (green < 70, yellow < 90, red
/// otherwise). Tolerant of Claude Code schema drift — missing fields are
/// simply omitted from the line.
fn fallback_render(payload: &serde_json::Value) -> String {
    let model = payload
        .pointer("/model/display_name")
        .and_then(|m| m.as_str())
        .unwrap_or("Claude");

    let mut line = format!("\x1b[1m{model}\x1b[0m");

    if let Some(pct) = context_percent(payload) {
        let color = if pct < 70 {
            32 // green
        } else if pct < 90 {
            33 // yellow
        } else {
            31 // red
        };
        line.push_str(&format!(" \x1b[{color}mctx {pct}%\x1b[0m"));
    }

    line
}

/// Probes the known shapes of the context-window field in the Claude Code
/// statusLine payload; falls back to deriving a percentage from token counts
/// against a 200k window. Returns None when no signal exists.
fn context_percent(payload: &serde_json::Value) -> Option<u64> {
    const POINTER_PERCENT: &[&str] = &[
        "/context_window/used_percent",
        "/context_window/used_pct",
        "/context_window/percentage",
        "/context/used_percent",
        "/context/used_pct",
        "/context/percentage",
    ];
    for p in POINTER_PERCENT {
        if let Some(v) = payload.pointer(p).and_then(|v| v.as_f64()) {
            return Some(v.round() as u64);
        }
    }

    const POINTER_TOKENS: &[&str] = &[
        "/context_window/used_tokens",
        "/context/used_tokens",
        "/context_window/tokens_used",
    ];
    for p in POINTER_TOKENS {
        if let Some(tokens) = payload.pointer(p).and_then(|v| v.as_f64()) {
            return Some(((tokens / 200_000.0) * 100.0).round() as u64);
        }
    }

    None
}