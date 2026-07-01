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
    println!(
        "\n{}  doctor — diagnostic du projet {}\n",
        "▲ karbon".bold().red(),
        config.app.name.bold().cyan()
    );

    let mut r = Report::new();

    check_dependency(&mut r, root);
    check_routing(&mut r, root);
    check_admin_entities(&mut r, root);
    check_database(&mut r, root);
    check_migrations(&mut r, root);
    check_frontend(&mut r, config, root);
    if online {
        check_database_online(&mut r, root);
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
fn check_database_online(r: &mut Report, root: &Path) {
    let env_path = root.join(".env");
    let Ok(content) = fs::read_to_string(&env_path) else {
        r.info("Base (online)", "pas de .env — rien à tester");
        return;
    };
    let vars = parse_env(&content);
    let Some(url) = build_db_url(&vars) else {
        r.info("Base (online)", "DB non configurée — rien à tester");
        return;
    };
    let migration_count = fs::read_dir(root.join("migration"))
        .map(|es| {
            es.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0) as i64;

    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
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
                r.ok("Base (connexion)", "OK");
                let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _karbon_migrations")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0);
                let pending = (migration_count - applied).max(0);
                if pending > 0 {
                    r.warn(
                        "Migrations (online)",
                        &format!("{pending} en attente"),
                        Some("karbon migrate"),
                    );
                } else {
                    r.ok("Migrations (online)", "à jour");
                }
            }
            Err(e) => r.fail(
                "Base (connexion)",
                &format!("échec : {e}"),
                Some("vérifie les variables DB_* dans .env et que le serveur tourne"),
            ),
        }
    });
}

/// Build a connection URL from `.env` (`DATABASE_URL`, or `DB_*` vars).
fn build_db_url(vars: &std::collections::HashMap<String, String>) -> Option<String> {
    if let Some(url) = vars.get("DATABASE_URL").filter(|v| !v.is_empty()) {
        return Some(url.clone());
    }
    let name = vars
        .get("DB_NAME")
        .or_else(|| vars.get("DB_DATABASE"))
        .filter(|v| !v.is_empty())?;
    let driver = vars
        .get("DB_DRIVER")
        .or_else(|| vars.get("DB_CONNECTION"))
        .map(String::as_str)
        .unwrap_or("mysql");
    let get = |k: &str, d: &str| {
        vars.get(k)
            .filter(|v| !v.is_empty())
            .cloned()
            .unwrap_or_else(|| d.to_string())
    };
    match driver {
        "sqlite" => Some(format!("sqlite://{name}")),
        "postgres" | "postgresql" => {
            let user = vars
                .get("DB_USER")
                .or_else(|| vars.get("DB_USERNAME"))
                .map(String::as_str)
                .unwrap_or("postgres");
            let pass = get("DB_PASSWORD", "");
            Some(format!(
                "postgres://{user}:{pass}@{}:{}/{name}",
                get("DB_HOST", "127.0.0.1"),
                get("DB_PORT", "5432")
            ))
        }
        _ => {
            let user = vars
                .get("DB_USER")
                .or_else(|| vars.get("DB_USERNAME"))
                .map(String::as_str)
                .unwrap_or("root");
            let pass = get("DB_PASSWORD", "");
            Some(format!(
                "mysql://{user}:{pass}@{}:{}/{name}",
                get("DB_HOST", "127.0.0.1"),
                get("DB_PORT", "3306")
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// Checks
// ─────────────────────────────────────────────────────────────────

/// The `karbon` dependency: path dep (dev) vs crates.io version, studio feature.
fn check_dependency(r: &mut Report, root: &Path) {
    let Some(cargo) = app_dir(root).map(|d| d.join("Cargo.toml")) else {
        r.fail(
            "Crate backend introuvable",
            "ni app/ ni api/ ne contient de Cargo.toml",
            Some("lance `karbon doctor` depuis la racine d'un projet Karbon"),
        );
        return;
    };
    let Ok(content) = fs::read_to_string(&cargo) else {
        r.fail("Lecture de app/Cargo.toml", "impossible", None);
        return;
    };
    let Some(dep) = content
        .lines()
        .find(|l| l.trim_start().starts_with("karbon ") || l.trim_start().starts_with("karbon="))
    else {
        r.fail(
            "Dépendance `karbon`",
            "absente de app/Cargo.toml",
            Some("ajoute `karbon = { package = \"karbon-framework\", ... }`"),
        );
        return;
    };

    if dep.contains("path") {
        r.ok("Dépendance `karbon`", "path dep locale (dev)");
    } else if dep.contains("git") {
        r.ok("Dépendance `karbon`", "dépendance git");
    } else if dep.contains("version") {
        r.warn(
            "Dépendance `karbon`",
            "résolue depuis crates.io (version publiée, potentiellement ancienne)",
            Some(
                "en dev contre ce dépôt : `karbon new <app> --local <chemin-du-repo-karbon>` (path dep + features à jour)",
            ),
        );
    }

    if dep.contains("studio") {
        r.info(
            "Feature `studio`",
            "activée — dashboard + toolbar dispo en dev",
        );
    } else {
        r.info(
            "Feature `studio`",
            "absente — pas de Studio (ajoute `features = [\"studio\"]`)",
        );
    }
}

/// Route auto-wiring markers + controllers generated but not mounted.
fn check_routing(r: &mut Report, root: &Path) {
    let Some(src) = app_src(root) else { return };
    let main_rs = src.join("main.rs");
    let Ok(main) = fs::read_to_string(&main_rs) else {
        r.warn("main.rs", "introuvable", None);
        return;
    };

    if main.contains("// karbon:api-routes") {
        r.ok("Auto-montage des routes", "marqueurs présents dans main.rs");
    } else {
        r.warn(
            "Auto-montage des routes",
            "marqueurs absents — les contrôleurs générés ne se câbleront pas tout seuls",
            Some(
                "ajoute `// karbon:api-routes` dans api_routes() et `// karbon:routes` dans build_router()",
            ),
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
                "Contrôleur non monté",
                &format!("`{ty}` est généré mais absent de main.rs"),
                Some(&format!(
                    ".nest(controller::{module}::{ty}::prefix(), controller::{module}::{ty}::router())  — ou relance `karbon generate ...` (auto-montage)"
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
                "Admin sans entité",
                &format!("`{base}_admin.rs` référence l'entité `{base}` introuvable"),
                Some(&format!(
                    "lance `karbon generate crud {}` avant l'admin",
                    pascal(base)
                )),
            );
        }
    }
}

/// Database configuration (read .env, never connects).
fn check_database(r: &mut Report, root: &Path) {
    let env_path = root.join(".env");
    if !env_path.exists() {
        r.warn(
            "Fichier .env",
            "absent",
            Some("copie `.env.example` → `.env`"),
        );
        return;
    }
    let Ok(content) = fs::read_to_string(&env_path) else {
        return;
    };
    let vars = parse_env(&content);

    let has_url = vars.get("DATABASE_URL").is_some_and(|v| !v.is_empty());
    let db_name = vars.get("DB_NAME").map(String::as_str).unwrap_or("");

    if has_url || !db_name.is_empty() {
        let driver = vars
            .get("DB_DRIVER")
            .or_else(|| vars.get("DB_CONNECTION"))
            .cloned()
            .unwrap_or_else(|| "mysql".to_string());
        r.ok("Base de données", &format!("configurée ({driver})"));
    } else {
        r.info(
            "Base de données",
            "non configurée (DB_NAME vide) — l'app tourne sans DB ; migrations & admin en ont besoin",
        );
    }
}

/// Migration files: count + duplicate version-number detection.
fn check_migrations(r: &mut Report, root: &Path) {
    let dir = root.join("migration");
    let Ok(entries) = fs::read_dir(&dir) else {
        r.info("Migrations", "pas de dossier migration/");
        return;
    };
    let files: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".sql"))
        .collect();

    if files.is_empty() {
        r.info("Migrations", "aucune migration");
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
            &format!("préfixe `{p}` en double — ordre d'application ambigu"),
            Some("renumérote les fichiers en double dans migration/"),
        );
    } else {
        r.ok(
            "Migrations",
            &format!("{} fichier(s), numérotation OK", files.len()),
        );
    }
}

/// Frontend project sanity (only for full skeletons).
fn check_frontend(r: &mut Report, config: &KarbonConfig, root: &Path) {
    let Some(fc) = &config.frontend else {
        r.info("Frontend", "aucun (skeleton micro / backend-only)");
        return;
    };
    let dir = root.join(&fc.dir);
    if !dir.exists() {
        r.fail(
            "Frontend",
            &format!("dossier `{}` introuvable", fc.dir),
            None,
        );
        return;
    }
    if !dir.join("package.json").exists() {
        r.warn("Frontend", "package.json absent", None);
    }

    // SvelteKit needs src/app.html.
    if dir.join("svelte.config.js").exists() && !dir.join("src/app.html").exists() {
        r.fail(
            "Frontend (SvelteKit)",
            "src/app.html manquant",
            Some("crée src/app.html (requis par SvelteKit)"),
        );
    }

    // Vite dev proxy must forward backend routes.
    let vite = dir.join("vite.config.ts");
    if let Ok(content) = fs::read_to_string(&vite) {
        let mut missing = Vec::new();
        for route in ["/api", "/_studio", "/health", "/admin"] {
            if !content.contains(&format!("'{route}'"))
                && !content.contains(&format!("\"{route}\""))
            {
                missing.push(route);
            }
        }
        if missing.is_empty() {
            r.ok("Proxy Vite", "relaie /api, /_studio, /health, /admin");
        } else {
            r.warn(
                "Proxy Vite",
                &format!("relais manquant(s) : {}", missing.join(", ")),
                Some("ajoute-les au bloc `server.proxy` de vite.config.ts → http://localhost:<port-backend>"),
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
        Self {
            ok: 0,
            warn: 0,
            fail: 0,
        }
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
            println!(
                "  {} Tout est bon ({} vérifications OK)\n",
                "✓".green().bold(),
                self.ok
            );
        } else {
            println!(
                "  {} {} OK · {} avertissement(s) · {} problème(s)\n",
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
            vars.insert(
                k.trim().to_string(),
                v.trim().trim_matches('"').trim_matches('\'').to_string(),
            );
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case() {
        assert_eq!(pascal("post"), "Post");
        assert_eq!(pascal("blog_comment"), "BlogComment");
    }

    #[test]
    fn controller_types() {
        assert_eq!(
            controller_type("post_controller").as_deref(),
            Some("PostController")
        );
        assert_eq!(
            controller_type("blog_comment_admin").as_deref(),
            Some("BlogCommentAdminController")
        );
        assert_eq!(controller_type("health"), None);
    }

    #[test]
    fn declared_modules_parse() {
        let s = "pub mod health;\npub mod post_controller;\nmod internal;";
        let m = declared_modules(s);
        assert_eq!(m, vec!["health", "post_controller", "internal"]);
    }
}
