use crate::config::KarbonConfig;
use colored::Colorize;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::time::{Instant, Duration};

pub fn run(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  {} — production build\n",
        "▲ karbon".bold().red(),
        config.app.name.bold()
    );

    let total: Instant = Instant::now();

    // ── Step 1: Build frontend ──
    build_frontend(config, root)?;

    // ── Step 2: Build backend ──
    build_backend(config, root)?;

    let elapsed: Duration = total.elapsed();
    println!(
        "\n  {} Build complete in {:.1}s\n",
        "✓".green().bold(),
        elapsed.as_secs_f64()
    );

    Ok(())
}

fn build_frontend(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let frontend_dir = config.frontend_dir(root);

    println!("  {} Building frontend...", "→".blue());
    let start = Instant::now();

    // Ensure node_modules exist
    if !frontend_dir.join("node_modules").exists() {
        println!("    {} Installing dependencies...", "↓".blue());
        run_cmd("npm", &["install"], &frontend_dir)?;
    }

    let parts: Vec<&str> = config.frontend.build_cmd.split_whitespace().collect();
    run_cmd(parts[0], &parts[1..], &frontend_dir)?;

    println!(
        "  {} Frontend built in {:.1}s",
        "✓".green(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn build_backend(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!("  {} Building backend (release)...", "→".blue());
    let start: Instant = Instant::now();

    run_cmd(
        "cargo",
        &["build", "--release", "-p", &config.backend.package],
        root,
    )?;

    println!(
        "  {} Backend built in {:.1}s",
        "✓".green(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn run_cmd(cmd: &str, args: &[&str], dir: &Path) -> Result<(), String> {
    let status: ExitStatus = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("Failed to run `{cmd}`: {e}"))?;

    if !status.success() {
        return Err(format!("`{cmd} {}` failed with exit code {:?}", args.join(" "), status.code()));
    }
    Ok(())
}
