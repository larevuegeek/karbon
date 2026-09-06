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
            _ => Err(format!(
                "Unknown frontend '{}'. Available: svelte, nextjs",
                s
            )),
        }
    }

    fn label(&self) -> &str {
        match self {
            Self::Svelte => "SvelteKit",
            Self::Nextjs => "Next.js",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Skeleton {
    /// Backend + frontend (default).
    Full,
    /// Backend-only micro-kernel (no frontend).
    Micro,
}

impl Skeleton {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "full" | "default" => Ok(Self::Full),
            "micro" | "minimal" | "api" => Ok(Self::Micro),
            _ => Err(format!("Unknown skeleton '{}'. Available: full, micro", s)),
        }
    }
}

pub fn run(
    name: &str,
    frontend: Frontend,
    skeleton: Skeleton,
    template: &str,
    local: &str,
) -> Result<(), String> {
    let kind_label = match skeleton {
        Skeleton::Full => frontend.label().to_string(),
        Skeleton::Micro => "backend-only / micro".to_string(),
    };
    println!(
        "\n{}  Creating project {} ({})\n",
        "▲ karbon".bold().red(),
        name.bold().cyan(),
        kind_label.bold()
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
        "migration".to_string(),
    ];

    // Frontend-specific dirs (skipped for the micro skeleton)
    if skeleton == Skeleton::Full {
        dirs.push("frontend/static".to_string());
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
    }

    for dir in &dirs {
        let path = root.join(dir);
        fs::create_dir_all(&path).map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
        print_created(dir, true);
    }

    // Shared files (backend + config) — main.rs and karbon.toml differ per skeleton
    let (karbon_toml_tpl, main_rs_tpl) = match skeleton {
        Skeleton::Full => (templates::KARBON_TOML, templates::MAIN_RS),
        Skeleton::Micro => (templates::KARBON_TOML_MICRO, templates::MAIN_RS_MICRO),
    };

    let mut files: Vec<(&str, &str)> = vec![
        ("karbon.toml", karbon_toml_tpl),
        (".env", templates::ENV_EXAMPLE),
        (".env.example", templates::ENV_EXAMPLE),
        (".gitignore", templates::GITIGNORE),
        ("Cargo.toml", templates::CARGO_WORKSPACE),
        ("app/Cargo.toml", templates::CARGO_APP),
        ("app/src/main.rs", main_rs_tpl),
        ("app/src/controller/mod.rs", templates::CONTROLLER_MOD),
        ("app/src/controller/health.rs", templates::HEALTH_CONTROLLER),
        ("app/src/entity/mod.rs", templates::ENTITY_MOD),
        ("docs/index.md", templates::DOCS_INDEX),
    ];

    // Micro: welcome page served by the backend at `/`
    if skeleton == Skeleton::Micro {
        files.push(("app/src/welcome.html", templates::WELCOME_HTML));
    }

    // Frontend-specific files (skipped for the micro skeleton)
    if skeleton == Skeleton::Full {
        match frontend {
            Frontend::Svelte => {
                files.extend([
                    ("frontend/package.json", templates::svelte::PACKAGE_JSON),
                    (
                        "frontend/svelte.config.js",
                        templates::svelte::SVELTE_CONFIG,
                    ),
                    ("frontend/vite.config.ts", templates::svelte::VITE_CONFIG),
                    ("frontend/tsconfig.json", templates::svelte::TSCONFIG),
                    ("frontend/src/app.css", templates::svelte::APP_CSS),
                    ("frontend/src/app.html", templates::svelte::APP_HTML),
                    ("frontend/src/app.d.ts", templates::svelte::APP_D_TS),
                    (
                        "frontend/src/routes/+layout.svelte",
                        templates::svelte::LAYOUT,
                    ),
                    ("frontend/src/routes/+page.svelte", templates::svelte::PAGE),
                    ("frontend/src/lib/api.ts", templates::svelte::API_TS),
                    (
                        "frontend/src/hooks.server.ts",
                        templates::svelte::HOOKS_SERVER,
                    ),
                ]);
            }
            Frontend::Nextjs => {
                files.extend([
                    ("frontend/package.json", templates::nextjs::PACKAGE_JSON),
                    ("frontend/next.config.ts", templates::nextjs::NEXT_CONFIG),
                    ("frontend/tsconfig.json", templates::nextjs::TSCONFIG),
                    (
                        "frontend/src/app/globals.css",
                        templates::nextjs::GLOBALS_CSS,
                    ),
                    ("frontend/src/app/layout.tsx", templates::nextjs::LAYOUT),
                    ("frontend/src/app/page.tsx", templates::nextjs::PAGE),
                    ("frontend/src/lib/api.ts", templates::nextjs::API_TS),
                ]);
            }
        }
    }

    // Adapt karbon.toml for Next.js (full skeleton only)
    let karbon_toml_override = match (skeleton, frontend) {
        (Skeleton::Full, Frontend::Nextjs) => Some((
            "karbon.toml",
            templates::KARBON_TOML.replace("node build/index.js", "npm run start"),
        )),
        _ => None,
    };

    for (path, content) in &files {
        let full_path = root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        // Use overridden content if available
        let actual_content =
            if let Some((override_path, ref override_content)) = karbon_toml_override {
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
            .replace("{{PROJECT_NAME_TITLE}}", &title)
            .replace("{{KARBON_VERSION}}", env!("CARGO_PKG_VERSION"));
        fs::write(&full_path, rendered)
            .map_err(|e| format!("Cannot write {}: {e}", full_path.display()))?;
        print_created(path, false);
    }

    // Dev convenience: point the generated project at a local karbon checkout.
    if !local.is_empty() {
        apply_local_dep(root, local)?;
    } else {
        println!(
            "\n  {} No {} given — `karbon` resolves to the published crates.io version.",
            "⚠".yellow(),
            "--local".bold()
        );
        println!(
            "    For dev against this checkout (current features, studio, no-DB), use: {}",
            format!("karbon new {name} --local <path-to-karbon-repo>").dimmed()
        );
    }

    // Initialize git
    let git_result = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .status();
    if git_result.is_ok() {
        print_created(".git/", true);
    }

    // Starter template: scaffold a Post CRUD + admin on top of the skeleton.
    let is_blog = template.eq_ignore_ascii_case("blog");
    if is_blog {
        println!(
            "\n  {} Scaffolding blog starter (Post CRUD + admin)...\n",
            "▲".red()
        );
        let gen_opts = crate::commands::generate::GenOptions::default();
        let post_fields = ["title:string".to_string(), "slug:string".to_string()];
        crate::commands::generate::run("crud", "Post", &post_fields, root, gen_opts)?;
        crate::commands::generate::run("admin", "Post", &[], root, gen_opts)?;
    } else if !template.eq_ignore_ascii_case("none") {
        return Err(format!(
            "Unknown template '{template}'. Available: none, blog"
        ));
    }

    println!(
        "\n  {} Project {} created! ({})\n",
        "✓".green().bold(),
        name.bold().cyan(),
        kind_label
    );
    println!("  Next steps:\n");
    println!("    {} {}", "cd".dimmed(), name);
    if is_blog {
        println!(
            "    {} (set DB_NAME in .env first) to generate the migration",
            "karbon migrate diff".bold()
        );
        println!("    {} to create the table", "karbon migrate".bold());
    }
    println!("    {} to start developing\n", "karbon dev".bold());

    Ok(())
}

/// Rewrite the generated `app/Cargo.toml` so `karbon` is a path dependency on a
/// local checkout (dev convenience — the framework isn't on crates.io yet).
fn apply_local_dep(root: &Path, local: &str) -> Result<(), String> {
    let abs =
        fs::canonicalize(local).map_err(|e| format!("--local path '{local}' not found: {e}"))?;
    let fw = abs.join("karbon-framework");
    if !fw.join("Cargo.toml").exists() {
        return Err(format!(
            "--local: no `karbon-framework` crate found at {}",
            fw.display()
        ));
    }
    let fw_str = fw.to_string_lossy().replace('\\', "/");
    let fw_str = fw_str.strip_prefix("//?/").unwrap_or(&fw_str).to_string();

    let app_cargo = root.join("app/Cargo.toml");
    let content =
        fs::read_to_string(&app_cargo).map_err(|e| format!("read app/Cargo.toml: {e}"))?;
    // Replace the whole `karbon = { … }` line (preserves the studio feature).
    let mut replaced = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("karbon = {") {
                replaced = true;
                format!(
                    "karbon = {{ package = \"karbon-framework\", path = \"{fw_str}\", features = [\"studio\"] }}"
                )
            } else {
                l.to_string()
            }
        })
        .collect();
    if !replaced {
        return Err("app/Cargo.toml: `karbon` dependency line not found".to_string());
    }
    lines.push(String::new());
    fs::write(&app_cargo, lines.join("\n")).map_err(|e| format!("write app/Cargo.toml: {e}"))?;
    println!(
        "  {} app/Cargo.toml → karbon = path \"{}\"",
        "✓".green(),
        fw_str.dimmed()
    );
    Ok(())
}

fn print_created(path: &str, is_dir: bool) {
    if path.is_empty() {
        return;
    }
    let icon = if is_dir { "📁" } else { "  " };
    println!("  {} {}", icon, path.dimmed());
}
