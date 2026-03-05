use serde::Serialize;
use sqlx::PgPool;

/// Represents a JSOBN column in discovered in the DB
#[derive(Debug, Clone, Serialize)]
pub struct JsonbColumn {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub estimated_rows: Option<i64>,
}

impl JsonbColumn {
    /// Get the fully qualified column name
    pub fn full_name(&self) -> String {
        format!("{}.{}.{}", self.schema, self.table, self.column)
    }
}

/// Discover all JSONB columns in the DB
///
/// Queries information_schema to find all columns with the type 'Jsonb',
/// excluding system schemas (pg_catalog, information_schema).
/// Also, fetch estimated row counts from pg_stat_user_tables
pub async fn discover_jsonb_columns(pool: &PgPool) -> Result<Vec<JsonbColumn>, sqlx::Error> {
    let columns = sqlx::query_as::<_, (String, String, String, Option<i64>)>(
        r#"
          SELECT
              c.table_schema,
              c.table_name,
              c.column_name,
              s.n_live_tup as estimated_rows
          FROM information_schema.columns c
          LEFT JOIN pg_stat_user_tables s
              ON s.schemaname = c.table_schema
              AND s.relname = c.table_name
          WHERE c.data_type = 'jsonb'
              AND c.table_schema NOT IN ('pg_catalog', 'information_schema')
          ORDER BY c.table_schema, c.table_name, c.column_name
          "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(schema, table, column, estimated_rows)| JsonbColumn {
        schema,
        table,
        column,
        estimated_rows,
    })
    .collect();

    Ok(columns)
}

/// Get row count estimate for a specific table.
///
/// This uses PostgreSQL metadata (`pg_stat_user_tables.n_live_tup` with
/// fallback to `pg_class.reltuples`) instead of `COUNT(*)` so strategy
/// selection stays fast and avoids long-running discovery queries.
pub async fn get_row_count(pool: &PgPool, schema: &str, table: &str) -> Result<i64, sqlx::Error> {
    let count: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT GREATEST(
            0,
            COALESCE(
                s.n_live_tup::bigint,
                c.reltuples::bigint,
                0
            )
        )
        FROM pg_class c
        JOIN pg_namespace n
            ON n.oid = c.relnamespace
        LEFT JOIN pg_stat_user_tables s
            ON s.relid = c.oid
        WHERE n.nspname = $1
          AND c.relname = $2
          AND c.relkind IN ('r', 'p')
        LIMIT 1
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_optional(pool)
    .await?;

    // Most PostgreSQL deployments: metadata estimate is sufficient.
    if let Some(c) = count
        && c > 0
    {
        return Ok(c);
    }

    // Citus fallback: if this is a distributed table and regular stats are
    // missing/stale, derive a conservative estimate from shard metadata.
    // This keeps sampler strategy selection safe without running COUNT(*).
    if is_citus_distributed_table(pool, schema, table).await? {
        let shard_count: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM pg_dist_shard s
            JOIN pg_class c
                ON c.oid = s.logicalrelid
            JOIN pg_namespace n
                ON n.oid = c.relnamespace
            WHERE n.nspname = $1
              AND c.relname = $2
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(pool)
        .await?;

        if let Some(shards) = shard_count
            && shards > 0
        {
            // Heuristic: assume at least 100k rows per shard when stats are unavailable.
            // This intentionally biases toward safer sampling strategies.
            return Ok((shards * 100_000).max(100_000));
        }
    }

    count.ok_or(sqlx::Error::RowNotFound)
}

async fn is_citus_distributed_table(
    pool: &PgPool,
    schema: &str,
    table: &str,
) -> Result<bool, sqlx::Error> {
    // Avoid touching Citus catalogs unless they exist.
    let has_dist_partition: bool =
        sqlx::query_scalar("SELECT to_regclass('pg_dist_partition') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !has_dist_partition {
        return Ok(false);
    }

    let distributed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM pg_dist_partition p
            JOIN pg_class c
                ON c.oid = p.logicalrelid
            JOIN pg_namespace n
                ON n.oid = c.relnamespace
            WHERE n.nspname = $1
              AND c.relname = $2
        )
        "#,
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await?;

    Ok(distributed)
}
