mod database;
mod builder;
mod pagination;
mod query;
mod repository;
mod paginated_query;
#[cfg(test)]
mod pagination_test;
pub mod filter_value;
pub mod seeder;

pub use database::Database;
pub use builder::{InsertBuilder, UpdateBuilder, DeleteBuilder, CountBuilder};
pub use pagination::{Paginated, PaginationParams};
pub use query::QueryBuilder;
pub use repository::{CrudRepository, WhereValue};
pub use paginated_query::PaginatedQuery;

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

/// Generates an INSERT SQL string with the correct placeholder syntax.
/// For PostgreSQL, appends `RETURNING id`.
pub fn insert_sql(table: &str, columns: &[&str]) -> String {
    let placeholders: Vec<String> = (1..=columns.len())
        .map(placeholder)
        .collect();

    let base = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        placeholders.join(", ")
    );

    #[cfg(feature = "mysql")]
    { return base; }

    #[cfg(feature = "postgres")]
    { return format!("{} RETURNING id", base); }
}

/// Executes an INSERT query and returns the new row's ID.
/// MySQL: uses `last_insert_id()`. PostgreSQL: uses `RETURNING id`.
pub async fn execute_insert<'q, E>(
    query: sqlx::query::Query<'q, Db, DbArguments>,
    executor: E,
) -> crate::error::AppResult<u64>
where
    E: sqlx::Executor<'q, Database = Db>,
{
    #[cfg(feature = "mysql")]
    {
        let result: <Db as sqlx::Database>::QueryResult = query.execute(executor).await?;
        Ok(result.last_insert_id())
    }

    #[cfg(feature = "postgres")]
    {
        use sqlx::Row;
        let row: DbRow = query.fetch_one(executor).await?;
        let id: i64 = row.get(0);
        Ok(id as u64)
    }
}
