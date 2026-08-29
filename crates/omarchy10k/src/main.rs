mod bridge;
mod configure;
mod doctor;
mod hook_event;
mod intro;
mod layer;
mod prompt;
mod script;
mod statusline;
mod update;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "omarchy10k", version, about = "Omarchy10k — reactive shell UI for Bash")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum LookAction {
    /// List available Looks (curated + user-saved)
    List,
    /// Apply a Look atomically
    Apply {
        name: String,
        /// Try in-memory only (reverted by the next config reload)
        #[arg(long)]
        transient: bool,
    },
    /// Snapshot the current appearance as a named Look
    Save {
        name: String,
        #[arg(short, long, default_value = "")]
        label: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Render the prompt by querying the daemon
    Prompt {
        /// Current working directory
        #[arg(long, default_value = ".")]
        cwd: String,
        /// Exit code of the last command
        #[arg(long, default_value_t = 0)]
        exit_code: i32,
        /// Duration of the last command in milliseconds
        #[arg(long, default_value_t = 0)]
        cmd_duration_ms: u64,
        /// Terminal width in columns
        #[arg(long, default_value_t = 80)]
        cols: u16,
        /// Number of background jobs
        #[arg(long, default_value_t = 0)]
        jobs: u32,
    },

    /// Emit the Bash adapter script for sourcing in .bashrc
    Init {
        /// Shell to generate init script for
        shell: String,
    },

    /// Show the shell layer claim map (inventory + effective policy)
    Layer {
        /// Output as JSON (for the Control Center panel)
        #[arg(long)]
        json: bool,
    },

    Doctor,

    /// Signal the daemon to reload its configuration
    Reload,

    /// Browse and apply named appearance bundles (Looks)
    Look {
        #[command(subcommand)]
        action: LookAction,
    },

    /// Run a prompt render benchmark
    Benchmark {
        /// Number of iterations
        #[arg(long, default_value_t = 100)]
        iterations: u32,
    },

    /// Dump daemon state for debugging
    Debug,

    /// Run as a persistent bridge coprocess between Bash and the daemon
    Bridge {
        /// Socket path (defaults to auto-detected)
        #[arg(long)]
        socket: Option<String>,
    },

    /// Render the Claude Code statusline via the daemon (reads statusLine JSON from stdin)
    Statusline,

    /// One-time themed welcome on first shell start
    Intro {
        /// Render even if the intro marker file exists
        #[arg(long)]
        force: bool,
    },

    /// Interactive setup wizard — pick style, separators, frame, and icons
    /// with a live prompt preview
    Configure,

    /// Extract the left prompt from a JSON daemon response (used internally)
    #[command(name = "parse-prompt", hide = true)]
    ParsePrompt,

    /// Update Omarchy10k: pull latest source, rebuild, and reinstall
    Update {
        /// Skip git pull (rebuild from current source tree)
        #[arg(long)]
        no_pull: bool,
        /// Skip rebuilding (just reinstall existing binaries + plugin)
        #[arg(long)]
        no_build: bool,
    },

    /// Run a shell-level end-to-end benchmark measuring real prompt latency
    /// List and run user-defined quick actions (~/.config/omarchy10k/scripts)
    Script {
        /// list | run
        action: String,
        /// Script name (required for run)
        name: Option<String>,
    },

    /// Dispatch a desktop hook event to Omarchy's hook system
    HookEvent {
        /// Event name, e.g. battery-low, post-update, font-set
        name: String,
        /// Event arguments (e.g. battery percentage)
        args: Vec<String>,
    },

    #[command(name = "benchmark-shell", hide = true)]
    BenchmarkShell {
        /// Number of iterations
        #[arg(long, default_value_t = 100)]
        iterations: u32,
        /// Path to the bash adapter script
        #[arg(long)]
        adapter: Option<String>,
    },
}

fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let ppid = std::env::var("O10K_PARENT_PID")
        .or_else(|_| std::env::var("PPID"))
        .unwrap_or_else(|_| "0".into());
    std::path::PathBuf::from(runtime_dir).join(format!("omarchy10k-{ppid}.sock"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Prompt {
            cwd,
            exit_code,
            cmd_duration_ms,
            cols,
            jobs,
        } => {
            let cwd = if cwd == "." {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".into())
            } else {
                cwd
            };
            prompt::render(&socket_path(), &cwd, exit_code, cmd_duration_ms, cols, jobs).await?;
        }

        Commands::Init { shell } => {
            if shell != "bash" {
                eprintln!("omarchy10k: only 'bash' is supported (got '{shell}')");
                std::process::exit(1);
            }
            let layer_cfg = layer::load_layer_config();
            if layer_cfg.global != layer::Policy::Extend {
                eprintln!(
                    "# [shell.layer] policy = \"{}\" — non-default, baked into this init (see `omarchy10k layer`)",
                    layer_cfg.global.as_str()
                );
            }
            if !layer_cfg.overrides.is_empty() {
                let list = layer_cfg
                    .overrides
                    .iter()
                    .map(|(k, v)| format!("{k} = \"{}\"", v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!("# [shell.layer.overrides] {list}");
            }
            print!("{}", layer::prelude(&layer_cfg));
            print!("{}", include_str!("../../../shell/omarchy10k.bash"));
        }

        Commands::Layer { json } => {
            let layer_cfg = layer::load_layer_config();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&layer::render_json(&layer_cfg))?
                );
            } else {
                print!("{}", layer::render_map(&layer_cfg));
            }
        }

        Commands::Doctor => {
            doctor::run(&socket_path()).await?;
        }

        Commands::Reload => {
            prompt::send_command(&socket_path(), "reload_config").await?;
            println!("config reloaded");
        }

        Commands::Look { action } => match action {
            LookAction::List => {
                prompt::send_command(&socket_path(), "looks").await?;
            }
            LookAction::Apply { name, transient } => {
                let request = serde_json::json!({
                    "command": "looks_apply",
                    "name": name,
                    "transient": transient,
                });
                let response =
                    prompt::send_request(&socket_path(), &request.to_string()).await?;
                println!("{response}");
            }
            LookAction::Save { name, label } => {
                let request = serde_json::json!({
                    "command": "looks_save",
                    "name": name,
                    "label": label,
                });
                let response =
                    prompt::send_request(&socket_path(), &request.to_string()).await?;
                println!("{response}");
            }
        },

        Commands::Benchmark { iterations } => {
            prompt::benchmark(&socket_path(), iterations).await?;
        }

        Commands::Debug => {
            prompt::send_command(&socket_path(), "status").await?;
        }

        Commands::Update { no_pull, no_build } => {
            update::run(no_pull, no_build)?;
        }

        Commands::Bridge { socket } => {
            let sock = socket
                .map(std::path::PathBuf::from)
                .unwrap_or_else(socket_path);
            bridge::run(&sock).await?;
        }
        Commands::Statusline => {
            statusline::run(&socket_path()).await?;
        }

        Commands::Script { action, name } => {
            script::run(&socket_path(), &action, name.as_deref()).await?;
        }

        Commands::HookEvent { name, args } => {
            hook_event::run(&name, &args, hook_event::find_dispatcher().as_deref(), &hook_event::default_hook_root())?;
        }

        Commands::Configure => {
            configure::run().await?;
        }

        Commands::Intro { force } => {
            intro::run(&socket_path(), force).await?;
        }

        Commands::ParsePrompt => {
            let mut input = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&input) {
                if let Some(left) = v.get("left").and_then(|l| l.as_str()) {
                    print!("{left}");
                }
            }
        }

        Commands::BenchmarkShell { iterations, adapter } => {
            prompt::benchmark_shell(&socket_path(), iterations, adapter.as_deref()).await?;
        }
    }

    Ok(())
}
