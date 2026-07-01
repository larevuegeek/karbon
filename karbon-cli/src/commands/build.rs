use crate::config::KarbonConfig;
use colored::Colorize;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub fn run(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  {} — production build\n",
        "▲ karbon".bold().red(),
        config.app.name.bold()
    );

    let total: Instant = Instant::now();

    // ── Step 1: Build frontend (skipped for backend-only / micro projects) ──
    if config.frontend.is_some() {
        build_frontend(config, root)?;
    }

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
    let frontend = config.frontend.as_ref().ok_or("No [frontend] configured")?;
    let frontend_dir = root.join(&frontend.dir);

    println!("  {} Building frontend...", "→".blue());
    let start = Instant::now();

    // Ensure node_modules exist
    if !frontend_dir.join("node_modules").exists() {
        println!("    {} Installing dependencies...", "↓".blue());
        run_cmd("npm", &["install"], &frontend_dir)?;
    }

    let parts: Vec<&str> = frontend.build_cmd.split_whitespace().collect();
    run_cmd(parts[0], &parts[1..], &frontend_dir)?;

    println!(
        "  {} Frontend built in {:.1}s",
        "✓".green(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn build_backend(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let packages = config.backend.all_packages();

    for pkg in packages {
        println!("  {} Building {} (release)...", "→".blue(), pkg);
        let start: Instant = Instant::now();

        run_cmd("cargo", &["build", "--release", "-p", pkg], root)?;

        println!(
            "  {} {} built in {:.1}s",
            "✓".green(),
            pkg,
            start.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

fn run_cmd(cmd: &str, args: &[&str], dir: &Path) -> Result<(), String> {
    let status: ExitStatus = Command::new(resolve_cmd(cmd))
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("Failed to run `{cmd}`: {e}"))?;

    if !status.success() {
        return Err(format!(
            "`{cmd} {}` failed with exit code {:?}",
            args.join(" "),
            status.code()
        ));
    }
    Ok(())
}

/// On Windows, resolve npm/npx to their .cmd wrappers
fn resolve_cmd(cmd: &str) -> String {
    #[cfg(windows)]
    {
        if matches!(cmd, "npm" | "npx" | "pnpm" | "yarn") {
            return format!("{cmd}.cmd");
        }
    }
    cmd.to_string()
}
