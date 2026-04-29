use std::marker::PhantomData;

use super::{Db, DbArguments, DbPool, DbRow, placeholder};
use super::pagination::PaginationParams;
use super::repository::WhereValue;
use crate::error::AppResult;

/// Builder pour les requêtes paginées avec tri, recherche et filtres WHERE.
///
/// # Exemples
///
/// ```ignore
/// // Simple
/// PaginatedQuery::<MediaFile>::new("SELECT * FROM media_file")
///     .allowed_sorts(&["id", "filename", "created", "size"])
///     .default_sort("id")
///     .execute(pool, params)
///     .await
///
/// // Avec WHERE
/// PaginatedQuery::<CommentListItem>::new(
///     "SELECT c.id, c.comment, u.username FROM comment c LEFT JOIN users u ON c.user_id = u.id"
/// )
///     .where_eq("c.content_id", content_id.into())
///     .default_sort("c.created")
///     .default_order("ASC")
///     .execute(pool, params)
///     .await
///
/// // Avec recherche
/// PaginatedQuery::<UserListItem>::new("SELECT u.id, u.username FROM users u")
///     .allowed_sorts(&["id", "username", "email", "created"])
///     .sort_prefix("u")
///     .search_columns(&["u.username", "u.email"])
///     .execute(pool, params)
///     .await
/// ```
pub struct PaginatedQuery<T> {
    base_sql: String,
    count_from: Option<String>,
    allowed_sorts: Vec<String>,
    default_sort: String,
    default_order: String,
    sort_prefix: Option<String>,
    search_columns: Vec<String>,
    where_conditions: Vec<(String, &'static str, WhereValue)>,
    _phantom: PhantomData<T>,
}

impl<T> PaginatedQuery<T>
where
    T: Send + Unpin + for<'r> sqlx::FromRow<'r, DbRow>,
{
    /// Crée un nouveau builder à partir d'une requête SELECT de base (sans ORDER BY/LIMIT).
    pub fn new(base_sql: &str) -> Self {
        Self {
            base_sql: base_sql.to_string(),
            count_from: None,
            allowed_sorts: Vec::new(),
            default_sort: "id".to_string(),
            default_order: "DESC".to_string(),
            sort_prefix: None,
            search_columns: Vec::new(),
            where_conditions: Vec::new(),
            _phantom: PhantomData,
        }
    }

    /// Colonnes de tri autorisées (whitelist pour éviter l'injection SQL).
    pub fn allowed_sorts(mut self, sorts: &[&str]) -> Self {
        self.allowed_sorts = sorts.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Colonne de tri par défaut.
    pub fn default_sort(mut self, sort: &str) -> Self {
        self.default_sort = sort.to_string();
        self
    }

    /// Direction de tri par défaut ("ASC" ou "DESC").
    pub fn default_order(mut self, order: &str) -> Self {
        self.default_order = order.to_uppercase();
        self
    }

    /// Préfixe de table pour le ORDER BY (ex: "u" → `ORDER BY u.id`).
    pub fn sort_prefix(mut self, prefix: &str) -> Self {
        self.sort_prefix = Some(prefix.to_string());
        self
    }

    /// Colonnes à rechercher avec LIKE (activé si `params.search` est renseigné).
    pub fn search_columns(mut self, columns: &[&str]) -> Self {
        self.search_columns = columns.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Ajoute une condition WHERE (col = ?).
    pub fn where_eq(mut self, column: &str, value: WhereValue) -> Self {
        self.where_conditions.push((column.to_string(), "=", value));
        self
    }

    /// Ajoute une condition WHERE (col >= ?).
    pub fn where_gte(mut self, column: &str, value: WhereValue) -> Self {
        self.where_conditions.push((column.to_string(), ">=", value));
        self
    }

    /// Ajoute une condition WHERE (col <= ?).
    pub fn where_lte(mut self, column: &str, value: WhereValue) -> Self {
        self.where_conditions.push((column.to_string(), "<=", value));
        self
    }

    /// Surcharge la requête COUNT (utile si le FROM est complexe).
    /// Par défaut, le COUNT est déduit automatiquement de la requête de base.
    pub fn count_from(mut self, sql: &str) -> Self {
        self.count_from = Some(sql.to_string());
        self
    }

    /// Exécute la requête paginée et retourne `(Vec<T>, total)`.
    pub async fn execute(
        self,
        pool: &DbPool,
        params: &PaginationParams,
    ) -> AppResult<(Vec<T>, u64)> {
        // ─── Résoudre le tri ───
        let sort_col = if self.allowed_sorts.is_empty() {
            self.default_sort.clone()
        } else {
            let allowed_refs: Vec<&str> = self.allowed_sorts.iter().map(|s| s.as_str()).collect();
            params.sort_column(&allowed_refs, &self.default_sort).to_string()
        };

        let order = if params.order.to_lowercase() == "asc" {
            "ASC"
        } else if params.order.to_lowercase() == "desc" {
            "DESC"
        } else {
            &self.default_order
        };

        let sort_expr = match &self.sort_prefix {
            Some(prefix) if !sort_col.contains('.') => format!("{}.{}", prefix, sort_col),
            _ => sort_col,
        };

        // ─── Construire WHERE avec placeholder tracking ───
        let mut where_clauses: Vec<String> = Vec::new();
        let mut bind_values: Vec<WhereValue> = Vec::new();
        let mut ph_idx: usize = 0;

        for (col, op, val) in &self.where_conditions {
            ph_idx += 1;
            where_clauses.push(format!("{} {} {}", col, op, placeholder(ph_idx)));
            bind_values.push(val.clone());
        }

        // Search (LIKE)
        let has_search = !self.search_columns.is_empty() && params.search.is_some();
        if has_search {
            let like_clauses: Vec<String> = self.search_columns
                .iter()
                .map(|col| {
                    ph_idx += 1;
                    format!("{} LIKE {}", col, placeholder(ph_idx))
                })
                .collect();
            where_clauses.push(format!("({})", like_clauses.join(" OR ")));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };

        // ─── Requête data ───
        ph_idx += 1;
        let limit_ph = placeholder(ph_idx);
        ph_idx += 1;
        let offset_ph = placeholder(ph_idx);

        let data_sql = format!(
            "{}{} ORDER BY {} {} LIMIT {} OFFSET {}",
            self.base_sql, where_sql, sort_expr, order, limit_ph, offset_ph
        );

        // ─── Requête count (réutilise les mêmes index de placeholder que le WHERE) ───
        let count_sql = if let Some(ref custom) = self.count_from {
            format!("{}{}", custom, where_sql)
        } else {
            let from_part = extract_from_clause(&self.base_sql);
            format!("SELECT COUNT(*) {}{}", from_part, where_sql)
        };

        // ─── Bind et exécution ───
        let like_value = params.search.as_ref().map(|s| format!("%{}%", s));
        let search_col_count = self.search_columns.len();

        // Data query
        let mut data_query = sqlx::query_as::<Db, T>(&data_sql);
        for val in &bind_values {
            data_query = bind_where_value(data_query, val);
        }
        if has_search {
            if let Some(ref like) = like_value {
                for _ in 0..search_col_count {
                    data_query = data_query.bind(like.as_str());
                }
            }
        }
        data_query = data_query.bind(params.safe_per_page() as i64).bind(params.offset() as i64);
        let items = data_query.fetch_all(pool).await?;

        // Count query
        let mut count_query = sqlx::query_as::<Db, (i64,)>(&count_sql);
        for val in &bind_values {
            count_query = bind_where_value_tuple(count_query, val);
        }
        if has_search {
            if let Some(ref like) = like_value {
                for _ in 0..search_col_count {
                    count_query = count_query.bind(like.as_str());
                }
            }
        }
        let (total,) = count_query.fetch_one(pool).await?;

        Ok((items, total as u64))
    }
}

/// Extrait la clause FROM d'un SELECT (tout après le premier FROM/from).
fn extract_from_clause(sql: &str) -> &str {
    // Find the first top-level " FROM " (i.e. not inside parentheses), so that subqueries
    // like `(SELECT ... FROM tbl)` in the SELECT list don't fool us.
    let bytes = sql.as_bytes();
    let upper = sql.to_uppercase();
    let upper_bytes = upper.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b' ' if depth == 0 && i + 6 <= upper_bytes.len() && &upper_bytes[i..i + 6] == b" FROM " => {
                return &sql[i..];
            }
            _ => {}
        }
        i += 1;
    }
    sql
}

/// Bind une WhereValue sur un query_as<T>
fn bind_where_value<'q, T>(
    query: sqlx::query::QueryAs<'q, Db, T, DbArguments>,
    value: &'q WhereValue,
) -> sqlx::query::QueryAs<'q, Db, T, DbArguments>
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

/// Bind une WhereValue sur un query_as<(i64,)> (pour les COUNT)
fn bind_where_value_tuple<'q>(
    query: sqlx::query::QueryAs<'q, Db, (i64,), DbArguments>,
    value: &'q WhereValue,
) -> sqlx::query::QueryAs<'q, Db, (i64,), DbArguments> {
    match value {
        WhereValue::Int(v) => query.bind(*v),
        WhereValue::Float(v) => query.bind(*v),
        WhereValue::String(v) => query.bind(v.as_str()),
        WhereValue::Bool(v) => query.bind(*v),
        WhereValue::DateTime(v) => query.bind(*v),
    }
}
