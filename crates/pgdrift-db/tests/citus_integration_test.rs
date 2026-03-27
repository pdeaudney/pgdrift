use pgdrift_db::Sampler;
use pgdrift_db::discovery::get_row_count;
use pgdrift_db::test_utils::TestDb;
use sqlx::PgPool;
use testcontainers::{
    GenericImage, ImageExt,
    core::{ContainerAsync, WaitFor},
    runners::AsyncRunner,
};

struct CitusTestDb {
    pool: PgPool,
    _container: ContainerAsync<GenericImage>,
}

impl CitusTestDb {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let image = GenericImage::new("citusdata/citus", "12.1.6")
            .with_wait_for(WaitFor::message_on_stderr(
                "database system is ready to accept connections",
            ))
            .with_env_var("POSTGRES_USER", "pgdrift")
            .with_env_var("POSTGRES_PASSWORD", "pgdrift_test")
            .with_env_var("POSTGRES_DB", "pgdrift_test");

        let container = image.start().await?;
        let port = container.get_host_port_ipv4(5432).await?;
        let database_url = format!(
            "postgres://pgdrift:pgdrift_test@127.0.0.1:{}/pgdrift_test",
            port
        );

        let mut attempts = 0;
        let pool = loop {
            match PgPool::connect(&database_url).await {
                Ok(p) => break p,
                Err(_) if attempts < 60 => {
                    attempts += 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                }
                Err(e) => return Err(Box::new(e)),
            }
        };

        Ok(Self {
            pool,
            _container: container,
        })
    }
}

#[tokio::test]
async fn test_citus_distributed_table_row_estimate_and_sampling() {
    if std::env::var("PGDRIFT_TEST_CITUS").ok().as_deref() != Some("1") {
        eprintln!("Skipping Citus integration test (set PGDRIFT_TEST_CITUS=1 to enable)");
        return;
    }

    let db = match CitusTestDb::new().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping Citus integration test (startup failed): {}", e);
            return;
        }
    };

    sqlx::query("CREATE EXTENSION IF NOT EXISTS citus")
        .execute(&db.pool)
        .await
        .expect("Failed to enable Citus extension");

    sqlx::query(
        r#"
        CREATE TABLE events (
            id BIGINT PRIMARY KEY,
            metadata JSONB NOT NULL
        )
        "#,
    )
    .execute(&db.pool)
    .await
    .expect("Failed to create events table");

    sqlx::query("SELECT create_distributed_table('events', 'id')")
        .execute(&db.pool)
        .await
        .expect("Failed to convert events to distributed table");

    sqlx::query(
        r#"
        INSERT INTO events (id, metadata)
        SELECT g, jsonb_build_object('n', g, 'kind', 'event')
        FROM generate_series(1, 500) g
        "#,
    )
    .execute(&db.pool)
    .await
    .expect("Failed to insert distributed table rows");

    let estimated = get_row_count(&db.pool, "public", "events")
        .await
        .expect("Failed to get row estimate for Citus distributed table");
    assert!(
        estimated > 0,
        "Expected positive row estimate for distributed table, got {}",
        estimated
    );

    let sampler = Sampler::new(&db.pool, "public", "events", None, 100)
        .await
        .expect("Failed to build sampler for distributed table")
        .show_progress(false);

    let samples = sampler
        .sample(&db.pool, "public", "events", "metadata")
        .await
        .expect("Failed to sample from distributed table");

    assert!(!samples.is_empty(), "Expected non-empty samples");
    assert!(
        samples.len() <= 100,
        "Expected sample limit to be respected"
    );
}

#[tokio::test]
async fn test_citus_fallback_uses_shard_catalog_when_stats_missing() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Base table exists but has no rows, so reltuples/n_live_tup should be 0.
    sqlx::query(
        r#"
        CREATE TABLE events (
            id BIGINT PRIMARY KEY,
            metadata JSONB NOT NULL
        )
        "#,
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create events table");

    // Mock minimal Citus catalogs in public schema so to_regclass('pg_dist_partition')
    // resolves and our Citus detection/fallback path can be exercised.
    sqlx::query(
        r#"
        CREATE TABLE pg_dist_partition (
            logicalrelid OID NOT NULL
        )
        "#,
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create mock pg_dist_partition");

    sqlx::query(
        r#"
        CREATE TABLE pg_dist_shard (
            logicalrelid OID NOT NULL,
            shardid BIGINT NOT NULL
        )
        "#,
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create mock pg_dist_shard");

    sqlx::query(
        r#"
        INSERT INTO pg_dist_partition (logicalrelid)
        VALUES ('public.events'::regclass::oid)
        "#,
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to populate mock pg_dist_partition");

    sqlx::query(
        r#"
        INSERT INTO pg_dist_shard (logicalrelid, shardid)
        VALUES
            ('public.events'::regclass::oid, 1),
            ('public.events'::regclass::oid, 2),
            ('public.events'::regclass::oid, 3)
        "#,
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to populate mock pg_dist_shard");

    let estimated = get_row_count(&test_db.pool, "public", "events")
        .await
        .expect("Expected mocked Citus fallback estimate");
    assert_eq!(
        estimated, 300_000,
        "Expected shard-based fallback estimate (3 * 100_000)"
    );

    let sampler = Sampler::new(&test_db.pool, "public", "events", None, 1_000)
        .await
        .expect("Failed to create sampler from Citus fallback estimate");
    let info = sampler.strategy_info();
    assert!(
        info.contains("Reservoir"),
        "Expected medium-table strategy from Citus fallback estimate, got: {}",
        info
    );
}
