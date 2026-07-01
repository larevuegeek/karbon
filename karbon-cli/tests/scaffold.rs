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
    for f in [
        "karbon.toml",
        "app/Cargo.toml",
        "app/src/main.rs",
        "app/src/welcome.html",
        "docs/index.md",
    ] {
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
        main.contains("/openapi.json")
            && main.contains("/docs")
            && main.contains("// karbon:openapi-api"),
        "OpenAPI / Swagger routes missing from main.rs"
    );

    // `--local` rewrote the dependency to a path dep with the studio feature.
    let cargo = std::fs::read_to_string(proj.join("app/Cargo.toml")).unwrap();
    assert!(cargo.contains("path ="), "local path dep not applied");
    assert!(
        cargo.contains("studio"),
        "studio feature not applied by --local"
    );

    // `doctor` runs clean (exit 0, no failures, sees the local dep).
    let out = karbon().current_dir(&proj).arg("doctor").output().unwrap();
    assert!(out.status.success(), "doctor exited non-zero");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains('✗'),
        "doctor reported a failure:\n{stdout}"
    );
    assert!(
        stdout.contains("path dep locale"),
        "doctor didn't detect the local dependency:\n{stdout}"
    );
}

#[test]
fn full_svelte_frontend_files_and_proxy() {
    let tmp = TempDir::new("full");
    let proj = scaffold(&tmp, "site", "full");

    // SvelteKit needs app.html (regression: it used to be missing).
    assert!(
        proj.join("frontend/src/app.html").exists(),
        "frontend/src/app.html missing"
    );

    // The Vite dev proxy must forward every backend route.
    let vite = std::fs::read_to_string(proj.join("frontend/vite.config.ts")).unwrap();
    for route in ["/api", "/_studio", "/health", "/admin"] {
        assert!(
            vite.contains(route),
            "vite proxy is missing `{route}`:\n{vite}"
        );
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
    assert!(
        !entities.join("tag.rs").exists(),
        "dry-run wrote an entity file"
    );

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
            "generate",
            "crud",
            "Post",
            "title:string",
            "body:text",
            "views:int",
            "published:bool",
            "summary:string?",
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

    let status = Command::new("cargo")
        .current_dir(&proj)
        .args(["build", "-p", "app"])
        .status()
        .expect("run cargo build on the generated project");
    assert!(status.success(), "generated project failed to compile");
}
