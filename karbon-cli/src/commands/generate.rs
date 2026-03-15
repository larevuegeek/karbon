use colored::Colorize;
use std::fs;
use std::path::Path;

pub fn run(kind: &str, name: &str, root: &Path) -> Result<(), String> {
    println!(
        "\n{}  generate {} {}\n",
        "▲ karbon".bold().red(),
        kind.bold(),
        name.bold().cyan()
    );

    match kind {
        "entity" => generate_entity(name, root),
        "controller" => generate_controller(name, root),
        "crud" => generate_crud(name, root),
        _ => Err(format!(
            "Unknown generator '{}'. Available: entity, controller, crud",
            kind
        )),
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

fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create dir {}: {e}", parent.display()))?;
    }
    fs::write(path, content)
        .map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    println!("  {} {}", "✓".green(), path.display().to_string().dimmed());
    Ok(())
}

fn append_mod(mod_file: &Path, module_name: &str) -> Result<(), String> {
    let content = fs::read_to_string(mod_file).unwrap_or_default();
    let mod_line = format!("pub mod {};", module_name);
    if content.contains(&mod_line) {
        return Ok(());
    }
    let new_content = if content.trim().is_empty() {
        format!("{mod_line}\n")
    } else {
        format!("{content}\n{mod_line}\n")
    };
    fs::write(mod_file, new_content)
        .map_err(|e| format!("Cannot update {}: {e}", mod_file.display()))?;
    println!("  {} {} (added `{}`)", "✓".green(), mod_file.display().to_string().dimmed(), mod_line);
    Ok(())
}

// ─── Generators ───

fn generate_entity(name: &str, root: &Path) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let table = plural(name);
    let file_name = snake(name);

    let content = format!(
        r#"use chrono::NaiveDateTime;
use serde::{{Deserialize, Serialize}};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct {name} {{
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}}

/// DTO for inserting a new {name}
#[derive(Debug, Deserialize, framework::Insertable)]
#[table_name("{table}")]
pub struct New{name} {{
    pub title: String,
    pub slug: String,
}}

/// DTO for updating a {name}
#[derive(Debug, Default, Deserialize, framework::Updatable)]
#[table_name("{table}")]
pub struct Update{name} {{
    #[primary_key]
    pub id: i64,
    pub title: Option<String>,
    pub slug: Option<String>,
}}
"#
    );

    let entity_path = app_dir.join(format!("entity/{file_name}.rs"));
    write_file(&entity_path, &content)?;
    append_mod(&app_dir.join("entity/mod.rs"), &file_name)?;

    // Generate migration
    let migration_dir = root.join("migration");
    fs::create_dir_all(&migration_dir).ok();
    let migration_content = format!(
        r#"CREATE TABLE IF NOT EXISTS `{table}` (
    `id` BIGINT AUTO_INCREMENT PRIMARY KEY,
    `title` VARCHAR(255) NOT NULL,
    `slug` VARCHAR(255) NOT NULL UNIQUE,
    `created` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    `updated` DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    INDEX `idx_{table}_slug` (`slug`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
"#
    );

    let existing: Vec<_> = fs::read_dir(&migration_dir)
        .map(|entries| entries.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();
    let next_num = existing.len() + 1;
    let migration_path = migration_dir.join(format!("{:04}_create_{table}.sql", next_num));
    write_file(&migration_path, &migration_content)?;

    println!(
        "\n  {} Entity {} generated\n",
        "✓".green().bold(),
        name.bold()
    );
    Ok(())
}

fn generate_controller(name: &str, root: &Path) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let prefix = format!("/{}", plural(name).replace('_', "-"));

    let content = format!(
        r#"use axum::extract::{{Path, State}};
use axum::response::IntoResponse;
use axum::Json;

use framework::http::AppState;
use framework::error::AppResult;
use framework::security::AuthGuard;

pub struct {name}Controller;

#[framework::controller(prefix = "{prefix}", role = "ROLE_ADMIN")]
impl {name}Controller {{
    #[framework::get("/")]
    async fn list(
        auth: AuthGuard,
        State(state): State<AppState>,
    ) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": "TODO: list {}" }})))
    }}

    #[framework::get("/{{id}}")]
    async fn show(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: show {{id}}") }})))
    }}

    #[framework::post("/")]
    async fn create(
        auth: AuthGuard,
        State(state): State<AppState>,
    ) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": "TODO: create" }})))
    }}

    #[framework::put("/{{id}}")]
    async fn update(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: update {{id}}") }})))
    }}

    #[framework::delete("/{{id}}")]
    async fn delete(
        auth: AuthGuard,
        State(state): State<AppState>,
        Path(id): Path<i64>,
    ) -> AppResult<impl IntoResponse> {{
        Ok(Json(serde_json::json!({{ "message": format!("TODO: delete {{id}}") }})))
    }}
}}
"#,
        plural(name)
    );

    let controller_path = app_dir.join(format!("controller/{file_name}_controller.rs"));
    write_file(&controller_path, &content)?;
    append_mod(
        &app_dir.join("controller/mod.rs"),
        &format!("{file_name}_controller"),
    )?;

    println!(
        "\n  {} Controller {} generated (prefix: {})\n",
        "✓".green().bold(),
        name.bold(),
        prefix.cyan()
    );
    Ok(())
}

fn generate_crud(name: &str, root: &Path) -> Result<(), String> {
    let app_dir = find_app_dir(root)?;
    let file_name = snake(name);
    let table = plural(name);

    // 1. Entity
    generate_entity(name, root)?;

    // 2. Repository
    let repo_content = format!(
        r#"use sqlx::MySqlPool;

use crate::entity::{file_name}::{{{name}, New{name}, Update{name}}};
use framework::db::{{CrudRepository, PaginatedQuery, PaginationParams}};
use framework::error::AppResult;

impl CrudRepository for {name} {{
    const TABLE: &'static str = "{table}";
    const ENTITY_NAME: &'static str = "{name}";
    const HAS_SLUG: bool = true;
}}

pub struct {name}Repository;

impl {name}Repository {{
    pub async fn find_paginated(
        pool: &MySqlPool,
        params: &PaginationParams,
    ) -> AppResult<(Vec<{name}>, u64)> {{
        PaginatedQuery::<{name}>::new("SELECT * FROM {table}")
            .allowed_sorts(&["id", "title", "created"])
            .default_sort("id")
            .search_columns(&["title"])
            .execute(pool, params)
            .await
    }}

    pub async fn create(pool: &MySqlPool, dto: New{name}) -> AppResult<u64> {{
        dto.insert(pool).await
    }}

    pub async fn update(pool: &MySqlPool, dto: Update{name}) -> AppResult<u64> {{
        dto.update(pool).await
    }}
}}
"#
    );

    let repo_path = app_dir.join(format!("repository/{file_name}_repository.rs"));
    write_file(&repo_path, &repo_content)?;

    // Ensure repository/mod.rs exists
    let repo_mod = app_dir.join("repository/mod.rs");
    if !repo_mod.exists() {
        write_file(&repo_mod, "")?;
    }
    append_mod(&repo_mod, &format!("{file_name}_repository"))?;

    // Ensure entity is exported properly
    let entity_mod = app_dir.join("entity/mod.rs");
    let entity_mod_content = fs::read_to_string(&entity_mod).unwrap_or_default();
    let use_line = format!(
        "pub use {file_name}::{{{name}, New{name}, Update{name}}};"
    );
    if !entity_mod_content.contains(&use_line) {
        let updated = format!("{entity_mod_content}\n{use_line}\n");
        fs::write(&entity_mod, updated).ok();
    }

    // 3. Controller
    generate_controller(name, root)?;

    println!(
        "\n  {} Full CRUD for {} generated (entity + repo + controller + migration)\n",
        "✓".green().bold(),
        name.bold()
    );
    Ok(())
}
