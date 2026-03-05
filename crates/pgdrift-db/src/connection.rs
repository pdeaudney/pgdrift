use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ConnectionPool {
    pool: PgPool,
}

impl ConnectionPool {
    /// Create a new connection pool from a database URL
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        Self::new_with_max_lifetime_secs(database_url, None).await
    }

    /// Create a new connection pool with optional max-lifetime override.
    ///
    /// If `max_lifetime_secs` is `None`, the value is read from
    /// `PGDRIFT_POOL_MAX_LIFETIME_SECS` (default: 45 seconds).
    pub async fn new_with_max_lifetime_secs(
        database_url: &str,
        max_lifetime_secs: Option<u64>,
    ) -> Result<Self, sqlx::Error> {
        let max_connections = read_env_u32("PGDRIFT_POOL_MAX_CONNECTIONS", 5);
        let acquire_timeout_secs = read_env_u64("PGDRIFT_POOL_ACQUIRE_TIMEOUT_SECS", 30);
        let max_lifetime_secs =
            max_lifetime_secs.unwrap_or_else(|| read_env_u64("PGDRIFT_POOL_MAX_LIFETIME_SECS", 45));
        let idle_timeout_secs = read_env_u64("PGDRIFT_POOL_IDLE_TIMEOUT_SECS", 15);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
            .max_lifetime(Some(Duration::from_secs(max_lifetime_secs)))
            .idle_timeout(Some(Duration::from_secs(idle_timeout_secs)))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Get a reference to the underlying PgPool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Test the database connection by executing a simple query
    pub async fn test_connection(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").fetch_one(&self.pool).await?;

        Ok(())
    }
}

fn read_env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}
