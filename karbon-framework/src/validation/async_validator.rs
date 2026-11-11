use sqlx::AssertSqlSafe;

use crate::db::{DbPool, placeholder};
use crate::error::{AppError, AppResult};

/// Asynchronous validator for the rules that need the database.
///
/// # Examples
///
/// ```ignore
/// // Creation — check uniqueness
/// AsyncValidator::new(pool)
///     .unique("users", "username", &input.username, "This username is already taken")
///     .unique("users", "email", &input.email, "This email address is already in use")
///     .validate()
///     .await?;
///
/// // Update — leave the current ID out
/// AsyncValidator::new(pool)
///     .unique_except("users", "username", &input.username, user_id, "This username is already taken")
///     .unique_except("users", "email", &input.email, user_id, "This email address is already in use")
///     .validate()
///     .await?;
///
/// // Check that a foreign key exists
/// AsyncValidator::new(pool)
///     .exists("category", "id", category_id, "Category not found")
///     .validate()
///     .await?;
/// ```
pub struct AsyncValidator<'a> {
    pool: &'a DbPool,
    rules: Vec<ValidationRule>,
}

enum ValidationRule {
    Unique { table: String, column: String, value: String, except_id: Option<i64>, message: String },
    Exists { table: String, column: String, value: i64, message: String },
}

impl<'a> AsyncValidator<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool, rules: Vec::new() }
    }

    /// Checks that a value is unique in a table.
    /// Returns `Conflict` when the value already exists.
    pub fn unique(mut self, table: &str, column: &str, value: &str, message: &str) -> Self {
        self.rules.push(ValidationRule::Unique {
            table: table.to_string(),
            column: column.to_string(),
            value: value.to_string(),
            except_id: None,
            message: message.to_string(),
        });
        self
    }

    /// Checks that a value is unique, leaving one ID out (for updates).
    /// Returns `Conflict` when ANOTHER record already holds this value.
    pub fn unique_except(mut self, table: &str, column: &str, value: &str, except_id: i64, message: &str) -> Self {
        self.rules.push(ValidationRule::Unique {
            table: table.to_string(),
            column: column.to_string(),
            value: value.to_string(),
            except_id: Some(except_id),
            message: message.to_string(),
        });
        self
    }

    /// Like `unique_except`, but only when the value is `Some`.
    /// Useful for partial updates with optional fields.
    pub fn unique_except_if_some(
        self,
        table: &str,
        column: &str,
        value: &Option<String>,
        except_id: i64,
        message: &str,
    ) -> Self {
        match value {
            Some(v) => self.unique_except(table, column, v, except_id, message),
            None => self,
        }
    }

    /// Checks that a record exists (e.g. a valid foreign key).
    /// Returns `NotFound` when it does not.
    pub fn exists(mut self, table: &str, column: &str, value: i64, message: &str) -> Self {
        self.rules.push(ValidationRule::Exists {
            table: table.to_string(),
            column: column.to_string(),
            value,
            message: message.to_string(),
        });
        self
    }

    /// Runs every rule. Returns the first error met.
    pub async fn validate(self) -> AppResult<()> {
        for rule in &self.rules {
            match rule {
                ValidationRule::Unique { table, column, value, except_id, message } => {
                    let sql = match except_id {
                        Some(_) => format!(
                            "SELECT COUNT(*) FROM {} WHERE {} = {} AND id != {}",
                            table,
                            column,
                            placeholder(1),
                            placeholder(2)
                        ),
                        None => format!("SELECT COUNT(*) FROM {} WHERE {} = {}", table, column, placeholder(1)),
                    };

                    let mut query = sqlx::query_as::<_, (i64,)>(AssertSqlSafe(sql)).bind(value);
                    if let Some(id) = except_id {
                        query = query.bind(*id);
                    }

                    let (count,) = query.fetch_one(self.pool).await?;
                    if count > 0 {
                        return Err(AppError::Conflict(message.clone()));
                    }
                }
                ValidationRule::Exists { table, column, value, message } => {
                    let sql = format!("SELECT COUNT(*) FROM {} WHERE {} = {}", table, column, placeholder(1));
                    let (count,): (i64,) = sqlx::query_as(AssertSqlSafe(sql)).bind(*value).fetch_one(self.pool).await?;
                    if count == 0 {
                        return Err(AppError::NotFound(message.clone()));
                    }
                }
            }
        }
        Ok(())
    }
}
