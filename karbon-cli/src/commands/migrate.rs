use colored::Colorize;
use sqlx::any::{AnyPoolOptions, install_default_drivers};
use sqlx::{AnyPool, Row};
use std::fs;
use std::path::{Path, PathBuf};

const MIGRATIONS_TABLE: &str = "_karbon_migrations";

/// Entry point — `action` is one of: "up" (default), "status", "rollback", "diff".
pub fn run(root: &Path, action: &str, name: Option<&str>) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to start async runtime: {e}"))?;
    rt.block_on(run_async(root, action, name))
}

async fn run_async(root: &Path, action: &str, name: Option<&str>) -> Result<(), String> {
    println!(
        "\n{}  migrate {}\n",
        "▲ karbon".bold().red(),
        action.dimmed()
    );

    let migration_dir = root.join("migration");
    if action != "diff" && !migration_dir.exists() {
        return Err("No migration/ directory found".to_string());
    }

    let database_url = read_database_url(&root.join(".env"))?;

    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .map_err(|e| format!("Cannot connect to database: {e}"))?;

    ensure_migrations_table(&pool).await?;

    match action {
        "up" | "" => migrate_up(&pool, &migration_dir).await,
        "status" => migrate_status(&pool, &migration_dir).await,
        "rollback" | "down" => migrate_rollback(&pool, &migration_dir).await,
        "diff" => diff::run(&pool, &migration_dir, root, &database_url, name).await,
        other => Err(format!(
            "Unknown migrate action '{other}'. Available: up, status, rollback, diff"
        )),
    }
}

/// `migrate diff` — compare the entities (source of truth) against the live
/// database schema and generate a migration for the difference. Additive only:
/// new tables → CREATE, new columns → ALTER ADD COLUMN. It never drops or alters
/// existing tables/columns (safe by default; review the file before applying).
mod diff {
    use colored::Colorize;
    use sqlx::{AnyPool, Row};
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::Path;

    #[derive(Clone, Copy, PartialEq)]
    enum Drv {
        Mysql,
        Postgres,
        Sqlite,
    }

    struct Column {
        name: String,
        sql_type: String,
        nullable: bool,
    }
    struct TableDef {
        name: String,
        columns: Vec<Column>,
    }

    pub async fn run(
        pool: &AnyPool,
        migration_dir: &Path,
        root: &Path,
        url: &str,
        name: Option<&str>,
    ) -> Result<(), String> {
        let driver = driver_from_url(url);
        let entity_dir = ["app/src/entity", "api/src/entity"]
            .iter()
            .map(|d| root.join(d))
            .find(|p| p.exists())
            .ok_or("Aucun dossier entity/ trouvé")?;
        let tables = parse_entities(&entity_dir, driver)?;
        if tables.is_empty() {
            return Err("Aucune entité trouvée dans entity/".to_string());
        }
        let current = introspect(pool, driver).await?;

        let q = if driver == Drv::Mysql { '`' } else { '"' };
        let mut up = String::new();
        let mut down = String::new();
        let mut changes = 0u32;

        for t in &tables {
            match current.get(&t.name.to_lowercase()) {
                None => {
                    up.push_str(&create_table_sql(&t.name, &t.columns, driver));
                    down.push_str(&format!("DROP TABLE IF EXISTS {q}{}{q};\n", t.name));
                    changes += 1;
                    println!("  {} table {}", "+".green(), t.name.bold());
                }
                Some(cols) => {
                    for c in &t.columns {
                        if !cols.contains(&c.name.to_lowercase()) {
                            let nn = if c.nullable { "" } else { " NOT NULL" };
                            up.push_str(&format!(
                                "ALTER TABLE {q}{}{q} ADD COLUMN {q}{}{q} {}{nn};\n",
                                t.name, c.name, c.sql_type
                            ));
                            down.push_str(&format!(
                                "ALTER TABLE {q}{}{q} DROP COLUMN {q}{}{q};\n",
                                t.name, c.name
                            ));
                            changes += 1;
                            println!("  {} {}.{}", "+".green(), t.name, c.name.bold());
                        }
                    }
                }
            }
        }

        if changes == 0 {
            println!(
                "\n  {} Schéma à jour — aucune migration générée\n",
                "✓".green()
            );
            return Ok(());
        }

        fs::create_dir_all(migration_dir).ok();
        let next = next_number(migration_dir);
        let fname = format!("{next:04}_{}.sql", slugify(name.unwrap_or("diff")));
        let content = format!("-- migrate:up\n{up}\n-- migrate:down\n{down}");
        fs::write(migration_dir.join(&fname), content)
            .map_err(|e| format!("écriture {fname}: {e}"))?;

        println!(
            "\n  {} {changes} changement(s) → {}",
            "✓".green().bold(),
            format!("migration/{fname}").cyan()
        );
        println!(
            "  {} relis le fichier puis applique avec {}\n",
            "→".blue(),
            "karbon migrate".bold()
        );
        Ok(())
    }

    fn driver_from_url(url: &str) -> Drv {
        if url.starts_with("postgres") {
            Drv::Postgres
        } else if url.starts_with("sqlite") {
            Drv::Sqlite
        } else {
            Drv::Mysql
        }
    }

    fn parse_entities(dir: &Path, driver: Drv) -> Result<Vec<TableDef>, String> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).map_err(|e| format!("lecture entity/: {e}"))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_none_or(|x| x != "rs") {
                continue;
            }
            if path.file_name().is_some_and(|n| n == "mod.rs") {
                continue;
            }
            let content = fs::read_to_string(&path).unwrap_or_default();
            if let Some(t) = parse_entity(&content, driver) {
                out.push(t);
            }
        }
        Ok(out)
    }

    fn parse_entity(content: &str, driver: Drv) -> Option<TableDef> {
        let (entity_name, fields) = find_entity_struct(content)?;
        let name = table_name(content, &entity_name);
        let columns = fields
            .iter()
            .filter(|(n, _)| {
                !matches!(
                    n.as_str(),
                    "id" | "created" | "updated" | "created_at" | "updated_at"
                )
            })
            .map(|(n, ty)| {
                let (sql_type, nullable) = rust_to_sql(ty, driver);
                Column {
                    name: n.clone(),
                    sql_type,
                    nullable,
                }
            })
            .collect();
        Some(TableDef { name, columns })
    }

    /// The entity struct = the first `pub struct` whose fields include `id`.
    fn find_entity_struct(content: &str) -> Option<(String, Vec<(String, String)>)> {
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if let Some(rest) = lines[i].trim().strip_prefix("pub struct ") {
                let sname = rest.split([' ', '{', '<']).next().unwrap_or("").to_string();
                let mut fields = Vec::new();
                let mut j = i + 1;
                while j < lines.len() && lines[j].trim() != "}" {
                    if let Some(f) = lines[j].trim().strip_prefix("pub ")
                        && let Some((n, t)) = f.split_once(':')
                    {
                        fields.push((
                            n.trim().to_string(),
                            t.trim().trim_end_matches(',').to_string(),
                        ));
                    }
                    j += 1;
                }
                if fields.iter().any(|(n, _)| n == "id") {
                    return Some((sname, fields));
                }
                i = j;
            }
            i += 1;
        }
        None
    }

    fn table_name(content: &str, entity: &str) -> String {
        let marker = "#[table_name(\"";
        if let Some(i) = content.find(marker) {
            let rest = &content[i + marker.len()..];
            if let Some(j) = rest.find('"') {
                return rest[..j].to_string();
            }
        }
        plural(&snake(entity))
    }

    fn rust_to_sql(rust_ty: &str, driver: Drv) -> (String, bool) {
        let nullable = rust_ty.starts_with("Option<");
        let inner = rust_ty
            .trim_start_matches("Option<")
            .trim_end_matches('>')
            .trim();
        let sql = match inner {
            "String" => {
                if driver == Drv::Sqlite {
                    "TEXT"
                } else {
                    "VARCHAR(255)"
                }
            }
            "i32" => {
                if driver == Drv::Mysql {
                    "INT"
                } else {
                    "INTEGER"
                }
            }
            "i64" => {
                if driver == Drv::Sqlite {
                    "INTEGER"
                } else {
                    "BIGINT"
                }
            }
            "f64" | "f32" => match driver {
                Drv::Mysql => "DOUBLE",
                Drv::Postgres => "DOUBLE PRECISION",
                Drv::Sqlite => "REAL",
            },
            "bool" => {
                if driver == Drv::Sqlite {
                    "INTEGER"
                } else {
                    "BOOLEAN"
                }
            }
            "chrono::NaiveDateTime" | "NaiveDateTime" => {
                if driver == Drv::Postgres {
                    "TIMESTAMP"
                } else {
                    "DATETIME"
                }
            }
            "chrono::NaiveDate" | "NaiveDate" => "DATE",
            "serde_json::Value" | "Value" => match driver {
                Drv::Mysql => "JSON",
                Drv::Postgres => "JSONB",
                Drv::Sqlite => "TEXT",
            },
            _ => "TEXT",
        };
        (sql.to_string(), nullable)
    }

    fn create_table_sql(table: &str, cols: &[Column], driver: Drv) -> String {
        let q = if driver == Drv::Mysql { '`' } else { '"' };
        let mut lines = vec![match driver {
            Drv::Mysql => format!("    {q}id{q} BIGINT AUTO_INCREMENT PRIMARY KEY"),
            Drv::Postgres => format!("    {q}id{q} BIGSERIAL PRIMARY KEY"),
            Drv::Sqlite => format!("    {q}id{q} INTEGER PRIMARY KEY AUTOINCREMENT"),
        }];
        for c in cols {
            let nn = if c.nullable { "" } else { " NOT NULL" };
            let uniq = if c.name == "slug" { " UNIQUE" } else { "" };
            lines.push(format!("    {q}{}{q} {}{nn}{uniq}", c.name, c.sql_type));
        }
        let ts = if driver == Drv::Postgres {
            "TIMESTAMP"
        } else {
            "DATETIME"
        };
        lines.push(format!(
            "    {q}created{q} {ts} NOT NULL DEFAULT CURRENT_TIMESTAMP"
        ));
        if driver == Drv::Mysql {
            lines.push(format!(
                "    {q}updated{q} DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP"
            ));
        } else {
            lines.push(format!(
                "    {q}updated{q} {ts} NOT NULL DEFAULT CURRENT_TIMESTAMP"
            ));
        }
        let suffix = if driver == Drv::Mysql {
            " ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"
        } else {
            ""
        };
        format!(
            "CREATE TABLE IF NOT EXISTS {q}{table}{q} (\n{}\n){suffix};\n",
            lines.join(",\n")
        )
    }

    async fn introspect(
        pool: &AnyPool,
        driver: Drv,
    ) -> Result<HashMap<String, HashSet<String>>, String> {
        let mut map: HashMap<String, HashSet<String>> = HashMap::new();
        match driver {
            Drv::Mysql | Drv::Postgres => {
                let sql = if driver == Drv::Mysql {
                    "SELECT TABLE_NAME, COLUMN_NAME FROM information_schema.columns WHERE TABLE_SCHEMA = DATABASE()"
                } else {
                    "SELECT table_name, column_name FROM information_schema.columns WHERE table_schema = 'public'"
                };
                let rows: Vec<(String, String)> = sqlx::query_as(sql)
                    .fetch_all(pool)
                    .await
                    .map_err(|e| format!("introspection : {e}"))?;
                for (t, c) in rows {
                    map.entry(t.to_lowercase())
                        .or_default()
                        .insert(c.to_lowercase());
                }
            }
            Drv::Sqlite => {
                let tables: Vec<(String,)> = sqlx::query_as(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                )
                .fetch_all(pool)
                .await
                .map_err(|e| format!("introspection : {e}"))?;
                for (t,) in tables {
                    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                        continue;
                    }
                    let cols = sqlx::query(&format!("PRAGMA table_info(\"{t}\")"))
                        .fetch_all(pool)
                        .await
                        .map_err(|e| format!("introspection {t} : {e}"))?;
                    let set = map.entry(t.to_lowercase()).or_default();
                    for row in cols {
                        let cn: String = row.try_get("name").unwrap_or_default();
                        set.insert(cn.to_lowercase());
                    }
                }
            }
        }
        Ok(map)
    }

    fn next_number(dir: &Path) -> usize {
        fs::read_dir(dir)
            .map(|es| {
                es.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "sql"))
                    .filter_map(|p| {
                        p.file_name()?
                            .to_string_lossy()
                            .split('_')
                            .next()?
                            .parse::<usize>()
                            .ok()
                    })
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
            + 1
    }

    fn slugify(s: &str) -> String {
        let out: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        if out.is_empty() {
            "diff".to_string()
        } else {
            out
        }
    }

    fn snake(name: &str) -> String {
        let mut r = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                r.push('_');
            }
            r.push(c.to_ascii_lowercase());
        }
        r
    }

    fn plural(s: &str) -> String {
        if s.ends_with('s') || s.ends_with('x') || s.ends_with('z') {
            format!("{s}es")
        } else if s.ends_with('y') && !s.ends_with("ey") && !s.ends_with("ay") && !s.ends_with("oy")
        {
            format!("{}ies", &s[..s.len() - 1])
        } else {
            format!("{s}s")
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Actions
// ─────────────────────────────────────────────────────────────────

async fn migrate_up(pool: &AnyPool, dir: &Path) -> Result<(), String> {
    let files = list_migrations(dir)?;
    let applied = applied_versions(pool).await?;

    let pending: Vec<&(String, PathBuf)> =
        files.iter().filter(|(v, _)| !applied.contains(v)).collect();

    if pending.is_empty() {
        println!(
            "  {} Database is up to date ({} applied)\n",
            "✓".green(),
            applied.len()
        );
        return Ok(());
    }

    println!("  {} {} pending migration(s)\n", "→".blue(), pending.len());

    for (version, path) in pending {
        let sql = fs::read_to_string(path).map_err(|e| format!("Cannot read {version}: {e}"))?;
        let (up, _down) = parse_migration(&sql);

        if up.trim().is_empty() {
            println!("  {} {} (empty, skipped)", "⊘".dimmed(), version.dimmed());
            continue;
        }

        sqlx::raw_sql(&up)
            .execute(pool)
            .await
            .map_err(|e| format!("Migration {version} failed: {e}"))?;

        record_migration(pool, version).await?;
        println!("  {} {}", "✓".green(), version);
    }

    println!("\n  {} Migrations applied\n", "✓".green().bold());
    Ok(())
}

async fn migrate_status(pool: &AnyPool, dir: &Path) -> Result<(), String> {
    let files = list_migrations(dir)?;
    let applied = applied_versions(pool).await?;

    if files.is_empty() {
        println!("  {} No migration files found\n", "⚠".yellow());
        return Ok(());
    }

    let pending = files.iter().filter(|(v, _)| !applied.contains(v)).count();

    for (version, _) in &files {
        if applied.contains(version) {
            println!("  {} {}  {}", "✓".green(), version, "applied".dimmed());
        } else {
            println!("  {} {}  {}", "○".yellow(), version, "pending".yellow());
        }
    }

    println!(
        "\n  {} {} applied, {} pending\n",
        "→".blue(),
        applied.len(),
        pending
    );
    Ok(())
}

async fn migrate_rollback(pool: &AnyPool, dir: &Path) -> Result<(), String> {
    let files = list_migrations(dir)?;
    let applied = applied_versions(pool).await?;

    // Last applied = highest version (filenames are zero-padded / ordered) that is applied.
    let last = files.iter().rev().find(|(v, _)| applied.contains(v));

    let Some((version, path)) = last else {
        println!("  {} Nothing to roll back\n", "⚠".yellow());
        return Ok(());
    };

    let sql = fs::read_to_string(path).map_err(|e| format!("Cannot read {version}: {e}"))?;
    let (_up, down) = parse_migration(&sql);

    let Some(down) = down else {
        return Err(format!(
            "Migration {version} has no `-- migrate:down` section — cannot roll back"
        ));
    };

    println!("  {} Rolling back {}\n", "→".blue(), version.bold());

    if !down.trim().is_empty() {
        sqlx::raw_sql(&down)
            .execute(pool)
            .await
            .map_err(|e| format!("Rollback of {version} failed: {e}"))?;
    }

    forget_migration(pool, version).await?;
    println!("  {} Rolled back {}\n", "✓".green().bold(), version);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// Tracking table
// ─────────────────────────────────────────────────────────────────

async fn ensure_migrations_table(pool: &AnyPool) -> Result<(), String> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {MIGRATIONS_TABLE} (\
         version VARCHAR(255) NOT NULL PRIMARY KEY, \
         applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"
    );
    sqlx::raw_sql(&sql)
        .execute(pool)
        .await
        .map_err(|e| format!("Cannot create migrations table: {e}"))?;
    Ok(())
}

async fn applied_versions(pool: &AnyPool) -> Result<std::collections::HashSet<String>, String> {
    let rows = sqlx::query(&format!("SELECT version FROM {MIGRATIONS_TABLE}"))
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Cannot read applied migrations: {e}"))?;
    Ok(rows.iter().map(|r| r.get::<String, _>("version")).collect())
}

async fn record_migration(pool: &AnyPool, version: &str) -> Result<(), String> {
    validate_version(version)?;
    // `version` is validated to a safe charset, so inlining is injection-safe and
    // avoids placeholder-syntax differences across the Any driver.
    sqlx::raw_sql(&format!(
        "INSERT INTO {MIGRATIONS_TABLE} (version) VALUES ('{version}')"
    ))
    .execute(pool)
    .await
    .map_err(|e| format!("Cannot record migration {version}: {e}"))?;
    Ok(())
}

async fn forget_migration(pool: &AnyPool, version: &str) -> Result<(), String> {
    validate_version(version)?;
    sqlx::raw_sql(&format!(
        "DELETE FROM {MIGRATIONS_TABLE} WHERE version = '{version}'"
    ))
    .execute(pool)
    .await
    .map_err(|e| format!("Cannot remove migration {version}: {e}"))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// Migration files
// ─────────────────────────────────────────────────────────────────

/// List `.sql` migrations sorted by filename, returning (version, path).
fn list_migrations(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut files: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .map_err(|e| format!("Cannot read migration/: {e}"))?
        .filter_map(|entry| entry.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "sql"))
        .map(|p| {
            let version = p.file_name().unwrap().to_string_lossy().to_string();
            (version, p)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Split a migration into its `up` and optional `down` parts.
///
/// Sections are delimited by `-- migrate:up` / `-- migrate:down` marker lines
/// (dbmate-style). A file without markers is treated as all-`up` (no rollback).
fn parse_migration(sql: &str) -> (String, Option<String>) {
    let mut up = String::new();
    let mut down = String::new();
    let mut saw_down_marker = false;
    // 0 = pre-marker (→ up), 1 = up, 2 = down
    let mut section = 0u8;

    for line in sql.lines() {
        let t = line.trim().to_lowercase();
        if t.starts_with("-- migrate:up") || t.starts_with("--migrate:up") {
            section = 1;
            continue;
        }
        if t.starts_with("-- migrate:down") || t.starts_with("--migrate:down") {
            section = 2;
            saw_down_marker = true;
            continue;
        }
        if section == 2 {
            down.push_str(line);
            down.push('\n');
        } else {
            up.push_str(line);
            up.push('\n');
        }
    }

    let down = if saw_down_marker { Some(down) } else { None };
    (up, down)
}

fn validate_version(version: &str) -> Result<(), String> {
    if version.is_empty()
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!("Unsafe migration filename: {version}"));
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// DATABASE_URL resolution
// ─────────────────────────────────────────────────────────────────

fn read_database_url(env_path: &Path) -> Result<String, String> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return Ok(url);
    }

    if env_path.exists() {
        let content = fs::read_to_string(env_path).map_err(|e| format!("Cannot read .env: {e}"))?;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(url) = line.strip_prefix("DATABASE_URL=") {
                return Ok(url.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }

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
    let pass = vars.get("DB_PASSWORD").cloned().unwrap_or_default();

    let scheme = match driver.as_str() {
        "mysql" | "mariadb" => "mysql",
        "postgres" | "postgresql" => "postgres",
        _ => return None,
    };

    Some(format!("{scheme}://{user}:{pass}@{host}:{port}/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_up_and_down() {
        let sql = "-- migrate:up\nCREATE TABLE t (id INT);\n-- migrate:down\nDROP TABLE t;\n";
        let (up, down) = parse_migration(sql);
        assert!(up.contains("CREATE TABLE t"));
        assert!(!up.contains("DROP TABLE"));
        assert_eq!(down.as_deref().map(str::trim), Some("DROP TABLE t;"));
    }

    #[test]
    fn no_markers_is_all_up() {
        let sql = "CREATE TABLE t (id INT);";
        let (up, down) = parse_migration(sql);
        assert!(up.contains("CREATE TABLE t"));
        assert!(down.is_none());
    }

    #[test]
    fn up_only_marker_has_no_down() {
        let sql = "-- migrate:up\nCREATE TABLE t (id INT);";
        let (_up, down) = parse_migration(sql);
        assert!(down.is_none());
    }

    #[test]
    fn rejects_unsafe_version() {
        assert!(validate_version("0001_create_posts.sql").is_ok());
        assert!(validate_version("../etc/passwd").is_err());
        assert!(validate_version("a'; DROP TABLE x; --").is_err());
    }
}
