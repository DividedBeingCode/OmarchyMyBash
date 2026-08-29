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
    /// Display-visible segment toggles (segments step); name → enabled.
    segments: std::collections::BTreeMap<&'static str, bool>,
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
            segments: WIZARD_SEGMENTS.iter().map(|s| (*s, true)).collect(),
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
                || !step_os_icon(&daemon, &mut c).await?
                || !step_contexts(&daemon, &mut c).await?
                || !step_segments(&daemon, &mut c).await?
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

/// A realistic prompt state used by the context-preview step and by the
/// default preview everywhere else.
#[derive(Clone, Copy)]
struct Ctx {
    exit_code: i32,
    in_ssh: bool,
    git_branch: &'static str,
    git_staged: u32,
    git_unstaged: u32,
}

impl Default for Ctx {
    fn default() -> Self {
        Self { exit_code: 0, in_ssh: false, git_branch: "main", git_staged: 0, git_unstaged: 0 }
    }
}

/// The realistic states cycled by the context-preview step, in order.
const CONTEXT_STATES: &[(&str, Ctx)] = &[
    ("Clean success", Ctx { exit_code: 0, in_ssh: false, git_branch: "main", git_staged: 0, git_unstaged: 0 }),
    ("Failed command (✘ 1)", Ctx { exit_code: 1, in_ssh: false, git_branch: "main", git_staged: 0, git_unstaged: 0 }),
    ("Dirty repo", Ctx { exit_code: 0, in_ssh: false, git_branch: "main", git_staged: 2, git_unstaged: 3 }),
    ("SSH session", Ctx { exit_code: 0, in_ssh: true, git_branch: "main", git_staged: 0, git_unstaged: 0 }),
];

fn context_request_json(c: &Choices, ctx: &Ctx, patch: &serde_json::Value) -> String {
    serde_json::json!({
        "type": "preview",
        "cwd": "~/projects/my-app",
        "exit_code": ctx.exit_code,
        "cmd_duration_ms": 3200,
        "cols": 110,
        "jobs": 1,
        "in_ssh": ctx.in_ssh,
        "git_branch": ctx.git_branch,
        "git_staged": ctx.git_staged,
        "git_unstaged": ctx.git_unstaged,
        "style_preset": c.preset,
        "style_separators": c.separator,
        "style_frame": c.frame,
        "prompt_newline": c.newline,
        "patch": patch,
    })
    .to_string()
        + "\n"
}

fn preview_request(c: &Choices) -> String {
    // Manual JSON: every interpolated value is a known-safe catalog literal.
    context_request_json(c, &Ctx::default(), &choices_patch(c))
}

/// `config_set`-shaped patch carrying every wizard answer the daemon's
/// style_* shortcuts cannot express: segment toggles, OS icon, prompt
/// character glyphs, transient prompt, and frame gap character.
fn choices_patch(c: &Choices) -> serde_json::Value {
    let char_glyph = prompt_glyph(c.prompt_char);
    let mut segments = serde_json::Map::new();
    segments.insert(
        "character".into(),
        serde_json::json!({
            "success": char_glyph, "error": char_glyph, "transient": char_glyph,
        }),
    );
    for (name, enabled) in &c.segments {
        let mut entry = serde_json::Map::new();
        entry.insert("enabled".into(), serde_json::json!(enabled));
        if *name == "os" {
            entry.insert("icon".into(), serde_json::json!(c.os_icon));
        }
        segments.insert((*name).into(), serde_json::Value::Object(entry));
    }
    serde_json::json!({
        "prompt": { "transient": c.transient },
        "segments": segments,
    })
}

fn prompt_glyph(key: &str) -> &'static str {
    match key {
        "angle" => ">",
        "dollar" => "$",
        "lambda" => "\u{3bb}",
        _ => "\u{276f}",
    }
}

/// Sends one newline-terminated JSON request and reads one JSON response.
async fn daemon_roundtrip(sock: &PathBuf, body: &str) -> Result<Option<serde_json::Value>> {
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
    Ok(serde_json::from_str(line.trim())?)
}

/// Returns Some((left, right)) ANSI text on success.
async fn request_preview(sock: &PathBuf, body: &str) -> Result<Option<(String, String)>> {
    let v = match daemon_roundtrip(sock, body).await? {
        Some(v) => v,
        None => return Ok(None),
    };
    let left = v.get("left").and_then(|l| l.as_str()).unwrap_or("").to_string();
    let right = v.get("right").and_then(|r| r.as_str()).unwrap_or("").to_string();
    Ok(Some((left, right)))
}

/// Sends a control request and fails on a non-ok status.
async fn send_control(daemon: &DaemonHandle, body: serde_json::Value) -> Result<()> {
    let body = format!("{}\n", body);
    let resp = daemon_roundtrip(&daemon.sock, &body).await?;
    match resp {
        Some(v) if v.get("status").and_then(|s| s.as_str()) == Some("ok") => Ok(()),
        other => anyhow::bail!(
            "daemon rejected request: {}",
            other.map(|o| o.to_string()).unwrap_or_else(|| "no response".into())
        ),
    }
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

async fn preview_lines_ctx(
    daemon: &DaemonHandle,
    c: &Choices,
    ctx: &Ctx,
) -> Result<Vec<String>> {
    let cols = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(100);
    let body = context_request_json(c, ctx, &choices_patch(c));
    match request_preview(&daemon.sock, &body).await? {
        Some((left, right)) => Ok(compose_prompt(&left, &right, cols)
            .split('\n')
            .map(|s| s.to_string())
            .collect()),
        None => Ok(vec!["\u{1b}[31mpreview unavailable\u{1b}[0m".into()]),
    }
}

fn screen_header(title: &str) -> Result<()> {
    println!("Omarchy10k Configure\r");
    println!("\u{1b}[2m{}\u{1b}[0m\r", "─".repeat(60));
    println!("{}\r", title);
    println!();
    Ok(())
}

async fn preview_lines(daemon: &DaemonHandle, c: &Choices) -> Result<Vec<String>> {
    preview_lines_ctx(daemon, c, &Ctx::default()).await
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
    Up,
    Down,
    Space,
    Enter,
    Back,
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
                    KeyCode::Char('b') | KeyCode::Char('B') => Key::Back,
                    KeyCode::Char(' ') => Key::Space,
                    KeyCode::Enter => Key::Enter,
                    KeyCode::Up => Key::Up,
                    KeyCode::Down => Key::Down,
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
            // Up/Down/Space/Enter/Back are used by the dedicated steps below;
            // plain question screens ignore them, as before.
            Key::Up | Key::Down | Key::Space | Key::Enter | Key::Back | Key::Ignore => {}
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

// Display-visible segments offered in the segments step (mirrors the
// daemon's ALL_SEGMENTS catalog).
const WIZARD_SEGMENTS: &[&str] = &[
    "os", "ssh", "container", "directory", "git", "python_env", "toolchain", "nix", "ai", "k8s",
    "exit_status", "command_duration", "jobs", "time", "battery", "load",
    "package_version", "dir_writable", "aws_profile", "docker_context",
    "kubectl_context", "terraform_workspace", "vpn", "gcloud_project",
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
            Key::Up | Key::Down | Key::Space | Key::Enter | Key::Back => {}
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

// ── Context preview step ───────────────────────────────────────────────────

/// Pure state machine for the context-preview step (unit-tested): returns
/// the next state index, or None when the user accepts and the step ends.
fn context_step_idx(idx: usize, key: &Key, len: usize) -> Option<usize> {
    match key {
        Key::Choice(1) => Some((idx + 1) % len),
        Key::Choice(2) => None,
        Key::Back => Some((idx + len - 1) % len),
        _ => Some(idx),
    }
}

async fn step_contexts(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    let len = CONTEXT_STATES.len();
    let mut idx = 0usize;
    loop {
        let (label, ctx) = &CONTEXT_STATES[idx];
        screen_clear()?;
        let lines = preview_lines_ctx(daemon, c, ctx).await?;
        draw_preview(&lines)?;
        println!("State {}/{}: \u{1b}[1m{}\u{1b}[0m\r", idx + 1, len, label);
        println!();
        println!("  \u{1b}[1m[1]\u{1b}[0m Next state");
        println!("  \u{1b}[1m[2]\u{1b}[0m Looks good — continue");
        println!();
        println!("\u{1b}[2m[b] previous state  [r] restart  [q] quit\u{1b}[0m\r");
        io::stdout().flush()?;

        match read_key().await? {
            Key::Restart => return Ok(false),
            Key::Quit => return Err(signal_err(false)),
            k => match context_step_idx(idx, &k, len) {
                Some(next) => idx = next,
                None => return Ok(true), // accepted
            },
        }
    }
}


async fn step_segments(daemon: &DaemonHandle, c: &mut Choices) -> Result<bool> {
    let mut sel = 0usize;
    loop {
        screen_clear()?;
        screen_header("Segments — space toggles, arrows move, enter accepts")?;
        let lines = preview_lines_ctx(daemon, c, &Ctx::default()).await?;
        draw_preview(&lines)?;
        for (i, name) in WIZARD_SEGMENTS.iter().enumerate() {
            let on = c.segments[name];
            let cursor = if i == sel { '\u{276f}' } else { ' ' };
            let state = if on { "\u{1b}[32m\u{25cf} on \u{1b}[0m" } else { "\u{1b}[2m\u{25cb} off\u{1b}[0m" };
            println!("{} {} {}\r", cursor, state, name);
        }
        println!();
        println!("\u{1b}[2m[\u{2191}\u{2193}] move  [space] toggle  [enter] accept  [r] restart  [q] quit\u{1b}[0m\r");
        io::stdout().flush()?;

        match read_key().await? {
            Key::Up => sel = (sel + WIZARD_SEGMENTS.len() - 1) % WIZARD_SEGMENTS.len(),
            Key::Down => sel = (sel + 1) % WIZARD_SEGMENTS.len(),
            Key::Space => {
                let name = WIZARD_SEGMENTS[sel];
                let on = c.segments.get(&name).copied().unwrap_or(true);
                c.segments.insert(name, !on);
            }
            Key::Enter => return Ok(true),
            Key::Back => sel = (sel + WIZARD_SEGMENTS.len() - 1) % WIZARD_SEGMENTS.len(),
            Key::Restart => return Ok(false),
            Key::Quit => return Err(signal_err(false)),
            _ => {}
        }
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
        println!("  \u{1b}[1m[1]\u{1b}[0m Save to config.toml and finish");
        println!("  \u{1b}[1m[2]\u{1b}[0m Save as a Look (named wizard-{})", c.preset);
        println!("  \u{1b}[1m[3]\u{1b}[0m Save as project profile (.o10k.toml here)");
        println!("  \u{1b}[1m[4]\u{1b}[0m Restart from the beginning");
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
            Key::Choice(2) => {
                let name = format!("wizard-{}", c.preset);
                let label = format!("Configure wizard ({})", c.preset);
                let patch = full_patch_value(c);
                let req = serde_json::json!({
                    "type": "config",
                    "command": "set",
                    "config": {
                        "looks": {
                            name.clone(): { "label": label, "palette": "keep", "patch": patch },
                        }
                    },
                });
                match send_control(daemon, req).await {
                    Ok(()) => {
                        println!();
                        println!("Saved as Look \u{1b}[1m{}\u{1b}[0m in config.toml.\r", name);
                    }
                    Err(e) => {
                        println!();
                        println!("\u{1b}[31mSaving the Look failed: {}\u{1b}[0m\r", e);
                    }
                }
                println!("Press any key to exit.\r");
                io::stdout().flush()?;
                let _ = read_key().await?;
                return Ok(true);
            }
            Key::Choice(3) => {
                match write_profile_toml(c) {
                    Ok(path) => {
                        println!();
                        println!("Saved project profile {}\r", path.display());
                        println!("It applies automatically when a shell starts in this directory.\r");
                    }
                    Err(e) => {
                        println!();
                        println!("\u{1b}[31mSaving the project profile failed: {}\u{1b}[0m\r", e);
                    }
                }
                println!("Press any key to exit.\r");
                io::stdout().flush()?;
                let _ = read_key().await?;
                return Ok(true);
            }
            Key::Choice(4) => return Ok(false),
            Key::Restart => return Ok(false),
            Key::Quit => return Err(signal_err(false)),
            Key::Choice(_) | Key::Ignore => {}
            Key::Up | Key::Down | Key::Space | Key::Enter | Key::Back => {}
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
    let char_glyph = prompt_glyph(c.prompt_char);
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

/// The complete wizard answer as a `config_set`-shaped patch: style,
/// prompt, and every segment table (character glyphs, OS icon, toggles).
/// Used verbatim for the Look patch and the project profile file.
fn full_patch_value(c: &Choices) -> serde_json::Value {
    let frame = if c.frame == "none" {
        serde_json::json!({ "enabled": false })
    } else {
        serde_json::json!({ "enabled": true, "gap_char": c.gap_char })
    };
    let mut segments = choices_patch(c)["segments"].as_object().cloned().unwrap_or_default();
    segments.insert("character".into(), {
        let g = prompt_glyph(c.prompt_char);
        serde_json::json!({ "success": g, "error": g, "transient": g })
    });
    serde_json::json!({
        "style": {
            "preset": c.preset,
            "separators": { "left": c.separator, "right": c.separator },
            "frame": frame,
        },
        "prompt": { "newline": c.newline, "transient": c.transient },
        "segments": segments,
    })
}

/// Serializes the wizard answer as a bare `config_set`-shaped patch table —
/// the exact format ProfilesDaemon reads from `.o10k.toml`.
fn profile_toml(patch: &serde_json::Value) -> Result<String> {
    let tbl = toml::Value::try_from(patch).context("patch is not TOML-representable")?;
    let mut s = String::from("# omarchy10k project profile (bare config_set-shaped patch)\n");
    s.push_str(&toml::to_string(&tbl).context("patch failed to serialize as TOML")?);
    Ok(s)
}

fn write_profile_toml(c: &Choices) -> Result<PathBuf> {
    let path = std::env::current_dir()?.join(".o10k.toml");
    std::fs::write(&path, profile_toml(&full_patch_value(c))?)?;
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

    #[test]
    fn test_context_request_carries_ctx_fields() {
        let ctx = Ctx { exit_code: 1, in_ssh: true, git_branch: "main", git_staged: 2, git_unstaged: 3 };
        let j = context_request_json(&Choices::default(), &ctx, &serde_json::json!({}));
        let v: serde_json::Value = serde_json::from_str(j.trim()).unwrap();
        assert_eq!(v["exit_code"], 1);
        assert_eq!(v["in_ssh"], true);
        assert_eq!(v["git_staged"], 2);
        assert_eq!(v["git_unstaged"], 3);
        assert_eq!(v["git_branch"], "main");
        assert_eq!(v["type"], "preview");
    }

    #[test]
    fn test_context_states_cover_all_four() {
        let labels: Vec<&str> = CONTEXT_STATES.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            vec!["Clean success", "Failed command (✘ 1)", "Dirty repo", "SSH session"]
        );
        // Error and clean states must differ only in exit_code semantics.
        assert_eq!(CONTEXT_STATES[0].1.exit_code, 0);
        assert_eq!(CONTEXT_STATES[1].1.exit_code, 1);
        assert!(CONTEXT_STATES[3].1.in_ssh);
    }

    #[test]
    fn test_context_step_idx_transitions() {
        let len = CONTEXT_STATES.len();
        assert_eq!(context_step_idx(0, &Key::Choice(1), len), Some(1));
        assert_eq!(context_step_idx(len - 1, &Key::Choice(1), len), Some(0)); // wraps
        assert_eq!(context_step_idx(0, &Key::Back, len), Some(len - 1)); // prev wraps
        assert_eq!(context_step_idx(2, &Key::Choice(2), len), None); // accept
        assert_eq!(context_step_idx(2, &Key::Ignore, len), Some(2)); // no-op
    }

    #[test]
    fn test_choices_patch_shape() {
        let mut c = Choices::default();
        c.os_icon = "arch";
        c.prompt_char = "lambda";
        c.segments.insert("battery", false);
        let v = choices_patch(&c);
        assert_eq!(v["prompt"]["transient"], true);
        assert_eq!(v["segments"]["os"]["icon"], "arch");
        assert_eq!(v["segments"]["character"]["success"], "\u{3bb}");
        assert_eq!(v["segments"]["battery"]["enabled"], false);
        assert_eq!(v["segments"]["git"]["enabled"], true);
    }

    #[test]
    fn test_full_patch_and_profile_toml() {
        let mut c = Choices::default();
        c.frame = "left";
        c.segments.insert("time", false);
        let patch = full_patch_value(&c);
        assert_eq!(patch["style"]["preset"], "rainbow");
        assert_eq!(patch["style"]["frame"]["enabled"], true);
        assert_eq!(patch["prompt"]["newline"], true);
        assert_eq!(patch["segments"]["time"]["enabled"], false);

        let toml_text = profile_toml(&patch).unwrap();
        let parsed: toml::Value = toml::from_str(&toml_text).unwrap();
        assert_eq!(parsed["style"]["preset"].as_str(), Some("rainbow"));
        assert_eq!(parsed["style"]["frame"]["gap_char"].as_str(), Some("\u{2500}"));
        assert_eq!(parsed["segments"]["time"]["enabled"], toml::Value::Boolean(false));
        // The profile file must parse back as a bare patch table — no
        // wrapper key.
        assert!(parsed.as_table().unwrap().contains_key("style"));
        assert!(parsed.as_table().unwrap().contains_key("segments"));
        assert!(!parsed.as_table().unwrap().contains_key("patch"));
    }

    #[test]
    fn test_full_patch_no_frame_has_no_gap() {
        let c = Choices::default();
        let patch = full_patch_value(&c);
        assert_eq!(patch["style"]["frame"]["enabled"], false);
        assert!(patch["style"]["frame"].get("gap_char").is_none());
    }

    #[test]
    fn test_wizard_segments_default_all_enabled() {
        let c = Choices::default();
        assert_eq!(c.segments.len(), WIZARD_SEGMENTS.len());
        assert!(c.segments.values().all(|&on| on));
    }
}
