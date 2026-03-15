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

    // ── Step 1: Start frontend SSR (Node) in background ──
    let frontend_child = start_frontend_ssr(config, root)?;
    children.push(frontend_child);

    // ── Step 2: Start backend with reverse proxy to frontend ──
    let backend_child = start_backend(config, root)?;
    children.push(backend_child);

    println!(
        "\n  {} Application running → {}  (single port)",
        "●".green().bold(),
        format!("http://localhost:{}", config.backend.port).cyan().bold()
    );
    println!(
        "    {} /api/*   → Rust (Axum)",
        "├".dimmed()
    );
    println!(
        "    {} /*       → Frontend SSR (proxied)",
        "└".dimmed()
    );
    println!("\n  {} to stop\n", "Ctrl+C".bold().yellow());

    wait_for_any(&mut children);

    Ok(())
}

fn start_frontend_ssr(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let frontend_dir = config.frontend_dir(root);
    let build_dir = frontend_dir.join("build");

    if !build_dir.exists() {
        return Err(format!(
            "Frontend build not found at {}. Run `karbon build` first.",
            build_dir.display()
        ));
    }

    let parts: Vec<&str> = config.frontend.serve_cmd.split_whitespace().collect();
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    // Set PORT env for the Node SSR server
    let mut env = HashMap::new();
    env.insert("PORT".to_string(), config.frontend.port.to_string());

    println!(
        "  {} Frontend SSR (internal) → :{} ",
        "→".blue(),
        config.frontend.port
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

    // Pass frontend URL to the backend so it enables reverse proxy
    let frontend_url = format!("http://localhost:{}", config.frontend.port);
    let mut env = HashMap::new();
    env.insert("KARBON_FRONTEND_URL".to_string(), frontend_url);

    println!(
        "  {} Backend API + Proxy    → :{}",
        "→".blue(),
        config.backend.port
    );

    spawn_command_with_env(
        binary.to_str().unwrap(),
        &[],
        root,
        "backend",
        &env,
    )
}
