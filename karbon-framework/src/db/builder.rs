use sqlx::Arguments;

use super::{Db, DbArguments, placeholder};
use crate::error::AppResult;

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
            let result = sqlx::query_with(&sql, self.args).execute(executor).await?;
            Ok(result.last_insert_id())
        }

        #[cfg(feature = "sqlite")]
        {
            let result = sqlx::query_with(&sql, self.args).execute(executor).await?;
            Ok(result.last_insert_rowid() as u64)
        }

        #[cfg(feature = "postgres")]
        {
            use sqlx::Row;
            let row = sqlx::query_with(&sql, self.args)
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

    /// Execute the UPDATE and return rows_affected
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        if self.set_clauses.is_empty() {
            return Ok(0);
        }

        let where_clause = if self.where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.where_clauses.join(" AND "))
        };

        let sql = format!(
            "UPDATE {} SET {}{}",
            self.table,
            self.set_clauses.join(", "),
            where_clause,
        );

        let result = sqlx::query_with(&sql, self.args).execute(executor).await?;

        Ok(result.rows_affected())
    }
}

// ─────────────────────────────────────────────
// DeleteBuilder
// ─────────────────────────────────────────────

/// Runtime DELETE query builder.
///
/// Accepts either a `&DbPool` or a `&mut Transaction` via `execute()`.
pub struct DeleteBuilder {
    table: String,
    where_clauses: Vec<String>,
    args: DbArguments,
    param_count: usize,
}

impl DeleteBuilder {
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

    /// WHERE <raw SQL>
    pub fn where_raw(mut self, condition: &str) -> Self {
        self.where_clauses.push(condition.to_string());
        self
    }

    /// Execute the DELETE and return rows_affected
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<u64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        let where_clause = if self.where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.where_clauses.join(" AND "))
        };

        let sql = format!("DELETE FROM {}{}", self.table, where_clause);

        let result = sqlx::query_with(&sql, self.args).execute(executor).await?;

        Ok(result.rows_affected())
    }
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

    /// Execute the COUNT and return the result
    pub async fn execute<'e, E>(self, executor: E) -> AppResult<i64>
    where
        E: sqlx::Executor<'e, Database = Db>,
    {
        let where_clause = if self.where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.where_clauses.join(" AND "))
        };

        let sql = format!("SELECT COUNT(*) FROM {}{}", self.table, where_clause);

        let (count,): (i64,) = sqlx::query_as_with(&sql, self.args)
            .fetch_one(executor)
            .await?;

        Ok(count)
    }
}
