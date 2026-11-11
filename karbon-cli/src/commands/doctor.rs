use crate::config::KarbonConfig;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// `karbon doctor` — static diagnostics for common project pitfalls.
///
/// Fast and offline: it never connects to the database. It catches the mistakes
/// that produce confusing failures — controllers generated but not mounted,
/// `generate admin` without an entity, a `crates.io` dependency in dev instead of
/// `--local`, an incomplete Vite proxy, duplicate migration numbers, etc.
pub fn run(config: &KarbonConfig, root: &Path, online: bool) -> Result<(), String> {
    println!("\n{}  doctor — diagnosing project {}\n", "▲ karbon".bold().red(), config.app.name.bold().cyan());

    let mut r = Report::new();

    check_dependency(&mut r, root);
    check_routing(&mut r, root);
    check_admin_entities(&mut r, root);
    check_database(&mut r, root);
    check_migrations(&mut r, &config.migration_dir(root));
    check_frontend(&mut r, config, root);
    if online {
        check_database_online(&mut r, root, &config.migration_dir(root));
    }

    r.summary();
    // Non-zero exit on hard failures so the check is usable in CI / the terminal.
    if r.fail > 0 {
        std::process::exit(1);
    }
    Ok(())
}

/// Connect to the database (opt-in, `--db`) and report connectivity + pending
/// migrations. Reuses the sqlx `Any` driver (like `karbon migrate`).
fn check_database_online(r: &mut Report, root: &Path, migration_dir: &Path) {
    let env_path = root.join(".env");
    let Ok(content) = fs::read_to_string(&env_path) else {
        r.info("Database (online)", "no .env — nothing to test");
        return;
    };
    let vars = parse_env(&content);
    let Some(url) = crate::db_url::from_vars(&vars, crate::db_url::compiled_driver(root)) else {
        r.info("Database (online)", "no database configured — nothing to test");
        return;
    };
    // Connecting would create a missing SQLite file: a diagnostic must not do that.
    if let Some(file) = crate::db_url::sqlite_file(&url)
        && !root.join(file).exists()
    {
        r.info(
            "Database (online)",
            &format!("`{file}` not created yet — `karbon migrate` or the first run creates it"),
        );
        return;
    }
    let _ = std::env::set_current_dir(root);
    let migration_count = fs::read_dir(migration_dir)
        .map(|es| {
            es.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0) as i64;

    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return;
    };
    rt.block_on(async move {
        sqlx::any::install_default_drivers();
        match sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(&url)
            .await
        {
            Ok(pool) => {
                r.ok("Database (connection)", "OK");
                let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _karbon_migrations")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
                let pending = (migration_count - applied).max(0);
                if pending > 0 {
                    r.warn("Migrations (online)", &format!("{pending} pending"), Some("karbon migrate"));
                } else {
                    r.ok("Migrations (online)", "up to date");
                }
            }
            Err(e) => r.fail(
                "Database (connection)",
                &format!("failed: {e}"),
                Some("check the DB_* variables in .env and that the server is running"),
            ),
        }
    });
}

// ─────────────────────────────────────────────────────────────────
// Checks
// ─────────────────────────────────────────────────────────────────

/// The `karbon` dependency: path dep (dev) vs crates.io version, studio feature.
fn check_dependency(r: &mut Report, root: &Path) {
    let Some(cargo) = app_dir(root).map(|d| d.join("Cargo.toml")) else {
        r.fail(
            "Backend crate not found",
            "neither app/ nor api/ holds a Cargo.toml",
            Some("run `karbon doctor` from the root of a Karbon project"),
        );
        return;
    };
    let Ok(content) = fs::read_to_string(&cargo) else {
        r.fail("app/Cargo.toml", "cannot be read", None);
        return;
    };
    let Some(dep) = content
        .lines()
        .find(|l| l.trim_start().starts_with("karbon ") || l.trim_start().starts_with("karbon="))
    else {
        r.fail(
            "`karbon` dependency",
            "missing from app/Cargo.toml",
            Some("add `karbon = { package = \"karbon-framework\", ... }`"),
        );
        return;
    };

    if dep.contains("path") {
        r.ok("`karbon` dependency", "local path dep (dev)");
    } else if dep.contains("git") {
        r.ok("`karbon` dependency", "git dependency");
    } else if dep.contains("version") {
        r.warn(
            "`karbon` dependency",
            "resolved from crates.io (published version, possibly older than this checkout)",
            Some(
                "to develop against this repository: `karbon new <app> --local <path-to-karbon-repo>` (path dep, current features)",
            ),
        );
    }

    if dep.contains("studio") {
        r.info("`studio` feature", "enabled — dashboard and toolbar available in dev");
    } else {
        r.info("`studio` feature", "disabled — no Studio (add `features = [\"studio\"]`)");
    }
}

/// Route auto-wiring markers + controllers generated but not mounted.
fn check_routing(r: &mut Report, root: &Path) {
    let Some(src) = app_src(root) else { return };
    let main_rs = src.join("main.rs");
    let Ok(main) = fs::read_to_string(&main_rs) else {
        r.warn("main.rs", "not found", None);
        return;
    };

    if main.contains("// karbon:api-routes") {
        r.ok("Route auto-wiring", "markers present in main.rs");
    } else {
        r.warn(
            "Route auto-wiring",
            "markers missing — generated controllers will not be mounted automatically",
            Some("add `// karbon:api-routes` in api_routes() and `// karbon:routes` in build_router()"),
        );
    }

    // Controllers declared in controller/mod.rs but not referenced in main.rs.
    let mod_rs = src.join("controller/mod.rs");
    let Ok(mods) = fs::read_to_string(&mod_rs) else {
        return;
    };
    for module in declared_modules(&mods) {
        let Some(ty) = controller_type(&module) else {
            continue; // not a controller module (e.g. `health`)
        };
        if !main.contains(&ty) {
            r.warn(
                "Controller not mounted",
                &format!("`{ty}` is generated but missing from main.rs"),
                Some(&format!(
                    ".nest(controller::{module}::{ty}::prefix(), controller::{module}::{ty}::router())  — or run `karbon generate ...` again (auto-wiring)"
                )),
            );
        }
    }
}

/// Every `*_admin.rs` controller needs its backing entity to exist.
fn check_admin_entities(r: &mut Report, root: &Path) {
    let Some(src) = app_src(root) else { return };
    let controller_dir = src.join("controller");
    let Ok(entries) = fs::read_dir(&controller_dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(base) = name.strip_suffix("_admin.rs") else {
            continue;
        };
        let entity = src.join(format!("entity/{base}.rs"));
        if !entity.exists() {
            r.fail(
                "Admin without entity",
                &format!("`{base}_admin.rs` refers to the entity `{base}`, which does not exist"),
                Some(&format!("run `karbon generate crud {}` before the admin", pascal(base))),
            );
        }
    }
}

/// Database configuration (read .env, never connects).
fn check_database(r: &mut Report, root: &Path) {
    let env_path = root.join(".env");
    if !env_path.exists() {
        r.warn(".env file", "missing", Some("copy `.env.example` to `.env`, then run `karbon secret --write`"));
        return;
    }
    let Ok(content) = fs::read_to_string(&env_path) else {
        return;
    };
    let vars = parse_env(&content);
    check_jwt_secret(r, vars.get("JWT_SECRET").map(String::as_str).unwrap_or(""));

    let has_url = vars.get("DATABASE_URL").is_some_and(|v| !v.is_empty());
    let db_name = vars.get("DB_NAME").map(String::as_str).unwrap_or("");

    if has_url || !db_name.is_empty() {
        let driver = vars
            .get("DB_DRIVER")
            .or_else(|| vars.get("DB_CONNECTION"))
            .cloned()
            .unwrap_or_else(|| crate::db_url::compiled_driver(root).to_string());
        r.ok("Database", &format!("configured ({driver})"));
    } else {
        r.info(
            "Database",
            "not configured (DB_NAME empty) — the app runs without one; migrations and the admin need it",
        );
    }
}

/// `JWT_SECRET`: the published placeholder is a hard failure — the framework refuses to
/// start with it, and anyone could forge a token if it did.
fn check_jwt_secret(r: &mut Report, secret: &str) {
    use super::secret::{MIN_LEN, PLACEHOLDERS};
    const LABEL: &str = "JWT_SECRET";
    const FIX: Option<&str> = Some("`karbon secret --write` generates one and writes it to .env");

    let secret = secret.trim();
    if secret.is_empty() {
        r.info(LABEL, "empty — authentication disabled (every token is rejected)");
    } else if PLACEHOLDERS.contains(&secret) {
        r.fail(LABEL, "this is the public example value: the app refuses to start with it", FIX);
    } else if secret.len() < MIN_LEN {
        r.warn(LABEL, &format!("shorter than {MIN_LEN} characters — refused outside development/test"), FIX);
    } else {
        r.ok(LABEL, "set");
    }
}

/// Migration files: count + duplicate version-number detection.
fn check_migrations(r: &mut Report, dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        r.info("Migrations", "no migration/ directory");
        return;
    };
    let files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".sql"))
        .collect();

    if files.is_empty() {
        r.info("Migrations", "no migration yet");
        return;
    }

    // Duplicate NNNN_ prefixes → would apply ambiguously.
    let mut prefixes: Vec<&str> = files
        .iter()
        .filter_map(|f| f.split('_').next())
        .filter(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit()))
        .collect();
    prefixes.sort_unstable();
    let dup = prefixes.windows(2).find(|w| w[0] == w[1]).map(|w| w[0]);

    if let Some(p) = dup {
        r.warn(
            "Migrations",
            &format!("prefix `{p}` used twice — the order they apply in is ambiguous"),
            Some("renumber the duplicate files in migration/"),
        );
    } else {
        r.ok("Migrations", &format!("{} file(s), numbering OK", files.len()));
    }
}

/// Frontend project sanity (only for full skeletons).
fn check_frontend(r: &mut Report, config: &KarbonConfig, root: &Path) {
    let Some(fc) = &config.frontend else {
        r.info("Frontend", "none (micro skeleton / backend-only)");
        return;
    };
    let dir = root.join(&fc.dir);
    if !dir.exists() {
        r.fail("Frontend", &format!("directory `{}` not found", fc.dir), None);
        return;
    }
    if !dir.join("package.json").exists() {
        r.warn("Frontend", "package.json missing", None);
    }

    // SvelteKit needs src/app.html.
    if dir.join("svelte.config.js").exists() && !dir.join("src/app.html").exists() {
        r.fail("Frontend (SvelteKit)", "src/app.html missing", Some("create src/app.html (SvelteKit requires it)"));
    }

    // Vite dev proxy must forward backend routes.
    let vite = dir.join("vite.config.ts");
    if let Ok(content) = fs::read_to_string(&vite) {
        let mut missing = Vec::new();
        for route in ["/api", "/_studio", "/health", "/admin"] {
            if !content.contains(&format!("'{route}'")) && !content.contains(&format!("\"{route}\"")) {
                missing.push(route);
            }
        }
        if missing.is_empty() {
            r.ok("Vite proxy", "forwards /api, /_studio, /health, /admin");
        } else {
            r.warn(
                "Vite proxy",
                &format!("route(s) not forwarded: {}", missing.join(", ")),
                Some("add them to the `server.proxy` block of vite.config.ts → http://localhost:<backend-port>"),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Report
// ─────────────────────────────────────────────────────────────────

struct Report {
    ok: u32,
    warn: u32,
    fail: u32,
}

impl Report {
    fn new() -> Self {
        Self { ok: 0, warn: 0, fail: 0 }
    }

    fn ok(&mut self, label: &str, detail: &str) {
        self.ok += 1;
        println!("  {} {} — {}", "✓".green(), label.bold(), detail.dimmed());
    }

    fn info(&mut self, label: &str, detail: &str) {
        println!("  {} {} — {}", "ℹ".blue(), label.bold(), detail.dimmed());
    }

    fn warn(&mut self, label: &str, detail: &str, fix: Option<&str>) {
        self.warn += 1;
        println!("  {} {} — {}", "⚠".yellow(), label.bold(), detail);
        if let Some(fix) = fix {
            println!("      {} {}", "→".dimmed(), fix.dimmed());
        }
    }

    fn fail(&mut self, label: &str, detail: &str, fix: Option<&str>) {
        self.fail += 1;
        println!("  {} {} — {}", "✗".red(), label.bold(), detail);
        if let Some(fix) = fix {
            println!("      {} {}", "→".dimmed(), fix.dimmed());
        }
    }

    fn summary(&self) {
        println!();
        if self.fail == 0 && self.warn == 0 {
            println!("  {} All good ({} checks OK)\n", "✓".green().bold(), self.ok);
        } else {
            println!(
                "  {} {} OK · {} warning(s) · {} problem(s)\n",
                "→".blue(),
                self.ok.to_string().green(),
                self.warn.to_string().yellow(),
                self.fail.to_string().red()
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────

fn app_dir(root: &Path) -> Option<PathBuf> {
    for d in ["app", "api"] {
        let p = root.join(d);
        if p.join("Cargo.toml").exists() {
            return Some(p);
        }
    }
    None
}

fn app_src(root: &Path) -> Option<PathBuf> {
    app_dir(root).map(|d| d.join("src")).filter(|p| p.exists())
}

/// `pub mod foo;` lines → ["foo", ...].
fn declared_modules(mod_rs: &str) -> Vec<String> {
    mod_rs
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix("pub mod ")
                .or_else(|| l.strip_prefix("mod "))
                .and_then(|rest| rest.split(';').next())
                .map(|s| s.trim().to_string())
        })
        .collect()
}

/// Module name → its controller type, or None if it isn't a controller module.
/// `post_controller` → `PostController`, `post_admin` → `PostAdminController`.
fn controller_type(module: &str) -> Option<String> {
    if let Some(base) = module.strip_suffix("_admin") {
        Some(format!("{}AdminController", pascal(base)))
    } else {
        module
            .strip_suffix("_controller")
            .map(|base| format!("{}Controller", pascal(base)))
    }
}

/// snake_case → PascalCase.
fn pascal(s: &str) -> String {
    s.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn parse_env(content: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            vars.insert(k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_placeholder_fails_the_diagnostic() {
        let verdict = |secret: &str| {
            let mut r = Report::new();
            check_jwt_secret(&mut r, secret);
            (r.ok, r.warn, r.fail)
        };
        assert_eq!(verdict("change-me-to-a-random-secret"), (0, 0, 1));
        assert_eq!(verdict("short-but-private"), (0, 1, 0));
        assert_eq!(verdict(&super::super::secret::generate()), (1, 0, 0));
        assert_eq!(verdict(""), (0, 0, 0));
    }

    #[test]
    fn pascal_case() {
        assert_eq!(pascal("post"), "Post");
        assert_eq!(pascal("blog_comment"), "BlogComment");
    }

    #[test]
    fn controller_types() {
        assert_eq!(controller_type("post_controller").as_deref(), Some("PostController"));
        assert_eq!(controller_type("blog_comment_admin").as_deref(), Some("BlogCommentAdminController"));
        assert_eq!(controller_type("health"), None);
    }

    #[test]
    fn declared_modules_parse() {
        let s = "pub mod health;\npub mod post_controller;\nmod internal;";
        let m = declared_modules(s);
        assert_eq!(m, vec!["health", "post_controller", "internal"]);
    }
}
