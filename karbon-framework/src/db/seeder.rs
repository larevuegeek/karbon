use std::future::Future;
use std::pin::Pin;

use super::DbPool;
use crate::error::AppResult;

/// Trait for database seeders.
///
/// ```ignore
/// struct UserSeeder;
///
/// impl Seeder for UserSeeder {
///     fn name(&self) -> &str { "users" }
///
///     fn seed<'a>(&'a self, pool: &'a DbPool) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>> {
///         Box::pin(async move {
///             NewUser { username: "admin".into(), ... }.insert(pool).await?;
///             NewUser { username: "user".into(), ... }.insert(pool).await?;
///             Ok(())
///         })
///     }
/// }
/// ```
pub trait Seeder: Send + Sync {
    fn name(&self) -> &str;
    fn seed<'a>(&'a self, pool: &'a DbPool) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>;
}

/// Run a list of seeders sequentially
pub async fn run_seeders(pool: &DbPool, seeders: &[Box<dyn Seeder>]) -> AppResult<()> {
    for seeder in seeders {
        tracing::info!("Seeding: {}", seeder.name());
        seeder.seed(pool).await?;
        tracing::info!("Seeded: {} ✓", seeder.name());
    }
    Ok(())
}
