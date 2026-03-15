use crate::db::{DbPool, placeholder};
use crate::error::{AppError, AppResult};

/// Validateur asynchrone pour les règles qui nécessitent la base de données.
///
/// # Exemples
///
/// ```ignore
/// // Création — vérifier l'unicité
/// AsyncValidator::new(pool)
///     .unique("users", "username", &input.username, "Ce nom d'utilisateur est déjà pris")
///     .unique("users", "email", &input.email, "Cette adresse email est déjà utilisée")
///     .validate()
///     .await?;
///
/// // Mise à jour — exclure l'ID courant
/// AsyncValidator::new(pool)
///     .unique_except("users", "username", &input.username, user_id, "Ce nom d'utilisateur est déjà pris")
///     .unique_except("users", "email", &input.email, user_id, "Cette adresse email est déjà utilisée")
///     .validate()
///     .await?;
///
/// // Vérifier qu'une FK existe
/// AsyncValidator::new(pool)
///     .exists("category", "id", category_id, "Catégorie introuvable")
///     .validate()
///     .await?;
/// ```
pub struct AsyncValidator<'a> {
    pool: &'a DbPool,
    rules: Vec<ValidationRule>,
}

enum ValidationRule {
    Unique {
        table: String,
        column: String,
        value: String,
        except_id: Option<i64>,
        message: String,
    },
    Exists {
        table: String,
        column: String,
        value: i64,
        message: String,
    },
}

impl<'a> AsyncValidator<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self {
            pool,
            rules: Vec::new(),
        }
    }

    /// Vérifie qu'une valeur est unique dans une table.
    /// Retourne `Conflict` si la valeur existe déjà.
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

    /// Vérifie qu'une valeur est unique, en excluant un ID (pour les mises à jour).
    /// Retourne `Conflict` si un AUTRE enregistrement a déjà cette valeur.
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

    /// Comme `unique_except`, mais uniquement si la valeur est `Some`.
    /// Utile pour les mises à jour partielles avec des champs optionnels.
    pub fn unique_except_if_some(self, table: &str, column: &str, value: &Option<String>, except_id: i64, message: &str) -> Self {
        match value {
            Some(v) => self.unique_except(table, column, v, except_id, message),
            None => self,
        }
    }

    /// Vérifie qu'un enregistrement existe (ex: FK valide).
    /// Retourne `NotFound` si l'enregistrement n'existe pas.
    pub fn exists(mut self, table: &str, column: &str, value: i64, message: &str) -> Self {
        self.rules.push(ValidationRule::Exists {
            table: table.to_string(),
            column: column.to_string(),
            value,
            message: message.to_string(),
        });
        self
    }

    /// Exécute toutes les règles. Retourne la première erreur rencontrée.
    pub async fn validate(self) -> AppResult<()> {
        for rule in &self.rules {
            match rule {
                ValidationRule::Unique { table, column, value, except_id, message } => {
                    let sql = match except_id {
                        Some(_) => format!(
                            "SELECT COUNT(*) FROM {} WHERE {} = {} AND id != {}",
                            table, column, placeholder(1), placeholder(2)
                        ),
                        None => format!(
                            "SELECT COUNT(*) FROM {} WHERE {} = {}",
                            table, column, placeholder(1)
                        ),
                    };

                    let mut query = sqlx::query_as::<_, (i64,)>(&sql).bind(value);
                    if let Some(id) = except_id {
                        query = query.bind(*id);
                    }

                    let (count,) = query.fetch_one(self.pool).await?;
                    if count > 0 {
                        return Err(AppError::Conflict(message.clone()));
                    }
                }
                ValidationRule::Exists { table, column, value, message } => {
                    let sql = format!(
                        "SELECT COUNT(*) FROM {} WHERE {} = {}",
                        table, column, placeholder(1)
                    );
                    let (count,): (i64,) = sqlx::query_as(&sql)
                        .bind(*value)
                        .fetch_one(self.pool)
                        .await?;
                    if count == 0 {
                        return Err(AppError::NotFound(message.clone()));
                    }
                }
            }
        }
        Ok(())
    }
}
