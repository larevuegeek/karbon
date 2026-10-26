use sqlx::{Arguments, AssertSqlSafe};

use super::{Db, DbArguments, placeholder};
use crate::error::{AppError, AppResult};

/// `column IN (?, …)` en liant chaque valeur ; une liste vide donne `1 = 0` (ne correspond à
/// rien) plutôt qu'un `IN ()` invalide.
fn in_clause<T>(
    column: &str,
    values: &[T],
    args: &mut DbArguments,
    param_count: &mut usize,
) -> String
where
    T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
{
    if values.is_empty() {
        return "1 = 0".to_string();
    }
    let placeholders: Vec<String> = values
        .iter()
        .map(|v| {
            *param_count += 1;
            args.add(v.clone()).unwrap();
            placeholder(*param_count)
        })
        .collect();
    format!("{} IN ({})", column, placeholders.join(", "))
}

fn where_sql(clauses: &[String]) -> String {
    if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    }
}

// ─────────────────────────────────────────────
// InsertBuilder
// ─────────────────────────────────────────────

/// Runtime INSERT query builder for custom inserts.
///
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`.
pub struct InsertBuilder {
    table: String,
    columns: Vec<String>,
    args: DbArguments,
}

impl InsertBuilder {
    pub fn into(table: &str) -> Self {
        Self {
            table: table.to_string(),
            columns: Vec::new(),
            args: DbArguments::default(),
        }
    }

    /// Add a column with a bound value
    pub fn set<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.columns.push(column.to_string());
        self.args.add(value).unwrap();
        self
    }

    /// Add a column with an Option value (binds NULL if None)
    pub fn set_optional<T>(mut self, column: &str, value: Option<T>) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.columns.push(column.to_string());
        self.args.add(value).unwrap();
        self
    }

    /// Execute the INSERT and return last_insert_id
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        let placeholders: Vec<String> = (1..=self.columns.len()).map(placeholder).collect();

        #[cfg(any(feature = "mysql", feature = "sqlite"))]
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            self.columns.join(", "),
            placeholders.join(", ")
        );

        #[cfg(feature = "postgres")]
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING id",
            self.table,
            self.columns.join(", "),
            placeholders.join(", ")
        );

        #[cfg(feature = "mysql")]
        {
            let result = sqlx::query_with(AssertSqlSafe(sql), self.args)
                .execute(executor)
                .await?;
            Ok(result.last_insert_id())
        }

        #[cfg(feature = "sqlite")]
        {
            let result = sqlx::query_with(AssertSqlSafe(sql), self.args)
                .execute(executor)
                .await?;
            Ok(result.last_insert_rowid() as u64)
        }

        #[cfg(feature = "postgres")]
        {
            use sqlx::Row;
            let row = sqlx::query_with(AssertSqlSafe(sql), self.args)
                .fetch_one(executor)
                .await?;
            let id: i64 = row.get(0);
            Ok(id as u64)
        }
    }
}

// ─────────────────────────────────────────────
// UpdateBuilder
// ─────────────────────────────────────────────

/// Runtime UPDATE query builder for custom updates.
///
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`.
pub struct UpdateBuilder {
    table: String,
    set_clauses: Vec<String>,
    where_clauses: Vec<String>,
    args: DbArguments,
    param_count: usize,
}

impl UpdateBuilder {
    pub fn table(table: &str) -> Self {
        Self {
            table: table.to_string(),
            set_clauses: Vec::new(),
            where_clauses: Vec::new(),
            args: DbArguments::default(),
            param_count: 0,
        }
    }

    /// SET column = ? with a bound value
    pub fn set<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.set_clauses
            .push(format!("{} = {}", column, placeholder(self.param_count)));
        self.args.add(value).unwrap();
        self
    }

    /// SET column = <raw SQL expression> (e.g. NOW(), NULL, column + 1)
    pub fn set_raw(mut self, column: &str, expr: &str) -> Self {
        self.set_clauses.push(format!("{} = {}", column, expr));
        self
    }

    /// SET column = ? only if value is Some, skip if None
    pub fn set_if<T>(mut self, column: &str, value: Option<T>) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        if let Some(v) = value {
            self.param_count += 1;
            self.set_clauses
                .push(format!("{} = {}", column, placeholder(self.param_count)));
            self.args.add(v).unwrap();
        }
        self
    }

    /// WHERE column = ? with a bound value
    pub fn where_eq<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses
            .push(format!("{} = {}", column, placeholder(self.param_count)));
        self.args.add(value).unwrap();
        self
    }

    /// WHERE <raw SQL> (e.g. "revoked = FALSE", "expires_at > NOW()")
    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_clauses.push(condition.to_string());
        self
    }

    /// WHERE column != ? with a bound value
    pub fn where_ne<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses
            .push(format!("{} != {}", column, placeholder(self.param_count)));
        self.args.add(value).unwrap();
        self
    }

    /// WHERE column IN (?, …) ; une liste vide ne correspond à aucune ligne.
    pub fn where_in<T>(mut self, column: &str, values: &[T]) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
    {
        let clause = in_clause(column, values, &mut self.args, &mut self.param_count);
        self.where_clauses.push(clause);
        self
    }

    /// Requête SQL générée (sans les valeurs).
    pub fn to_sql(&self) -> String {
        format!(
            "UPDATE {} SET {}{}",
            self.table,
            self.set_clauses.join(", "),
            where_sql(&self.where_clauses)
        )
    }

    /// Execute the UPDATE and return rows_affected
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        if self.set_clauses.is_empty() {
            return Ok(0);
        }
        let sql = self.to_sql();

        let result = sqlx::query_with(AssertSqlSafe(sql), self.args)
            .execute(executor)
            .await?;

        Ok(result.rows_affected())
    }
}

// ─────────────────────────────────────────────
// DeleteBuilder
// ─────────────────────────────────────────────

/// Runtime DELETE query builder.
///
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`. A DELETE without
/// any condition is refused unless [`all`](Self::all) says so.
pub struct DeleteBuilder {
    table: String,
    where_clauses: Vec<String>,
    args: DbArguments,
    param_count: usize,
    all_rows: bool,
    #[cfg(feature = "mysql")]
    limit: Option<u64>,
}

impl DeleteBuilder {
    pub fn from(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clauses: Vec::new(),
            args: DbArguments::default(),
            param_count: 0,
            all_rows: false,
            #[cfg(feature = "mysql")]
            limit: None,
        }
    }

    /// WHERE column = ?
    pub fn where_eq<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, "=", value)
    }

    /// WHERE column < ?
    pub fn where_lt<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, "<", value)
    }

    /// WHERE column <= ?
    pub fn where_lte<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, "<=", value)
    }

    /// WHERE column > ?
    pub fn where_gt<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, ">", value)
    }

    /// WHERE column >= ?
    pub fn where_gte<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, ">=", value)
    }

    /// WHERE column < now, by the database clock: right for a column the database fills
    /// itself (`NOW()`), whatever the application's time zone.
    pub fn where_before_now(mut self, column: &str) -> Self {
        self.where_clauses.push(format!("{column} < {NOW_SQL}"));
        self
    }

    /// WHERE column < now − `age`, by the database clock. The age is bound, in seconds.
    pub fn where_older_than(mut self, column: &str, age: std::time::Duration) -> Self {
        self.param_count += 1;
        let cutoff = older_than_sql(&placeholder(self.param_count));
        self.where_clauses.push(format!("{column} < {cutoff}"));
        self.args.add(age_seconds(age)).unwrap();
        self
    }

    /// WHERE <raw SQL>
    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_clauses.push(condition.to_string());
        self
    }

    /// WHERE column IN (?, …) ; une liste vide ne correspond à aucune ligne.
    pub fn where_in<T>(mut self, column: &str, values: &[T]) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
    {
        let clause = in_clause(column, values, &mut self.args, &mut self.param_count);
        self.where_clauses.push(clause);
        self
    }

    /// Deletes every row of the table: required to run a DELETE without condition.
    pub fn all(mut self) -> Self {
        self.all_rows = true;
        self
    }

    /// `LIMIT n` — MySQL only, hence absent from the other drivers: bounds one pass of a
    /// large purge, the rest goes on the next one.
    #[cfg(feature = "mysql")]
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Requête SQL générée (sans les valeurs).
    pub fn to_sql(&self) -> String {
        #[allow(unused_mut)]
        let mut sql = format!(
            "DELETE FROM {}{}",
            self.table,
            where_sql(&self.where_clauses)
        );

        #[cfg(feature = "mysql")]
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        sql
    }

    /// Execute the DELETE and return rows_affected
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        if self.where_clauses.is_empty() && !self.all_rows {
            return Err(AppError::Internal(format!(
                "DELETE FROM {} without condition refused: add a where_* or call .all()",
                self.table
            )));
        }

        let sql = self.to_sql();

        let result = sqlx::query_with(AssertSqlSafe(sql), self.args)
            .execute(executor)
            .await?;

        Ok(result.rows_affected())
    }

    fn where_op<T>(mut self, column: &str, op: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses.push(format!(
            "{} {} {}",
            column,
            op,
            placeholder(self.param_count)
        ));
        self.args.add(value).unwrap();
        self
    }
}

#[cfg(not(feature = "sqlite"))]
const NOW_SQL: &str = "NOW()";
#[cfg(feature = "sqlite")]
const NOW_SQL: &str = "datetime('now')";

#[cfg(feature = "mysql")]
fn older_than_sql(seconds: &str) -> String {
    format!("NOW() - INTERVAL {seconds} SECOND")
}

#[cfg(feature = "postgres")]
fn older_than_sql(seconds: &str) -> String {
    format!("NOW() - make_interval(secs => {seconds})")
}

#[cfg(feature = "sqlite")]
fn older_than_sql(seconds: &str) -> String {
    format!("datetime('now', '-' || {seconds} || ' seconds')")
}

#[cfg(not(feature = "postgres"))]
fn age_seconds(age: std::time::Duration) -> i64 {
    i64::try_from(age.as_secs()).unwrap_or(i64::MAX)
}

/// `make_interval(secs => …)` takes a double precision.
#[cfg(feature = "postgres")]
fn age_seconds(age: std::time::Duration) -> f64 {
    age.as_secs() as f64
}

// ─────────────────────────────────────────────
// CountBuilder
// ─────────────────────────────────────────────

/// Runtime COUNT query builder.
///
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`.
pub struct CountBuilder {
    table: String,
    where_clauses: Vec<String>,
    args: DbArguments,
    param_count: usize,
}

impl CountBuilder {
    pub fn from(table: &str) -> Self {
        Self {
            table: table.to_string(),
            where_clauses: Vec::new(),
            args: DbArguments::default(),
            param_count: 0,
        }
    }

    /// WHERE column = ?
    pub fn where_eq<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses
            .push(format!("{} = {}", column, placeholder(self.param_count)));
        self.args.add(value).unwrap();
        self
    }

    /// WHERE column != ?
    pub fn where_ne<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses
            .push(format!("{} != {}", column, placeholder(self.param_count)));
        self.args.add(value).unwrap();
        self
    }

    /// WHERE <raw SQL>
    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_clauses.push(condition.to_string());
        self
    }

    /// WHERE column IN (?, …) ; une liste vide ne correspond à aucune ligne.
    pub fn where_in<T>(mut self, column: &str, values: &[T]) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
    {
        let clause = in_clause(column, values, &mut self.args, &mut self.param_count);
        self.where_clauses.push(clause);
        self
    }

    /// Requête SQL générée (sans les valeurs).
    pub fn to_sql(&self) -> String {
        format!(
            "SELECT COUNT(*) FROM {}{}",
            self.table,
            where_sql(&self.where_clauses)
        )
    }

    /// Execute the COUNT and return the result
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<i64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        let sql = self.to_sql();

        let (count,): (i64,) = sqlx::query_as_with(AssertSqlSafe(sql), self.args)
            .fetch_one(executor)
            .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod where_in_tests {
    use super::*;

    #[test]
    fn update_where_in_binds_each_value() {
        let b = UpdateBuilder::table("forum_topic")
            .set_raw("pinned", "1")
            .where_in("id", &[3_i64, 5, 8]);
        // `placeholder()` renders `?` or `$n` depending on the driver feature, so the
        // expectation is built the same way instead of hardcoding one backend's syntax.
        assert_eq!(
            b.to_sql(),
            format!(
                "UPDATE forum_topic SET pinned = 1 WHERE id IN ({}, {}, {})",
                placeholder(1),
                placeholder(2),
                placeholder(3)
            )
        );
        assert_eq!(b.param_count, 3);
    }

    #[test]
    fn delete_where_in_combines_with_other_conditions() {
        let b = DeleteBuilder::from("forum_post")
            .where_eq("active", 0_i64)
            .where_in("topic_id", &[1_i64, 2]);
        assert_eq!(
            b.to_sql(),
            format!(
                "DELETE FROM forum_post WHERE active = {} AND topic_id IN ({}, {})",
                placeholder(1),
                placeholder(2),
                placeholder(3)
            )
        );
    }

    #[test]
    fn empty_where_in_matches_nothing() {
        let b = DeleteBuilder::from("forum_post").where_in::<i64>("id", &[]);
        assert_eq!(b.to_sql(), "DELETE FROM forum_post WHERE 1 = 0");
        let c = CountBuilder::from("content").where_in::<i64>("id", &[]);
        assert_eq!(c.to_sql(), "SELECT COUNT(*) FROM content WHERE 1 = 0");
    }
}

#[cfg(test)]
mod delete_tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn comparisons_bind_their_value() {
        let b = DeleteBuilder::from("logs")
            .where_eq("site", 2_i64)
            .where_lt("created_at", 10_i64)
            .where_gte("level", 3_i64);
        assert_eq!(
            b.to_sql(),
            format!(
                "DELETE FROM logs WHERE site = {} AND created_at < {} AND level >= {}",
                placeholder(1),
                placeholder(2),
                placeholder(3)
            )
        );
    }

    #[test]
    fn age_conditions_use_the_database_clock() {
        let b = DeleteBuilder::from("tokens")
            .where_before_now("exp")
            .where_older_than("seen_at", Duration::from_secs(86_400));
        assert_eq!(
            b.to_sql(),
            format!(
                "DELETE FROM tokens WHERE exp < {NOW_SQL} AND seen_at < {}",
                older_than_sql(&placeholder(1))
            )
        );
        assert_eq!(b.param_count, 1);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn mysql_limit_closes_the_statement() {
        let b = DeleteBuilder::from("sessions")
            .where_older_than("expires_at", Duration::from_secs(60))
            .limit(20_000);
        assert_eq!(
            b.to_sql(),
            "DELETE FROM sessions WHERE expires_at < NOW() - INTERVAL ? SECOND LIMIT 20000"
        );
    }

    #[test]
    fn without_condition_only_all_is_explicit() {
        assert_eq!(DeleteBuilder::from("t").all().to_sql(), "DELETE FROM t");
        assert!(!DeleteBuilder::from("t").all_rows);
    }
}
