mod builder;
mod database;
pub mod filter_value;
mod paginated_query;
mod pagination;
#[cfg(test)]
mod pagination_test;
mod pool_settings;
mod query;
mod repository;
pub mod seeder;
mod select_builder;

pub use builder::{CountBuilder, DeleteBuilder, InsertBuilder, UpdateBuilder};
pub use database::Database;
pub use paginated_query::PaginatedQuery;
pub use pagination::{Paginated, PaginationParams};
pub use pool_settings::PoolSettings;
pub use query::QueryBuilder;
pub use repository::{CrudRepository, WhereValue};
pub use select_builder::{Order, SelectBuilder};

// ─── Database driver selection ───
//
// Exactly one of `mysql` / `postgres` / `sqlite` must be enabled. They define the same
// type aliases and helpers, so enabling several (e.g. `--all-features`) collides. These
// guards turn that into a single clear message instead of a wall of duplicate-definition
// errors.
#[cfg(any(
    all(feature = "mysql", feature = "postgres"),
    all(feature = "mysql", feature = "sqlite"),
    all(feature = "postgres", feature = "sqlite"),
))]
compile_error!(
    "karbon-framework: database drivers `mysql`, `postgres` and `sqlite` are mutually \
     exclusive — enable exactly one (note: `--all-features` is unsupported for this reason)."
);

#[cfg(not(any(feature = "mysql", feature = "postgres", feature = "sqlite")))]
compile_error!(
    "karbon-framework: no database driver enabled — enable exactly one of the `mysql`, \
     `postgres` or `sqlite` features."
);

// ─── Database driver type aliases ───

#[cfg(feature = "mysql")]
pub type Db = sqlx::MySql;
#[cfg(feature = "mysql")]
pub type DbPool = sqlx::MySqlPool;
#[cfg(feature = "mysql")]
pub type DbPoolOptions = sqlx::mysql::MySqlPoolOptions;
#[cfg(feature = "mysql")]
pub type DbRow = sqlx::mysql::MySqlRow;
#[cfg(feature = "mysql")]
pub type DbArguments = sqlx::mysql::MySqlArguments;

#[cfg(feature = "postgres")]
pub type Db = sqlx::Postgres;
#[cfg(feature = "postgres")]
pub type DbPool = sqlx::PgPool;
#[cfg(feature = "postgres")]
pub type DbPoolOptions = sqlx::postgres::PgPoolOptions;
#[cfg(feature = "postgres")]
pub type DbRow = sqlx::postgres::PgRow;
#[cfg(feature = "postgres")]
pub type DbArguments = sqlx::postgres::PgArguments;

#[cfg(feature = "sqlite")]
pub type Db = sqlx::Sqlite;
#[cfg(feature = "sqlite")]
pub type DbPool = sqlx::SqlitePool;
#[cfg(feature = "sqlite")]
pub type DbPoolOptions = sqlx::sqlite::SqlitePoolOptions;
#[cfg(feature = "sqlite")]
pub type DbRow = sqlx::sqlite::SqliteRow;
#[cfg(feature = "sqlite")]
pub type DbArguments = sqlx::sqlite::SqliteArguments<'static>;

// ─── SQL identifier validation ───

/// Validate one identifier segment: must start with an ASCII letter or `_`, then only
/// ASCII letters/digits/`_`. No spaces, no Unicode, no quotes — closes the classes of
/// gaps that let identifiers become injection vectors.
fn valid_ident_segment(seg: &str) -> bool {
    let mut chars = seg.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Strict SQL identifier check (table / column / alias). Accepts dotted identifiers
/// (`table.column`) and a lone or trailing `*`. Rejects everything else.
pub fn is_valid_identifier(s: &str) -> bool {
    if s == "*" {
        return true;
    }
    if s.is_empty() || s.len() > 128 {
        return false;
    }
    s.split('.')
        .all(|seg| seg == "*" || valid_ident_segment(seg))
}

/// Whitelist an ORDER BY direction; returns the canonical uppercase form or `None`.
pub fn normalize_direction(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_uppercase().as_str() {
        "ASC" => Some("ASC"),
        "DESC" => Some("DESC"),
        _ => None,
    }
}

// ─── SQL dialect helpers ───

/// Returns a SQL placeholder for the given 1-based parameter index.
/// MySQL: always `?`. PostgreSQL: `$1`, `$2`, etc.
#[cfg(feature = "mysql")]
pub fn placeholder(_n: usize) -> String {
    "?".to_string()
}

#[cfg(feature = "postgres")]
pub fn placeholder(n: usize) -> String {
    format!("${}", n)
}

#[cfg(feature = "sqlite")]
pub fn placeholder(_n: usize) -> String {
    "?".to_string()
}

/// Generates an INSERT SQL string with the correct placeholder syntax.
/// For PostgreSQL, appends `RETURNING id`.
pub fn insert_sql(table: &str, columns: &[&str]) -> String {
    let placeholders: Vec<String> = (1..=columns.len()).map(placeholder).collect();

    let base = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        placeholders.join(", ")
    );

    #[cfg(any(feature = "mysql", feature = "sqlite"))]
    {
        base
    }

    #[cfg(feature = "postgres")]
    {
        format!("{} RETURNING id", base)
    }
}

/// Extracts the last insert ID from a query result.
#[cfg(feature = "mysql")]
pub fn last_insert_id(result: &<Db as sqlx::Database>::QueryResult) -> u64 {
    result.last_insert_id()
}

#[cfg(feature = "postgres")]
pub fn last_insert_id(_result: &<Db as sqlx::Database>::QueryResult) -> u64 {
    // For PostgreSQL, use RETURNING id in the query instead
    0
}

#[cfg(feature = "sqlite")]
pub fn last_insert_id(result: &<Db as sqlx::Database>::QueryResult) -> u64 {
    result.last_insert_rowid() as u64
}
