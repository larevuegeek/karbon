use colored::Colorize;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::Path;

/// Field types the assistant offers (matches `parse_fields`).
const FIELD_TYPES: &[&str] = &[
    "string", "text", "int", "bigint", "float", "bool", "datetime", "date", "json",
];

/// Resolve the field specs for entity/crud: use the ones given on the command
/// line, or — when none are given and stdin is interactive — launch the Symfony
/// `make:entity`-style field assistant. Non-interactive (Studio terminal, pipes,
/// CI) with no fields falls back to the `title`/`slug` default in `parse_fields`.
fn resolve_fields(fields: &[String], opts: GenOptions) -> Result<Vec<String>, String> {
    if opts.interactive || (fields.is_empty() && std::io::stdin().is_terminal()) {
        prompt_fields()
    } else {
        Ok(fields.to_vec())
    }
}

/// Interactive field builder (one field at a time: name → type → nullable).
fn prompt_fields() -> Result<Vec<String>, String> {
    println!(
        "\n  {} Assistant de champs — entrée vide sur le nom pour terminer\n",
        "▸".cyan().bold()
    );
    let mut fields: Vec<String> = Vec::new();
    loop {
        let name = prompt(&format!("  Champ #{} — nom : ", fields.len() + 1))?;
        let name = name.trim();
        if name.is_empty() {
            break;
        }
        let name = snake(name);

        let ty = loop {
            let raw = prompt("    type [string] : ")?;
            let t = raw.trim();
            let t = if t.is_empty() { "string" } else { t };
            if FIELD_TYPES.contains(&t) {
                break t.to_string();
            }
            println!(
                "    {} type inconnu. Disponibles : {}",
                "✗".red(),
                FIELD_TYPES.join(", ").dimmed()
            );
        };

        let nullable = matches!(
            prompt("    nullable ? (o/N) : ")?
                .trim()
                .to_lowercase()
                .as_str(),
            "o" | "oui" | "y" | "yes"
        );

        let spec = format!("{name}:{ty}{}", if nullable { "?" } else { "" });
        println!("    {} {}\n", "✓".green(), spec.cyan());
        fields.push(spec);
    }
    if fields.is_empty() {
        println!(
            "  {} aucun champ — valeurs par défaut (title, slug)\n",
            "ℹ".blue()
        );
    }
    Ok(fields)
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin()
        .read_line(&mut s)
        .map_err(|e| format!("lecture stdin : {e}"))?;
    Ok(s)
}

/// Options shared by every generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct GenOptions {
    /// Print what would happen, write nothing.
    pub dry_run: bool,
    /// Overwrite existing files instead of skipping them.
    pub force: bool,
    /// Force the interactive field assistant (otherwise auto when stdin is a TTY).
    pub interactive: bool,
}

pub fn run(
    kind: &str,
    name: &str,
    fields: &[String],
    root: &Path,
    opts: GenOptions,
) -> Result<(), String> {
    println!(
        "\n{}  generate {} {}{}\n",
        "▲ karbon".bold().red(),
        kind.bold(),
        name.bold().cyan(),
        if opts.dry_run {
            "  (dry-run)".yellow().to_string()
        } else {
            String::new()
        }
    );

    match kind {
        "entity" => generate_entity(name, &resolve_fields(fields, opts)?, root, opts),
        "controller" => generate_controller(name, root, opts),
        "crud" => generate_crud(name, &resolve_fields(fields, opts)?, root, opts),
        "admin" => generate_admin(name, root, opts),
        _ => Err(format!(
            "Unknown generator '{}'. Available: entity, controller, crud, admin",
            kind
        )),
    }
}

// ─── Fields & drivers ───

#[derive(Debug, Clone, Copy, PartialEq)]
enum Driver {
    Mysql,
    Postgres,
    Sqlite,
}

/// Detect the project's DB driver from the `karbon` dependency features in
/// `app/Cargo.toml` (default: MySQL).
fn detect_driver(root: &Path) -> Driver {
    for dir in ["app", "api"] {
        let cargo = root.join(dir).join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo) {
            let line = content
                .lines()
                .find(|l| l.trim_start().starts_with("karbon"))
                .unwrap_or("");
            if line.contains("postgres") {
                return Driver::Postgres;
            }
            if line.contains("sqlite") {
                return Driver::Sqlite;
            }
            return Driver::Mysql;
        }
    }
    Driver::Mysql
}

struct Field {
    name: String,
    /// Rust type (already wrapped in `Option<…>` when nullable).
    rust_ty: String,
    /// Base Rust type (without the `Option<…>` wrapper).
    base_ty: String,
    /// One of: string text int bigint float bool datetime date json.
    kind: String,
}

/// Parse `name:type[?]` specs. Empty input → a sensible default (`title`, `slug`).
fn parse_fields(specs: &[String]) -> Result<Vec<Field>, String> {
    let owned: Vec<String> = if specs.is_empty() {
        vec!["title:string".into(), "slug:string".into()]
    } else {
        specs.to_vec()
    };

    owned
        .iter()
        .map(|spec| {
            let (raw_name, raw_ty) = spec
                .split_once(':')
                .ok_or_else(|| format!("Invalid field `{spec}` — expected `name:type`"))?;
            let nullable = raw_ty.ends_with('?');
            let kind = raw_ty.trim_end_matches('?').to_lowercase();
            let base_ty = rust_base_type(&kind)
                .ok_or_else(|| format!("Unknown type `{kind}` in `{spec}` (string text int bigint float bool datetime date json)"))?;
            let rust_ty = if nullable {
                format!("Option<{base_ty}>")
            } else {
                base_ty.to_string()
            };
            Ok(Field {
                name: snake(raw_name),
                rust_ty,
                base_ty: base_ty.to_string(),
                kind,
            })
        })
        .collect()
}

/// Base Rust type for a field kind (driver-independent; the SQL column type is
/// derived later by `migrate diff`).
fn rust_base_type(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "string" | "text" => "String",
        "int" => "i32",
        "bigint" => "i64",
        "float" => "f64",
        "bool" => "bool",
        "datetime" => "chrono::NaiveDateTime",
        "date" => "chrono::NaiveDate",
        "json" => "serde_json::Value",
        _ => return None,
    })
}

// ─── Admin field introspection ───

/// Parse the first `pub struct … { … }` in an entity file into
/// `(field, kind, nullable)`, excluding `id`/`created`/`updated`.
fn parse_entity_fields(src: &str) -> Vec<(String, String, bool)> {
    let Some(start) = src.find("pub struct ") else {
        return Vec::new();
    };
    let body = &src[start..];
    let (Some(open), Some(close)) = (body.find('{'), body.find('}')) else {
        return Vec::new();
    };
    body[open + 1..close]
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(',');
            let rest = line.strip_prefix("pub ")?;
            let (name, ty) = rest.split_once(':')?;
            let name = name.trim().to_string();
            if matches!(name.as_str(), "id" | "created" | "updated") {
                return None;
            }
            let ty = ty.trim();
            let (nullable, base) =
                match ty.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
                    Some(inner) => (true, inner.trim()),
                    None => (false, ty),
                };
            let kind = match base {
                "String" => "string",
                "i32" => "int",
                "i64" => "bigint",
                "f64" => "float",
                "bool" => "bool",
                "chrono::NaiveDateTime" => "datetime",
                "chrono::NaiveDate" => "date",
                "serde_json::Value" => "json",
                _ => "string",
            };
            Some((name, kind.to_string(), nullable))
        })
        .collect()
}

/// snake_case → "Title Case" label.
fn humanize_label(name: &str) -> String {
    let s = name.replace('_', " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// Rust expression yielding a `String` for displaying `it.<name>` (list/edit).
fn display_expr(name: &str, nullable: bool) -> String {
    if nullable {
        format!("it.{name}.as_ref().map(|v| v.to_string()).unwrap_or_default()")
    } else {
        format!("it.{name}.to_string()")
    }
}

/// Coerce a submitted form string into the field's base (non-null) Rust value.
fn base_coerce(name: &str, kind: &str) -> String {
    match kind {
        "int" => format!(
            "form.value(\"{name}\").and_then(|v| v.parse::<i32>().ok()).unwrap_or_default()"
        ),
        "bigint" => format!(
            "form.value(\"{name}\").and_then(|v| v.parse::<i64>().ok()).unwrap_or_default()"
        ),
        "float" => format!(
            "form.value(\"{name}\").and_then(|v| v.parse::<f64>().ok()).unwrap_or_default()"
        ),
        "bool" => format!(
            "form.value(\"{name}\").map(|v| v == \"true\" || v == \"on\" || v == \"1\").unwrap_or(false)"
        ),
        "datetime" => format!(
            "form.value(\"{name}\").and_then(|v| v.parse().ok()).unwrap_or_else(|| chrono::Utc::now().naive_utc())"
        ),
        "date" => format!(
            "form.value(\"{name}\").and_then(|v| v.parse().ok()).unwrap_or_else(|| chrono::Utc::now().naive_utc().date())"
        ),
        "json" => format!(
            "form.value(\"{name}\").and_then(|v| serde_json::from_str(v).ok()).unwrap_or(serde_json::Value::Null)"
        ),
        _ => format!("form.value(\"{name}\").unwrap_or_default().to_string()"),
    }
}

/// Coerce a submitted form string into an `Option<base>` (nullable field).
fn opt_coerce(name: &str, kind: &str) -> String {
    match kind {
        "string" | "text" => {
            format!("form.value(\"{name}\").filter(|v| !v.is_empty()).map(|v| v.to_string())")
        }
        "bool" => {
            format!("form.value(\"{name}\").map(|v| v == \"true\" || v == \"on\" || v == \"1\")")
        }
        "json" => format!(
            "form.value(\"{name}\").filter(|v| !v.is_empty()).and_then(|v| serde_json::from_str(v).ok())"
        ),
        _ => {
            format!("form.value(\"{name}\").filter(|v| !v.is_empty()).and_then(|v| v.parse().ok())")
        }
    }
}

// ─── Helpers ───

fn snake(name: &str) -> String {
    // PascalCase → snake_case: "PostComment" → "post_comment"
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn plural(name: &str) -> String {
    let s = snake(name);
    if s.ends_with('s') || s.ends_with('x') || s.ends_with('z') {
        format!("{s}es")
    } else if s.ends_with('y') && !s.ends_with("ey") && !s.ends_with("ay") && !s.ends_with("oy") {
        format!("{}ies", &s[..s.len() - 1])
    } else {
        format!("{s}s")
    }
}

fn find_app_dir(root: &Path) -> Result<std::path::PathBuf, String> {
    // Try "app/src" first, then "api/src"
    for dir in &["app/src", "api/src"] {
        let path = root.join(dir);
        if path.exists() {
            return Ok(path);
        }
    }
    Err("Cannot find app/src or api/src directory".to_string())
}

fn write_file(path: &Path, content: &str, opts: GenOptions) -> Result<(), String> {
    let exists = path.exists();
    let shown = path.display().to_string();

    if opts.dry_run {
        let verb = if exists { "écraserait" } else { "créerait" };
        println!("  {} {} {}", "○".dimmed(), verb.dimmed(), shown.dimmed());
        return Ok(());
    }
    if exists && !opts.force {
        println!(
            "  {} {} {}",
            "⊘".yellow(),
            shown.dimmed(),
            "(existe déjà — `--force` pour écraser)".dimmed()
        );
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create dir {}: {e}", parent.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let mark = if exists {
        "↻".yellow()
    } else {
        "✓".green()
    };
    println!("  {} {}", mark, shown.dimmed());
    Ok(())
}

/// Ensure `app/src/main.rs` declares `pub mod <module>;` so generated files are
/// compiled into the crate. Inserts it before `pub mod controller;` if present.
fn ensure_main_module(app_dir: &Path, module: &str, opts: GenOptions) -> Result<(), String> {
    let main_rs = app_dir.join("main.rs");
    let content = match fs::read_to_string(&main_rs) {
        Ok(c) => c,
        Err(_) => return Ok(()), // no main.rs (e.g. lib project) — nothing to wire
    };
    let line = format!("pub mod {module};");
    if content.contains(&line) {
        return Ok(());
    }
    if opts.dry_run {
        println!("  {} app/src/main.rs (ajouterait `{}`)", "○".dimmed(), line);
        return Ok(());
    }
    let new_content = if let Some(pos) = content.find("pub mod controller;") {
        let (head, tail) = content.split_at(pos);
        format!("{head}{line}\n{tail}")
    } else {
        format!("{line}\n{content}")
    };
    fs::write(&main_rs, new_content)
        .map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
    println!("  {} app/src/main.rs (added `{}`)", "✓".green(), line);
    Ok(())
}

/// Mount a controller in `main.rs` by inserting a `.nest(...)` line right before
/// the given marker comment. Falls back to printing a manual instruction when the
/// marker is absent (older projects scaffolded before route auto-wiring).
fn wire_route(
    app_dir: &Path,
    marker: &str,
    nest_line: &str,
    manual_hint: &str,
    opts: GenOptions,
) -> Result<(), String> {
    let main_rs = app_dir.join("main.rs");
    let content = match fs::read_to_string(&main_rs) {
        Ok(c) => c,
        Err(_) => return Ok(()), // no main.rs (e.g. lib project)
    };
    if content.contains(nest_line.trim()) {
        return Ok(()); // already mounted
    }
    let Some(pos) = content.find(marker) else {
        println!("  {} Mount it: {}", "→".blue(), manual_hint.dimmed());
        return Ok(());
    };
    if opts.dry_run {
        println!(
            "  {} app/src/main.rs (monterait {})",
            "○".dimmed(),
            nest_line.trim()
        );
        return Ok(());
    }
    let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..pos]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let insert = format!("{indent}{}\n", nest_line.trim());
    let mut new_content = String::with_capacity(content.len() + insert.len());
    new_content.push_str(&content[..line_start]);
    new_content.push_str(&insert);
    new_content.push_str(&content[line_start..]);
    fs::write(&main_rs, new_content)
        .map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
    println!(
        "  {} app/src/main.rs (mounted {})",
        "✓".green(),
        nest_line.trim()
    );
    Ok(())
}

/// Insert a line right before a marker comment in `main.rs` (idempotent). Unlike
/// `wire_route`, it's silent when the marker is absent (older skeletons simply
/// don't get OpenAPI aggregation).
fn append_at_marker(
    app_dir: &Path,
    marker: &str,
    line: &str,
    opts: GenOptions,
) -> Result<(), String> {
    let main_rs = app_dir.join("main.rs");
    let content = match fs::read_to_string(&main_rs) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    if content.contains(line.trim()) {
        return Ok(());
    }
    let Some(pos) = content.find(marker) else {
        return Ok(());
    };
    if opts.dry_run {
        println!(
            "  {} app/src/main.rs (ajouterait OpenAPI pour {})",
            "○".dimmed(),
            line.split("::").nth(1).unwrap_or("controller")
        );
        return Ok(());
    }
    let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..pos]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    let insert = format!("{indent}{}\n", line.trim());
    let mut new_content = String::with_capacity(content.len() + insert.len());
    new_content.push_str(&content[..line_start]);
    new_content.push_str(&insert);
    new_content.push_str(&content[line_start..]);
    fs::write(&main_rs, new_content)
        .map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
    println!("  {} app/src/main.rs (OpenAPI route)", "✓".green());
    Ok(())
}

fn append_mod(mod_file: &Path, module_name: &str, opts: GenOptions) -> Result<(), String> {
    let content = fs::read_to_string(mod_file).unwrap_or_default();
    let mod_line = format!("pub mod {};", module_name);
    if content.contains(&mod_line) {
        return Ok(());
    }
    if opts.dry_run {
        println!(
            "  {} {} (ajouterait `{}`)",
            "○".dimmed(),
            mod_file.display().to_string().dimmed(),
            mod_line
        );
        return Ok(());
    }
    let new_content = if content.trim().is_empty() {
        format!("{mod_line}\n")
    } else {
        format!("{content}\n{mod_line}\n")
    };
    fs::write(mod_file, new_content)
        .map_err(|e| format!("Cannot update {}: {e}", mod_file.display()))?;
    println!(
        "  {} {} (added `{}`)",
        "✓".green(),
        mod_file.display().to_string().dimmed(),
        mod_line
    );
    Ok(())
}

// ─── Generators ───

fn generate_entity(
    name: &str,
    field_specs: &[String],
    root: &Path,
    opts: GenOptions,
) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let table = plural(name);
    let file_name = snake(name);
    let driver = detect_driver(root);
    let fields = parse_fields(field_specs)?;
    let has_title = fields.iter().any(|f| f.name == "title");

    // Entity struct fields.
    let struct_fields = fields
        .iter()
        .map(|f| format!("    pub {}: {},", f.name, f.rust_ty))
        .collect::<Vec<_>>()
        .join("\n");

    // Insert DTO: a `slug` field auto-derives from `title` when both exist.
    let new_fields = fields
        .iter()
        .map(|f| {
            if f.name == "slug" && has_title {
                format!(
                    "    #[slug_from(\"title\")]\n    pub {}: {},",
                    f.name, f.rust_ty
                )
            } else {
                format!("    pub {}: {},", f.name, f.rust_ty)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Update DTO: every field optional.
    let update_fields = fields
        .iter()
        .map(|f| format!("    pub {}: Option<{}>,", f.name, f.base_ty))
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        r#"use serde::{{Deserialize, Serialize}};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct {name} {{
    pub id: i64,
{struct_fields}
    pub created: chrono::NaiveDateTime,
    pub updated: chrono::NaiveDateTime,
}}

/// DTO for inserting a new {name}
#[derive(Debug, Deserialize, karbon::Insertable)]
#[table_name("{table}")]
pub struct New{name} {{
{new_fields}
}}

/// DTO for updating a {name}
#[derive(Debug, Default, Deserialize, karbon::Updatable)]
#[table_name("{table}")]
pub struct Update{name} {{
    #[primary_key]
    pub id: i64,
{update_fields}
}}
"#
    );

    let entity_path = app_dir.join(format!("entity/{file_name}.rs"));
    write_file(&entity_path, &content, opts)?;
    append_mod(&app_dir.join("entity/mod.rs"), &file_name, opts)?;
    ensure_main_module(&app_dir, "entity", opts)?;

    // No migration is written here: the entity is the source of truth. Run
    // `karbon migrate diff` to generate the migration from the entity ↔ DB diff.
    let field_summary = fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "\n  {} Entity {} generated [{}] ({:?})",
        "✓".green().bold(),
        name.bold(),
        field_summary.dimmed(),
        driver
    );
    println!(
        "  {} {} pour générer la migration (compare entités ↔ base)\n",
        "→".blue(),
        "karbon migrate diff".bold()
    );
    Ok(())
}

fn generate_controller(name: &str, root: &Path, opts: GenOptions) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let prefix = format!("/{}", plural(name).replace('_', "-"));

    let content = format!(
        r#"use axum::extract::Path;
use axum::response::IntoResponse;
use axum::Json;

use karbon::error::AppResult;
use karbon::security::AuthGuard;

pub struct {name}Controller;

// Stub controller — add `State(state): State<karbon::http::AppState>` to a handler
// to access the database, config, etc. (`use axum::extract::State;`).
#[karbon::controller(prefix = "{prefix}", role = "ROLE_ADMIN")]
impl {name}Controller {{
    #[karbon::get("/")]
    async fn list(auth: AuthGuard) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": "TODO: list {}" }})))
    }}

    #[karbon::get("/{{id}}", id = "int:min=1")]
    async fn show(auth: AuthGuard, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: show {{id}}") }})))
    }}

    #[karbon::post("/")]
    async fn create(auth: AuthGuard) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": "TODO: create" }})))
    }}

    #[karbon::put("/{{id}}", id = "int:min=1")]
    async fn update(auth: AuthGuard, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: update {{id}}") }})))
    }}

    #[karbon::delete("/{{id}}", id = "int:min=1")]
    async fn delete(auth: AuthGuard, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: delete {{id}}") }})))
    }}
}}
"#,
        plural(name)
    );

    let controller_path = app_dir.join(format!("controller/{file_name}_controller.rs"));
    write_file(&controller_path, &content, opts)?;
    append_mod(
        &app_dir.join("controller/mod.rs"),
        &format!("{file_name}_controller"),
        opts,
    )?;
    ensure_main_module(&app_dir, "controller", opts)?;
    wire_route(
        &app_dir,
        "// karbon:api-routes",
        &format!(
            ".nest(controller::{file_name}_controller::{name}Controller::prefix(), controller::{file_name}_controller::{name}Controller::router())"
        ),
        &format!(".nest({name}Controller::prefix(), {name}Controller::router())"),
        opts,
    )?;
    append_at_marker(
        &app_dir,
        "// karbon:openapi-api",
        &format!(
            "routes.extend(controller::{file_name}_controller::{name}Controller::openapi_paths().into_iter().map(|(m, p)| (m.to_string(), format!(\"{{API_PREFIX}}{{p}}\"))));"
        ),
        opts,
    )?;

    println!(
        "\n  {} Controller {} generated (prefix: {})\n",
        "✓".green().bold(),
        name.bold(),
        format!("{API_PREFIX}{prefix}").cyan()
    );
    Ok(())
}

const API_PREFIX: &str = "/api/v1";

fn generate_crud(
    name: &str,
    field_specs: &[String],
    root: &Path,
    opts: GenOptions,
) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let table = plural(name);
    let fields = parse_fields(field_specs)?;

    let has_slug = fields.iter().any(|f| f.name == "slug");
    let searchable: Vec<&str> = fields
        .iter()
        .filter(|f| f.kind == "string" || f.kind == "text")
        .map(|f| f.name.as_str())
        .collect();
    let quoted = |cols: &[&str]| {
        cols.iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let search_cols = format!("&[{}]", quoted(&searchable));
    let mut sort_cols = vec!["id"];
    sort_cols.extend(&searchable);
    sort_cols.push("created");
    let allowed_sorts = format!("&[{}]", quoted(&sort_cols));

    // 1. Entity
    generate_entity(name, field_specs, root, opts)?;

    // 2. Repository (driver-agnostic — uses the abstract `DbPool`).
    let repo_content = format!(
        r#"use crate::entity::{file_name}::{{{name}, New{name}, Update{name}}};
use karbon::db::{{CrudRepository, DbPool, PaginatedQuery, PaginationParams}};
use karbon::error::AppResult;

impl CrudRepository for {name} {{
    const TABLE: &'static str = "{table}";
    const ENTITY_NAME: &'static str = "{name}";
    const HAS_SLUG: bool = {has_slug};
}}

pub struct {name}Repository;

impl {name}Repository {{
    pub async fn find_paginated(
        pool: &DbPool,
        params: &PaginationParams,
    ) -> AppResult<(Vec<{name}>, u64)> {{
        PaginatedQuery::<{name}>::new("SELECT * FROM {table}")
            .allowed_sorts({allowed_sorts})
            .default_sort("id")
            .search_columns({search_cols})
            .execute(pool, params)
            .await
    }}

    pub async fn create(pool: &DbPool, dto: New{name}) -> AppResult<u64> {{
        dto.insert(pool).await
    }}

    pub async fn update(pool: &DbPool, dto: Update{name}) -> AppResult<u64> {{
        dto.update(pool).await
    }}
}}
"#
    );

    let repo_path = app_dir.join(format!("repository/{file_name}_repository.rs"));
    write_file(&repo_path, &repo_content, opts)?;

    // Ensure repository/mod.rs exists
    let repo_mod = app_dir.join("repository/mod.rs");
    if !repo_mod.exists() {
        write_file(&repo_mod, "", opts)?;
    }
    append_mod(&repo_mod, &format!("{file_name}_repository"), opts)?;
    ensure_main_module(&app_dir, "repository", opts)?;

    // Ensure entity is exported properly
    let entity_mod = app_dir.join("entity/mod.rs");
    let entity_mod_content = fs::read_to_string(&entity_mod).unwrap_or_default();
    let use_line = format!("pub use {file_name}::{{{name}, New{name}, Update{name}}};");
    if !entity_mod_content.contains(&use_line) && !opts.dry_run {
        let updated = format!("{entity_mod_content}\n{use_line}\n");
        fs::write(&entity_mod, updated).ok();
    }

    // 3. Controller
    generate_controller(name, root, opts)?;

    println!(
        "\n  {} Full CRUD for {} generated (entity + repo + controller)\n",
        "✓".green().bold(),
        name.bold()
    );
    Ok(())
}

/// Generate an auto-admin controller (list + create/edit forms + delete) for an
/// entity. Assumes the entity + `New*`/`Update*` DTOs exist (run after `g crud`).
fn generate_admin(name: &str, root: &Path, opts: GenOptions) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let prefix = format!("/admin/{}", plural(name).replace('_', "-"));

    // The admin controller imports the entity + its `New*`/`Update*` DTOs and calls
    // `CrudRepository` methods on it. Fail fast (before writing broken code) if the
    // entity hasn't been scaffolded yet.
    let entity_file = app_dir.join(format!("entity/{file_name}.rs"));
    if !entity_file.exists() {
        return Err(format!(
            "entity `{name}` not found ({}).\n  Run `{}` first, then `{}`.",
            entity_file.display(),
            format!("karbon generate crud {name}").bold(),
            format!("karbon generate admin {name}").bold(),
        ));
    }
    // Field-aware admin: read the entity's fields and drive the list, form and
    // New/Update construction from them.
    let entity_src = fs::read_to_string(&entity_file).unwrap_or_default();
    let fields = parse_entity_fields(&entity_src);
    if fields.is_empty() {
        return Err(format!(
            "could not read fields from entity `{name}` ({}).",
            entity_file.display()
        ));
    }

    let list_headers = fields
        .iter()
        .map(|(n, _, _)| format!("<th>{}</th>", humanize_label(n)))
        .collect::<String>();
    let list_cells = fields
        .iter()
        .map(|(n, _, nl)| {
            format!(
                "            cells.push_str(&format!(\"<td>{{}}</td>\", esc(&({}))));",
                display_expr(n, *nl)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let form_fields = fields
        .iter()
        .map(|(n, _, nl)| {
            let req = if *nl { "" } else { ".required()" };
            format!(
                "        .add(Field::text(\"{n}\", \"{}\"){req})",
                humanize_label(n)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let form_fill = fields
        .iter()
        .map(|(n, _, nl)| {
            format!(
                "        values.insert(\"{n}\".to_string(), {});",
                display_expr(n, *nl)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let new_fields = fields
        .iter()
        .map(|(n, k, nl)| {
            let expr = if *nl {
                opt_coerce(n, k)
            } else {
                base_coerce(n, k)
            };
            format!("            {n}: {expr},")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let update_fields = fields
        .iter()
        .map(|(n, k, _)| format!("            {n}: Some({}),", base_coerce(n, k)))
        .collect::<Vec<_>>()
        .join("\n");

    // Token-replacement template (avoids escaping the many braces in Rust+HTML+JS).
    let content = ADMIN_CONTROLLER_TEMPLATE
        .replace("%LIST_HEADERS%", &list_headers)
        .replace("%LIST_CELLS%", &list_cells)
        .replace("%FORM_FIELDS%", &form_fields)
        .replace("%FORM_FILL%", &form_fill)
        .replace("%NEW_FIELDS%", &new_fields)
        .replace("%UPDATE_FIELDS%", &update_fields)
        .replace("%NAME%", name)
        .replace("%FILE%", &file_name)
        .replace("%PREFIX%", &prefix);

    let controller_path = app_dir.join(format!("controller/{file_name}_admin.rs"));
    write_file(&controller_path, &content, opts)?;
    append_mod(
        &app_dir.join("controller/mod.rs"),
        &format!("{file_name}_admin"),
        opts,
    )?;
    ensure_main_module(&app_dir, "controller", opts)?;
    ensure_main_module(&app_dir, "entity", opts)?;
    wire_route(
        &app_dir,
        "// karbon:routes",
        &format!(
            ".nest(controller::{file_name}_admin::{name}AdminController::prefix(), controller::{file_name}_admin::{name}AdminController::router())"
        ),
        &format!(".nest({name}AdminController::prefix(), {name}AdminController::router())"),
        opts,
    )?;
    append_at_marker(
        &app_dir,
        "// karbon:openapi-root",
        &format!(
            "routes.extend(controller::{file_name}_admin::{name}AdminController::openapi_paths().into_iter().map(|(m, p)| (m.to_string(), p.to_string())));"
        ),
        opts,
    )?;

    println!(
        "\n  {} Admin for {} generated (prefix: {})\n",
        "✓".green().bold(),
        name.bold(),
        prefix.cyan()
    );
    Ok(())
}

const ADMIN_CONTROLLER_TEMPLATE: &str = r##"use std::collections::HashMap;

use axum::extract::{Form as AxumForm, Path, State};
use axum::response::{Html, IntoResponse, Redirect, Response};

use karbon::db::CrudRepository;
use karbon::error::AppResult;
use karbon::form::{Field, Form};
use karbon::http::AppState;
use karbon::security::AuthGuard;

use crate::entity::%FILE%::{%NAME%, New%NAME%, Update%NAME%};

const PREFIX: &str = "%PREFIX%";

pub struct %NAME%AdminController;

#[karbon::controller(prefix = "%PREFIX%", role = "ROLE_ADMIN")]
impl %NAME%AdminController {
    #[karbon::get("/")]
    async fn index(auth: AuthGuard, State(state): State<AppState>) -> AppResult<Response> {
        let items = %NAME%::find_all(state.db.pool(), Some(("id", "DESC"))).await?;
        let mut rows = String::new();
        for it in &items {
            let mut cells = String::new();
%LIST_CELLS%
            rows.push_str(&format!(
                "<tr><td>{id}</td>{cells}<td><a href=\"{p}/{id}/edit\">Edit</a> <form method=\"post\" action=\"{p}/{id}/delete\" style=\"display:inline\"><button>Delete</button></form></td></tr>",
                id = it.id, p = PREFIX, cells = cells
            ));
        }
        let body = format!(
            "<a class=\"btn\" href=\"{p}/new\">+ New</a><table><thead><tr><th>ID</th>%LIST_HEADERS%<th></th></tr></thead><tbody>{rows}</tbody></table>",
            p = PREFIX
        );
        Ok(Html(layout("%NAME% admin", &body)).into_response())
    }

    #[karbon::get("/new")]
    async fn new_form(auth: AuthGuard) -> AppResult<Response> {
        let body = build_form(None).render_form(PREFIX, "Create");
        Ok(Html(layout("New %NAME%", &body)).into_response())
    }

    #[karbon::post("/")]
    async fn create(
        auth: AuthGuard,
        State(state): State<AppState>,
        AxumForm(data): AxumForm<HashMap<String, String>>,
    ) -> AppResult<Response> {
        let mut form = build_form(None);
        if !form.handle(&data) {
            let body = form.render_form(PREFIX, "Create");
            return Ok(Html(layout("New %NAME%", &body)).into_response());
        }
        New%NAME% {
%NEW_FIELDS%
        }
        .insert(state.db.pool())
        .await?;
        Ok(Redirect::to(PREFIX).into_response())
    }

    #[karbon::get("/{id}/edit", id = "int:min=1")]
    async fn edit_form(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> AppResult<Response> {
        let Some(item) = %NAME%::find_by_id(state.db.pool(), id).await? else {
            return Ok(Redirect::to(PREFIX).into_response());
        };
        let action = format!("{}/{}", PREFIX, id);
        let body = build_form(Some(&item)).render_form(&action, "Save");
        Ok(Html(layout("Edit %NAME%", &body)).into_response())
    }

    #[karbon::post("/{id}", id = "int:min=1")]
    async fn update(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
        AxumForm(data): AxumForm<HashMap<String, String>>,
    ) -> AppResult<Response> {
        let mut form = build_form(None);
        if !form.handle(&data) {
            let action = format!("{}/{}", PREFIX, id);
            let body = form.render_form(&action, "Save");
            return Ok(Html(layout("Edit %NAME%", &body)).into_response());
        }
        Update%NAME% {
            id,
%UPDATE_FIELDS%
        }
        .update(state.db.pool())
        .await?;
        Ok(Redirect::to(PREFIX).into_response())
    }

    #[karbon::post("/{id}/delete", id = "int:min=1")]
    async fn delete(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> AppResult<Response> {
        %NAME%::delete(state.db.pool(), id).await?;
        Ok(Redirect::to(PREFIX).into_response())
    }
}

fn build_form(item: Option<&%NAME%>) -> Form {
    let mut form = Form::new()
%FORM_FIELDS%;
    if let Some(it) = item {
        let mut values = HashMap::new();
%FORM_FILL%
        form = form.fill(&values);
    }
    form
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn layout(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{t}</title>
<style>body{{font-family:system-ui,sans-serif;max-width:920px;margin:2rem auto;padding:0 1rem;color:#1a1a1a}}h1{{font-size:1.4rem}}table{{width:100%;border-collapse:collapse;margin-top:1rem}}th,td{{text-align:left;padding:.5rem;border-bottom:1px solid #e5e5e5}}.form-field{{margin:.75rem 0}}label{{display:block;font-weight:600;margin-bottom:.25rem}}input,textarea,select{{width:100%;padding:.5rem;border:1px solid #ccc;border-radius:6px}}.form-error{{color:#c0392b;font-size:.85rem;margin-top:.25rem}}.has-error input{{border-color:#c0392b}}.btn,button{{display:inline-block;padding:.45rem .9rem;background:#111;color:#fff;border:0;border-radius:6px;cursor:pointer;text-decoration:none;font-size:.9rem}}</style>
</head><body><h1>{t}</h1>{b}
<script>
document.addEventListener('submit', async function(e) {{
  var form = e.target;
  if ((form.getAttribute('method') || '').toLowerCase() !== 'post') return;
  e.preventDefault();
  var m = document.cookie.match(/csrf_token=([^;]+)/);
  var res = await fetch(form.getAttribute('action'), {{
    method: 'POST',
    headers: {{ 'X-CSRF-Token': m ? m[1] : '', 'Content-Type': 'application/x-www-form-urlencoded' }},
    body: new URLSearchParams(new FormData(form)),
    redirect: 'follow'
  }});
  if (res.redirected) window.location = res.url; else window.location.reload();
}});
</script>
</body></html>"#,
        t = esc(title),
        b = body
    )
}
"##;
