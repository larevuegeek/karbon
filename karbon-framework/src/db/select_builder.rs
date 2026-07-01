use super::{DbPool, DbRow, placeholder};
use sqlx::Row;

/// Validate a SQL identifier (table name, column name, alias).
/// Strict: ASCII letters/digits/underscore, dotted segments, or `*`. No spaces/Unicode.
fn validate_identifier(s: &str) -> bool {
    super::is_valid_identifier(s)
}

/// Validate a column list (e.g., "id, name, email, u.created_at").
/// Each comma-separated entry must be a valid identifier.
fn validate_column_list(s: &str) -> bool {
    if s.trim().is_empty() {
        return false;
    }
    s.split(',').all(|c| super::is_valid_identifier(c.trim()))
}

/// Validate a JOIN clause.
/// Allows: alphanumeric, underscore, dot, space, equals, and common JOIN keywords.
/// Rejects semicolons, quotes, comments, and other injection vectors.
fn validate_join(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    !s.contains(';')
        && !s.contains('\'')
        && !s.contains('"')
        && !s.contains("--")
        && !s.contains("/*")
}

/// Sort direction
#[derive(Debug, Clone, Copy)]
pub enum Order {
    Asc,
    Desc,
}

impl std::fmt::Display for Order {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Order::Asc => write!(f, "ASC"),
            Order::Desc => write!(f, "DESC"),
        }
    }
}

/// A typed, fluent SELECT query builder with parameterized conditions.
///
/// ```ignore
/// use karbon::db::{SelectBuilder, Order};
///
/// let users: Vec<User> = SelectBuilder::table("users")
///     .columns("id, name, email, created_at")
///     .where_eq("active", true)
///     .where_like("name", "%alice%")
///     .where_gt("age", 18)
///     .where_in("role", &["admin", "editor"])
///     .where_null("deleted_at")
///     .order_by("created_at", Order::Desc)
///     .limit(20)
///     .offset(0)
///     .fetch_all(&pool)
///     .await?;
///
/// let count = SelectBuilder::table("users")
///     .where_eq("active", true)
///     .count(&pool)
///     .await?;
///
/// let user: Option<User> = SelectBuilder::table("users")
///     .where_eq("email", "alice@example.com")
///     .fetch_one(&pool)
///     .await?;
/// ```
pub struct SelectBuilder {
    table: String,
    columns: String,
    joins: Vec<String>,
    conditions: Vec<Condition>,
    order: Vec<(String, Order)>,
    limit_val: Option<u32>,
    offset_val: Option<u32>,
    group_by_val: Option<String>,
}

enum Condition {
    Eq(String, BindValue),
    Ne(String, BindValue),
    Gt(String, BindValue),
    Gte(String, BindValue),
    Lt(String, BindValue),
    Lte(String, BindValue),
    Like(String, BindValue),
    IsNull(String),
    IsNotNull(String),
    In(String, Vec<BindValue>),
    Raw(String),
}

#[derive(Clone)]
#[doc(hidden)]
pub enum BindValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

#[doc(hidden)]
pub trait IntoBindValue {
    fn into_bind_value(self) -> BindValue;
}

impl IntoBindValue for i32 {
    fn into_bind_value(self) -> BindValue {
        BindValue::Int(self as i64)
    }
}

impl IntoBindValue for i64 {
    fn into_bind_value(self) -> BindValue {
        BindValue::Int(self)
    }
}

impl IntoBindValue for u32 {
    fn into_bind_value(self) -> BindValue {
        BindValue::Int(self as i64)
    }
}

impl IntoBindValue for f64 {
    fn into_bind_value(self) -> BindValue {
        BindValue::Float(self)
    }
}

impl IntoBindValue for bool {
    fn into_bind_value(self) -> BindValue {
        BindValue::Bool(self)
    }
}

impl IntoBindValue for &str {
    fn into_bind_value(self) -> BindValue {
        BindValue::String(self.to_string())
    }
}

impl IntoBindValue for String {
    fn into_bind_value(self) -> BindValue {
        BindValue::String(self)
    }
}

impl IntoBindValue for chrono::DateTime<chrono::Utc> {
    fn into_bind_value(self) -> BindValue {
        BindValue::String(self.format("%Y-%m-%d %H:%M:%S%.6f").to_string())
    }
}

impl SelectBuilder {
    pub fn table(table: &str) -> Self {
        assert!(validate_identifier(table), "Invalid table name: {table}");
        Self {
            table: table.to_string(),
            columns: "*".to_string(),
            joins: Vec::new(),
            conditions: Vec::new(),
            order: Vec::new(),
            limit_val: None,
            offset_val: None,
            group_by_val: None,
        }
    }

    pub fn columns(mut self, cols: &str) -> Self {
        assert!(validate_column_list(cols), "Invalid column list: {cols}");
        self.columns = cols.to_string();
        self
    }

    /// Add a JOIN clause. Must not contain SQL injection vectors (;, quotes, comments).
    pub fn join(mut self, join_clause: &str) -> Self {
        assert!(
            validate_join(join_clause),
            "Invalid JOIN clause: contains forbidden characters"
        );
        self.joins.push(join_clause.to_string());
        self
    }

    pub fn where_eq<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Eq(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_ne<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Ne(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_gt<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Gt(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_gte<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Gte(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_lt<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Lt(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_lte<V: IntoBindValue>(mut self, column: &str, value: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::Lte(column.to_string(), value.into_bind_value()));
        self
    }

    pub fn where_like<V: IntoBindValue>(mut self, column: &str, pattern: V) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions.push(Condition::Like(
            column.to_string(),
            pattern.into_bind_value(),
        ));
        self
    }

    pub fn where_null(mut self, column: &str) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions.push(Condition::IsNull(column.to_string()));
        self
    }

    pub fn where_not_null(mut self, column: &str) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        self.conditions
            .push(Condition::IsNotNull(column.to_string()));
        self
    }

    pub fn where_in<V: IntoBindValue + Clone>(mut self, column: &str, values: &[V]) -> Self {
        assert!(validate_identifier(column), "Invalid column name: {column}");
        let bind_values: Vec<BindValue> =
            values.iter().map(|v| v.clone().into_bind_value()).collect();
        self.conditions
            .push(Condition::In(column.to_string(), bind_values));
        self
    }

    /// Add a raw WHERE clause. **WARNING**: This is NOT parameterized.
    /// Only use with hardcoded strings, NEVER with user input.
    pub fn where_raw(mut self, raw: &str) -> Self {
        assert!(
            validate_join(raw),
            "where_raw contains forbidden characters"
        );
        self.conditions.push(Condition::Raw(raw.to_string()));
        self
    }

    pub fn order_by(mut self, column: &str, direction: Order) -> Self {
        assert!(
            validate_identifier(column),
            "Invalid ORDER BY column: {column}"
        );
        self.order.push((column.to_string(), direction));
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit_val = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset_val = Some(offset);
        self
    }

    pub fn group_by(mut self, clause: &str) -> Self {
        assert!(
            validate_column_list(clause),
            "Invalid GROUP BY clause: {clause}"
        );
        self.group_by_val = Some(clause.to_string());
        self
    }

    /// Execute and return all matching rows
    pub async fn fetch_all<T>(self, pool: &DbPool) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> sqlx::FromRow<'r, DbRow> + Send + Unpin,
    {
        let (sql, binds) = self.build_select();
        let mut query = sqlx::query_as::<_, T>(&sql);
        for bind in &binds {
            query = bind_value(query, bind);
        }
        query.fetch_all(pool).await
    }

    /// Execute and return the first matching row
    pub async fn fetch_one<T>(self, pool: &DbPool) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> sqlx::FromRow<'r, DbRow> + Send + Unpin,
    {
        let (sql, binds) = self.limit(1).build_select();
        let mut query = sqlx::query_as::<_, T>(&sql);
        for bind in &binds {
            query = bind_value(query, bind);
        }
        query.fetch_optional(pool).await
    }

    /// Execute a COUNT(*) query with the same conditions
    pub async fn count(self, pool: &DbPool) -> Result<i64, sqlx::Error> {
        let (sql, binds) = self.build_count();
        let mut query = sqlx::query(&sql);
        for bind in &binds {
            query = bind_value_raw(query, bind);
        }
        let row = query.fetch_one(pool).await?;
        Ok(row.try_get::<i64, _>(0).unwrap_or(0))
    }

    /// Build the SELECT SQL and collect bind values
    fn build_select(self) -> (String, Vec<BindValue>) {
        let mut binds = Vec::new();
        let mut idx = 1usize;

        let joins = self.joins.join(" ");
        let where_clause = self.build_where(&mut binds, &mut idx);

        let order_clause = if self.order.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = self
                .order
                .iter()
                .map(|(col, dir)| format!("{} {}", col, dir))
                .collect();
            format!(" ORDER BY {}", parts.join(", "))
        };

        let group = self
            .group_by_val
            .as_ref()
            .map(|g| format!(" GROUP BY {}", g))
            .unwrap_or_default();

        let limit = self
            .limit_val
            .map(|l| format!(" LIMIT {}", l))
            .unwrap_or_default();

        let offset = self
            .offset_val
            .map(|o| format!(" OFFSET {}", o))
            .unwrap_or_default();

        let sql = format!(
            "SELECT {} FROM {} {}{}{}{}{}{}",
            self.columns, self.table, joins, where_clause, group, order_clause, limit, offset
        );

        (sql.trim().to_string(), binds)
    }

    fn build_count(self) -> (String, Vec<BindValue>) {
        let mut binds = Vec::new();
        let mut idx = 1usize;

        let joins = self.joins.join(" ");
        let where_clause = self.build_where(&mut binds, &mut idx);

        let group = self
            .group_by_val
            .as_ref()
            .map(|g| format!(" GROUP BY {}", g))
            .unwrap_or_default();

        let sql = format!(
            "SELECT COUNT(*) FROM {} {}{}{}",
            self.table, joins, where_clause, group
        );

        (sql.trim().to_string(), binds)
    }

    fn build_where(&self, binds: &mut Vec<BindValue>, idx: &mut usize) -> String {
        if self.conditions.is_empty() {
            return String::new();
        }

        let parts: Vec<String> = self
            .conditions
            .iter()
            .map(|c| match c {
                Condition::Eq(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} = {}", col, ph)
                }
                Condition::Ne(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} != {}", col, ph)
                }
                Condition::Gt(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} > {}", col, ph)
                }
                Condition::Gte(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} >= {}", col, ph)
                }
                Condition::Lt(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} < {}", col, ph)
                }
                Condition::Lte(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} <= {}", col, ph)
                }
                Condition::Like(col, val) => {
                    let ph = placeholder(*idx);
                    *idx += 1;
                    binds.push(val.clone());
                    format!("{} LIKE {}", col, ph)
                }
                Condition::IsNull(col) => format!("{} IS NULL", col),
                Condition::IsNotNull(col) => format!("{} IS NOT NULL", col),
                Condition::In(col, vals) => {
                    let placeholders: Vec<String> = vals
                        .iter()
                        .map(|v| {
                            let ph = placeholder(*idx);
                            *idx += 1;
                            binds.push(v.clone());
                            ph
                        })
                        .collect();
                    format!("{} IN ({})", col, placeholders.join(", "))
                }
                Condition::Raw(raw) => raw.clone(),
            })
            .collect();

        format!(" WHERE {}", parts.join(" AND "))
    }
}

// Helper: bind a value to a query_as
fn bind_value<'q, T>(
    query: sqlx::query::QueryAs<'q, super::Db, T, <super::Db as sqlx::Database>::Arguments<'q>>,
    value: &'q BindValue,
) -> sqlx::query::QueryAs<'q, super::Db, T, <super::Db as sqlx::Database>::Arguments<'q>>
where
    T: for<'r> sqlx::FromRow<'r, DbRow>,
{
    match value {
        BindValue::Int(v) => query.bind(*v),
        BindValue::Float(v) => query.bind(*v),
        BindValue::String(v) => query.bind(v.as_str()),
        BindValue::Bool(v) => query.bind(*v),
    }
}

// Helper: bind a value to a raw query
fn bind_value_raw<'q>(
    query: sqlx::query::Query<'q, super::Db, <super::Db as sqlx::Database>::Arguments<'q>>,
    value: &'q BindValue,
) -> sqlx::query::Query<'q, super::Db, <super::Db as sqlx::Database>::Arguments<'q>> {
    match value {
        BindValue::Int(v) => query.bind(*v),
        BindValue::Float(v) => query.bind(*v),
        BindValue::String(v) => query.bind(v.as_str()),
        BindValue::Bool(v) => query.bind(*v),
    }
}
