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

    println!();
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
    if socket_path.exists() {
        match crate::prompt::send_command(socket_path, "status").await {
            Ok(_) => println!("  Daemon                         ✓ running"),
            Err(_) => println!("  Daemon                         ✘ socket exists but unresponsive"),
        }
    } else {
        println!("  Daemon                         - not running");
    }
}

fn check_hooks() {
    let prompt_cmd = std::env::var("PROMPT_COMMAND").unwrap_or_default();
    if prompt_cmd.contains("__o10k_render_prompt") || prompt_cmd.contains("o10k") {
        println!("  Hook conflicts                 ✓ none detected");
    } else {
        println!("  Hook conflicts                 ? (check after init)");
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
