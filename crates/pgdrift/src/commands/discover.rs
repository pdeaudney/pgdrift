use crate::output::{OutputFormat, print_columns};
use anyhow::{Context, Result};
use pgdrift_db::{ConnectionPool, discover_jsonb_columns};

/// runs the discover command to find JSONB columns in the database
pub async fn run(
    database_url: &str,
    format: OutputFormat,
    pool_max_lifetime_secs: Option<u64>,
) -> Result<()> {
    let conn = ConnectionPool::new_with_max_lifetime_secs(database_url, pool_max_lifetime_secs)
        .await
        .context("Failed to connect to database")?;

    conn.test_connection()
        .await
        .context("Failed to test database connection")?;

    let columns = discover_jsonb_columns(conn.pool())
        .await
        .context("Failed to discover JSONB columns")?;

    print_columns(&columns, &format);

    Ok(())
}
