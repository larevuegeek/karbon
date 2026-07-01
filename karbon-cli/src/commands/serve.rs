use crate::config::KarbonConfig;
use crate::process::{spawn_command_with_env, wait_for_any};
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Child;

pub fn run(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  {} — production\n",
        "▲ karbon".bold().red(),
        config.app.name.bold()
    );

    let mut children: Vec<Child> = Vec::new();

    let has_frontend = config.frontend.is_some();

    // ── Step 1: Start frontend SSR (Node) in background, if any ──
    if has_frontend {
        let frontend_child = start_frontend_ssr(config, root)?;
        children.push(frontend_child);
    }

    // ── Step 2: Start backend (with reverse proxy to frontend if present) ──
    let backend_child = start_backend(config, root)?;
    children.push(backend_child);

    println!(
        "\n  {} Application running → {}  ({})",
        "●".green().bold(),
        format!("http://localhost:{}", config.backend.port)
            .cyan()
            .bold(),
        if has_frontend {
            "single port"
        } else {
            "API-only"
        }
    );
    if has_frontend {
        println!("    {} /api/*   → Rust (Axum)", "├".dimmed());
        println!("    {} /*       → Frontend SSR (proxied)", "└".dimmed());
    } else {
        println!("    {} /*       → Rust (Axum)", "└".dimmed());
    }
    println!("\n  {} to stop\n", "Ctrl+C".bold().yellow());

    wait_for_any(&mut children);

    Ok(())
}

fn start_frontend_ssr(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let frontend = config.frontend.as_ref().ok_or("No [frontend] configured")?;
    let frontend_dir = root.join(&frontend.dir);
    let build_dir = frontend_dir.join("build");

    if !build_dir.exists() {
        return Err(format!(
            "Frontend build not found at {}. Run `karbon build` first.",
            build_dir.display()
        ));
    }

    let parts: Vec<&str> = frontend.serve_cmd.split_whitespace().collect();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    // Set PORT env for the Node SSR server
    let mut env = HashMap::new();
    env.insert("PORT".to_string(), frontend.port.to_string());

    println!(
        "  {} Frontend SSR (internal) → :{} ",
        "→".blue(),
        frontend.port
    );

    spawn_command_with_env(parts[0], &args, &frontend_dir, "frontend-ssr", &env)
}

fn start_backend(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let binary = if cfg!(windows) {
        root.join(format!("target/release/{}.exe", config.backend.package))
    } else {
        root.join(format!("target/release/{}", config.backend.package))
    };

    if !binary.exists() {
        return Err(format!(
            "Backend binary not found at {}. Run `karbon build` first.",
            binary.display()
        ));
    }

    // Pass frontend URL to the backend so it enables reverse proxy (only if a
    // frontend is configured — backend-only projects serve everything directly).
    let mut env = HashMap::new();
    if let Some(frontend) = &config.frontend {
        env.insert(
            "KARBON_FRONTEND_URL".to_string(),
            format!("http://localhost:{}", frontend.port),
        );
    }

    println!(
        "  {} Backend{} → :{}",
        "→".blue(),
        if config.frontend.is_some() {
            " API + Proxy   "
        } else {
            " API           "
        },
        config.backend.port
    );

    spawn_command_with_env(binary.to_str().unwrap(), &[], root, "backend", &env)
}
