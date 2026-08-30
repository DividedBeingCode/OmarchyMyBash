//! `omarchy10k intro` — one-time themed welcome (3.3).
//!
//! Renders a rich simulated prompt through the daemon preview path, framed
//! with palette swatches, a terminal-capabilities line, and the measured
//! render latency. One-shot: a marker file under the XDG state directory
//! suppresses further runs. `O10K_NO_INTRO` gates CI; `--force` bypasses the
//! marker. Daemon down → exits silently without writing the marker.

use std::io::Write as _;
use std::path::{Path, PathBuf};


pub async fn run(socket_path: &Path, force: bool) -> anyhow::Result<()> {
    if std::env::var_os("O10K_NO_INTRO").is_some() {
        return Ok(());
    }
    let marker = marker_path();
    if !force && marker.exists() {
        return Ok(());
    }

    let start = std::time::Instant::now();

    let cols: u16 = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(80);
    let preview_request = serde_json::json!({
        "type": "preview",
        "cwd": "~/projects/my-app",
        "exit_code": 0,
        "cmd_duration_ms": 2345,
        "cols": cols,
        "jobs": 1,
        "in_ssh": false,
    });

    let response = match crate::prompt::send_request(socket_path, &preview_request.to_string()).await
    {
        Ok(r) => r,
        // Daemon down → skip silently. No marker: the next shell retries.
        Err(_) => return Ok(()),
    };
    let elapsed = start.elapsed();

    let v: serde_json::Value = match serde_json::from_str(&response) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if v.get("status").and_then(|s| s.as_str()) != Some("ok") {
        return Ok(());
    }
    let left = v.get("left").and_then(|l| l.as_str()).unwrap_or("");
    if left.is_empty() {
        return Ok(());
    }

    let palette = fetch_palette(socket_path).await;

    render_intro(left, palette.as_ref(), elapsed);

    // Mark shown only after a successful render so a failed attempt retries.
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, format!("omarchy10k {}\n", env!("CARGO_PKG_VERSION")));

    Ok(())
}

/// `$XDG_STATE_HOME/omarchy10k/intro_shown`, via the `directories` crate's
/// ProjectDirs state dir with an XDG/home fallback (state_dir is None on
/// some platforms).
fn marker_path() -> PathBuf {
    if let Some(pd) = directories::ProjectDirs::from("", "", "omarchy10k") {
        if let Some(state) = pd.state_dir() {
            return state.join("intro_shown");
        }
    }
    let state_home = std::env::var("XDG_STATE_HOME")
        .ok()
        .filter(|s| Path::new(s).is_absolute())
        .map(PathBuf::from)
        .or_else(|| {
            directories::BaseDirs::new().map(|d| d.home_dir().join(".local/state"))
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    state_home.join("omarchy10k").join("intro_shown")
}

async fn fetch_palette(socket_path: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let request = serde_json::json!({ "type": "control", "command": "palette" });
    let response = crate::prompt::send_request(socket_path, &request.to_string())
        .await
        .ok()?;
    let v: serde_json::Value = serde_json::from_str(&response).ok()?;
    let palette = v.get("palette")?.as_object()?.clone();
    Some(palette)
}

/// Rows for the optional first-run mascot.
///
/// Opt-in by dropping a PNG at `$XDG_CONFIG_HOME/omarchy10k/mascot.png` (or
/// pointing `[intro] mascot` at one). Empty unless the file exists AND the
/// terminal reports truecolor — half-blocks rely on 24-bit fg/bg pairs, and on
/// a 16-colour terminal they would render as noise.
///
/// We ship the renderer, not the art: sprite images are a copyright question,
/// so the user supplies their own.
fn mascot_rows() -> Vec<String> {
    let truecolor = std::env::var("COLORTERM")
        .map(|v| v.contains("truecolor") || v.contains("24bit"))
        .unwrap_or(false);
    if !truecolor {
        return Vec::new();
    }
    let Some(path) = configured_mascot().or_else(mascot_path) else {
        return Vec::new();
    };
    // Where the terminal implements the kitty graphics protocol -- Ghostty,
    // kitty, wezterm -- send the real image instead of a half-block
    // approximation. Half-blocks give two vertical samples per cell; this
    // gives the terminal the PNG.
    //
    // foot stays on half-blocks deliberately: it implements sixel, not kitty
    // graphics. One high-quality path plus one universal fallback beats three
    // partial ones.
    let caps = crate::terminal::TermCaps::for_kind(crate::terminal::kind_from_env());
    if caps.has_kitty_graphics {
        // 32x16 cells matches the half-block footprint below, so the intro
        // lays out identically either way.
        if let Some(img) = crate::sprite::render_kitty(&path, 32, 16) {
            return vec![img];
        }
        // Falls through to half-blocks if the file could not be read -- the
        // same outcome the half-block path would reach.
    }

    // 32 columns keeps the sprite beside the framed preview rather than
    // dwarfing it, and bounds the work regardless of the source image size.
    crate::sprite::render(&path, 32, "  ").unwrap_or_default()
}

/// `[intro] mascot` from config.toml, if it names a file that exists.
///
/// Read straight from the file rather than asked of the daemon: the intro
/// runs once, often before any daemon exists.
fn configured_mascot() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))?;
    let text = std::fs::read_to_string(base.join("omarchy10k").join("config.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let raw = value.get("intro")?.get("mascot")?.as_str()?.trim().to_string();
    if raw.is_empty() {
        return None;
    }
    // A leading ~ is the one expansion worth doing; anything else is the
    // user's business.
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        PathBuf::from(std::env::var("HOME").ok()?).join(rest)
    } else {
        PathBuf::from(raw)
    };
    expanded.is_file().then_some(expanded)
}

/// `$XDG_CONFIG_HOME/omarchy10k/mascot.png`, or `None` when absent.
fn mascot_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok()?;
    let p = base.join("omarchy10k").join("mascot.png");
    p.is_file().then_some(p)
}

fn render_intro(
    left: &str,
    palette: Option<&serde_json::Map<String, serde_json::Value>>,
    latency: std::time::Duration,
) {
    let mut out = String::new();

    // Optional mascot: a user-supplied PNG rendered as half-blocks beside the
    // header. Opt-in and silent when absent — nothing here may make a first
    // run noisier or slower than it already is.
    let mascot = mascot_rows();

    // Header
    out.push_str(&format!(
        "\x1b[1m omarchy10k v{}\x1b[0m — reactive shell UI\n",
        env!("CARGO_PKG_VERSION")
    ));

    for row in &mascot {
        out.push_str(row);
        out.push('\n');
    }

    // Framed live preview (rounded frame, display width from ANSI-stripped text)
    let lines: Vec<String> = left.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    let plain: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();
    let width = plain.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let width = width.max(1);

    out.push_str(&format!("  ╭{}╮\n", "─".repeat(width + 2)));
    for (i, line) in lines.iter().enumerate() {
        let pad = width - plain[i].chars().count();
        out.push_str(&format!("  │{line}{}│\n", " ".repeat(pad)));
    }
    out.push_str(&format!("  ╰{}╯\n", "─".repeat(width + 2)));

    // Palette swatches
    if let Some(palette) = palette {
        const KEYS: &[&str] = &[
            "accent", "foreground", "muted", "background", "red", "green", "yellow", "blue",
        ];
        out.push_str("\n palette:");
        for key in KEYS {
            if let Some(hex) = palette.get(*key).and_then(|v| v.as_str()) {
                let Some((r, g, b)) = parse_hex(hex) else { continue };
                out.push_str(&format!(
                    " \x1b[48;2;{r};{g};{b}m   \x1b[0m",
                ));
            }
        }
        out.push('\n');
        for key in KEYS {
            if let Some(hex) = palette.get(*key).and_then(|v| v.as_str()) {
                out.push_str(&format!("  {key:<11}{hex}"));
                out.push('\n');
            }
        }
    }

    // Terminal capabilities + latency
    out.push_str(&format!("\n {}\n", termcaps_line()));
    out.push_str(&format!(
        " renders in {:.1}ms (measured round-trip)\n",
        latency.as_secs_f64() * 1000.0
    ));

    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}

fn termcaps_line() -> String {
    let term_program = std::env::var("TERM_PROGRAM")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("TERM").ok())
        .unwrap_or_else(|| "unknown".into());
    let colorterm = std::env::var("COLORTERM")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".into());
    let bash = std::env::var("BASH_VERSION").unwrap_or_default();
    let mut line = format!("term: {term_program} · color: {colorterm}");
    if !bash.is_empty() {
        line.push_str(&format!(" · bash: {bash}"));
    }
    line
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Removes ANSI escape sequences (CSI and OSC) so plain width can be measured.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                while let Some(n) = chars.next() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(n) = chars.next() {
                    if n == '\x07' {
                        break;
                    }
                    if n == '\x1b' {
                        chars.next(); // consume ST terminator
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}