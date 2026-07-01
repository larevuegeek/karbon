use crate::config::KarbonConfig;
use crate::process::{kill_child, spawn_command, spawn_command_with_env};
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

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
    let mut backend: Option<Child> = Some(start_backend_binary(config, root)?);

    // ── Frontend (skipped for backend-only / micro projects) ──
    let mut frontend: Option<Child> = if config.frontend.is_some() {
        Some(start_frontend(config, root)?)
    } else {
        None
    };

    println!(
        "\n  {} Backend   → {}",
        "●".green(),
        format!("http://localhost:{}", config.backend.port).cyan()
    );
    if let Some(frontend) = &config.frontend {
        println!(
            "  {} Frontend  → {}",
            "●".green(),
            format!("http://localhost:{}", frontend.port).cyan()
        );
    }
    println!(
        "  {} watching {} — backend recompiles on save",
        "↻".magenta(),
        "app/src".dimmed()
    );
    println!("\n  {} to stop\n", "Ctrl+C".bold().yellow());

    supervise(config, root, &mut backend, &mut frontend);
    Ok(())
}

/// Supervisor loop: watches the backend sources and recompiles + restarts the
/// backend binary on change (the frontend keeps its own HMR). Also forwards
/// Ctrl+C and exits if a watched process dies on its own.
fn supervise(
    config: &KarbonConfig,
    root: &Path,
    backend: &mut Option<Child>,
    frontend: &mut Option<Child>,
) {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst)).ok();

    let watch_dir = ["app/src", "api/src"]
        .iter()
        .map(|d| root.join(d))
        .find(|p| p.exists());
    let mut last = watch_dir
        .as_ref()
        .map(|d| latest_mtime(d))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    while running.load(Ordering::SeqCst) {
        // A watched process died on its own (crash / stopped).
        let backend_died = backend
            .as_mut()
            .is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
        if backend_died {
            // Likely a runtime crash; keep the dev session alive so a fix + save
            // recompiles instead of killing everything.
            *backend = None;
            eprintln!(
                "  {} Backend exited — fix the issue and save to reload",
                "✗".red()
            );
        }
        let frontend_died = frontend
            .as_mut()
            .is_some_and(|c| matches!(c.try_wait(), Ok(Some(_))));
        if frontend_died {
            eprintln!("  {} Frontend exited — stopping", "✗".red());
            break;
        }

        if let Some(dir) = &watch_dir {
            let now = latest_mtime(dir);
            if now > last {
                // Debounce: let the editor finish writing the file(s).
                std::thread::sleep(Duration::from_millis(250));
                last = latest_mtime(dir);
                reload_backend(config, root, backend);
            }
        }

        std::thread::sleep(Duration::from_millis(350));
    }

    if let Some(mut child) = backend.take() {
        kill_child(&mut child);
    }
    if let Some(mut child) = frontend.take() {
        kill_child(&mut child);
    }
    println!("\n  {} Stopped.\n", "✓".green());
}

/// Kill the running backend (so the binary is unlocked), rebuild, and restart it.
fn reload_backend(config: &KarbonConfig, root: &Path, backend: &mut Option<Child>) {
    println!(
        "\n  {} Change detected — recompiling backend...",
        "↻".magenta()
    );
    if let Some(mut child) = backend.take() {
        kill_child(&mut child);
    }

    let ok = Command::new("cargo")
        .args(["build", "-p", &config.backend.package])
        .current_dir(root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !ok {
        eprintln!(
            "  {} Build failed — fix the error and save again",
            "✗".red()
        );
        return;
    }

    match start_backend_binary(config, root) {
        Ok(child) => {
            *backend = Some(child);
            println!("  {} Backend reloaded\n", "✓".green());
        }
        Err(e) => eprintln!("  {} {e}", "✗".red()),
    }
}

/// Most recent modification time under `dir` (recursive).
fn latest_mtime(dir: &Path) -> SystemTime {
    let mut latest = SystemTime::UNIX_EPOCH;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return latest;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let t = latest_mtime(&path);
            if t > latest {
                latest = t;
            }
        } else if let Ok(Ok(m)) = entry.metadata().map(|meta| meta.modified())
            && m > latest
        {
            latest = m;
        }
    }
    latest
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

    // Tell the backend (Studio's terminal) which `karbon` binary to shell out to.
    let mut env = HashMap::new();
    if let Ok(exe) = std::env::current_exe() {
        env.insert("KARBON_BIN".to_string(), exe.to_string_lossy().to_string());
    }

    spawn_command_with_env(binary.to_str().unwrap_or("api"), &[], root, "backend", &env)
}

fn start_frontend(config: &KarbonConfig, root: &Path) -> Result<Child, String> {
    let frontend = config.frontend.as_ref().ok_or("No [frontend] configured")?;
    let frontend_dir = root.join(&frontend.dir);

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
    let parts: Vec<&str> = frontend.dev_cmd.split_whitespace().collect();
    let cmd = parts[0];
    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    spawn_command(cmd, &args, &frontend_dir, "frontend")
}
