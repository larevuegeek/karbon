use std::marker::PhantomData;

use sqlx::AssertSqlSafe;

use super::pagination::PaginationParams;
use super::repository::WhereValue;
use super::{Db, DbPool, DbRow, placeholder};
use crate::error::{AppError, AppResult};

/// Condition WHERE d'une requête paginée.
#[derive(Debug, Clone)]
enum Condition {
    /// `col <op> ?` — colonne validée comme identifiant.
    Column(String, &'static str, WhereValue),
    /// Condition SQL libre (écrite par le code, jamais par l'utilisateur) avec ses valeurs,
    /// une par `?`.
    Raw(String, Vec<WhereValue>),
}

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
/// // Avec conditions libres et tri calculé
/// PaginatedQuery::<ContentListItem>::new("SELECT c.id, c.title FROM content c")
///     .where_raw("(c.status = 1 OR c.user_id = ?)", vec![user_id.into()])
///     .where_raw("EXISTS (SELECT 1 FROM content_field_tags t WHERE t.content_id = c.id AND t.term_id = ?)", vec![tag_id.into()])
///     .order_raw("c.une DESC, c.created DESC")
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
    where_conditions: Vec<Condition>,
    fixed_order: Option<String>,
    _phantom: PhantomData<T>,
}

/// Requêtes et valeurs prêtes à exécuter (séparé de l'exécution pour être testable).
#[derive(Debug)]
struct Prepared {
    data_sql: String,
    count_sql: String,
    binds: Vec<WhereValue>,
    like: Option<String>,
    like_count: usize,
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
            fixed_order: None,
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
        self.where_conditions
            .push(Condition::Column(column.to_string(), "=", value));
        self
    }

    /// Ajoute une condition WHERE (col >= ?).
    pub fn where_gte(mut self, column: &str, value: WhereValue) -> Self {
        self.where_conditions
            .push(Condition::Column(column.to_string(), ">=", value));
        self
    }

    /// Ajoute une condition WHERE (col <= ?).
    pub fn where_lte(mut self, column: &str, value: WhereValue) -> Self {
        self.where_conditions
            .push(Condition::Column(column.to_string(), "<=", value));
        self
    }

    /// Ajoute une condition WHERE libre (`OR`, `EXISTS`, sous-requête, `IN`…), combinée aux
    /// autres par `AND`. Chaque `?` reçoit, dans l'ordre, une valeur de `values` (numérotés
    /// automatiquement en `$n` sous PostgreSQL). La condition est du SQL écrit par le code :
    /// n'y interpolez jamais une saisie utilisateur, passez-la en valeur.
    pub fn where_raw(mut self, condition: &str, values: Vec<WhereValue>) -> Self {
        self.where_conditions
            .push(Condition::Raw(condition.to_string(), values));
        self
    }

    /// Tri fixe (expression SQL écrite par le code, ex. `c.une DESC, c.created DESC`) à la place
    /// du tri choisi par `params.sort` / `params.order`.
    pub fn order_raw(mut self, expr: &str) -> Self {
        self.fixed_order = Some(expr.to_string());
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
        let p = self.prepare(params)?;

        let mut data_query = sqlx::query_as::<Db, T>(AssertSqlSafe(p.data_sql));
        for val in &p.binds {
            data_query = bind_where_value(data_query, val);
        }
        if let Some(ref like) = p.like {
            for _ in 0..p.like_count {
                data_query = data_query.bind(like.as_str());
            }
        }
        data_query = data_query
            .bind(params.safe_per_page() as i64)
            .bind(params.offset() as i64);
        let items = data_query.fetch_all(pool).await?;

        let mut count_query = sqlx::query_as::<Db, (i64,)>(AssertSqlSafe(p.count_sql));
        for val in &p.binds {
            count_query = bind_where_value_tuple(count_query, val);
        }
        if let Some(ref like) = p.like {
            for _ in 0..p.like_count {
                count_query = count_query.bind(like.as_str());
            }
        }
        let (total,) = count_query.fetch_one(pool).await?;

        Ok((items, total as u64))
    }

    /// Construit les requêtes data / COUNT et la liste ordonnée des valeurs à lier.
    fn prepare(&self, params: &PaginationParams) -> AppResult<Prepared> {
        // ─── Tri ───
        let order_sql = match &self.fixed_order {
            Some(expr) => expr.clone(),
            None => {
                let sort_col = if self.allowed_sorts.is_empty() {
                    self.default_sort.clone()
                } else {
                    let allowed_refs: Vec<&str> =
                        self.allowed_sorts.iter().map(|s| s.as_str()).collect();
                    params
                        .sort_column(&allowed_refs, &self.default_sort)
                        .to_string()
                };
                // Direction en liste blanche (jamais une chaîne de requête interpolée).
                let order = if params.order.eq_ignore_ascii_case("asc") {
                    "ASC"
                } else if params.order.eq_ignore_ascii_case("desc") {
                    "DESC"
                } else {
                    crate::db::normalize_direction(&self.default_order).unwrap_or("ASC")
                };
                if !crate::db::is_valid_identifier(&sort_col) {
                    return Err(AppError::BadRequest(format!(
                        "Invalid sort column: {sort_col}"
                    )));
                }
                let sort_expr = match &self.sort_prefix {
                    Some(prefix) if !sort_col.contains('.') => {
                        if !crate::db::is_valid_identifier(prefix) {
                            return Err(AppError::BadRequest(format!(
                                "Invalid sort prefix: {prefix}"
                            )));
                        }
                        format!("{}.{}", prefix, sort_col)
                    }
                    _ => sort_col,
                };
                format!("{sort_expr} {order}")
            }
        };

        // ─── WHERE (placeholders numérotés dans l'ordre des valeurs) ───
        let mut where_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<WhereValue> = Vec::new();
        let mut ph_idx: usize = 0;
        for cond in &self.where_conditions {
            match cond {
                Condition::Column(col, op, val) => {
                    if !crate::db::is_valid_identifier(col) {
                        return Err(AppError::BadRequest(format!(
                            "Invalid filter column: {col}"
                        )));
                    }
                    ph_idx += 1;
                    where_clauses.push(format!("{} {} {}", col, op, placeholder(ph_idx)));
                    binds.push(val.clone());
                }
                Condition::Raw(sql, values) => {
                    let (numbered, count) = number_placeholders(sql, &mut ph_idx);
                    if count != values.len() {
                        return Err(AppError::Internal(format!(
                            "where_raw : {count} placeholder(s) pour {} valeur(s) dans « {sql} »",
                            values.len()
                        )));
                    }
                    where_clauses.push(format!("({numbered})"));
                    binds.extend(values.iter().cloned());
                }
            }
        }

        for col in &self.search_columns {
            if !crate::db::is_valid_identifier(col) {
                return Err(AppError::BadRequest(format!(
                    "Invalid search column: {col}"
                )));
            }
        }
        let has_search = !self.search_columns.is_empty() && params.search.is_some();
        if has_search {
            let like_clauses: Vec<String> = self
                .search_columns
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

        ph_idx += 1;
        let limit_ph = placeholder(ph_idx);
        ph_idx += 1;
        let offset_ph = placeholder(ph_idx);
        let data_sql = format!(
            "{}{} ORDER BY {} LIMIT {} OFFSET {}",
            self.base_sql, where_sql, order_sql, limit_ph, offset_ph
        );
        // Le COUNT réutilise les mêmes index de placeholder que le WHERE.
        let count_sql = match &self.count_from {
            Some(custom) => format!("{}{}", custom, where_sql),
            None => format!(
                "SELECT COUNT(*) {}{}",
                extract_from_clause(&self.base_sql).trim_start(),
                where_sql
            ),
        };

        Ok(Prepared {
            data_sql,
            count_sql,
            binds,
            like: if has_search {
                params.search.as_ref().map(|s| format!("%{}%", s))
            } else {
                None
            },
            like_count: if has_search {
                self.search_columns.len()
            } else {
                0
            },
        })
    }
}

/// Remplace chaque `?` hors chaînes et identifiants entre guillemets par le placeholder du
/// backend (`?` ou `$n`) ; renvoie le SQL et le nombre de placeholders.
fn number_placeholders(sql: &str, next: &mut usize) -> (String, usize) {
    let mut out = String::with_capacity(sql.len());
    let mut quote: Option<char> = None;
    let mut count = 0;
    for c in sql.chars() {
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' | '`' => {
                    quote = Some(c);
                    out.push(c);
                }
                '?' => {
                    *next += 1;
                    count += 1;
                    out.push_str(&placeholder(*next));
                }
                _ => out.push(c),
            },
        }
    }
    (out, count)
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
            b' ' if depth == 0
                && i + 6 <= upper_bytes.len()
                && &upper_bytes[i..i + 6] == b" FROM " =>
            {
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

/// Bind une WhereValue sur un query_as<(i64,)> (pour les COUNT)
fn bind_where_value_tuple<'q>(
    query: sqlx::query::QueryAs<'q, Db, (i64,), <Db as sqlx::Database>::Arguments>,
    value: &'q WhereValue,
) -> sqlx::query::QueryAs<'q, Db, (i64,), <Db as sqlx::Database>::Arguments> {
    match value {
        WhereValue::Int(v) => query.bind(*v),
        WhereValue::Float(v) => query.bind(*v),
        WhereValue::String(v) => query.bind(v.as_str()),
        WhereValue::Bool(v) => query.bind(*v),
        WhereValue::DateTime(v) => query.bind(*v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(sqlx::FromRow)]
    #[allow(dead_code)]
    struct Row {
        id: i64,
    }

    fn params(search: Option<&str>) -> PaginationParams {
        PaginationParams {
            page: 2,
            per_page: 10,
            sort: None,
            order: "desc".into(),
            search: search.map(str::to_string),
        }
    }

    #[test]
    fn where_raw_keeps_value_order_and_wraps_condition() {
        let p = PaginatedQuery::<Row>::new("SELECT c.id FROM content c")
            .where_eq("c.type", "movie".into())
            .where_raw(
                "(c.status = ? OR c.user_id = ?)",
                vec![1_i64.into(), 7_i64.into()],
            )
            .default_sort("c.id")
            .prepare(&params(None))
            .unwrap();
        // Built with `placeholder()` so the test holds under every driver feature.
        let (p1, p2, p3) = (placeholder(1), placeholder(2), placeholder(3));
        assert_eq!(
            p.data_sql,
            format!(
                "SELECT c.id FROM content c WHERE c.type = {p1} AND ((c.status = {p2} OR c.user_id = {p3})) ORDER BY c.id DESC LIMIT {} OFFSET {}",
                placeholder(4),
                placeholder(5)
            )
        );
        assert_eq!(
            p.count_sql,
            format!(
                "SELECT COUNT(*) FROM content c WHERE c.type = {p1} AND ((c.status = {p2} OR c.user_id = {p3}))"
            )
        );
        assert_eq!(p.binds.len(), 3);
    }

    #[test]
    fn where_raw_ignores_question_marks_in_literals() {
        let p = PaginatedQuery::<Row>::new("SELECT id FROM t")
            .where_raw("title <> '?' AND id = ?", vec![3_i64.into()])
            .prepare(&params(None))
            .unwrap();
        assert!(
            p.data_sql
                .contains(&format!("(title <> '?' AND id = {})", placeholder(1)))
        );
    }

    #[test]
    fn where_raw_rejects_value_count_mismatch() {
        let r = PaginatedQuery::<Row>::new("SELECT id FROM t")
            .where_raw("id = ? OR id = ?", vec![1_i64.into()])
            .prepare(&params(None));
        assert!(r.is_err());
    }

    #[test]
    fn order_raw_overrides_requested_sort() {
        let mut ps = params(None);
        ps.sort = Some("title".into());
        let p = PaginatedQuery::<Row>::new("SELECT id FROM t")
            .allowed_sorts(&["title"])
            .order_raw("une DESC, created DESC")
            .prepare(&ps)
            .unwrap();
        assert!(p.data_sql.ends_with(&format!(
            "ORDER BY une DESC, created DESC LIMIT {} OFFSET {}",
            placeholder(1),
            placeholder(2)
        )));
    }

    #[test]
    fn search_binds_after_where_values() {
        let p = PaginatedQuery::<Row>::new("SELECT id FROM t")
            .where_raw("a = ?", vec![1_i64.into()])
            .search_columns(&["title", "body"])
            .prepare(&params(Some("geek")))
            .unwrap();
        assert!(p.data_sql.contains(&format!(
            "WHERE (a = {}) AND (title LIKE {} OR body LIKE {})",
            placeholder(1),
            placeholder(2),
            placeholder(3)
        )));
        assert_eq!(p.like.as_deref(), Some("%geek%"));
        assert_eq!(p.like_count, 2);
    }
}
