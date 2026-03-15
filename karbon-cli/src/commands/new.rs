use crate::templates;
use colored::Colorize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Frontend {
    Svelte,
    Nextjs,
}

impl Frontend {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "svelte" | "sveltekit" => Ok(Self::Svelte),
            "next" | "nextjs" | "next.js" | "react" => Ok(Self::Nextjs),
            _ => Err(format!("Unknown frontend '{}'. Available: svelte, nextjs", s)),
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Svelte => "SvelteKit",
            Self::Nextjs => "Next.js",
        }
    }
}

pub fn run(name: &str, frontend: Frontend) -> Result<(), String> {
    println!(
        "\n{}  Creating project {} ({})\n",
        "▲ karbon".bold().red(),
        name.bold().cyan(),
        frontend.label().bold()
    );

    let root = Path::new(name);

    if root.exists() {
        return Err(format!("Directory '{}' already exists", name));
    }

    // Derive name variants
    let snake = name.replace('-', "_");
    let title = name
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Create directory structure — backend is always the same
    let mut dirs = vec![
        "".to_string(),
        "app/src/controller".to_string(),
        "app/src/entity".to_string(),
        "app/src/repository".to_string(),
        "app/src/service".to_string(),
        "frontend/static".to_string(),
        "migration".to_string(),
    ];

    // Frontend-specific dirs
    match frontend {
        Frontend::Svelte => {
            dirs.push("frontend/src/routes".to_string());
            dirs.push("frontend/src/lib".to_string());
        }
        Frontend::Nextjs => {
            dirs.push("frontend/src/app".to_string());
            dirs.push("frontend/src/lib".to_string());
            dirs.push("frontend/public".to_string());
        }
    }

    for dir in &dirs {
        let path = root.join(dir);
        fs::create_dir_all(&path)
            .map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
        print_created(dir, true);
    }

    // Shared files (backend + config)
    let mut files: Vec<(&str, &str)> = vec![
        ("karbon.toml", templates::KARBON_TOML),
        (".env", templates::ENV_EXAMPLE),
        (".env.example", templates::ENV_EXAMPLE),
        (".gitignore", templates::GITIGNORE),
        ("Cargo.toml", templates::CARGO_WORKSPACE),
        ("app/Cargo.toml", templates::CARGO_APP),
        ("app/src/main.rs", templates::MAIN_RS),
        ("app/src/controller/mod.rs", templates::CONTROLLER_MOD),
        ("app/src/controller/health.rs", templates::HEALTH_CONTROLLER),
        ("app/src/entity/mod.rs", templates::ENTITY_MOD),
    ];

    // Frontend-specific files
    match frontend {
        Frontend::Svelte => {
            files.extend([
                ("frontend/package.json", templates::svelte::PACKAGE_JSON),
                ("frontend/svelte.config.js", templates::svelte::SVELTE_CONFIG),
                ("frontend/vite.config.ts", templates::svelte::VITE_CONFIG),
                ("frontend/tsconfig.json", templates::svelte::TSCONFIG),
                ("frontend/src/app.css", templates::svelte::APP_CSS),
                ("frontend/src/app.d.ts", templates::svelte::APP_D_TS),
                ("frontend/src/routes/+layout.svelte", templates::svelte::LAYOUT),
                ("frontend/src/routes/+page.svelte", templates::svelte::PAGE),
                ("frontend/src/lib/api.ts", templates::svelte::API_TS),
                ("frontend/src/hooks.server.ts", templates::svelte::HOOKS_SERVER),
            ]);
        }
        Frontend::Nextjs => {
            files.extend([
                ("frontend/package.json", templates::nextjs::PACKAGE_JSON),
                ("frontend/next.config.ts", templates::nextjs::NEXT_CONFIG),
                ("frontend/tsconfig.json", templates::nextjs::TSCONFIG),
                ("frontend/src/app/globals.css", templates::nextjs::GLOBALS_CSS),
                ("frontend/src/app/layout.tsx", templates::nextjs::LAYOUT),
                ("frontend/src/app/page.tsx", templates::nextjs::PAGE),
                ("frontend/src/lib/api.ts", templates::nextjs::API_TS),
            ]);
        }
    }

    // Adapt karbon.toml for Next.js
    let karbon_toml_override = match frontend {
        Frontend::Nextjs => Some((
            "karbon.toml",
            templates::KARBON_TOML
                .replace("npm run dev", "npm run dev")
                .replace("npm run build", "npm run build")
                .replace("node build/index.js", "npm run start"),
        )),
        _ => None,
    };

    for (path, content) in &files {
        let full_path = root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        // Use overridden content if available
        let actual_content = if let Some((override_path, ref override_content)) = karbon_toml_override {
            if *path == override_path {
                override_content.as_str()
            } else {
                content
            }
        } else {
            content
        };

        let rendered = actual_content
            .replace("{{PROJECT_NAME}}", name)
            .replace("{{PROJECT_NAME_SNAKE}}", &snake)
            .replace("{{PROJECT_NAME_TITLE}}", &title);
        fs::write(&full_path, rendered)
            .map_err(|e| format!("Cannot write {}: {e}", full_path.display()))?;
        print_created(path, false);
    }

    // Initialize git
    let git_result = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status();
    if git_result.is_ok() {
        print_created(".git/", true);
    }

    println!(
        "\n  {} Project {} created! ({})\n",
        "✓".green().bold(),
        name.bold().cyan(),
        frontend.label()
    );
    println!("  Next steps:\n");
    println!("    {} {}", "cd".dimmed(), name);
    println!("    {} to start developing\n", "karbon dev".bold());

    Ok(())
}

fn print_created(path: &str, is_dir: bool) {
    if path.is_empty() {
        return;
    }
    let icon = if is_dir { "📁" } else { "  " };
    println!("  {} {}", icon, path.dimmed());
}
