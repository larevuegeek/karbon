use crate::config::KarbonConfig;
use crate::process::{spawn_command, wait_for_any};
use colored::Colorize;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub fn run(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  {} — dev mode\n",
        "▲ karbon".bold().red(),
        config.app.name.bold()
    );

    // ── Build backend first ──
    println!("  {} Compiling backend...", "⟳".yellow());
    let build_status = Command::new("cargo")
        .args(["build", "-p", &config.backend.package])
        .current_dir(root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("cargo build failed: {e}"))?;

    if !build_status.success() {
        return Err("Backend compilation failed".to_string());
    }
    println!("  {} Backend compiled", "✓".green());

    // ── Run the binary directly (no cargo run intermediary) ──
    let mut children: Vec<Child> = Vec::new();

    let backend_child = start_backend_binary(config, root)?;
    children.push(backend_child);

    // ── Frontend ──
    let frontend_child = start_frontend(config, root)?;
    children.push(frontend_child);

    println!(
        "\n  {} Backend   → {}",
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

/// Run the compiled binary directly — karbon owns the process, no intermediary.
fn start_backend_binary(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let binary = if cfg!(windows) {
        root.join(format!("target/debug/{}.exe", config.backend.package))
    } else {
        root.join(format!("target/debug/{}", config.backend.package))
    };

    if !binary.exists() {
        return Err(format!("Binary not found: {}", binary.display()));
    }

    spawn_command(
        binary.to_str().unwrap_or("api"),
        &[],
        root,
        "backend",
    )
}

fn start_frontend(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let frontend_dir = config.frontend_dir(root);

    if !frontend_dir.join("node_modules").exists() {
        println!("  {} Installing frontend dependencies...", "↓".blue());
        #[cfg(windows)]
        let status = Command::new("cmd")
            .args(["/C", "npm", "install"])
            .current_dir(&frontend_dir)
            .status()
            .map_err(|e| format!("npm install failed: {e}"))?;
        #[cfg(not(windows))]
        let status = Command::new("npm")
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
