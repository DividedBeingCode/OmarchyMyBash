use std::path::Path;
use std::process::Command;

pub async fn run(socket_path: &Path) -> anyhow::Result<()> {
    println!("Omarchy10k Doctor");
    println!("══════════════════════════════════════");
    println!();

    check_bash();
    check_nerd_font();
    check_truecolor();
    check_blesh();
    check_omarchy();
    check_mise();
    check_atuin();
    check_zoxide();
    check_fzf();
    check_terminal();
    check_daemon(socket_path).await;
    check_hooks();
    check_config();

    check_shell_layer();
    Ok(())
}

fn check_bash() {
    let version = std::env::var("BASH_VERSION").unwrap_or_default();
    if version.is_empty() {
        if let Ok(output) = Command::new("bash").arg("--version").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().next() {
                let ver = line
                    .split_whitespace()
                    .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
                    .unwrap_or("unknown");
                let major: u32 = ver.split('.').next().unwrap_or("0").parse().unwrap_or(0);
                let status = if major >= 5 { "✓" } else { "⚠" };
                println!("  Bash              {:<12}{status}", ver);
                return;
            }
        }
        println!("  Bash              unknown      ✘ not found");
    } else {
        let major: u32 = version.split('.').next().unwrap_or("0").parse().unwrap_or(0);
        let status = if major >= 5 { "✓" } else { "⚠ upgrade recommended" };
        println!("  Bash              {:<12}{status}", version);
    }
}

fn check_nerd_font() {
    let term = std::env::var("TERM").unwrap_or_default();
    if term == "dumb" || term.is_empty() {
        println!("  Nerd Font                      ? (can't detect in dumb terminal)");
    } else {
        println!("  Nerd Font                      ? (visual check recommended)");
    }
}

fn check_truecolor() {
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let status = if colorterm == "truecolor" || colorterm == "24bit" {
        "✓"
    } else {
        "⚠ COLORTERM not set to truecolor"
    };
    println!("  TrueColor                      {status}");
}

fn check_blesh() {
    let ble_version = std::env::var("BLE_VERSION").unwrap_or_default();
    if ble_version.is_empty() {
        if which_exists("ble.sh") || Path::new(&format!(
            "{}/.local/share/blesh/ble.sh",
            std::env::var("HOME").unwrap_or_default()
        )).exists() {
            println!("  ble.sh            installed    ⚠ not loaded (source before omarchy10k)");
        } else {
            println!("  ble.sh                         - not installed (optional, enables enhanced mode)");
        }
    } else {
        println!("  ble.sh            {:<12}✓ enhanced mode available", ble_version);
    }
}

fn check_omarchy() {
    let omarchy_path = std::env::var("OMARCHY_PATH").unwrap_or_default();
    if omarchy_path.is_empty() {
        println!("  Omarchy                        - not detected");
    } else {
        let theme_name_path = format!(
            "{}/.local/state/omarchy/current/theme.name",
            std::env::var("HOME").unwrap_or_default()
        );
        let theme = std::fs::read_to_string(theme_name_path)
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string();
        println!("  Omarchy           {:<12}✓ theme: {theme}", "Quattro");
    }
}

fn check_mise() {
    print_tool_check("Mise", "mise", &["--version"]);
}

fn check_atuin() {
    print_tool_check("Atuin", "atuin", &["--version"]);
}

fn check_zoxide() {
    print_tool_check("Zoxide", "zoxide", &["--version"]);
}

fn check_fzf() {
    print_tool_check("fzf", "fzf", &["--version"]);
}

fn check_terminal() {
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    let display = if !term_program.is_empty() {
        term_program
    } else {
        term
    };
    println!("  Terminal           {:<12}✓", display);
}

async fn check_daemon(socket_path: &Path) {
    let runtime_dir = socket_path
        .parent()
        .unwrap_or_else(|| Path::new("/tmp"));

    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(runtime_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("omarchy10k-") && name_str.ends_with(".sock") {
                found.push(entry.path());
            }
        }
    }

    if found.is_empty() {
        println!("  Daemon                         - not running");
        return;
    }

    let mut any_ok = false;
    for sock in &found {
        let shell_pid = sock
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("omarchy10k-"))
            .and_then(|n| n.strip_suffix(".sock"))
            .unwrap_or("?");

        match crate::prompt::send_command(sock, "status").await {
            Ok(_) => {
                println!("  Daemon (shell {shell_pid})           ✓ running");
                any_ok = true;
            }
            Err(_) => {
                println!("  Daemon (shell {shell_pid})           ✘ unresponsive");
            }
        }
    }

    if !any_ok {
        println!("  Daemon                         ✘ all sockets unresponsive");
    }
}

fn check_hooks() {
    // PROMPT_COMMAND is not inherited by child processes, so a doctor run can
    // never observe it. The adapter exports O10K_PARENT_PID when installed
    // (shell/omarchy10k.bash) — use that as the adapter-installed signal.
    match std::env::var("O10K_PARENT_PID") {
        Ok(pid) if !pid.is_empty() => {
            println!("  Hook conflicts                 ✓ adapter installed (O10K_PARENT_PID set)");
        }
        _ => {
            println!("  Hook conflicts                 ? (adapter not detected — check after init)");
        }
    }
}

fn check_config() {
    let config_path = directories::BaseDirs::new()
        .map(|d| d.config_dir().join("omarchy10k/config.toml"))
        .unwrap_or_default();

    if config_path.exists() {
        println!("  Config             {}", config_path.display());
    } else {
        println!("  Config                         - using defaults (no config.toml)");
    }
}

fn check_shell_layer() {
    use crate::layer::{effective_action, load_layer_config, CLAIMS};
    let cfg = load_layer_config();
    match (&cfg.source, cfg.overrides.is_empty()) {
        (Some(p), false) => println!(
            "  Shell layer        policy: {} (overrides: {}), {}",
            cfg.global.as_str(),
            cfg.overrides
                .iter()
                .map(|(k, v)| format!("{k}={}", v.as_str()))
                .collect::<Vec<_>>()
                .join(","),
            p.display()
        ),
        (Some(p), true) => println!(
            "  Shell layer        policy: {}, {}",
            cfg.global.as_str(),
            p.display()
        ),
        (None, _) => println!(
            "  Shell layer        policy: {} (defaults; no config.toml)",
            cfg.global.as_str()
        ),
    }
    for claim in CLAIMS {
        println!(
            "    {:<12}{:<11}{}",
            claim.name,
            effective_action(claim, &cfg),
            claim.note
        );
    }

    // The adapter unhooks starship/ghostty from PROMPT_COMMAND at init now;
    // the old manual surgery in ~/.bashrc is redundant if present.
    let home = std::env::var("HOME").unwrap_or_default();
    let bashrc = format!("{home}/.bashrc");
    match std::fs::read_to_string(&bashrc) {
        Ok(text)
            if text.contains("starship_precmd") && text.contains("unset PS0") =>
        {
            println!(
                "  Prompt handoff     ✓ handled by the adapter at init — legacy prompt surgery in ~/.bashrc (starship_precmd substitution + unset PS0) is safe to remove"
            );
        }
        Ok(_) => println!("  Prompt handoff     ✓ no legacy prompt surgery in ~/.bashrc"),
        Err(_) => println!("  Prompt handoff     ? ~/.bashrc not readable"),
    }
    match std::env::var("O10K_PARENT_PID") {
        Ok(pid) if !pid.is_empty() => {
            println!("  Prompt ownership   ✓ o10k owns the prompt (starship unhooked at init)");
        }
        _ => {
            println!("  Prompt ownership   ? (adapter not detected — check after init)");
        }
    }

    // Terminal include wiring (installed by install.sh, confirmed with TermInc).
    let ghostty = format!("{home}/.config/ghostty/config");
    match std::fs::read_to_string(&ghostty) {
        Ok(text) => {
            let static_line = "config-file = ?\"~/.config/omarchy10k/ghostty.conf\"";
            let themed_line =
                "config-file = ?\"~/.local/state/omarchy/current/theme/o10k-ghostty.conf\"";
            for (label, line) in [("static", static_line), ("themed", themed_line)] {
                if text.contains(line) {
                    println!("  Ghostty include    ✓ {label} wired");
                } else {
                    println!(
                        "  Ghostty include    - {label} missing (run install.sh to add it)"
                    );
                }
            }
        }
        Err(_) => println!("  Ghostty include    ? ~/.config/ghostty/config not readable"),
    }
    let foot = format!("{home}/.config/foot/foot.ini");
    match std::fs::read_to_string(&foot) {
        Ok(text) if text.contains("o10k-foot.ini") => {
            println!("  Foot include       ✓ wired");
        }
        Ok(_) => println!("  Foot include       - missing (run install.sh to add it)"),
        Err(_) => println!("  Foot include       ? ~/.config/foot/foot.ini not readable"),
    }
}

fn print_tool_check(label: &str, cmd: &str, args: &[&str]) {
    if let Ok(output) = Command::new(cmd).args(args).output() {
        let version = String::from_utf8_lossy(&output.stdout)
            .trim()
            .split_whitespace()
            .last()
            .unwrap_or("unknown")
            .to_string();
        println!("  {:<20}{:<12}✓", label, version);
    } else {
        println!("  {:<20}             -", label);
    }
}

fn which_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .is_ok_and(|o| o.status.success())
}
