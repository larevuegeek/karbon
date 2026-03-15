use crate::config::KarbonConfig;
use crate::process::{spawn_command, wait_for_any};
use colored::Colorize;
use std::path::Path;
use std::process::Child;

pub fn run(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  {} — dev mode\n",
        "▲ karbon".bold().red(),
        config.app.name.bold()
    );

    let mut children: Vec<Child> = Vec::new();

    // ── Backend ──
    let backend_child = start_backend(config, root)?;
    children.push(backend_child);

    // ── Frontend ──
    let frontend_child = start_frontend(config, root)?;
    children.push(frontend_child);

    println!(
        "  {} Backend   → {}",
        "●".green(),
        format!("http://localhost:{}", config.backend.port).cyan()
    );
    println!(
        "  {} Frontend  → {}",
        "●".green(),
        format!("http://localhost:{}", config.frontend.port).cyan()
    );
    println!("\n  {} to stop\n", "Ctrl+C".bold().yellow());

    // Wait for either process to exit
    wait_for_any(&mut children);

    Ok(())
}

fn start_backend(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let has_watch = which::which("cargo-watch").is_ok();

    let (cmd, args) = if config.backend.watch && has_watch {
        println!(
            "  {} cargo-watch detected — hot-reload enabled",
            "✓".green()
        );
        (
            "cargo".to_string(),
            vec![
                "watch".to_string(),
                "-x".to_string(),
                format!("run -p {}", config.backend.package),
                "-w".to_string(),
                format!("{}/src", config.backend.package),
                "-w".to_string(),
                "karbon-framework/src".to_string(),
            ],
        )
    } else {
        if config.backend.watch && !has_watch {
            println!(
                "  {} cargo-watch not found — install with: {}",
                "!".yellow(),
                "cargo install cargo-watch".bold()
            );
        }
        (
            "cargo".to_string(),
            vec![
                "run".to_string(),
                "-p".to_string(),
                config.backend.package.clone(),
            ],
        )
    };

    spawn_command(&cmd, &args, root, "backend")
}

fn start_frontend(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let frontend_dir = config.frontend_dir(root);

    if !frontend_dir.join("node_modules").exists() {
        println!("  {} Installing frontend dependencies...", "↓".blue());
        let status = std::process::Command::new("npm")
            .arg("install")
            .current_dir(&frontend_dir)
            .status()
            .map_err(|e| format!("npm install failed: {e}"))?;
        if !status.success() {
            return Err("npm install failed".to_string());
        }
    }

    // Parse "npm run dev" into command + args
    let parts: Vec<&str> = config.frontend.dev_cmd.split_whitespace().collect();
    let cmd = parts[0];
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    spawn_command(cmd, &args, &frontend_dir, "frontend")
}
