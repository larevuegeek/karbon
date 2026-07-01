mod commands;
mod config;
mod process;
mod templates;

use clap::{Parser, Subcommand};
use colored::Colorize;
use commands::new::{Frontend, Skeleton};
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
        /// Project skeleton: full (backend + frontend) or micro (backend-only)
        #[arg(long, short, default_value = "full")]
        skeleton: String,
        /// Starter template: none (default) or blog (scaffolds a Post CRUD + admin)
        #[arg(long, short, default_value = "none")]
        template: String,
        /// Dev only: path to a local karbon checkout, so the generated project
        /// depends on it (path dep) instead of crates.io. E.g. --local ../karbon
        #[arg(long, default_value = "")]
        local: String,
    },
    /// Diagnose common project issues. Exits non-zero if any check fails.
    Doctor {
        /// Also connect to the database and check connectivity + pending migrations
        #[arg(long)]
        db: bool,
    },
    /// Build documentation. Actions: build (renders docs/*.md → docs/_site/)
    Docs {
        /// Docs action: build
        #[arg(default_value = "build")]
        action: String,
    },
    /// Generate boilerplate code
    #[command(alias = "g")]
    Generate {
        /// What to generate: entity, controller, crud, admin
        kind: String,
        /// Name (PascalCase, e.g. "Post", "BlogComment")
        name: String,
        /// Field specs for entity/crud: name:type[?] (e.g. title:string body:text published:bool)
        /// Types: string text int bigint float bool datetime date json. `?` = nullable.
        fields: Vec<String>,
        /// Show what would be generated without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Overwrite existing files instead of skipping them
        #[arg(long)]
        force: bool,
        /// Build fields interactively (Symfony make:entity-style assistant)
        #[arg(long, short)]
        interactive: bool,
    },
    /// Run database migrations from the migration/ directory
    /// Actions: up (default), status, rollback, diff
    Migrate {
        /// Migration action: up, status, rollback, diff
        #[arg(default_value = "up")]
        action: String,
        /// For `diff`: optional name for the generated migration file
        name: Option<String>,
    },
    /// Deploy the project
    /// Targets: publish, publish:build, docker, paas (config files), fly, railway
    Deploy {
        /// Deploy target: publish, publish:build, docker, paas, fly, railway
        #[arg(default_value = "publish:build")]
        target: String,
    },
}

fn main() {
    let cli = Cli::parse();

    // Commands that don't need karbon.toml
    if let Commands::New {
        name,
        frontend,
        skeleton,
        template,
        local,
    } = &cli.command
    {
        let fe = match Frontend::from_str(frontend) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("\n{} {e}\n", "error:".red().bold());
                std::process::exit(1);
            }
        };
        let sk = match Skeleton::from_str(skeleton) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("\n{} {e}\n", "error:".red().bold());
                std::process::exit(1);
            }
        };
        if let Err(e) = commands::new::run(name, fe, sk, template, local) {
            eprintln!("\n{} {e}\n", "error:".red().bold());
            std::process::exit(1);
        }
        return;
    }

    // `docs` works anywhere with a docs/ folder — no karbon.toml required.
    if let Commands::Docs { action } = &cli.command {
        let root = std::env::current_dir().unwrap_or_default();
        if let Err(e) = commands::docs::run(&root, action) {
            eprintln!("\n{} {e}\n", "error:".red().bold());
            std::process::exit(1);
        }
        return;
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
        Commands::Doctor { db } => commands::doctor::run(&config, &root, db),
        Commands::Generate {
            ref kind,
            ref name,
            ref fields,
            dry_run,
            force,
            interactive,
        } => commands::generate::run(
            kind,
            name,
            fields,
            &root,
            commands::generate::GenOptions {
                dry_run,
                force,
                interactive,
            },
        ),
        Commands::Migrate {
            ref action,
            ref name,
        } => commands::migrate::run(&root, action, name.as_deref()),
        Commands::Deploy { ref target } => commands::deploy::run(&config, &root, target),
        Commands::New { .. } | Commands::Docs { .. } => unreachable!(),
    };

    if let Err(e) = result {
        eprintln!("\n{} {e}\n", "error:".red().bold());
        std::process::exit(1);
    }
}
