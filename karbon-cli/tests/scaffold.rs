//! End-to-end CLI tests: scaffold a project, check its structure, run `doctor`
//! and the generators. The heavy "does the generated project compile?" test is
//! `#[ignore]`d (it builds the framework) and run by a dedicated CI job.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Repo root = parent of the karbon-cli crate dir.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("karbon-cli has a parent")
        .to_path_buf()
}

fn karbon() -> Command {
    Command::new(env!("CARGO_BIN_EXE_karbon"))
}

/// A unique temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("karbon_e2e_{tag}_{}_{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Scaffold a project of the given skeleton into `tmp`, returning its directory.
fn scaffold(tmp: &TempDir, name: &str, skeleton: &str) -> PathBuf {
    let status = karbon()
        .current_dir(tmp.path())
        .args(["new", name, "--skeleton", skeleton, "--local"])
        .arg(repo_root())
        .status()
        .expect("run karbon new");
    assert!(status.success(), "`karbon new` failed");
    tmp.path().join(name)
}

#[test]
fn micro_scaffold_structure_and_doctor() {
    let tmp = TempDir::new("micro");
    let proj = scaffold(&tmp, "demo", "micro");

    // Core files exist.
    for f in ["karbon.toml", "app/Cargo.toml", "app/src/main.rs", "app/src/welcome.html", "docs/index.md"] {
        assert!(proj.join(f).exists(), "missing {f}");
    }

    // Route auto-wiring markers are present.
    let main = std::fs::read_to_string(proj.join("app/src/main.rs")).unwrap();
    assert!(
        main.contains("// karbon:api-routes") && main.contains("// karbon:routes"),
        "auto-wiring markers missing from main.rs"
    );
    // OpenAPI + Swagger UI are wired out of the box.
    assert!(
        main.contains("/openapi.json") && main.contains("/docs") && main.contains("// karbon:openapi-api"),
        "OpenAPI / Swagger routes missing from main.rs"
    );

    // `--local` rewrote the dependency to a path dep with the studio feature.
    let cargo = std::fs::read_to_string(proj.join("app/Cargo.toml")).unwrap();
    assert!(cargo.contains("path ="), "local path dep not applied");
    assert!(cargo.contains("studio"), "studio feature not applied by --local");

    // The live `.env` carries its own random JWT secret; the published placeholder stays
    // in `.env.example` only (the framework refuses to start with it).
    let secret_of = |file: &str| {
        std::fs::read_to_string(proj.join(file))
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("JWT_SECRET=").map(str::to_string))
            .unwrap_or_else(|| panic!("no JWT_SECRET in {file}"))
    };
    let secret = secret_of(".env");
    assert_eq!(secret.len(), 64, "unexpected secret: {secret}");
    assert!(secret.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(secret_of(".env.example"), "change-me-to-a-random-secret");

    // `doctor` runs clean (exit 0, no failures, sees the local dep).
    let out = karbon().current_dir(&proj).arg("doctor").output().unwrap();
    assert!(out.status.success(), "doctor exited non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains('✗'), "doctor reported a failure:\n{stdout}");
    assert!(stdout.contains("local path dep"), "doctor didn't detect the local dependency:\n{stdout}");
}

#[test]
fn full_svelte_frontend_files_and_proxy() {
    let tmp = TempDir::new("full");
    let proj = scaffold(&tmp, "site", "full");

    // SvelteKit needs app.html (regression: it used to be missing).
    assert!(proj.join("frontend/src/app.html").exists(), "frontend/src/app.html missing");

    // The Vite dev proxy must forward every backend route.
    let vite = std::fs::read_to_string(proj.join("frontend/vite.config.ts")).unwrap();
    for route in ["/api", "/_studio", "/health", "/admin"] {
        assert!(vite.contains(route), "vite proxy is missing `{route}`:\n{vite}");
    }
}

#[test]
fn generators_entity_fields_and_dry_run() {
    let tmp = TempDir::new("gen");
    let proj = scaffold(&tmp, "app1", "micro");
    let entities = proj.join("app/src/entity");
    let migrations = proj.join("migration");

    let generate = |args: &[&str]| {
        let status = karbon()
            .current_dir(&proj)
            .arg("generate")
            .args(args)
            .status()
            .expect("run generate");
        assert!(status.success(), "generate {args:?} failed");
    };

    // Entity is created with the given typed fields.
    generate(&["entity", "Post", "title:string", "body:text", "views:int"]);
    let post = std::fs::read_to_string(entities.join("post.rs")).unwrap();
    assert!(post.contains("pub title: String"));
    assert!(post.contains("pub views: i32"));

    // Migrations are no longer created at `generate` time — `migrate diff` owns them.
    let migration_count = std::fs::read_dir(&migrations)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
                .count()
        })
        .unwrap_or(0);
    assert_eq!(migration_count, 0, "generate should not write migrations");

    // Dry-run writes nothing.
    generate(&["entity", "Tag", "--dry-run"]);
    assert!(!entities.join("tag.rs").exists(), "dry-run wrote an entity file");

    // Real run creates it.
    generate(&["entity", "Tag"]);
    assert!(entities.join("tag.rs").exists());
}

/// Heavy: scaffold a project and actually compile it against the local framework.
/// Ignored by default (builds the whole framework); the CI runs it with
/// `cargo test -p karbon-cli -- --ignored`.
#[test]
#[ignore = "compiles the framework — run explicitly in CI"]
fn generated_micro_project_compiles() {
    let tmp = TempDir::new("compile");
    let proj = scaffold(&tmp, "buildme", "micro");

    // A full CRUD with custom fields exercises macros + ORM + auto-wiring + the
    // field-aware entity/migration/repository generation.
    let status = karbon()
        .current_dir(&proj)
        .args([
            "generate", "crud", "Post", "title:string", "body:text", "views:int", "published:bool", "summary:string?",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    // The field-aware admin must compile against those custom fields too.
    let status = karbon()
        .current_dir(&proj)
        .args(["generate", "admin", "Post"])
        .status()
        .unwrap();
    assert!(status.success());

    // Field-aware admin: builds the list/form/New/Update from the custom fields.
    let status = karbon()
        .current_dir(&proj)
        .args(["generate", "admin", "Post"])
        .status()
        .unwrap();
    assert!(status.success());

    // Sign-in pages, JSON API and the User entity: the largest generated file.
    let status = karbon().current_dir(&proj).args(["generate", "auth"]).status().unwrap();
    assert!(status.success());

    let status = Command::new("cargo")
        .current_dir(&proj)
        .args(["build", "-p", "app"])
        .status()
        .expect("run cargo build on the generated project");
    assert!(status.success(), "generated project failed to compile");
}

/// The newcomer's first minutes, with nothing installed but the CLI: a new project has a
/// working database (SQLite), and `migrate diff` / `migrate` run on it as generated. They
/// used to stop at "DATABASE_URL not found": the `.env` names no driver.
#[test]
fn a_new_project_migrates_out_of_the_box() {
    let tmp = TempDir::new("sqlite");
    let status = karbon()
        .current_dir(tmp.path())
        .args(["new", "blog", "--skeleton", "micro", "--template", "blog", "--local"])
        .arg(repo_root())
        .status()
        .expect("run karbon new");
    assert!(status.success(), "`karbon new` failed");
    let proj = tmp.path().join("blog");

    let cargo = std::fs::read_to_string(proj.join("app/Cargo.toml")).unwrap();
    assert!(cargo.contains(r#"features = ["sqlite", "studio"]"#), "not wired for SQLite:\n{cargo}");
    let env = std::fs::read_to_string(proj.join(".env")).unwrap();
    assert!(env.lines().any(|l| l == "DB_NAME=app.db"), "no SQLite file in .env");
    assert!(!env.contains("DB_DRIVER") && !env.contains("DATABASE_URL"));

    let run = |args: &[&str]| {
        let out = karbon().current_dir(&proj).args(args).output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "`karbon {}` failed:\n{text}", args.join(" "));
        text
    };

    let diff = run(&["migrate", "diff"]);
    for table in ["posts", "users", "auth_tokens"] {
        assert!(diff.contains(&format!("table {table}")), "{table} missing from:\n{diff}");
    }
    let migration = std::fs::read_to_string(proj.join("migration/0001_diff.sql")).unwrap();
    assert!(migration.contains("INTEGER PRIMARY KEY AUTOINCREMENT"), "not SQLite DDL:\n{migration}");
    // `// karbon:unique` on the entity fields.
    assert!(migration.contains(r#""email" TEXT NOT NULL UNIQUE"#), "email is not unique:\n{migration}");
    assert!(migration.contains(r#""token_hash" TEXT NOT NULL UNIQUE"#), "{migration}");

    run(&["migrate"]);
    assert!(proj.join("app.db").exists(), "the SQLite file was not created");
    assert!(run(&["migrate", "status"]).contains("1 applied, 0 pending"));
    // The schema now matches the entities: nothing left to generate.
    assert!(run(&["migrate", "diff"]).contains("no migration generated"));

    let doctor = run(&["doctor", "--db"]);
    assert!(doctor.contains("configured (sqlite)") && !doctor.contains('✗'), "{doctor}");

    // The blog starter is behind ROLE_ADMIN: it comes with a way to sign in, and the
    // first administrator is one command away.
    let main = std::fs::read_to_string(proj.join("app/src/main.rs")).unwrap();
    assert!(main.contains("AuthController::router()") && main.contains("AuthApiController::router()"), "{main}");
    assert!(env.lines().any(|l| l == "LOGIN_PATH=/auth/login"), "LOGIN_PATH missing from .env");

    let created = run(&["user", "create", "Admin@Example.com", "--admin", "--password", "Str0ngPass"]);
    assert!(created.contains("admin@example.com created") && created.contains("ROLE_ADMIN"), "{created}");
    run(&["user", "create", "bob@example.com", "--password", "B0bPassword"]);
    assert!(run(&["user", "promote", "bob@example.com"]).contains("is now an administrator"));

    let fails = |args: &[&str], expected: &str| {
        let out = karbon().current_dir(&proj).args(args).output().unwrap();
        let text = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(!out.status.success() && text.contains(expected), "`karbon {}`: {text}", args.join(" "));
    };
    fails(&["user", "create", "admin@example.com", "--password", "Str0ngPass"], "already exists");
    fails(&["user", "create", "weak@example.com", "--password", "weak"], "at least 8 characters");
    fails(&["user", "create", "not-an-email", "--password", "Str0ngPass"], "not an email address");
    fails(&["user", "promote", "nobody@example.com"], "No account");
}

/// `generate auth` on its own, twice: the second run changes nothing.
#[test]
fn generate_auth_is_idempotent_and_takes_no_name() {
    let tmp = TempDir::new("auth");
    let proj = scaffold(&tmp, "site", "micro");
    let generate = |args: &[&str]| karbon().current_dir(&proj).args(args).output().unwrap();

    assert!(generate(&["generate", "auth"]).status.success());
    let (main, env) = (proj.join("app/src/main.rs"), proj.join(".env"));
    let first = (std::fs::read_to_string(&main).unwrap(), std::fs::read_to_string(&env).unwrap());
    assert!(first.0.matches("AuthController::router()").count() == 1 && first.1.matches("LOGIN_PATH=").count() == 1);
    assert!(
        ["entity/user.rs", "entity/auth_token.rs", "service/auth.rs", "controller/auth_controller.rs"]
            .iter()
            .all(|file| proj.join("app/src").join(file).exists())
    );

    assert!(generate(&["g", "auth"]).status.success());
    let second = (std::fs::read_to_string(&main).unwrap(), std::fs::read_to_string(&env).unwrap());
    assert_eq!(first, second, "a second `generate auth` changed main.rs or .env");

    let named = generate(&["generate", "auth", "Member"]);
    assert!(!named.status.success());
    assert!(String::from_utf8_lossy(&named.stderr).contains("takes no name"));

    let nameless = generate(&["generate", "crud"]);
    assert!(!nameless.status.success());
    assert!(String::from_utf8_lossy(&nameless.stderr).contains("Missing name"));
}

#[test]
fn the_database_option_wires_dependencies_and_env() {
    for (db, karbon_features, sqlx_driver, port) in [
        ("mysql", r#"features = ["studio"]"#, r#""mysql""#, "DB_PORT=3306"),
        ("postgres", r#"default-features = false, features = ["postgres", "studio"]"#, r#""postgres""#, "DB_PORT=5432"),
    ] {
        let tmp = TempDir::new(db);
        let status = karbon()
            .current_dir(tmp.path())
            .args(["new", "shop", "--skeleton", "micro", "--db", db, "--local"])
            .arg(repo_root())
            .status()
            .unwrap();
        assert!(status.success(), "`karbon new --db {db}` failed");
        let proj = tmp.path().join("shop");

        let cargo = std::fs::read_to_string(proj.join("app/Cargo.toml")).unwrap();
        assert!(cargo.contains(karbon_features), "{db}: {cargo}");
        let workspace = std::fs::read_to_string(proj.join("Cargo.toml")).unwrap();
        assert!(workspace.contains(sqlx_driver), "{db}: {workspace}");

        // A server-based database is left unset: nothing says that server exists yet.
        let env = std::fs::read_to_string(proj.join(".env")).unwrap();
        assert!(env.contains(port) && env.lines().any(|l| l == "DB_NAME="), "{db}: {env}");
    }

    let out = karbon().args(["new", "x", "--db", "oracle"]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Unknown database 'oracle'"));
}

/// `generate crud` used to leave a controller of TODOs, and `generate admin` a form where
/// a boolean was a required text box.
#[test]
fn crud_writes_a_working_api_and_admin_writes_typed_controls() {
    let tmp = TempDir::new("crud");
    let proj = scaffold(&tmp, "shop", "micro");
    let generate = |args: &[&str]| assert!(karbon().current_dir(&proj).args(args).status().unwrap().success());
    generate(&["generate", "crud", "Post", "title:string", "slug:string", "body:text", "views:int", "published:bool"]);
    generate(&["generate", "admin", "Post"]);
    let read = |file: &str| std::fs::read_to_string(proj.join("app/src").join(file)).unwrap();

    let controller = read("controller/post_controller.rs");
    for expected in [
        "PostRepository::find_paginated", "Paginated::new(items, total, &params)", "StatusCode::CREATED",
        "StatusCode::NO_CONTENT", "body.id = id;", r#"role = "ROLE_ADMIN""#,
    ] {
        assert!(controller.contains(expected), "`{expected}` missing from the controller:\n{controller}");
    }
    assert!(!controller.contains("TODO"), "the controller is still a stub:\n{controller}");

    // What an API client sends: no `id` in an update body, no `slug` when it derives from the title.
    let entity = read("entity/post.rs");
    assert!(entity.contains("#[serde(default)]\n    #[primary_key]\n    pub id: i64,"), "{entity}");
    assert!(entity.contains("#[serde(default)]\n    #[slug_from(\"title\")]\n    pub slug: String,"), "{entity}");

    let admin = read("controller/post_admin.rs");
    for expected in [
        r#"Field::text("title", "Title").required()"#,
        r#"Field::textarea("body", "Body").required()"#,
        r#"Field::number("views", "Views").required()"#,
        r#"Field::checkbox("published", "Published"))"#,
    ] {
        assert!(admin.contains(expected), "`{expected}` missing from the admin form:\n{admin}");
    }
}
