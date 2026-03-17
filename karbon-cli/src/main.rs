mod commands;
mod config;
mod process;
mod templates;

use clap::{Parser, Subcommand};
use colored::Colorize;
use commands::new::Frontend;
use config::KarbonConfig;

#[derive(Parser)]
#[command(
    name = "karbon",
    about = "Karbon — unified CLI for Rust full-stack projects (SvelteKit or Next.js)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start development servers (backend + frontend with hot-reload)
    Dev,
    /// Build the project for production
    Build,
    /// Run the production build (single port, reverse proxy)
    Serve,
    /// Create a new Karbon project
    New {
        /// Project name (will be used as directory name)
        name: String,
        /// Frontend framework: svelte (default) or nextjs
        #[arg(long, short, default_value = "svelte")]
        frontend: String,
    },
    /// Generate boilerplate code
    #[command(alias = "g")]
    Generate {
        /// What to generate: entity, controller, crud
        kind: String,
        /// Name (PascalCase, e.g. "Post", "BlogComment")
        name: String,
    },
    /// Run SQL migrations from the migration/ directory
    Migrate,
    /// Deploy the project
    /// Targets: publish (deploy only), publish:build (build + deploy), docker (generate Dockerfile)
    Deploy {
        /// Deploy target: publish, publish:build, docker
        #[arg(default_value = "publish:build")]
        target: String,
    },
}

fn main() {
    let cli = Cli::parse();

    // Commands that don't need karbon.toml
    match &cli.command {
        Commands::New { name, frontend } => {
            let fe = match Frontend::from_str(frontend) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("\n{} {e}\n", "error:".red().bold());
                    std::process::exit(1);
                }
            };
            if let Err(e) = commands::new::run(name, fe) {
                eprintln!("\n{} {e}\n", "error:".red().bold());
                std::process::exit(1);
            }
            return;
        }
        _ => {}
    }

    // Commands that need karbon.toml
    let (config, root) = match KarbonConfig::load() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Dev => commands::dev::run(&config, &root),
        Commands::Build => commands::build::run(&config, &root),
        Commands::Serve => commands::serve::run(&config, &root),
        Commands::Generate { ref kind, ref name } => {
            commands::generate::run(kind, name, &root)
        }
        Commands::Migrate => commands::migrate::run(&root),
        Commands::Deploy { ref target } => commands::deploy::run(&config, &root, target),
        Commands::New { .. } => unreachable!(),
    };

    if let Err(e) = result {
        eprintln!("\n{} {e}\n", "error:".red().bold());
        std::process::exit(1);
    }
}
