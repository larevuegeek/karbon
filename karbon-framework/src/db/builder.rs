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
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`. Values are bound in
/// the order of the generated SQL (SET, then WHERE), whatever the order of the calls. An
/// UPDATE without any condition is refused unless [`all`](Self::all) says so.
pub struct UpdateBuilder {
    table: String,
    set_clauses: Vec<Clause>,
    where_clauses: Vec<Clause>,
    param_count: usize,
    all_rows: bool,
    error: Option<String>,
    #[cfg(feature = "mysql")]
    limit: Option<u64>,
}

/// A piece of SQL whose `BIND` markers become placeholders, numbered in SQL order.
struct Clause {
    sql: String,
    binders: Vec<Binder>,
}

type Binder = Box<dyn FnOnce(&mut DbArguments) + Send>;

/// Stands for a bound value inside a clause until the SQL is rendered.
const BIND: char = '\u{0}';

impl Clause {
    fn raw(sql: String) -> Self {
        Self {
            sql,
            binders: Vec::new(),
        }
    }

    fn bound<T>(sql: String, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        Self {
            sql,
            binders: vec![binder(value)],
        }
    }
}

fn binder<T>(value: T) -> Binder
where
    T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
{
    Box::new(move |args: &mut DbArguments| {
        args.add(value).unwrap();
    })
}

impl UpdateBuilder {
    pub fn table(table: &str) -> Self {
        Self {
            table: table.to_string(),
            set_clauses: Vec::new(),
            where_clauses: Vec::new(),
            param_count: 0,
            all_rows: false,
            error: None,
            #[cfg(feature = "mysql")]
            limit: None,
        }
    }

    /// SET column = ? with a bound value
    pub fn set<T>(mut self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.set_clauses
            .push(Clause::bound(format!("{column} = {BIND}"), value));
        self
    }

    /// SET column = <raw SQL expression> (e.g. NULL, column + 1)
    pub fn set_raw(mut self, column: &str, expr: &str) -> Self {
        self.set_clauses
            .push(Clause::raw(format!("{column} = {expr}")));
        self
    }

    /// SET column = now, by the database clock.
    pub fn set_now(self, column: &str) -> Self {
        self.set_raw(column, NOW_SQL)
    }

    /// SET column = <expression> where every `?` of the expression receives `value`:
    /// `set_expr("quota", "GREATEST(COALESCE(quota, 0), ?)", 10)`. An expression without
    /// `?` makes `execute()` fail.
    pub fn set_expr<T>(mut self, column: &str, expr: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
    {
        let count = expr.matches('?').count();
        if count == 0 {
            self.error = Some(format!("set_expr({column}): no `?` in {expr:?}"));
            return self;
        }

        self.param_count += count;
        self.set_clauses.push(Clause {
            sql: format!("{column} = {}", expr.replace('?', &BIND.to_string())),
            binders: (0..count).map(|_| binder(value.clone())).collect(),
        });
        self
    }

    /// SET column = ? only if value is Some, skip if None
    pub fn set_if<T>(self, column: &str, value: Option<T>) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        match value {
            Some(v) => self.set(column, v),
            None => self,
        }
    }

    /// WHERE column = ? with a bound value
    pub fn where_eq<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, "=", value)
    }

    /// WHERE column != ? with a bound value
    pub fn where_ne<T>(self, column: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.where_op(column, "!=", value)
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

    /// WHERE column IS NULL
    pub fn where_null(self, column: &str) -> Self {
        self.where_raw(&format!("{column} IS NULL"))
    }

    /// WHERE column IS NOT NULL
    pub fn where_not_null(self, column: &str) -> Self {
        self.where_raw(&format!("{column} IS NOT NULL"))
    }

    /// WHERE column < now, by the database clock.
    pub fn where_before_now(self, column: &str) -> Self {
        self.where_raw(&format!("{column} < {NOW_SQL}"))
    }

    /// WHERE column > now, by the database clock.
    pub fn where_after_now(self, column: &str) -> Self {
        self.where_raw(&format!("{column} > {NOW_SQL}"))
    }

    /// WHERE column < now − `age`, by the database clock. The age is bound, in seconds.
    pub fn where_older_than(mut self, column: &str, age: std::time::Duration) -> Self {
        self.param_count += 1;
        self.where_clauses.push(Clause::bound(
            format!("{column} < {}", older_than_sql(&BIND.to_string())),
            age_seconds(age),
        ));
        self
    }

    /// WHERE <raw SQL> (e.g. "revoked = FALSE", "COALESCE(active, 0) = 0")
    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_clauses.push(Clause::raw(condition.to_string()));
        self
    }

    /// WHERE column IN (?, …) ; une liste vide ne correspond à aucune ligne.
    pub fn where_in<T>(mut self, column: &str, values: &[T]) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + Clone + 'static,
    {
        if values.is_empty() {
            return self.where_raw("1 = 0");
        }

        self.param_count += values.len();
        let markers = vec![BIND.to_string(); values.len()].join(", ");
        self.where_clauses.push(Clause {
            sql: format!("{column} IN ({markers})"),
            binders: values.iter().cloned().map(binder).collect(),
        });
        self
    }

    /// Updates every row of the table: required to run an UPDATE without condition.
    pub fn all(mut self) -> Self {
        self.all_rows = true;
        self
    }

    /// `LIMIT n` — MySQL only, hence absent from the other drivers.
    #[cfg(feature = "mysql")]
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Requête SQL générée (sans les valeurs).
    pub fn to_sql(&self) -> String {
        let mut n = 0;
        let mut render = |clauses: &[Clause]| -> Vec<String> {
            clauses
                .iter()
                .map(|c| {
                    c.sql
                        .split(BIND)
                        .enumerate()
                        .map(|(i, part)| {
                            if i == 0 {
                                part.to_string()
                            } else {
                                n += 1;
                                format!("{}{part}", placeholder(n))
                            }
                        })
                        .collect()
                })
                .collect()
        };

        let set = render(&self.set_clauses).join(", ");
        let conditions = render(&self.where_clauses);

        #[allow(unused_mut)]
        let mut sql = format!("UPDATE {} SET {set}{}", self.table, where_sql(&conditions));

        #[cfg(feature = "mysql")]
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        sql
    }

    /// Execute the UPDATE and return rows_affected
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        if let Some(error) = self.error {
            return Err(AppError::Internal(format!(
                "UPDATE {}: {error}",
                self.table
            )));
        }
        if self.set_clauses.is_empty() {
            return Ok(0);
        }
        if self.where_clauses.is_empty() && !self.all_rows {
            return Err(AppError::Internal(format!(
                "UPDATE {} without condition refused: add a where_* or call .all()",
                self.table
            )));
        }

        let sql = self.to_sql();

        let mut args = DbArguments::default();
        for clause in self.set_clauses.into_iter().chain(self.where_clauses) {
            for bind in clause.binders {
                bind(&mut args);
            }
        }

        let result = sqlx::query_with(AssertSqlSafe(sql), args)
            .execute(executor)
            .await?;

        Ok(result.rows_affected())
    }

    fn where_op<T>(mut self, column: &str, op: &str, value: T) -> Self
    where
        T: sqlx::Type<Db> + sqlx::Encode<'static, Db> + Send + 'static,
    {
        self.param_count += 1;
        self.where_clauses
            .push(Clause::bound(format!("{column} {op} {BIND}"), value));
        self
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

    /// WHERE column > now, by the database clock.
    pub fn where_after_now(mut self, column: &str) -> Self {
        self.where_clauses.push(format!("{column} > {NOW_SQL}"));
        self
    }

    /// WHERE column IS NULL
    pub fn where_null(mut self, column: &str) -> Self {
        self.where_clauses.push(format!("{column} IS NULL"));
        self
    }

    /// WHERE column IS NOT NULL
    pub fn where_not_null(mut self, column: &str) -> Self {
        self.where_clauses.push(format!("{column} IS NOT NULL"));
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

#[cfg(test)]
mod update_tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn values_follow_the_sql_order_whatever_the_call_order() {
        let b = UpdateBuilder::table("t")
            .where_eq("id", 7_i64)
            .set("name", "x".to_string())
            .where_lt("n", 3_i64)
            .set_now("updated");
        assert_eq!(
            b.to_sql(),
            format!(
                "UPDATE t SET name = {}, updated = {NOW_SQL} WHERE id = {} AND n < {}",
                placeholder(1),
                placeholder(2),
                placeholder(3)
            )
        );
        // SET first, then WHERE: the binders are applied in that order.
        assert_eq!(b.set_clauses[0].binders.len(), 1);
        assert_eq!(b.where_clauses[0].binders.len(), 1);
    }

    #[test]
    fn set_expr_binds_each_question_mark() {
        let b = UpdateBuilder::table("h")
            .set_expr("quota", "GREATEST(COALESCE(quota, 0), ?)", 5_i64)
            .set_expr("d", "IF(yearly, CEIL(? / 12), ?)", 24_i64)
            .where_eq("id", 1_i64);
        assert_eq!(
            b.to_sql(),
            format!(
                "UPDATE h SET quota = GREATEST(COALESCE(quota, 0), {}), d = IF(yearly, CEIL({} / 12), {}) WHERE id = {}",
                placeholder(1),
                placeholder(2),
                placeholder(3),
                placeholder(4)
            )
        );
        assert_eq!(b.param_count, 4);
    }

    #[test]
    fn set_expr_without_question_mark_is_recorded_as_an_error() {
        let b = UpdateBuilder::table("h").set_expr("q", "q + 1", 1_i64);
        assert!(b.error.is_some());
    }

    #[test]
    fn null_and_clock_conditions() {
        let b = UpdateBuilder::table("s")
            .set("used", 1_i64)
            .where_null("used_at")
            .where_not_null("token")
            .where_after_now("expires_at")
            .where_older_than("seen_at", Duration::from_secs(60));
        assert_eq!(
            b.to_sql(),
            format!(
                "UPDATE s SET used = {} WHERE used_at IS NULL AND token IS NOT NULL AND expires_at > {NOW_SQL} AND seen_at < {}",
                placeholder(1),
                older_than_sql(&placeholder(2))
            )
        );
    }

    #[test]
    fn without_condition_only_all_is_explicit() {
        assert!(!UpdateBuilder::table("t").set("a", 1_i64).all_rows);
        assert_eq!(
            UpdateBuilder::table("t").set_raw("a", "1").all().to_sql(),
            "UPDATE t SET a = 1"
        );
    }

    /// The binding order checked against a real database: `where_eq` before `set` used to
    /// write the id into the column and filter on the value with `?` placeholders.
    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn a_where_called_first_still_binds_after_the_set() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE t (id INTEGER, name TEXT); INSERT INTO t VALUES (1, 'a'), (2, 'b');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let changed = UpdateBuilder::table("t")
            .where_eq("id", 2_i64)
            .set("name", "z".to_string())
            .execute(&pool)
            .await
            .unwrap();

        let rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM t ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(rows, vec![(1, "a".to_string()), (2, "z".to_string())]);
    }

    #[cfg(feature = "mysql")]
    #[test]
    fn mysql_limit_closes_the_statement() {
        let b = UpdateBuilder::table("r")
            .set_raw("restart", "1")
            .all()
            .limit(1);
        assert_eq!(b.to_sql(), "UPDATE r SET restart = 1 LIMIT 1");
    }
}
