use colored::Colorize;
use std::fs;
use std::path::Path;

pub fn run(root: &Path) -> Result<(), String> {
    println!(
        "\n{}  migrate\n",
        "▲ karbon".bold().red(),
    );

    let migration_dir = root.join("migration");
    if !migration_dir.exists() {
        return Err("No migration/ directory found".to_string());
    }

    // Collect .sql files, sorted by name
    let mut files: Vec<_> = fs::read_dir(&migration_dir)
        .map_err(|e| format!("Cannot read migration/: {e}"))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "sql")
        })
        .collect();

    files.sort_by_key(|f| f.file_name());

    if files.is_empty() {
        println!("  {} No migration files found in migration/\n", "⚠".yellow());
        return Ok(());
    }

    // Read .env for DATABASE_URL
    let env_path = root.join(".env");
    let database_url = read_database_url(&env_path)?;

    println!("  {} Found {} migration file(s)", "→".blue(), files.len());
    println!("  {} Connecting to database...\n", "→".blue());

    // Execute migrations via a subprocess calling sqlx
    // We generate a small Rust script that runs the SQL files
    // But simpler: use the database CLI tools directly

    // Detect driver from URL
    let is_postgres = database_url.starts_with("postgres");

    for entry in &files {
        let path = entry.path();
        let filename = path.file_name().unwrap().to_string_lossy();

        let sql = fs::read_to_string(&path)
            .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

        if sql.trim().is_empty() {
            println!("  {} {} (empty, skipped)", "⊘".dimmed(), filename.dimmed());
            continue;
        }

        // Execute via database CLI
        if is_postgres {
            run_psql(&database_url, &sql, &filename)?;
        } else {
            run_mysql(&database_url, &sql, &filename)?;
        }

        println!("  {} {}", "✓".green(), filename);
    }

    println!(
        "\n  {} All migrations applied\n",
        "✓".green().bold()
    );

    Ok(())
}

fn read_database_url(env_path: &Path) -> Result<String, String> {
    // Try DATABASE_URL from env first
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }

    // Try reading from .env
    if env_path.exists() {
        let content = fs::read_to_string(env_path)
            .map_err(|e| format!("Cannot read .env: {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(url) = line.strip_prefix("DATABASE_URL=") {
                return Ok(url.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }

        // Try building from individual vars
        let vars = parse_env_vars(&content);
        if let Some(url) = build_database_url(&vars) {
            return Ok(url);
        }
    }

    Err("DATABASE_URL not found. Set it in .env or as an environment variable".to_string())
}

fn parse_env_vars(content: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            vars.insert(
                key.trim().to_string(),
                val.trim().trim_matches('"').trim_matches('\'').to_string(),
            );
        }
    }
    vars
}

fn build_database_url(vars: &std::collections::HashMap<String, String>) -> Option<String> {
    let driver = vars.get("DB_DRIVER").or(vars.get("DB_CONNECTION"))?;
    let host = vars.get("DB_HOST")?;
    let port = vars.get("DB_PORT")?;
    let name = vars.get("DB_NAME").or(vars.get("DB_DATABASE"))?;
    let user = vars.get("DB_USER").or(vars.get("DB_USERNAME"))?;
    let pass = vars.get("DB_PASSWORD").unwrap_or(&String::new()).clone();

    let scheme = match driver.as_str() {
        "mysql" | "mariadb" => "mysql",
        "postgres" | "postgresql" => "postgres",
        _ => return None,
    };

    Some(format!("{scheme}://{user}:{pass}@{host}:{port}/{name}"))
}

fn run_mysql(url: &str, sql: &str, _filename: &str) -> Result<(), String> {
    // Parse mysql://user:pass@host:port/db
    let url = url.strip_prefix("mysql://").unwrap_or(url);
    let (auth, rest) = url.split_once('@').ok_or("Invalid MySQL URL format")?;
    let (user, pass) = auth.split_once(':').unwrap_or((auth, ""));
    let (host_port, db) = rest.split_once('/').ok_or("Invalid MySQL URL format")?;
    let (host, port) = host_port.split_once(':').unwrap_or((host_port, "3306"));

    let args = vec![
        format!("-h{host}"),
        format!("-P{port}"),
        format!("-u{user}"),
        db.to_string(),
        "-e".to_string(),
        sql.to_string(),
    ];

    let cmd = if cfg!(windows) { "mysql.exe" } else { "mysql" };

    // Pass password via MYSQL_PWD env var instead of CLI args (avoids ps aux exposure)
    let mut command = std::process::Command::new(cmd);
    command.args(&args);
    if !pass.is_empty() {
        command.env("MYSQL_PWD", pass);
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to run mysql CLI: {e}. Is mysql in your PATH?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            return Err(format!("MySQL error: {}", stderr.trim()));
        }
    }

    Ok(())
}

fn run_psql(url: &str, sql: &str, _filename: &str) -> Result<(), String> {
    let cmd = if cfg!(windows) { "psql.exe" } else { "psql" };

    let output = std::process::Command::new(cmd)
        .args([url, "-c", sql])
        .output()
        .map_err(|e| format!("Failed to run psql CLI: {e}. Is psql in your PATH?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            return Err(format!("PostgreSQL error: {}", stderr.trim()));
        }
    }

    Ok(())
}
