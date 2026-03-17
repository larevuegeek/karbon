use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use crate::config::KarbonConfig;

pub fn run(config: &KarbonConfig, root: &Path, target: &str) -> Result<(), String> {
    println!(
        "\n{}  deploy {}\n",
        "▲ karbon".bold().red(),
        target.bold().cyan()
    );

    match target {
        "docker" | "dockerfile" => generate_dockerfile(config, root),
        "publish" => deploy_publish(config, root, false),
        "publish:build" | "publish-build" => deploy_publish(config, root, true),
        // Legacy
        "ssh" => deploy_publish(config, root, true),
        _ => Err(format!(
            "Unknown deploy target '{}'. Available: docker, publish, publish:build",
            target
        )),
    }
}

// ─────────────────────────────────────────────────────────────────
// Publish (local or SSH)
// ─────────────────────────────────────────────────────────────────

fn deploy_publish(config: &KarbonConfig, root: &Path, build_first: bool) -> Result<(), String> {
    let deploy = config.deploy.as_ref().ok_or(
        "[deploy] section missing in karbon.toml. Example:\n\n\
         [deploy]\n\
         path = \"/var/www/my-app\"\n\
         manager = \"pm2\"\n\
         # host = \"user@server\"  # optional, for remote deploy"
            .to_string(),
    )?;

    let is_remote = deploy.host.is_some();
    let dest_label = if let Some(ref host) = deploy.host {
        format!("{}:{}", host, deploy.path)
    } else {
        deploy.path.clone()
    };

    // ── Step 1: Build (optional) ──
    if build_first {
        println!("  {} Building for production...\n", "→".blue());
        crate::commands::build::run(config, root)?;
        println!();
    }

    // ── Step 2: Verify artifacts exist ──
    let binary_path = root.join(format!("target/release/{}", config.backend.package));
    if !binary_path.exists() {
        return Err(format!(
            "Binary not found at {}. Run `karbon build` first.",
            binary_path.display()
        ));
    }

    let frontend_build = config.frontend_dir(root).join("build");
    if !frontend_build.exists() {
        return Err(format!(
            "Frontend build not found at {}. Run `karbon build` first.",
            frontend_build.display()
        ));
    }

    println!("  {} Publishing to {}", "→".blue(), dest_label.bold());
    let start = Instant::now();

    // ── Step 3: Ensure destination directory exists ──
    run_on_target(deploy, &format!("mkdir -p {}/frontend", deploy.path))?;

    // ── Step 4: Sync files ──
    // Binary
    rsync_to_target(
        deploy,
        binary_path.to_str().unwrap(),
        &format!("{}/", deploy.path),
    )?;
    println!("    {} {}", "✓".green(), config.backend.package);

    // Frontend build
    rsync_to_target(
        deploy,
        &format!("{}/", frontend_build.to_str().unwrap()),
        &format!("{}/frontend/build/", deploy.path),
    )?;
    println!("    {} frontend/build/", "✓".green());

    // PM2 config (if exists)
    let pm2_path = root.join(&deploy.pm2_config);
    if pm2_path.exists() {
        rsync_to_target(
            deploy,
            pm2_path.to_str().unwrap(),
            &format!("{}/", deploy.path),
        )?;
        println!("    {} {}", "✓".green(), deploy.pm2_config);
    }

    // Migrations (if exist)
    let migration_dir = root.join("migration");
    if migration_dir.exists() {
        rsync_to_target(
            deploy,
            &format!("{}/", migration_dir.to_str().unwrap()),
            &format!("{}/migration/", deploy.path),
        )?;
        println!("    {} migration/", "✓".green());
    }

    // .env.example (if exists, as reference — never overwrite .env)
    let env_example = root.join(".env.example");
    if env_example.exists() {
        rsync_to_target(
            deploy,
            env_example.to_str().unwrap(),
            &format!("{}/", deploy.path),
        )?;
        println!("    {} .env.example", "✓".green());
    }

    // ── Step 5: Set ownership ──
    if let Some(ref user) = deploy.user {
        println!("\n  {} Setting ownership to {}...", "→".blue(), user);
        run_on_target(deploy, &format!("chown -R {user}:{user} {}", deploy.path))?;
        println!("    {} chown -R {}:{}", "✓".green(), user, user);
    }

    // ── Step 6: Restart process manager ──
    println!("\n  {} Restarting {}...", "→".blue(), deploy.manager);
    restart_manager(deploy, config)?;

    println!(
        "\n  {} Published to {} in {:.1}s\n",
        "✓".green().bold(),
        dest_label.bold(),
        start.elapsed().as_secs_f64()
    );

    Ok(())
}

/// Run rsync — local if no host, SSH if host is set
fn rsync_to_target(
    deploy: &crate::config::DeployConfig,
    src: &str,
    dest: &str,
) -> Result<(), String> {
    let mut args = vec!["-a", "--delete", src];

    let full_dest;
    if let Some(ref host) = deploy.host {
        full_dest = format!("{host}:{dest}");
        args.push(&full_dest);
    } else {
        args.push(dest);
    }

    let status = Command::new("rsync")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to run rsync: {e}"))?;

    if !status.success() {
        return Err(format!("rsync failed for {src}"));
    }

    Ok(())
}

/// Run a command on the target (local or SSH)
fn run_on_target(deploy: &crate::config::DeployConfig, cmd: &str) -> Result<(), String> {
    let status = if let Some(ref host) = deploy.host {
        Command::new("ssh")
            .args([host.as_str(), cmd])
            .status()
            .map_err(|e| format!("SSH failed: {e}"))?
    } else {
        Command::new("sh")
            .args(["-c", cmd])
            .status()
            .map_err(|e| format!("Command failed: {e}"))?
    };

    if !status.success() {
        return Err(format!("Command failed: {cmd}"));
    }

    Ok(())
}

/// Run a command as a specific user or as current user.
/// For PM2: forces PM2_HOME to the user's home so it connects to the right daemon.
fn run_as_user(deploy: &crate::config::DeployConfig, cmd: &str) -> Result<(), String> {
    let status = if let Some(ref user) = deploy.user {
        // Get the user's home dir from /etc/passwd
        let home_output = Command::new("sh")
            .args(["-c", &format!("getent passwd {user} | cut -d: -f6")])
            .output()
            .map_err(|e| format!("Failed to resolve home for {user}: {e}"))?;
        let user_home = String::from_utf8_lossy(&home_output.stdout).trim().to_string();
        let pm2_home = format!("{}/.pm2", if user_home.is_empty() { format!("/home/{user}") } else { user_home });

        // PM2_HOME forces PM2 to use the user's daemon, not root's
        let full_cmd = format!("PM2_HOME={pm2_home} {cmd}");
        Command::new("sudo")
            .args(["-H", "-u", user, "bash", "-lc", &full_cmd])
            .status()
            .map_err(|e| format!("sudo failed: {e}"))?
    } else if let Some(ref host) = deploy.host {
        Command::new("ssh")
            .args([host.as_str(), cmd])
            .status()
            .map_err(|e| format!("SSH failed: {e}"))?
    } else {
        Command::new("sh")
            .args(["-c", cmd])
            .status()
            .map_err(|e| format!("Command failed: {e}"))?
    };

    if !status.success() {
        return Err(format!("Command failed: {cmd}"));
    }

    Ok(())
}

/// Restart the process manager (runs as deploy.user if set)
fn restart_manager(
    deploy: &crate::config::DeployConfig,
    config: &KarbonConfig,
) -> Result<(), String> {
    match deploy.manager.as_str() {
        "pm2" => {
            let path = &deploy.path;
            let pm2_config = &deploy.pm2_config;

            // Stop existing processes (ignore errors if none running)
            run_as_user(deploy, &format!(
                "cd {path} && pm2 stop {pm2_config} 2>/dev/null; pm2 delete {pm2_config} 2>/dev/null; true"
            ))?;

            // Start fresh
            run_as_user(deploy, &format!(
                "cd {path} && pm2 start {pm2_config} && pm2 save"
            ))?;

            Ok(())
        }
        "systemd" => {
            let service = deploy
                .service
                .as_deref()
                .unwrap_or(&config.app.name);
            run_on_target(deploy, &format!("sudo systemctl restart {service}"))
        }
        other => Err(format!("Unknown manager: {other}. Use 'pm2' or 'systemd'.")),
    }
}

// ─────────────────────────────────────────────────────────────────
// Docker (unchanged)
// ─────────────────────────────────────────────────────────────────

fn validate_shell_safe(s: &str, field: &str) -> Result<(), String> {
    if s.contains(';')
        || s.contains('&')
        || s.contains('|')
        || s.contains('`')
        || s.contains('$')
        || s.contains('\n')
        || s.contains("..")
    {
        return Err(format!("{field} contains unsafe characters: {s}"));
    }
    Ok(())
}

fn generate_dockerfile(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let package = &config.backend.package;
    let frontend_dir = &config.frontend.dir;
    let serve_cmd = &config.frontend.serve_cmd;
    let port = config.backend.port;

    validate_shell_safe(package, "backend.package")?;
    validate_shell_safe(frontend_dir, "frontend.dir")?;
    validate_shell_safe(serve_cmd, "frontend.serve_cmd")?;

    let dockerfile = format!(
        r#"# ──────────────────────────────────────────────
# Karbon — Multi-stage production Dockerfile
# Generated by `karbon deploy docker`
# ──────────────────────────────────────────────

# Stage 1: Build frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/{frontend_dir}
COPY {frontend_dir}/package*.json ./
RUN npm ci
COPY {frontend_dir}/ ./
RUN npm run build

# Stage 2: Build backend
FROM rust:1.85-slim AS backend-builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY app/ app/
RUN cargo build --release -p {package}

# Stage 3: Runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates curl nodejs npm && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=backend-builder /app/target/release/{package} /app/{package}
COPY --from=frontend-builder /app/{frontend_dir}/build /app/{frontend_dir}/build
COPY --from=frontend-builder /app/{frontend_dir}/package*.json /app/{frontend_dir}/
WORKDIR /app/{frontend_dir}
RUN npm ci --omit=dev
WORKDIR /app

COPY migration/ /app/migration/
COPY .env.example /app/.env.example

RUN echo '#!/bin/sh' > /app/start.sh && \
    echo 'cd /app/{frontend_dir} && PORT=3004 {serve_cmd} &' >> /app/start.sh && \
    echo 'FRONTEND_PID=$!' >> /app/start.sh && \
    echo 'sleep 1' >> /app/start.sh && \
    echo 'KARBON_FRONTEND_URL=http://127.0.0.1:3004 /app/{package}' >> /app/start.sh && \
    echo 'kill $FRONTEND_PID 2>/dev/null' >> /app/start.sh && \
    chmod +x /app/start.sh

ENV RUST_LOG=info
EXPOSE {port}

CMD ["/app/start.sh"]
"#
    );

    let path = root.join("Dockerfile");
    fs::write(&path, dockerfile).map_err(|e| format!("Cannot write Dockerfile: {e}"))?;

    println!("  {} Dockerfile generated", "✓".green());

    let dockerignore = r#"target/
node_modules/
.git/
.env
*.log
"#;

    let ignore_path = root.join(".dockerignore");
    if !ignore_path.exists() {
        fs::write(&ignore_path, dockerignore)
            .map_err(|e| format!("Cannot write .dockerignore: {e}"))?;
        println!("  {} .dockerignore generated", "✓".green());
    }

    println!("\n  {}  Build and run with:", "→".blue());
    println!("     docker build -t {} .", config.app.name);
    println!(
        "     docker run -p {port}:{port} --env-file .env {}\n",
        config.app.name
    );

    Ok(())
}