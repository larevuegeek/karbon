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
        "paas" => generate_paas(config, root),
        "fly" => deploy_paas_cli(config, root, PaasCli::Fly),
        "railway" => deploy_paas_cli(config, root, PaasCli::Railway),
        "publish" => deploy_publish(config, root, false),
        "publish:build" | "publish-build" => deploy_publish(config, root, true),
        // Legacy
        "ssh" => deploy_publish(config, root, true),
        _ => Err(format!(
            "Unknown deploy target '{}'. Available: docker, paas, fly, railway, publish, publish:build",
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

    // Security: reject any deploy value that could inject into the shell/ssh commands below.
    validate_deploy_config(deploy, config)?;

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
    let all_packages = config.backend.all_packages();
    let mut binary_paths: Vec<std::path::PathBuf> = Vec::with_capacity(all_packages.len());
    for pkg in &all_packages {
        let p = root.join(format!("target/release/{}", pkg));
        if !p.exists() {
            return Err(format!(
                "Binary not found at {}. Run `karbon build` first.",
                p.display()
            ));
        }
        binary_paths.push(p);
    }

    let frontend_build = config.frontend_dir(root).map(|d| d.join("build"));
    if let Some(ref fb) = frontend_build
        && !fb.exists()
    {
        return Err(format!(
            "Frontend build not found at {}. Run `karbon build` first.",
            fb.display()
        ));
    }

    println!("  {} Publishing to {}", "→".blue(), dest_label.bold());
    let start = Instant::now();

    // ── Step 3: Ensure destination directory exists ──
    run_on_target(deploy, &format!("mkdir -p {}", deploy.path))?;

    // ── Step 4: Sync files ──
    // Backend binaries (main + extras)
    for (pkg, binary_path) in all_packages.iter().zip(binary_paths.iter()) {
        rsync_to_target(
            deploy,
            binary_path.to_str().unwrap(),
            &format!("{}/", deploy.path),
        )?;
        println!("    {} {}", "✓".green(), pkg);
    }

    // Frontend (skipped for backend-only / micro projects)
    if let (Some(frontend_dir), Some(frontend_build)) =
        (config.frontend_dir(root), frontend_build.as_ref())
    {
        run_on_target(deploy, &format!("mkdir -p {}/frontend", deploy.path))?;

        // Frontend build
        rsync_to_target(
            deploy,
            &format!("{}/", frontend_build.to_str().unwrap()),
            &format!("{}/frontend/build/", deploy.path),
        )?;
        println!("    {} frontend/build/", "✓".green());

        // Frontend package.json + lock (needed for npm install on server)
        for name in &["package.json", "package-lock.json"] {
            let file = frontend_dir.join(name);
            if file.exists() {
                rsync_to_target(
                    deploy,
                    file.to_str().unwrap(),
                    &format!("{}/frontend/", deploy.path),
                )?;
            }
        }
        println!("    {} frontend/package*.json", "✓".green());

        // Install production dependencies on server
        println!("\n  {} Installing frontend dependencies...", "→".blue());
        run_on_target(
            deploy,
            &format!(
                "cd {}/frontend && npm install --omit=dev --no-audit --no-fund 2>&1 | tail -1",
                deploy.path
            ),
        )?;
        println!("    {} npm install (production)", "✓".green());
    }

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
        let group = deploy.group.as_deref().unwrap_or(user);
        println!(
            "\n  {} Setting ownership to {}:{}...",
            "→".blue(),
            user,
            group
        );
        run_on_target(deploy, &format!("chown -R {user}:{group} {}", deploy.path))?;
        println!("    {} chown -R {}:{}", "✓".green(), user, group);
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
        let user_home = String::from_utf8_lossy(&home_output.stdout)
            .trim()
            .to_string();
        let pm2_home = format!(
            "{}/.pm2",
            if user_home.is_empty() {
                format!("/home/{user}")
            } else {
                user_home
            }
        );

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
            run_as_user(
                deploy,
                &format!(
                    "cd {path} && pm2 stop {pm2_config} 2>/dev/null; pm2 delete {pm2_config} 2>/dev/null; true"
                ),
            )?;

            // Start fresh
            run_as_user(
                deploy,
                &format!("cd {path} && pm2 start {pm2_config} && pm2 save"),
            )?;

            Ok(())
        }
        "systemd" => {
            let service = deploy.service.as_deref().unwrap_or(&config.app.name);
            run_on_target(deploy, &format!("sudo systemctl restart {service}"))
        }
        other => Err(format!("Unknown manager: {other}. Use 'pm2' or 'systemd'.")),
    }
}

// ─────────────────────────────────────────────────────────────────
// Docker (unchanged)
// ─────────────────────────────────────────────────────────────────

/// Allowlist-based validation for any value interpolated into a shell/ssh command.
///
/// Only `[A-Za-z0-9._/@:+=-]` are permitted, no leading `-` (blocks ssh/rsync option
/// injection like `-oProxyCommand=…`), no `..` (path tricks), and non-empty. Because
/// spaces, quotes, `;`, `&`, `|`, `$`, backticks, redirections and glob chars are all
/// rejected, a validated value cannot break out of the surrounding command.
fn validate_shell_safe(s: &str, field: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if s.starts_with('-') {
        return Err(format!(
            "{field} must not start with '-' (option injection): {s}"
        ));
    }
    if s.contains("..") {
        return Err(format!("{field} must not contain '..': {s}"));
    }
    if let Some(bad) = s.chars().find(|c| {
        !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | ':' | '+' | '=' | '-'))
    }) {
        return Err(format!("{field} contains an unsafe character '{bad}': {s}"));
    }
    Ok(())
}

/// Hardened denylist for values that legitimately contain spaces (a serve command, a
/// directory) but are written into a generated Dockerfile/script. Rejects shell
/// metacharacters and control chars while allowing spaces and normal path/command text.
fn validate_no_shell_meta(s: &str, field: &str) -> Result<(), String> {
    const BAD: &[char] = &[
        ';', '&', '|', '`', '$', '\n', '\r', '\t', '<', '>', '(', ')', '{', '}', '!', '\\', '"',
        '\'', '*', '?',
    ];
    if let Some(bad) = s.chars().find(|c| BAD.contains(c)) {
        return Err(format!("{field} contains an unsafe character '{bad}': {s}"));
    }
    if s.contains("..") {
        return Err(format!("{field} must not contain '..': {s}"));
    }
    Ok(())
}

/// Validate every `karbon.toml` value that reaches a shell/ssh command in the publish path.
fn validate_deploy_config(
    deploy: &crate::config::DeployConfig,
    config: &KarbonConfig,
) -> Result<(), String> {
    validate_shell_safe(&deploy.path, "deploy.path")?;
    validate_shell_safe(&deploy.pm2_config, "deploy.pm2_config")?;
    if let Some(host) = &deploy.host {
        validate_shell_safe(host, "deploy.host")?;
    }
    if let Some(user) = &deploy.user {
        validate_shell_safe(user, "deploy.user")?;
    }
    if let Some(group) = &deploy.group {
        validate_shell_safe(group, "deploy.group")?;
    }
    if let Some(service) = &deploy.service {
        validate_shell_safe(service, "deploy.service")?;
    }
    validate_shell_safe(&config.app.name, "app.name")?;
    Ok(())
}

fn generate_dockerfile(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let package = &config.backend.package;
    let port = config.backend.port;
    validate_shell_safe(package, "backend.package")?;

    // Backend-only ("micro") projects get a lean Node-free Dockerfile.
    let Some(frontend) = config.frontend.as_ref() else {
        return generate_dockerfile_backend_only(config, root);
    };
    let frontend_dir = &frontend.dir;
    let serve_cmd = &frontend.serve_cmd;

    validate_no_shell_meta(frontend_dir, "frontend.dir")?;
    validate_no_shell_meta(serve_cmd, "frontend.serve_cmd")?;

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
ENV APP_ENV=production
EXPOSE {port}

HEALTHCHECK --interval=30s --timeout=3s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:{port}/health || exit 1

CMD ["/app/start.sh"]
"#
    );

    let path = root.join("Dockerfile");
    fs::write(&path, dockerfile).map_err(|e| format!("Cannot write Dockerfile: {e}"))?;

    println!("  {} Dockerfile generated", "✓".green());

    let dockerignore = r#"target/
**/node_modules/
.git/
.gitignore
.env
.env.*
!.env.example
*.log
Dockerfile
.dockerignore
frontend/build/
frontend/.svelte-kit/
frontend/.next/
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
        "     docker run -p {port}:{port} --env-file .env {}",
        config.app.name
    );
    println!("\n  {}  Multi-arch (amd64 + arm64) via buildx:", "→".blue());
    println!(
        "     docker buildx build --platform linux/amd64,linux/arm64 -t {} --push .\n",
        config.app.name
    );

    Ok(())
}

/// Lean Dockerfile for backend-only ("micro") projects — no Node, no frontend.
fn generate_dockerfile_backend_only(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let package = &config.backend.package;
    let port = config.backend.port;

    let dockerfile = format!(
        r#"# ──────────────────────────────────────────────
# Karbon — backend-only (micro) Dockerfile
# Generated by `karbon deploy docker`
# ──────────────────────────────────────────────

# Stage 1: Build backend
FROM rust:1.85-slim AS backend-builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY app/ app/
RUN cargo build --release -p {package}

# Stage 2: Runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app

COPY --from=backend-builder /app/target/release/{package} /app/{package}
COPY migration/ /app/migration/
COPY .env.example /app/.env.example

ENV RUST_LOG=info
ENV APP_ENV=production
EXPOSE {port}

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:{port}/health || exit 1

CMD ["/app/{package}"]
"#
    );

    let path = root.join("Dockerfile");
    fs::write(&path, dockerfile).map_err(|e| format!("Cannot write Dockerfile: {e}"))?;
    println!("  {} Dockerfile generated (backend-only)", "✓".green());

    let dockerignore = "target/\n.git/\n.gitignore\n.env\n.env.*\n!.env.example\n*.log\nDockerfile\n.dockerignore\n";
    let ignore_path = root.join(".dockerignore");
    if !ignore_path.exists() {
        fs::write(&ignore_path, dockerignore)
            .map_err(|e| format!("Cannot write .dockerignore: {e}"))?;
        println!("  {} .dockerignore generated", "✓".green());
    }

    println!("\n  {}  Build and run with:", "→".blue());
    println!("     docker build -t {} .", config.app.name);
    println!(
        "     docker run -p {port}:{port} --env-file .env {}",
        config.app.name
    );
    println!("\n  {}  Multi-arch (amd64 + arm64) via buildx:", "→".blue());
    println!(
        "     docker buildx build --platform linux/amd64,linux/arm64 -t {} --push .\n",
        config.app.name
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// PaaS — generate platform config files (Fly.io, Render, Procfile)
// ─────────────────────────────────────────────────────────────────

fn generate_paas(config: &KarbonConfig, root: &Path) -> Result<(), String> {
    let name = &config.app.name;
    let port = config.backend.port;

    validate_shell_safe(name, "app.name")?;

    // Ensure a Dockerfile exists — every target here builds from it.
    if !root.join("Dockerfile").exists() {
        generate_dockerfile(config, root)?;
        println!();
    }

    let mut wrote = Vec::new();

    // Fly.io
    let fly = format!(
        r#"# Fly.io config — generated by `karbon deploy paas`
# Deploy with: fly launch --copy-config --no-deploy  (first time) then `fly deploy`
app = "{name}"
primary_region = "cdg"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = {port}
  force_https = true
  auto_stop_machines = "stop"
  auto_start_machines = true
  min_machines_running = 0

[[http_service.checks]]
  method = "get"
  path = "/health"
  interval = "30s"
  timeout = "3s"

[env]
  APP_ENV = "production"
  PORT = "{port}"
"#
    );
    write_if_absent(root, "fly.toml", &fly, &mut wrote)?;

    // Render
    let render = format!(
        r#"# Render config — generated by `karbon deploy paas`
# https://render.com/docs/blueprint-spec
services:
  - type: web
    name: {name}
    runtime: docker
    dockerfilePath: ./Dockerfile
    healthCheckPath: /health
    envVars:
      - key: APP_ENV
        value: production
      - key: PORT
        value: "{port}"
"#
    );
    write_if_absent(root, "render.yaml", &render, &mut wrote)?;

    // Generic Procfile (Heroku-style PaaS)
    let procfile = "web: /app/start.sh\n";
    write_if_absent(root, "Procfile", procfile, &mut wrote)?;

    if wrote.is_empty() {
        println!(
            "  {} PaaS config already present (nothing overwritten)",
            "✓".green()
        );
    } else {
        for f in &wrote {
            println!("  {} {}", "✓".green(), f);
        }
    }

    println!("\n  {}  Next steps:", "→".blue());
    println!("     Fly.io   → fly launch --copy-config --no-deploy   then  fly deploy");
    println!("     Render   → push to a repo connected to a Render Blueprint");
    println!("     Railway  → railway up  (uses the Dockerfile)\n");

    Ok(())
}

/// Write a file only if it does not already exist; record what was written.
fn write_if_absent(
    root: &Path,
    name: &str,
    content: &str,
    wrote: &mut Vec<String>,
) -> Result<(), String> {
    let path = root.join(name);
    if path.exists() {
        return Ok(());
    }
    fs::write(&path, content).map_err(|e| format!("Cannot write {name}: {e}"))?;
    wrote.push(name.to_string());
    Ok(())
}

#[derive(Clone, Copy)]
enum PaasCli {
    Fly,
    Railway,
}

impl PaasCli {
    fn bin(self) -> &'static str {
        match self {
            PaasCli::Fly => "flyctl",
            PaasCli::Railway => "railway",
        }
    }
    fn label(self) -> &'static str {
        match self {
            PaasCli::Fly => "Fly.io",
            PaasCli::Railway => "Railway",
        }
    }
    /// Arguments passed to the CLI to deploy.
    fn deploy_args(self) -> &'static [&'static str] {
        match self {
            PaasCli::Fly => &["deploy"],
            PaasCli::Railway => &["up"],
        }
    }
    fn install_hint(self) -> &'static str {
        match self {
            PaasCli::Fly => "https://fly.io/docs/flyctl/install/",
            PaasCli::Railway => "https://docs.railway.app/guides/cli",
        }
    }
}

/// Convenience wrapper: ensure PaaS config exists, then shell out to the
/// platform CLI (`flyctl deploy` / `railway up`).
fn deploy_paas_cli(config: &KarbonConfig, root: &Path, cli: PaasCli) -> Result<(), String> {
    // Make sure the platform config + Dockerfile are present.
    let config_file = match cli {
        PaasCli::Fly => "fly.toml",
        PaasCli::Railway => "render.yaml", // Railway uses the Dockerfile; render.yaml is harmless
    };
    if !root.join(config_file).exists() || !root.join("Dockerfile").exists() {
        generate_paas(config, root)?;
        println!();
    }

    // `flyctl` is sometimes installed as `fly`; try both for Fly.
    let candidates: &[&str] = match cli {
        PaasCli::Fly => &["flyctl", "fly"],
        PaasCli::Railway => &["railway"],
    };
    let bin = candidates
        .iter()
        .copied()
        .find(|c| which(c))
        .ok_or_else(|| {
            format!(
                "{} CLI not found ({}). Install it: {}",
                cli.label(),
                cli.bin(),
                cli.install_hint()
            )
        })?;

    println!(
        "  {} Deploying to {} via `{} {}`...\n",
        "→".blue(),
        cli.label().bold(),
        bin,
        cli.deploy_args().join(" ")
    );

    let status = Command::new(bin)
        .args(cli.deploy_args())
        .current_dir(root)
        .status()
        .map_err(|e| format!("Failed to run {bin}: {e}"))?;

    if !status.success() {
        return Err(format!("{} deploy failed", cli.label()));
    }

    println!("\n  {} Deployed to {}\n", "✓".green().bold(), cli.label());
    Ok(())
}

/// Whether a command is available on PATH.
fn which(cmd: &str) -> bool {
    let probe = if cfg!(windows) { "where" } else { "which" };
    Command::new(probe)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
