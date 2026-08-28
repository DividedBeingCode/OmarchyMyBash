// `omarchy10k configure` — p10k-style onboarding wizard.
//
// Full-screen alt-buffer TUI: one question per screen, live prompt preview
// rendered by the real daemon renderer (preview requests over the daemon
// socket, with per-request style overrides), then writes the chosen keys to
// ~/.config/omarchy10k/config.toml (backing up any existing file). The
// daemon's fs watcher picks the file up immediately — no restart needed.

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue, cursor::{Hide, Show, MoveTo}};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

// ── Choices ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Choices {
    preset: &'static str,
    separator: &'static str,
    newline: bool,
    frame: &'static str, // none | left | right | full
    gap_char: &'static str,
    transient: bool,
    prompt_char: &'static str,
    os_icon: &'static str,
}

impl Default for Choices {
    fn default() -> Self {
        Self {
            preset: "rainbow",
            separator: "powerline",
            newline: true,
            frame: "none",
            gap_char: "\u{2500}",
            transient: true,
            prompt_char: "chevron",
            os_icon: "auto",
        }
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

pub async fn run() -> Result<()> {
    let _guard = TerminalGuard::new()?;
    let daemon = DaemonHandle::connect().await?;

    loop {
        let mut c = Choices::default();
        let chain = async {
            if !step_style(&daemon, &mut c).await?
                || !step_separator(&daemon, &mut c).await?
                || !step_height(&daemon, &mut c).await?
                || !step_frame(&daemon, &mut c).await?
                || !step_transient(&daemon, &mut c).await?
                || !step_prompt_char(&daemon, &mut c).await?
                || !step_os_icon(&daemon, &mut c).await?
                || !step_confirm(&daemon, &c).await?
            {
                return Ok(false); // restart
            }
            Ok(true) // done
        };
        match chain.await {
            Ok(true) => return Ok(()),
            Ok(false) => continue,
            Err(e) => match decode_signal(&e) {
                Some(true) => continue,   // restart
                Some(false) => return Ok(()), // quit
                None => return Err(e),
            },
        }
    }
}

// Wizard control-flow signals ride through anyhow::Error; decoded in run().
fn signal_err(restart: bool) -> anyhow::Error {
    anyhow::anyhow!("__wizard_signal__{}", if restart { "restart" } else { "quit" })
}

fn decode_signal(e: &anyhow::Error) -> Option<bool> {
    let msg = e.to_string();
    let rest = msg.strip_prefix("__wizard_signal__")?;
    Some(rest == "restart")
}

// ── Terminal lifecycle ─────────────────────────────────────────────────────

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

// ── Daemon connection ──────────────────────────────────────────────────────

struct DaemonHandle {
    sock: PathBuf,
    child: Option<std::process::Child>,
}

impl Drop for DaemonHandle {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let pid = child.id().to_string();
            let _ = std::process::Command::new("kill")
                .args(["-TERM", &pid])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let _ = child.wait();
            // Our own transient socket — safe to unlink if the daemon's signal
            // handler raced us.
            let _ = std::fs::remove_file(&self.sock);
        }
    }
}

impl DaemonHandle {
    async fn connect() -> Result<Self> {
        if let Some(sock) = Self::find_live().await {
            return Ok(Self { sock, child: None });
        }
        // No live daemon — spawn a transient one that dies with us.
        let daemon_bin = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("omarchy10kd")))
            .context("cannot locate omarchy10kd next to omarchy10k")?;
        let mypid = std::process::id();
        let sock = runtime_dir().join(format!("omarchy10k-{mypid}.sock"));
        let child = std::process::Command::new(&daemon_bin)
            .env("O10K_PARENT_PID", mypid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("spawning {}", daemon_bin.display()))?;
        let handle = Self { sock, child: Some(child) };
        for _ in 0..40 {
            if handle.alive().await {
                return Ok(handle);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        anyhow::bail!("spawned daemon did not become ready")
    }

    async fn find_live() -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = std::fs::read_dir(runtime_dir())
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("omarchy10k-") && n.ends_with(".sock"))
                    .unwrap_or(false)
            })
            .collect();
        candidates.sort();
        for path in candidates {
            if Self::usable(&path).await {
                return Some(path);
            }
        }
        None
    }

    async fn usable(sock: &PathBuf) -> bool {
        let req = preview_request(&Choices::default());
        matches!(
            tokio::time::timeout(Duration::from_millis(700), request_preview(sock, &req)).await,
            Ok(Ok(Some(_)))
        )
    }

    async fn alive(&self) -> bool {
        Self::usable(&self.sock).await
    }
}

fn runtime_dir() -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()))
}

// ── Preview RPC ────────────────────────────────────────────────────────────

fn preview_request(c: &Choices) -> String {
    // Manual JSON: every interpolated value is a known-safe catalog literal.
    format!(
        "{{\"type\":\"preview\",\"cwd\":\"~/projects/my-app\",\"exit_code\":0,\"cmd_duration_ms\":3200,\"cols\":110,\"jobs\":1,\"git_branch\":\"main\",\"style_preset\":\"{}\",\"style_separators\":\"{}\",\"style_frame\":\"{}\",\"prompt_newline\":{}}}\n",
        c.preset, c.separator, c.frame, c.newline
    )
}

/// Returns Some((left, right)) ANSI text on success.
async fn request_preview(sock: &PathBuf, body: &str) -> Result<Option<(String, String)>> {
    let stream = tokio::time::timeout(Duration::from_millis(700), UnixStream::connect(sock)).await??;
    let (read_half, mut write_half) = stream.into_split();
    write_half.write_all(body.as_bytes()).await?;
    write_half.flush().await?;

    let mut line = String::new();
    let mut reader = BufReader::new(read_half);
    let n = tokio::time::timeout(Duration::from_millis(700), reader.read_line(&mut line)).await??;
    if n == 0 {
        return Ok(None);
    }
    let v: serde_json::Value = serde_json::from_str(line.trim())?;
    let left = v.get("left").and_then(|l| l.as_str()).unwrap_or("").to_string();
    let right = v.get("right").and_then(|r| r.as_str()).unwrap_or("").to_string();
    Ok(Some((left, right)))
}

/// Removes ANSI escapes (SGR + OSC) and returns plain text.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn visible_width(s: &str) -> usize {
    strip_ansi(s).chars().count()
}

/// Joins the left prompt with the right prompt right-aligned to `cols`.
fn compose_prompt(left: &str, right: &str, cols: usize) -> String {
    let mut lines: Vec<String> = left.split('\n').map(|s| s.to_string()).collect();
    if !right.is_empty() && !lines.is_empty() {
        let pad = cols
            .saturating_sub(visible_width(&lines[0]) + visible_width(right))
            .max(1);
        lines[0] = format!("{}{}{}", lines[0], " ".repeat(pad), right);
    }
    lines.join("\n")
}

// ── Screen engine ──────────────────────────────────────────────────────────

fn screen_clear() -> Result<()> {
    queue!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn screen_header(title: &str) -> Result<()> {
    println!("Omarchy10k Configure\r");
    println!("\u{1b}[2m{}\u{1b}[0m\r", "─".repeat(60));
    println!("{}\r", title);
    println!();
    Ok(())
}

async fn preview_lines(daemon: &DaemonHandle, c: &Choices) -> Result<Vec<String>> {
    let cols = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(100);
    match request_preview(&daemon.sock, &preview_request(c)).await? {
        Some((left, right)) => Ok(compose_prompt(&left, &right, cols)
            .split('\n')
            .map(|s| s.to_string())
            .collect()),
        None => Ok(vec!["\u{1b}[31mpreview unavailable\u{1b}[0m".into()]),
    }
}

fn draw_preview(lines: &[String]) -> Result<()> {
    for l in lines {
        println!("{}\r", l);
    }
    println!();
    Ok(())
}

fn draw_options(options: &[&str]) -> Result<()> {
    for (i, opt) in options.iter().enumerate() {
        println!("  \u{1b}[1m[{}]\u{1b}[0m {}\r", i + 1, opt);
    }
    println!();
    println!("\u{1b}[2m[r] restart  [q] quit\u{1b}[0m\r");
    io::stdout().flush()?;
    Ok(())
}

enum Key {
    Choice(usize),
    Restart,
    Quit,
    Ignore,
}

async fn read_key() -> Result<Key> {
    loop {
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                return Ok(match k.code {
                    KeyCode::Char('r') | KeyCode::Char('R') => Key::Restart,
                    KeyCode::Char('q') | KeyCode::Char('Q') => Key::Quit,
                    KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                        Key::Choice(c as usize - '0' as usize)
                    }
                    _ => Key::Ignore,
                });
            }
        } else {
            // Yield to the runtime so the transient preview daemon is not
            // starved while we poll for keys.
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// Renders one question screen with a live preview and returns the chosen
/// option index. Ok(None) means restart.
async fn ask_index(
    daemon: &DaemonHandle,
    c: &Choices,
    title: &str,
    question: &str,
    options: &[&str],
) -> Result<Option<usize>> {
    loop {
        screen_clear()?;
        screen_header(title)?;
        let lines = preview_lines(daemon, c).await?;
        draw_preview(&lines)?;
        println!("{}\r", question);
        println!();
        draw_options(options)?;

        match read_key().await? {
            Key::Choice(i) if i >= 1 && i <= options.len() => return Ok(Some(i - 1)),
            Key::Choice(_) => {}
            Key::Restart => return Ok(None),
            Key::Quit => return Err(signal_err(false)),
            Key::Ignore => {}
        }
    }
}

// ── Steps ──────────────────────────────────────────────────────────────────

const STYLE_KEYS: &[&str] = &["rainbow", "powerline", "lean", "classic", "framed"];
const SEPARATOR_KEYS: &[&str] = &[
    "powerline", "powerline_thin", "slanted", "round", "vertical", "fade", "fade_rev",
];
const FRAME_KEYS: &[&str] = &["none", "left", "right", "full"];
const GAP_KEYS: &[&str] = &["\u{2500}", "\u{b7}", ""];
const CHAR_KEYS: &[&str] = &["chevron", "angle", "dollar", "lambda"];
const ICON_KEYS: &[&str] = &[
    "arch", "ubuntu", "debian", "fedora", "nixos", "macos", "linux", "omarchy", "none",
];

async fn step_style(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    match ask_index(
        daemon,
        c,
        "Pick a style — the prompt updates live",
        "Style?",
        &[
            "Rainbow   (p10k signature — colored powerline fills)",
            "Powerline (filled, single accent color)",
            "Lean      (no fills, minimal)",
            "Classic   (vertical bars, unfilled)",
            "Framed    (box around the prompt)",
        ],
    )
    .await?
    {
        Some(i) => {
            c.preset = STYLE_KEYS[i];
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn step_separator(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    match ask_index(
        daemon,
        c,
        "Pick a segment separator",
        "Separator? (visible on filled styles; \u{2593}\u{2592}\u{2591} is the p10k gradient fade)",
        &[
            "Powerline arrow  \u{e0b0}",
            "Thin arrow       \u{e0b1}",
            "Slanted          \u{e0bc}",
            "Round            \u{e0b4}",
            "Vertical bar     \u{2502}",
            "Fade             \u{2593}\u{2592}\u{2591}",
            "Fade reversed    \u{2591}\u{2592}\u{2593}",
        ],
    )
    .await?
    {
        Some(i) => {
            c.separator = SEPARATOR_KEYS[i];
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn step_height(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    loop {
        screen_clear()?;
        screen_header("Prompt height")?;
        c.newline = true;
        let two = preview_lines(daemon, c).await?;
        c.newline = false;
        let one = preview_lines(daemon, c).await?;
        println!("Two lines:\r");
        draw_preview(&two)?;
        println!("One line:\r");
        draw_preview(&one)?;
        println!("Height?\r");
        println!();
        println!("  \u{1b}[1m[1]\u{1b}[0m One line");
        println!("  \u{1b}[1m[2]\u{1b}[0m Two lines");
        println!();
        println!("\u{1b}[2m[r] restart  [q] quit\u{1b}[0m\r");
        io::stdout().flush()?;

        match read_key().await? {
            Key::Choice(1) => {
                c.newline = false;
                return Ok(true);
            }
            Key::Choice(2) => {
                c.newline = true;
                return Ok(true);
            }
            Key::Restart => return Ok(false),
            Key::Quit => return Err(signal_err(false)),
            Key::Ignore => {}
            Key::Choice(_) => {}
        }
    }
}

async fn step_frame(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    let idx = match ask_index(
        daemon,
        c,
        "Frame (left frame draws \u{256d}\u{2500}\u{2502} connectors down the edge)",
        "Frame?",
        &["None", "Left only", "Right only", "Full frame"],
    )
    .await?
    {
        Some(i) => i,
        None => return Ok(false),
    };
    c.frame = FRAME_KEYS[idx];

    if c.frame != "none" {
        let idx = match ask_index(
            daemon,
            c,
            "Frame gap character (fills the line between left and right prompts)",
            "Gap?",
            &["Solid \u{2500}\u{2500}\u{2500}", "Dotted \u{b7}\u{b7}\u{b7}\u{b7}", "Blank (disconnected)"],
        )
        .await?
        {
            Some(i) => i,
            None => return Ok(false),
        };
        c.gap_char = GAP_KEYS[idx];
    }
    Ok(true)
}

async fn step_transient(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    match ask_index(
        daemon,
        c,
        "Transient prompt — past prompts collapse to a single \u{276f} after each command",
        "Transient prompt?",
        &["On (recommended)", "Off"],
    )
    .await?
    {
        Some(i) => {
            c.transient = i == 0;
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn step_prompt_char(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    match ask_index(
        daemon,
        c,
        "Prompt character",
        "Prompt char? (green on success, red on error)",
        &["\u{276f}  chevron", ">  angle", "$  dollar", "\u{3bb}  lambda"],
    )
    .await?
    {
        Some(i) => {
            c.prompt_char = CHAR_KEYS[i];
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn step_os_icon(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    match ask_index(
        daemon,
        c,
        "OS icon segment",
        "OS icon? (shown at the start of the prompt)",
        &[
            "\u{f303}  Arch",
            "\u{f31b}  Ubuntu",
            "\u{f306}  Debian",
            "\u{f30a}  Fedora",
            "\u{f313}  NixOS",
            "\u{f179}  macOS",
            "\u{f17c}  Linux",
            "\u{f312}  Omarchy",
            "\u{2205}  None",
        ],
    )
    .await?
    {
        Some(i) => {
            c.os_icon = ICON_KEYS[i];
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn step_confirm(daemon: &DaemonHandle, c: &Choices) -> Result<bool> {
    let lines = preview_lines(daemon, c).await?;
    let summary = format!(
        "preset={}  separator={}  {}-line  frame={}  transient={}  char={}  icon={}",
        c.preset,
        c.separator,
        if c.newline { "two" } else { "one" },
        c.frame,
        c.transient,
        c.prompt_char,
        c.os_icon
    );
    loop {
        screen_clear()?;
        screen_header("Final look")?;
        draw_preview(&lines)?;
        println!("\u{1b}[2m{}\u{1b}[0m\r", summary);
        println!();
        println!("Does it look good?\r");
        println!();
        println!("  \u{1b}[1m[1]\u{1b}[0m Save and finish");
        println!("  \u{1b}[1m[2]\u{1b}[0m Restart from the beginning");
        println!();
        println!("\u{1b}[2m[q] quit without saving\u{1b}[0m\r");
        io::stdout().flush()?;

        match read_key().await? {
            Key::Choice(1) => {
                let path = write_config(c)?;
                println!();
                println!("Saved {}\r", path.display());
                println!("Running daemons reload automatically — the next prompt uses it.\r");
                println!("Press any key to exit.\r");
                io::stdout().flush()?;
                let _ = read_key().await?;
                return Ok(true);
            }
            Key::Choice(2) => return Ok(false),
            Key::Restart => return Ok(false),
            Key::Quit => return Err(signal_err(false)),
            Key::Choice(_) | Key::Ignore => {}
        }
    }
}

// ── Config write ───────────────────────────────────────────────────────────

fn config_path() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("XDG_CONFIG_HOME or HOME must be set")?;
    Ok(base.join("omarchy10k").join("config.toml"))
}

fn render_config(c: &Choices) -> String {
    let char_glyph = match c.prompt_char {
        "angle" => ">",
        "dollar" => "$",
        "lambda" => "\u{3bb}",
        _ => "\u{276f}",
    };
    let mut s = String::new();
    s.push_str("# Generated by `omarchy10k configure`\n");
    s.push_str("[style]\n");
    s.push_str(&format!("preset = \"{}\"\n\n", c.preset));
    s.push_str("[style.separators]\n");
    s.push_str(&format!("left = \"{}\"\n", c.separator));
    s.push_str(&format!("right = \"{}\"\n\n", c.separator));
    s.push_str("[style.frame]\n");
    s.push_str(&format!("enabled = {}\n", c.frame != "none"));
    if c.frame != "none" {
        s.push_str(&format!("gap_char = \"{}\"\n", c.gap_char));
    }
    s.push_str("\n[prompt]\n");
    s.push_str(&format!("newline = {}\n", c.newline));
    s.push_str(&format!("transient = {}\n\n", c.transient));
    s.push_str("[segments.character]\n");
    s.push_str(&format!("success = \"{}\"\n", char_glyph));
    s.push_str(&format!("error = \"{}\"\n", char_glyph));
    s.push_str(&format!("transient = \"{}\"\n", char_glyph));
    s
}

fn write_config(c: &Choices) -> Result<PathBuf> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::copy(&path, path.with_extension("toml.bak"))?;
    }
    std::fs::write(&path, render_config(c))?;
    Ok(path)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\u{1b}[38;2;1;2;3mhello\u{1b}[0m"), "hello");
        assert_eq!(strip_ansi("\u{1b}]2;title\u{7}x"), "x");
        assert_eq!(strip_ansi("\u{1b}]2;t\u{1b}\\y"), "y");
    }

    #[test]
    fn test_visible_width() {
        assert_eq!(visible_width("\u{1b}[48;2;0;0;0m ab \u{1b}[0m"), 4);
    }

    #[test]
    fn test_compose_prompt_pads_right() {
        let s = compose_prompt("LEFT", "RIGHT", 10);
        let plain = strip_ansi(&s);
        assert!(plain.contains("LEFT") && plain.contains("RIGHT"));
        assert_eq!(plain.chars().count(), 10);
    }

    #[test]
    fn test_preview_request_shape() {
        let j = preview_request(&Choices::default());
        assert!(j.contains("\"style_preset\":\"rainbow\""));
        assert!(j.contains("\"prompt_newline\":true"));
        let v: serde_json::Value = serde_json::from_str(j.trim()).unwrap();
        assert_eq!(v["type"], "preview");
    }

    #[test]
    fn test_render_config_shape() {
        let mut c = Choices::default();
        c.frame = "left";
        c.separator = "fade";
        let text = render_config(&c);
        assert!(text.contains("preset = \"rainbow\""));
        assert!(text.contains("left = \"fade\""));
        assert!(text.contains("right = \"fade\""));
        assert!(text.contains("enabled = true"));
        assert!(text.contains("gap_char = \"\u{2500}\""));
        assert!(text.contains("[segments.character]"));
        assert!(text.contains("transient = true"));
        // No-frame configs must not carry a gap char.
        c.frame = "none";
        assert!(!render_config(&c).contains("gap_char"));
    }
}
