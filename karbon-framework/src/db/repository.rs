use std::future::Future;

use chrono::{DateTime, Utc};
use sqlx::AssertSqlSafe;

use super::{Db, DbPool, DbRow, placeholder};
use super::{DeleteBuilder, UpdateBuilder};
use crate::error::AppResult;

/// Dynamic value for WHERE clauses.
///
/// Thanks to the `From` implementations, you can simply write:
/// ```ignore
/// User::find_where(pool, &[("email", "foo@bar.com".into())]).await?;
/// User::find_all_where(pool, &[("active", 1_i64.into())]).await?;
/// ```
#[derive(Debug, Clone)]
pub enum WhereValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    DateTime(DateTime<Utc>),
}

impl From<i64> for WhereValue {
    fn from(v: i64) -> Self {
        WhereValue::Int(v)
    }
}

impl From<i32> for WhereValue {
    fn from(v: i32) -> Self {
        WhereValue::Int(v as i64)
    }
}

impl From<f64> for WhereValue {
    fn from(v: f64) -> Self {
        WhereValue::Float(v)
    }
}

impl From<f32> for WhereValue {
    fn from(v: f32) -> Self {
        WhereValue::Float(v as f64)
    }
}

impl From<&str> for WhereValue {
    fn from(v: &str) -> Self {
        WhereValue::String(v.to_string())
    }
}

impl From<String> for WhereValue {
    fn from(v: String) -> Self {
        WhereValue::String(v)
    }
}

impl From<bool> for WhereValue {
    fn from(v: bool) -> Self {
        WhereValue::Bool(v)
    }
}

impl From<DateTime<Utc>> for WhereValue {
    fn from(v: DateTime<Utc>) -> Self {
        WhereValue::DateTime(v)
    }
}

/// Generic trait for the basic CRUD operations.
///
/// Gives `find_by_id`, `count`, `delete`, `exists`, `find_all` and optionally
/// `find_by_slug` to any type that implements it.
///
/// Supports soft delete with `SOFT_DELETE = true`: entities are not removed but marked
/// with `deleted_at`, and every query filters the deleted ones out.
///
/// ```ignore
/// impl CrudRepository for Post {
///     const TABLE: &'static str = "posts";
///     const ENTITY_NAME: &'static str = "Post";
///     const HAS_SLUG: bool = true;
///     const SOFT_DELETE: bool = true;
/// }
/// ```
pub trait CrudRepository: Sized + Send + Unpin + for<'r> sqlx::FromRow<'r, DbRow> {
    const TABLE: &'static str;
    const ENTITY_NAME: &'static str;
    const HAS_SLUG: bool = false;

    /// Turns soft delete on. When `true`, `delete()` runs an UPDATE setting `deleted_at` to
    /// the database clock instead of a DELETE, and every query adds `WHERE deleted_at IS NULL`.
    const SOFT_DELETE: bool = false;

    // ─── Internal helpers ───

    /// The soft-delete filter, when enabled
    fn soft_filter() -> &'static str {
        if Self::SOFT_DELETE { " AND deleted_at IS NULL" } else { "" }
    }

    fn soft_where() -> &'static str {
        if Self::SOFT_DELETE { " WHERE deleted_at IS NULL" } else { "" }
    }

    // ─── By ID ───

    fn find_by_id(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        async move {
            let query = format!("SELECT * FROM {} WHERE id = {}{}", Self::TABLE, placeholder(1), Self::soft_filter());
            let result = sqlx::query_as::<Db, Self>(AssertSqlSafe(query))
                .bind(id)
                .fetch_optional(pool)
                .await?;
            Ok(result)
        }
    }

    // ─── By slug ───

    fn find_by_slug(pool: &DbPool, slug: &str) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        async move {
            if !Self::HAS_SLUG {
                return Err(crate::error::AppError::Internal(format!(
                    "find_by_slug called on {} which does not have HAS_SLUG = true",
                    Self::TABLE
                )));
            }
            let query = format!("SELECT * FROM {} WHERE slug = {}{}", Self::TABLE, placeholder(1), Self::soft_filter());
            let result = sqlx::query_as::<Db, Self>(AssertSqlSafe(query))
                .bind(slug)
                .fetch_optional(pool)
                .await?;
            Ok(result)
        }
    }

    // ─── Counting ───

    fn count(pool: &DbPool) -> impl Future<Output = AppResult<i64>> + Send {
        async move {
            let query = format!("SELECT COUNT(*) FROM {}{}", Self::TABLE, Self::soft_where());
            let (count,): (i64,) = sqlx::query_as(AssertSqlSafe(query)).fetch_one(pool).await?;
            Ok(count)
        }
    }

    fn exists(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<bool>> + Send {
        async move {
            let query =
                format!("SELECT COUNT(*) FROM {} WHERE id = {}{}", Self::TABLE, placeholder(1), Self::soft_filter());
            let (count,): (i64,) = sqlx::query_as(AssertSqlSafe(query)).bind(id).fetch_one(pool).await?;
            Ok(count > 0)
        }
    }

    // ─── Deletion ───

    /// Deletes an entity (soft delete when SOFT_DELETE = true, a real DELETE otherwise).
    fn delete(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move {
            if Self::SOFT_DELETE {
                UpdateBuilder::table(Self::TABLE)
                    .set_now("deleted_at")
                    .where_eq("id", id)
                    .execute(pool)
                    .await
            } else {
                DeleteBuilder::from(Self::TABLE).where_eq("id", id).execute(pool).await
            }
        }
    }

    /// Permanent deletion (ignores SOFT_DELETE).
    fn force_delete(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move { DeleteBuilder::from(Self::TABLE).where_eq("id", id).execute(pool).await }
    }

    /// Restores a soft-deleted entity.
    fn restore(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move {
            if !Self::SOFT_DELETE {
                return Err(crate::error::AppError::Internal(format!(
                    "restore called on {} which does not have SOFT_DELETE = true",
                    Self::TABLE
                )));
            }
            UpdateBuilder::table(Self::TABLE)
                .set_raw("deleted_at", "NULL")
                .where_eq("id", id)
                .execute(pool)
                .await
        }
    }

    // ─── Lists ───

    /// ```ignore
    /// let users = User::find_all(pool, None).await?;
    /// let categories = Category::find_all(pool, Some(("name", "ASC"))).await?;
    /// ```
    fn find_all(pool: &DbPool, order: Option<(&str, &str)>) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        let base = format!("SELECT * FROM {}{}", Self::TABLE, Self::soft_where());
        let clause = order_clause(order);
        async move {
            let query = format!("{}{}", base, clause?);
            let items = sqlx::query_as::<Db, Self>(AssertSqlSafe(query)).fetch_all(pool).await?;
            Ok(items)
        }
    }

    /// Fetches every entity, soft-deleted ones included.
    fn find_all_with_trashed(
        pool: &DbPool,
        order: Option<(&str, &str)>,
    ) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        let base = format!("SELECT * FROM {}", Self::TABLE);
        let clause = order_clause(order);
        async move {
            let query = format!("{}{}", base, clause?);
            let items = sqlx::query_as::<Db, Self>(AssertSqlSafe(query)).fetch_all(pool).await?;
            Ok(items)
        }
    }

    // ─── Dynamic lookup ───

    /// ```ignore
    /// let user = User::find_where(pool, &[("email", "foo@bar.com".into())]).await?;
    /// ```
    fn find_where(
        pool: &DbPool,
        conditions: &[(&str, WhereValue)],
    ) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        let built = build_where_query(Self::TABLE, conditions, None, Self::SOFT_DELETE);
        async move {
            let (sql, values) = built?;
            let mut query = sqlx::query_as::<Db, Self>(AssertSqlSafe(sql));
            for value in &values {
                query = bind_where_value_as(query, value);
            }
            Ok(query.fetch_optional(pool).await?)
        }
    }

    /// ```ignore
    /// let comments = Comment::find_all_where(pool, &[("status", "approved".into())], None).await?;
    /// ```
    fn find_all_where(
        pool: &DbPool,
        conditions: &[(&str, WhereValue)],
        order: Option<(&str, &str)>,
    ) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        let built = build_where_query(Self::TABLE, conditions, order, Self::SOFT_DELETE);
        async move {
            let (sql, values) = built?;
            let mut query = sqlx::query_as::<Db, Self>(AssertSqlSafe(sql));
            for value in &values {
                query = bind_where_value_as(query, value);
            }
            Ok(query.fetch_all(pool).await?)
        }
    }

    // ─── Relations ───

    /// **has-many**: loads every `Self` entity whose `foreign_key = owner_id`.
    ///
    /// ```ignore
    /// // Post hasMany Comment
    /// let comments = Comment::has_many(pool, "post_id", post.id).await?;
    /// ```
    /// (The **belongs-to** side is simply `Parent::find_by_id(pool, child.parent_id)`.)
    fn has_many(pool: &DbPool, foreign_key: &str, owner_id: i64) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        async move { Self::find_all_where(pool, &[(foreign_key, owner_id.into())], None).await }
    }

    /// **has-many batch** (avoids the N+1): loads in a single query every `Self` entity
    /// whose `foreign_key IN (owner_ids)`, grouped by the `foreign_key` value that `key`
    /// extracts.
    ///
    /// ```ignore
    /// let post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();
    /// let by_post = Comment::load_grouped_by(pool, "post_id", &post_ids, |c| c.post_id).await?;
    /// for post in &posts {
    ///     let comments = by_post.get(&post.id).cloned().unwrap_or_default();
    /// }
    /// ```
    fn load_grouped_by<F>(
        pool: &DbPool,
        foreign_key: &str,
        owner_ids: &[i64],
        key: F,
    ) -> impl Future<Output = AppResult<std::collections::HashMap<i64, Vec<Self>>>> + Send
    where
        F: Fn(&Self) -> i64 + Send,
    {
        async move {
            use std::collections::HashMap;
            if owner_ids.is_empty() {
                return Ok(HashMap::new());
            }
            if !crate::db::is_valid_identifier(foreign_key) {
                return Err(crate::error::AppError::BadRequest(format!(
                    "Invalid foreign key identifier: {foreign_key}"
                )));
            }
            let placeholders: Vec<String> = (1..=owner_ids.len()).map(placeholder).collect();
            let sql = format!(
                "SELECT * FROM {} WHERE {} IN ({}){}",
                Self::TABLE,
                foreign_key,
                placeholders.join(", "),
                Self::soft_filter()
            );
            let mut query = sqlx::query_as::<Db, Self>(AssertSqlSafe(sql));
            for id in owner_ids {
                query = query.bind(*id);
            }
            let rows = query.fetch_all(pool).await?;

            let mut grouped: HashMap<i64, Vec<Self>> = HashMap::new();
            for row in rows {
                grouped.entry(key(&row)).or_default().push(row);
            }
            Ok(grouped)
        }
    }
}

/// Build a validated ` ORDER BY col DIR` clause, or an empty string. Rejects a
/// non-identifier column or a direction that isn't `ASC`/`DESC` so caller-supplied
/// sort strings can never inject SQL.
fn order_clause(order: Option<(&str, &str)>) -> AppResult<String> {
    match order {
        None => Ok(String::new()),
        Some((col, dir)) => {
            if !crate::db::is_valid_identifier(col) {
                return Err(crate::error::AppError::BadRequest(format!("Invalid order column: {col}")));
            }
            let dir = crate::db::normalize_direction(dir)
                .ok_or_else(|| crate::error::AppError::BadRequest(format!("Invalid order direction: {dir}")))?;
            Ok(format!(" ORDER BY {} {}", col, dir))
        }
    }
}

fn build_where_query(
    table: &str,
    conditions: &[(&str, WhereValue)],
    order: Option<(&str, &str)>,
    soft_delete: bool,
) -> AppResult<(String, Vec<WhereValue>)> {
    // Validate every column identifier before it reaches the SQL string.
    for (col, _) in conditions {
        if !crate::db::is_valid_identifier(col) {
            return Err(crate::error::AppError::BadRequest(format!("Invalid column identifier: {col}")));
        }
    }
    let mut clauses: Vec<String> = conditions
        .iter()
        .enumerate()
        .map(|(i, (col, _))| format!("{} = {}", col, placeholder(i + 1)))
        .collect();
    if soft_delete {
        clauses.push("deleted_at IS NULL".to_string());
    }
    let values: Vec<WhereValue> = conditions.iter().map(|(_, v)| v.clone()).collect();
    let mut sql = if clauses.is_empty() {
        format!("SELECT * FROM {}", table)
    } else {
        format!("SELECT * FROM {} WHERE {}", table, clauses.join(" AND "))
    };
    sql.push_str(&order_clause(order)?);
    Ok((sql, values))
}

fn bind_where_value_as<'q, T>(
    query: sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments>,
    value: &'q WhereValue,
) -> sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments>
where
    T: Send + Unpin + for<'r> sqlx::FromRow<'r, DbRow>,
{
    match value {
        WhereValue::Int(v) => query.bind(*v),
        WhereValue::Float(v) => query.bind(*v),
        WhereValue::String(v) => query.bind(v.as_str()),
        WhereValue::Bool(v) => query.bind(*v),
        WhereValue::DateTime(v) => query.bind(*v),
    }
}
