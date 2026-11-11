use colored::Colorize;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::Path;

/// Field types the assistant offers (matches `parse_fields`).
const FIELD_TYPES: &[&str] = &["string", "text", "int", "bigint", "float", "bool", "datetime", "date", "json"];

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
    println!("\n  {} Field assistant — leave the name empty to finish\n", "▸".cyan().bold());
    let mut fields: Vec<String> = Vec::new();
    loop {
        let name = prompt(&format!("  Field #{} — name: ", fields.len() + 1))?;
        let name = name.trim();
        if name.is_empty() {
            break;
        }
        let name = snake(name);

        let ty = loop {
            let raw = prompt("    type [string]: ")?;
            let t = raw.trim();
            let t = if t.is_empty() { "string" } else { t };
            if FIELD_TYPES.contains(&t) {
                break t.to_string();
            }
            println!("    {} unknown type. Available: {}", "✗".red(), FIELD_TYPES.join(", ").dimmed());
        };

        let nullable =
            matches!(prompt("    nullable? (y/N): ")?.trim().to_lowercase().as_str(), "o" | "oui" | "y" | "yes");

        let spec = format!("{name}:{ty}{}", if nullable { "?" } else { "" });
        println!("    {} {}\n", "✓".green(), spec.cyan());
        fields.push(spec);
    }
    if fields.is_empty() {
        println!("  {} no field — using the defaults (title, slug)\n", "ℹ".blue());
    }
    Ok(fields)
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin()
        .read_line(&mut s)
        .map_err(|e| format!("Cannot read stdin: {e}"))?;
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

pub fn run(kind: &str, name: Option<&str>, fields: &[String], root: &Path, opts: GenOptions) -> Result<(), String> {
    // `auth` always creates a `User`: it takes no name. Every other generator needs one.
    let name = match (kind, name) {
        ("auth", None | Some("User")) => "User",
        ("auth", Some(other)) => {
            return Err(format!("`karbon generate auth` takes no name: it creates a `User` entity (got `{other}`)"));
        }
        (_, Some(name)) => name,
        (_, None) => return Err(format!("Missing name — usage: karbon generate {kind} <Name>")),
    };

    // The name becomes a file path and a Rust type, and Studio's terminal forwards it from
    // an HTTP request: `../../x` must never reach a `join`.
    if !is_identifier(name) {
        return Err(format!(
            "Invalid name `{name}` — expected an identifier such as `Post` or `PostComment` \
             (letters, digits and `_`, starting with a letter)"
        ));
    }

    println!(
        "\n{}  generate {} {}{}\n",
        "▲ karbon".bold().red(),
        kind.bold(),
        name.bold().cyan(),
        if opts.dry_run { "  (dry-run)".yellow().to_string() } else { String::new() }
    );

    match kind {
        "entity" => generate_entity(name, &resolve_fields(fields, opts)?, root, opts),
        "controller" => generate_controller(name, root, opts),
        "crud" => generate_crud(name, &resolve_fields(fields, opts)?, root, opts),
        "admin" => generate_admin(name, root, opts),
        "auth" => generate_auth(root, opts),
        _ => Err(format!("Unknown generator '{}'. Available: entity, controller, crud, admin, auth", kind)),
    }
}

// ─── Fields & drivers ───

#[derive(Debug, Clone, Copy, PartialEq)]
enum Driver {
    Mysql,
    Postgres,
    Sqlite,
}

/// The driver the project compiles with (see [`crate::db_url::compiled_driver`]).
fn detect_driver(root: &Path) -> Driver {
    match crate::db_url::compiled_driver(root) {
        "postgres" => Driver::Postgres,
        "sqlite" => Driver::Sqlite,
        _ => Driver::Mysql,
    }
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
            // Written as-is into the generated Rust source.
            if !is_identifier(raw_name) {
                return Err(format!(
                    "Invalid field name `{raw_name}` in `{spec}` — letters, digits and `_`, \
                     starting with a letter"
                ));
            }
            let nullable = raw_ty.ends_with('?');
            let kind = raw_ty.trim_end_matches('?').to_lowercase();
            let base_ty = rust_base_type(&kind).ok_or_else(|| {
                format!("Unknown type `{kind}` in `{spec}` (string text int bigint float bool datetime date json)")
            })?;
            let rust_ty = if nullable { format!("Option<{base_ty}>") } else { base_ty.to_string() };
            Ok(Field { name: snake(raw_name), rust_ty, base_ty: base_ty.to_string(), kind })
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
            // A trailing comment (`// karbon:unique`, `// karbon:text`) is not part of the type.
            let (line, comment) = line.split_once("//").unwrap_or((line, ""));
            let long_text = comment.contains("karbon:text");
            let line = line.trim().trim_end_matches(',');
            let rest = line.strip_prefix("pub ")?;
            let (name, ty) = rest.split_once(':')?;
            let name = name.trim().to_string();
            if matches!(name.as_str(), "id" | "created" | "updated") {
                return None;
            }
            let ty = ty.trim();
            let (nullable, base) = match ty.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
                Some(inner) => (true, inner.trim()),
                None => (false, ty),
            };
            let kind = match base {
                "String" if long_text => "text",
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
        "int" => format!("form.value(\"{name}\").and_then(|v| v.parse::<i32>().ok()).unwrap_or_default()"),
        "bigint" => format!("form.value(\"{name}\").and_then(|v| v.parse::<i64>().ok()).unwrap_or_default()"),
        "float" => format!("form.value(\"{name}\").and_then(|v| v.parse::<f64>().ok()).unwrap_or_default()"),
        "bool" => {
            format!("form.value(\"{name}\").map(|v| v == \"true\" || v == \"on\" || v == \"1\").unwrap_or(false)")
        }
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
        "json" => {
            format!("form.value(\"{name}\").filter(|v| !v.is_empty()).and_then(|v| serde_json::from_str(v).ok())")
        }
        _ => {
            format!("form.value(\"{name}\").filter(|v| !v.is_empty()).and_then(|v| v.parse().ok())")
        }
    }
}

// ─── Helpers ───

/// ASCII letters, digits and `_`, starting with a letter: safe as a file name and as a
/// Rust identifier — no separator, no `..`, nothing a path or a source file interprets.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

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
        let verb = if exists { "would overwrite" } else { "would create" };
        println!("  {} {} {}", "○".dimmed(), verb.dimmed(), shown.dimmed());
        return Ok(());
    }
    if exists && !opts.force {
        println!("  {} {} {}", "⊘".yellow(), shown.dimmed(), "(already exists — `--force` to overwrite)".dimmed());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create dir {}: {e}", parent.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let mark = if exists { "↻".yellow() } else { "✓".green() };
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
        println!("  {} app/src/main.rs (would add `{}`)", "○".dimmed(), line);
        return Ok(());
    }
    let new_content = if let Some(pos) = content.find("pub mod controller;") {
        let (head, tail) = content.split_at(pos);
        format!("{head}{line}\n{tail}")
    } else {
        format!("{line}\n{content}")
    };
    fs::write(&main_rs, new_content).map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
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
        println!("  {} app/src/main.rs (would mount {})", "○".dimmed(), nest_line.trim());
        return Ok(());
    }
    let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..pos].chars().take_while(|c| c.is_whitespace()).collect();
    let insert = format!("{indent}{}\n", nest_line.trim());
    let mut new_content = String::with_capacity(content.len() + insert.len());
    new_content.push_str(&content[..line_start]);
    new_content.push_str(&insert);
    new_content.push_str(&content[line_start..]);
    fs::write(&main_rs, new_content).map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
    println!("  {} app/src/main.rs (mounted {})", "✓".green(), nest_line.trim());
    Ok(())
}

/// Insert a line right before a marker comment in `main.rs` (idempotent). Unlike
/// `wire_route`, it's silent when the marker is absent (older skeletons simply
/// don't get OpenAPI aggregation).
fn append_at_marker(app_dir: &Path, marker: &str, line: &str, opts: GenOptions) -> Result<(), String> {
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
            "  {} app/src/main.rs (would add OpenAPI for {})",
            "○".dimmed(),
            line.split("::").nth(1).unwrap_or("controller")
        );
        return Ok(());
    }
    let line_start = content[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..pos].chars().take_while(|c| c.is_whitespace()).collect();
    let insert = format!("{indent}{}\n", line.trim());
    let mut new_content = String::with_capacity(content.len() + insert.len());
    new_content.push_str(&content[..line_start]);
    new_content.push_str(&insert);
    new_content.push_str(&content[line_start..]);
    fs::write(&main_rs, new_content).map_err(|e| format!("Cannot update {}: {e}", main_rs.display()))?;
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
        println!("  {} {} (would add `{}`)", "○".dimmed(), mod_file.display().to_string().dimmed(), mod_line);
        return Ok(());
    }
    let new_content = if content.trim().is_empty() {
        format!("{mod_line}\n")
    } else {
        format!("{content}\n{mod_line}\n")
    };
    fs::write(mod_file, new_content).map_err(|e| format!("Cannot update {}: {e}", mod_file.display()))?;
    println!("  {} {} (added `{}`)", "✓".green(), mod_file.display().to_string().dimmed(), mod_line);
    Ok(())
}

// ─── Generators ───

fn generate_entity(name: &str, field_specs: &[String], root: &Path, opts: GenOptions) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let table = plural(name);
    let file_name = snake(name);
    let driver = detect_driver(root);
    let fields = parse_fields(field_specs)?;
    let has_title = fields.iter().any(|f| f.name == "title");

    // Entity struct fields.
    let struct_fields = fields
        .iter()
        .map(|f| {
            // `text` and `string` are both a Rust `String`: the marker is what tells
            // `migrate diff` (TEXT column) and `generate admin` (textarea) apart.
            let marker = if f.kind == "text" { " // karbon:text" } else { "" };
            format!("    pub {}: {},{marker}", f.name, f.rust_ty)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Insert DTO: a `slug` field auto-derives from `title` when both exist.
    let new_fields = fields
        .iter()
        .map(|f| {
            if f.name == "slug" && has_title {
                // Optional in a request body: left empty, it derives from the title.
                format!("    #[serde(default)]\n    #[slug_from(\"title\")]\n    pub {}: {},", f.name, f.rust_ty)
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
    /// Taken from the URL by the API controller, hence optional in a request body.
    #[serde(default)]
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
    let field_summary = fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", ");
    println!("\n  {} Entity {} generated [{}] ({:?})", "✓".green().bold(), name.bold(), field_summary.dimmed(), driver);
    println!(
        "  {} {} to generate the migration (compares entities ↔ database)\n",
        "→".blue(),
        "karbon migrate diff".bold()
    );
    Ok(())
}

fn generate_controller(name: &str, root: &Path, opts: GenOptions) -> Result<(), String> {
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

    write_controller(name, &content, root, opts)
}

/// Writes `controller/<name>_controller.rs`, declares the module and mounts the controller
/// under the API prefix (routes + OpenAPI).
fn write_controller(name: &str, content: &str, root: &Path, opts: GenOptions) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let prefix = format!("/{}", plural(name).replace('_', "-"));

    let controller_path = app_dir.join(format!("controller/{file_name}_controller.rs"));
    write_file(&controller_path, content, opts)?;
    append_mod(&app_dir.join("controller/mod.rs"), &format!("{file_name}_controller"), opts)?;
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

fn generate_crud(name: &str, field_specs: &[String], root: &Path, opts: GenOptions) -> Result<(), String> {
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
    let quoted = |cols: &[&str]| cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
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

    // 3. Controller: a working REST API over the repository, not a stub.
    let prefix = format!("/{}", plural(name).replace('_', "-"));
    let controller = CRUD_CONTROLLER_TEMPLATE
        .replace("%NAME%", name)
        .replace("%FILE%", &file_name)
        .replace("%PREFIX%", &prefix)
        .replace("%LABEL%", &humanize_label(&file_name));
    write_controller(name, &controller, root, opts)?;

    println!("\n  {} Full CRUD for {} generated (entity + repo + controller)\n", "✓".green().bold(), name.bold());
    Ok(())
}

const CRUD_CONTROLLER_TEMPLATE: &str = r##"use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use karbon::db::{CrudRepository, Paginated, PaginationParams};
use karbon::error::{AppError, AppResult};
use karbon::http::AppState;
use karbon::security::AuthGuard;

use crate::entity::%FILE%::{%NAME%, New%NAME%, Update%NAME%};
use crate::repository::%FILE%_repository::%NAME%Repository;

pub struct %NAME%Controller;

/// REST API for %NAME%, generated by `karbon generate crud` — this file is yours.
///
/// Everything requires `ROLE_ADMIN`, like the controller's `role` says. To make reading
/// public, remove `role` from the line below and put `#[karbon::require_role("ROLE_ADMIN")]`
/// on `create`, `update` and `delete` (their `auth: AuthGuard` parameter is what it checks).
#[karbon::controller(prefix = "%PREFIX%", role = "ROLE_ADMIN")]
impl %NAME%Controller {
    /// `GET %PREFIX%?page=1&per_page=20&sort=id&order=desc&search=…`
    #[karbon::get("/")]
    async fn list(
        auth: AuthGuard,
        State(state): State<AppState>,
        Query(params): Query<PaginationParams>,
    ) -> AppResult<impl IntoResponse> {
        let (items, total) = %NAME%Repository::find_paginated(state.db.pool(), &params).await?;
        Ok(Json(Paginated::new(items, total, &params)))
    }

    #[karbon::get("/{id}", id = "int:min=1")]
    async fn show(auth: AuthGuard, State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
        Ok(Json(find(&state, id).await?))
    }

    /// `201 Created` with the new %LABEL%.
    #[karbon::post("/")]
    async fn create(
        auth: AuthGuard,
        State(state): State<AppState>,
        Json(body): Json<New%NAME%>,
    ) -> AppResult<impl IntoResponse> {
        let id = %NAME%Repository::create(state.db.pool(), body).await?;
        Ok((StatusCode::CREATED, Json(find(&state, id as i64).await?)))
    }

    /// Partial update: only the fields present in the body change.
    #[karbon::put("/{id}", id = "int:min=1")]
    async fn update(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
        Json(mut body): Json<Update%NAME%>,
    ) -> AppResult<impl IntoResponse> {
        find(&state, id).await?;
        body.id = id;
        %NAME%Repository::update(state.db.pool(), body).await?;
        Ok(Json(find(&state, id).await?))
    }

    /// `204 No Content`.
    #[karbon::delete("/{id}", id = "int:min=1")]
    async fn delete(auth: AuthGuard, State(state): State<AppState>, Path(id): Path<i64>) -> AppResult<impl IntoResponse> {
        find(&state, id).await?;
        %NAME%::delete(state.db.pool(), id).await?;
        Ok(StatusCode::NO_CONTENT)
    }
}

/// The %LABEL% with this id, or a 404.
async fn find(state: &AppState, id: i64) -> AppResult<%NAME%> {
    %NAME%::find_by_id(state.db.pool(), id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("%LABEL% #{id} not found")))
}
"##;

/// Where `karbon generate auth` serves its login page. Written to `.env` as `LOGIN_PATH`,
/// which is what makes the framework send a browser without a session there.
const LOGIN_PATH: &str = "/auth/login";

/// Sign-up / sign-in / sign-out: a `User` entity, browser pages with a session cookie, and
/// the JSON equivalent under the API prefix. Without it nothing generated behind a role
/// (`generate admin`, `generate crud`) can be opened: there is no way to become someone.
fn generate_auth(root: &Path, opts: GenOptions) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;

    // 1. Entities: the accounts, and the tokens handed to them (refresh, password reset).
    write_file(&app_dir.join("entity/user.rs"), crate::templates::AUTH_USER_ENTITY, opts)?;
    append_mod(&app_dir.join("entity/mod.rs"), "user", opts)?;
    write_file(&app_dir.join("entity/auth_token.rs"), crate::templates::AUTH_TOKEN_ENTITY, opts)?;
    append_mod(&app_dir.join("entity/mod.rs"), "auth_token", opts)?;
    ensure_main_module(&app_dir, "entity", opts)?;

    // 2. Service: the logic (accounts, sessions, password reset).
    write_file(&app_dir.join("service/auth.rs"), crate::templates::AUTH_SERVICE, opts)?;
    let service_mod = app_dir.join("service/mod.rs");
    if !service_mod.exists() {
        write_file(&service_mod, "", opts)?;
    }
    append_mod(&service_mod, "auth", opts)?;
    ensure_main_module(&app_dir, "service", opts)?;

    // 3. Controller: browser pages at the root, JSON under the API prefix.
    write_file(&app_dir.join("controller/auth_controller.rs"), crate::templates::AUTH_CONTROLLER, opts)?;
    append_mod(&app_dir.join("controller/mod.rs"), "auth_controller", opts)?;

    const PAGES: &str = "controller::auth_controller::AuthController";
    const API: &str = "controller::auth_controller::AuthApiController";
    wire_route(
        &app_dir,
        "// karbon:routes",
        &format!(".nest({PAGES}::prefix(), {PAGES}::router())"),
        ".nest(AuthController::prefix(), AuthController::router())",
        opts,
    )?;
    append_at_marker(
        &app_dir,
        "// karbon:openapi-root",
        &format!("routes.extend({PAGES}::openapi_paths().into_iter().map(|(m, p)| (m.to_string(), p.to_string())));"),
        opts,
    )?;
    wire_route(
        &app_dir,
        "// karbon:api-routes",
        &format!(".nest({API}::prefix(), {API}::router())"),
        ".nest(AuthApiController::prefix(), AuthApiController::router())  — in api_routes()",
        opts,
    )?;
    append_at_marker(
        &app_dir,
        "// karbon:openapi-api",
        &format!(
            "routes.extend({API}::openapi_paths().into_iter().map(|(m, p)| (m.to_string(), format!(\"{{API_PREFIX}}{{p}}\"))));"
        ),
        opts,
    )?;

    // 4. LOGIN_PATH: the framework redirects a browser without a session to it.
    for env_file in [".env", ".env.example"] {
        set_env_default(
            &root.join(env_file),
            "LOGIN_PATH",
            LOGIN_PATH,
            "Login page: a browser that opens a protected page without a session is sent here.",
            opts,
        )?;
    }

    println!(
        "\n  {} Authentication generated (sign-up, sign-in, session renewal, password reset — pages + JSON API)\n",
        "✓".green().bold()
    );
    println!("  Next steps:\n");
    println!("    {} then {}  to create the tables", "karbon migrate diff".bold(), "karbon migrate".bold());
    println!("    {}  for the first administrator", "karbon user create you@example.com --admin".bold());
    println!("    open {} (sign up at {})", LOGIN_PATH.cyan(), "/auth/register".cyan());
    println!("    JSON: POST {}", format!("{API_PREFIX}/auth/login").cyan());
    println!(
        "    {}",
        "Password-reset emails need SMTP_HOST; without it, in development, the link is written to the log.".dimmed()
    );
    println!();
    Ok(())
}

/// Append `KEY=value` (with a comment) to an env file that does not define the key yet.
/// A missing file is left alone: `.env.example` may have been removed on purpose.
fn set_env_default(path: &Path, key: &str, value: &str, comment: &str, opts: GenOptions) -> Result<(), String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    let defined = content.lines().any(|line| {
        let line = line.trim_start();
        let line = line.strip_prefix("export ").unwrap_or(line);
        line.split_once('=').is_some_and(|(k, _)| k.trim() == key)
    });
    if defined {
        return Ok(());
    }
    let shown = path.display().to_string();
    if opts.dry_run {
        println!("  {} {} (would add `{key}={value}`)", "○".dimmed(), shown.dimmed());
        return Ok(());
    }
    let separator = if content.ends_with('\n') || content.is_empty() { "" } else { "\n" };
    let updated = format!("{content}{separator}\n# {comment}\n{key}={value}\n");
    fs::write(path, updated).map_err(|e| format!("Cannot update {shown}: {e}"))?;
    println!("  {} {} (added `{key}={value}`)", "✓".green(), shown.dimmed());
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
        return Err(format!("could not read fields from entity `{name}` ({}).", entity_file.display()));
    }

    let list_headers = fields
        .iter()
        .map(|(n, _, _)| format!("<th>{}</th>", humanize_label(n)))
        .collect::<String>();
    let list_cells = fields
        .iter()
        .map(|(n, _, nl)| {
            format!("            cells.push_str(&format!(\"<td>{{}}</td>\", esc(&({}))));", display_expr(n, *nl))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let form_fields = fields
        .iter()
        .map(|(n, kind, nl)| {
            // The control follows the type: a boolean is a checkbox, not a text box in which
            // to type `true`. A checkbox is never required — unchecked is a value (false).
            let control = match kind.as_str() {
                "bool" => "checkbox",
                "text" => "textarea",
                "int" | "bigint" | "float" => "number",
                "date" => "date",
                _ => "text",
            };
            let req = if *nl || control == "checkbox" { "" } else { ".required()" };
            format!("        .add(Field::{control}(\"{n}\", \"{}\"){req})", humanize_label(n))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let form_fill = fields
        .iter()
        .map(|(n, _, nl)| format!("        values.insert(\"{n}\".to_string(), {});", display_expr(n, *nl)))
        .collect::<Vec<_>>()
        .join("\n");
    let new_fields = fields
        .iter()
        .map(|(n, k, nl)| {
            let expr = if *nl { opt_coerce(n, k) } else { base_coerce(n, k) };
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
    append_mod(&app_dir.join("controller/mod.rs"), &format!("{file_name}_admin"), opts)?;
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

    println!("\n  {} Admin for {} generated (prefix: {})\n", "✓".green().bold(), name.bold(), prefix.cyan());
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
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn layout(title: &str, body: &str) -> String {
    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{t}</title>
<style>body{{font-family:system-ui,sans-serif;max-width:920px;margin:2rem auto;padding:0 1rem;color:#1a1a1a}}h1{{font-size:1.4rem}}table{{width:100%;border-collapse:collapse;margin-top:1rem}}th,td{{text-align:left;padding:.5rem;border-bottom:1px solid #e5e5e5}}.form-field{{margin:.75rem 0}}label{{display:block;font-weight:600;margin-bottom:.25rem}}input,textarea,select{{width:100%;padding:.5rem;border:1px solid #ccc;border-radius:6px}}.form-error{{color:#c0392b;font-size:.85rem;margin-top:.25rem}}.has-error input{{border-color:#c0392b}}.btn,button{{display:inline-block;padding:.45rem .9rem;background:#111;color:#fff;border:0;border-radius:6px;cursor:pointer;text-decoration:none;font-size:.9rem}}</style>
</head><body><h1>{t}</h1>{b}
<script>
document.addEventListener('submit', async function(e) {{
  var form = e.target;
  if ((form.getAttribute('method') || '').toLowerCase() !== 'post') return;
  e.preventDefault();
  function csrf() {{ var m = document.cookie.match(/csrf_token=([^;]+)/); return m ? m[1] : ''; }}
  function send() {{
    return fetch(form.getAttribute('action'), {{
      method: 'POST',
      headers: {{ 'X-CSRF-Token': csrf(), 'Content-Type': 'application/x-www-form-urlencoded' }},
      body: new URLSearchParams(new FormData(form)),
      redirect: 'follow'
    }});
  }}
  var res = await send();
  // 401: the access token expired while the page was open. With `karbon generate auth`,
  // the session can be renewed — then the request is sent once more.
  if (res.status === 401) {{
    var renewed = await fetch('/auth/refresh', {{ method: 'POST', headers: {{ 'X-CSRF-Token': csrf() }} }});
    if (renewed.ok) res = await send();
  }}
  if (res.redirected) {{ window.location = res.url; return; }}
  // Not redirected: the form came back with its errors (or an error page). Show it —
  // reloading would silently drop what the server just said.
  var html = await res.text();
  if ((res.headers.get('content-type') || '').indexOf('text/html') === 0) {{
    document.open(); document.write(html); document.close();
  }} else {{
    var message = html; try {{ message = JSON.parse(html).message || html; }} catch (_) {{}}
    alert(message);
  }}
}});
</script>
</body></html>"#,
        t = esc(title),
        b = body
    )
}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_carry_no_path_and_no_code() {
        for ok in ["Post", "PostComment", "post_comment", "V2Order"] {
            assert!(is_identifier(ok), "{ok}");
        }
        for bad in
            ["", "../../evil", "..", "a/b", "a\\b", "C:evil", "Post.rs", "2Fast", "_private", "a-b", "Post;", "é"]
        {
            assert!(!is_identifier(bad), "{bad:?}");
        }
    }

    /// The Studio terminal forwards `generate entity <name>` from an HTTP request: a
    /// traversing name used to be joined onto `app/src/entity` and written.
    #[test]
    fn a_traversing_name_is_refused_before_anything_is_written() {
        let root = std::env::temp_dir().join(format!("karbon-gen-{}", std::process::id()));
        let entity_dir = root.join("app/src/entity");
        fs::create_dir_all(&entity_dir).unwrap();
        let opts = GenOptions { force: true, ..GenOptions::default() };

        for kind in ["entity", "controller", "crud", "admin"] {
            let err = run(kind, Some("../../../escaped"), &["title:string".into()], &root, opts)
                .expect_err("a traversing name was accepted");
            assert!(err.contains("Invalid name"), "{err}");
        }
        assert!(!root.join("escaped.rs").exists());
        assert!(!root.parent().unwrap().join("escaped.rs").exists());
        assert_eq!(fs::read_dir(&entity_dir).unwrap().count(), 0);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_field_name_that_is_not_an_identifier_is_refused() {
        let Err(err) = parse_fields(&["a}impl:string".into()]) else {
            panic!("a field name carrying code was accepted");
        };
        assert!(err.contains("Invalid field name"), "{err}");
        assert!(parse_fields(&["title:string".into(), "published_at:datetime?".into()]).is_ok());
    }
}
