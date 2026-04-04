//! flowctl CLI entry point.
//!
//! This binary provides the `flowctl` command-line interface for
//! managing development workflows.

use clap::Parser;

/// flowctl - development orchestration engine.
#[derive(Parser, Debug)]
#[command(name = "flowctl", version, about = "Development orchestration engine")]
struct Cli {
    /// Output as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Show current status.
    Status,
    /// Show version information.
    Version,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Status) => {
            if cli.json {
                println!(r#"{{"success": true, "status": "ok"}}"#);
            } else {
                println!("flowctl status: ok");
            }
        }
        Some(Commands::Version) => {
            println!("flowctl {}", env!("CARGO_PKG_VERSION"));
        }
        None => {
            println!("flowctl {}", env!("CARGO_PKG_VERSION"));
            println!("Use --help for usage information.");
        }
    }
}
