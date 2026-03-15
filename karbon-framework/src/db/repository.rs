use std::future::Future;

use chrono::{DateTime, Utc};

use super::{Db, DbPool, DbRow, placeholder};
use super::{DeleteBuilder, UpdateBuilder};
use crate::error::AppResult;

/// Valeur dynamique pour les clauses WHERE.
///
/// Grâce aux implémentations `From`, on peut écrire simplement :
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
    fn from(v: i64) -> Self { WhereValue::Int(v) }
}

impl From<i32> for WhereValue {
    fn from(v: i32) -> Self { WhereValue::Int(v as i64) }
}

impl From<f64> for WhereValue {
    fn from(v: f64) -> Self { WhereValue::Float(v) }
}

impl From<f32> for WhereValue {
    fn from(v: f32) -> Self { WhereValue::Float(v as f64) }
}

impl From<&str> for WhereValue {
    fn from(v: &str) -> Self { WhereValue::String(v.to_string()) }
}

impl From<String> for WhereValue {
    fn from(v: String) -> Self { WhereValue::String(v) }
}

impl From<bool> for WhereValue {
    fn from(v: bool) -> Self { WhereValue::Bool(v) }
}

impl From<DateTime<Utc>> for WhereValue {
    fn from(v: DateTime<Utc>) -> Self { WhereValue::DateTime(v) }
}

/// Trait générique pour les opérations CRUD de base.
///
/// Fournit automatiquement `find_by_id`, `count`, `delete`, `exists`,
/// `find_all` et optionnellement `find_by_slug` à tout type qui l'implémente.
///
/// Supporte le soft delete via `SOFT_DELETE = true` : les entités ne sont pas
/// supprimées mais marquées avec `deleted_at`. Les requêtes filtrent
/// automatiquement les entités supprimées.
///
/// ```ignore
/// impl CrudRepository for Post {
///     const TABLE: &'static str = "posts";
///     const ENTITY_NAME: &'static str = "Article";
///     const HAS_SLUG: bool = true;
///     const SOFT_DELETE: bool = true;
/// }
/// ```
pub trait CrudRepository: Sized + Send + Unpin + for<'r> sqlx::FromRow<'r, DbRow> {
    const TABLE: &'static str;
    const ENTITY_NAME: &'static str;
    const HAS_SLUG: bool = false;

    /// Active le soft delete. Si `true`, `delete()` fait un UPDATE SET deleted_at = NOW()
    /// au lieu d'un DELETE, et toutes les requêtes ajoutent `WHERE deleted_at IS NULL`.
    const SOFT_DELETE: bool = false;

    // ─── Helpers internes ───

    /// Retourne le filtre soft delete si activé
    fn soft_filter() -> &'static str {
        if Self::SOFT_DELETE { " AND deleted_at IS NULL" } else { "" }
    }

    fn soft_where() -> &'static str {
        if Self::SOFT_DELETE { " WHERE deleted_at IS NULL" } else { "" }
    }

    // ─── Par ID ───

    fn find_by_id(
        pool: &DbPool,
        id: i64,
    ) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        async move {
            let query = format!(
                "SELECT * FROM {} WHERE id = {}{}",
                Self::TABLE, placeholder(1), Self::soft_filter()
            );
            let result = sqlx::query_as::<Db, Self>(&query)
                .bind(id)
                .fetch_optional(pool)
                .await?;
            Ok(result)
        }
    }

    // ─── Par slug ───

    fn find_by_slug(
        pool: &DbPool,
        slug: &str,
    ) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        async move {
            if !Self::HAS_SLUG {
                return Err(crate::error::AppError::Internal(
                    format!("find_by_slug called on {} which does not have HAS_SLUG = true", Self::TABLE)
                ));
            }
            let query = format!(
                "SELECT * FROM {} WHERE slug = {}{}",
                Self::TABLE, placeholder(1), Self::soft_filter()
            );
            let result = sqlx::query_as::<Db, Self>(&query)
                .bind(slug)
                .fetch_optional(pool)
                .await?;
            Ok(result)
        }
    }

    // ─── Comptage ───

    fn count(pool: &DbPool) -> impl Future<Output = AppResult<i64>> + Send {
        async move {
            let query = format!("SELECT COUNT(*) FROM {}{}", Self::TABLE, Self::soft_where());
            let (count,): (i64,) = sqlx::query_as(&query).fetch_one(pool).await?;
            Ok(count)
        }
    }

    fn exists(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<bool>> + Send {
        async move {
            let query = format!(
                "SELECT COUNT(*) FROM {} WHERE id = {}{}",
                Self::TABLE, placeholder(1), Self::soft_filter()
            );
            let (count,): (i64,) = sqlx::query_as(&query).bind(id).fetch_one(pool).await?;
            Ok(count > 0)
        }
    }

    // ─── Suppression ───

    /// Supprime une entité (soft delete si SOFT_DELETE = true, sinon DELETE réel).
    fn delete(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move {
            if Self::SOFT_DELETE {
                UpdateBuilder::table(Self::TABLE)
                    .set_raw("deleted_at", "NOW()")
                    .where_eq("id", id)
                    .execute(pool)
                    .await
            } else {
                DeleteBuilder::from(Self::TABLE)
                    .where_eq("id", id)
                    .execute(pool)
                    .await
            }
        }
    }

    /// Suppression définitive (ignore SOFT_DELETE).
    fn force_delete(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move {
            DeleteBuilder::from(Self::TABLE)
                .where_eq("id", id)
                .execute(pool)
                .await
        }
    }

    /// Restaure une entité soft-deleted.
    fn restore(pool: &DbPool, id: i64) -> impl Future<Output = AppResult<u64>> + Send {
        async move {
            if !Self::SOFT_DELETE {
                return Err(crate::error::AppError::Internal(
                    format!("restore called on {} which does not have SOFT_DELETE = true", Self::TABLE)
                ));
            }
            UpdateBuilder::table(Self::TABLE)
                .set_raw("deleted_at", "NULL")
                .where_eq("id", id)
                .execute(pool)
                .await
        }
    }

    // ─── Listes ───

    /// ```ignore
    /// let users = User::find_all(pool, None).await?;
    /// let categories = Category::find_all(pool, Some(("name", "ASC"))).await?;
    /// ```
    fn find_all(
        pool: &DbPool,
        order: Option<(&str, &str)>,
    ) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        let mut query = format!("SELECT * FROM {}{}", Self::TABLE, Self::soft_where());
        if let Some((col, dir)) = order {
            query.push_str(&format!(" ORDER BY {} {}", col, dir));
        }
        async move {
            let items = sqlx::query_as::<Db, Self>(&query).fetch_all(pool).await?;
            Ok(items)
        }
    }

    /// Récupère toutes les entités y compris les soft-deleted.
    fn find_all_with_trashed(
        pool: &DbPool,
        order: Option<(&str, &str)>,
    ) -> impl Future<Output = AppResult<Vec<Self>>> + Send {
        let mut query = format!("SELECT * FROM {}", Self::TABLE);
        if let Some((col, dir)) = order {
            query.push_str(&format!(" ORDER BY {} {}", col, dir));
        }
        async move {
            let items = sqlx::query_as::<Db, Self>(&query).fetch_all(pool).await?;
            Ok(items)
        }
    }

    // ─── Recherche dynamique ───

    /// ```ignore
    /// let user = User::find_where(pool, &[("email", "foo@bar.com".into())]).await?;
    /// ```
    fn find_where(
        pool: &DbPool,
        conditions: &[(&str, WhereValue)],
    ) -> impl Future<Output = AppResult<Option<Self>>> + Send {
        let (sql, values) = build_where_query(Self::TABLE, conditions, None, Self::SOFT_DELETE);
        async move {
            let mut query = sqlx::query_as::<Db, Self>(&sql);
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
        let (sql, values) = build_where_query(Self::TABLE, conditions, order, Self::SOFT_DELETE);
        async move {
            let mut query = sqlx::query_as::<Db, Self>(&sql);
            for value in &values {
                query = bind_where_value_as(query, value);
            }
            Ok(query.fetch_all(pool).await?)
        }
    }
}

fn build_where_query(table: &str, conditions: &[(&str, WhereValue)], order: Option<(&str, &str)>, soft_delete: bool) -> (String, Vec<WhereValue>) {
    let mut clauses: Vec<String> = conditions.iter().enumerate()
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
    if let Some((col, dir)) = order {
        sql.push_str(&format!(" ORDER BY {} {}", col, dir));
    }
    (sql, values)
}

fn bind_where_value_as<'q, T>(
    query: sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments<'q>>,
    value: &'q WhereValue,
) -> sqlx::query::QueryAs<'q, Db, T, <Db as sqlx::Database>::Arguments<'q>>
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
