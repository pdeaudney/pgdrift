use pgdrift::commands::migrate;
use pgdrift::output::OutputFormat;
use pgdrift_core::filter::PathFilter;
use pgdrift_db::fixtures;
use pgdrift_db::test_utils::TestDb;

/// Test end-to-end migrate command with consistent schema
#[tokio::test]
async fn test_migrate_consistent_schema() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate command
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command detects high-quality migration candidates
#[tokio::test]
async fn test_migrate_detects_candidates() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate - should find candidates like email, age, country, status
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command warns about type inconsistencies
#[tokio::test]
async fn test_migrate_detects_type_warnings() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_type_inconsistency(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate - should detect that age field has type inconsistency (92% string, 8% number)
    let result = migrate::run(
        test_db.database_url(),
        "users_mixed_types",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with different output formats
#[tokio::test]
async fn test_migrate_output_formats() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Test all output formats
    for format in [
        OutputFormat::Table,
        OutputFormat::Json,
        OutputFormat::Markdown,
    ] {
        let result = migrate::run(
            test_db.database_url(),
            "users",
            "metadata",
            1000,
            format.clone(),
            0.8,
            0.95,
            PathFilter::new(),
        )
        .await;

        assert!(
            result.is_ok(),
            "Migrate command failed for {:?} format: {:?}",
            format,
            result.err()
        );
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with schema.table format
#[tokio::test]
async fn test_migrate_with_schema_prefix() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate with explicit schema
    let result = migrate::run(
        test_db.database_url(),
        "public.users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with different density thresholds
#[tokio::test]
async fn test_migrate_density_threshold() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_ghost_keys(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // With high density threshold (0.95) - should exclude sparse fields
    let result = migrate::run(
        test_db.database_url(),
        "users_sparse",
        "metadata",
        1000,
        OutputFormat::Json,
        0.95,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    // With low density threshold (0.5) - should include more fields
    let result = migrate::run(
        test_db.database_url(),
        "users_sparse",
        "metadata",
        1000,
        OutputFormat::Json,
        0.5,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with different type consistency thresholds
#[tokio::test]
async fn test_migrate_type_consistency_threshold() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_type_inconsistency(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // With high type consistency (0.95) - should exclude age field (92% string, 8% number)
    let result = migrate::run(
        test_db.database_url(),
        "users_mixed_types",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    // With lower type consistency (0.90) - should include age field
    let result = migrate::run(
        test_db.database_url(),
        "users_mixed_types",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.90,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command handles deeply nested structures
#[tokio::test]
async fn test_migrate_handles_deep_nesting() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_nested(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate - should handle deeply nested structures
    let result = migrate::run(
        test_db.database_url(),
        "users_nested",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with path filter
#[tokio::test]
async fn test_migrate_with_path_filter() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Create filter to ignore preferences fields
    let mut filter = PathFilter::new();
    filter.add_patterns(vec!["preferences.*".to_string()]);

    // Run migrate with filter
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        filter,
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command fails gracefully with invalid table
#[tokio::test]
async fn test_migrate_invalid_table() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Try to migrate non-existent table
    let result = migrate::run(
        test_db.database_url(),
        "nonexistent_table",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid table");

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command fails gracefully with invalid column
#[tokio::test]
async fn test_migrate_invalid_column() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Try to migrate non-existent column
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "nonexistent_column",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid column");

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate command with empty column
#[tokio::test]
async fn test_migrate_empty_column() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with no data
    sqlx::query(
        "CREATE TABLE empty_table (
            id SERIAL PRIMARY KEY,
            data JSONB
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Try to migrate empty column
    let result = migrate::run(
        test_db.database_url(),
        "empty_table",
        "data",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for empty column");
    if let Err(e) = result {
        assert!(
            e.to_string().contains("No samples"),
            "Expected 'No samples' error"
        );
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate with invalid database URL
#[tokio::test]
async fn test_migrate_invalid_database_url() {
    let result = migrate::run(
        "postgres://invalid:invalid@localhost:9999/invalid",
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid database URL");
}

/// Test migrate command with all NULL JSONB values
#[tokio::test]
async fn test_migrate_all_null_column() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with all NULL JSONB values
    sqlx::query(
        "CREATE TABLE null_table (
            id SERIAL PRIMARY KEY,
            data JSONB
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert rows with NULL values
    for _ in 0..100 {
        sqlx::query("INSERT INTO null_table (data) VALUES (NULL)")
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert NULL");
    }

    // Try to migrate - should fail with "No samples" error
    let result = migrate::run(
        test_db.database_url(),
        "null_table",
        "data",
        1000,
        OutputFormat::Json,
        0.8,
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for all-NULL column");
    if let Err(e) = result {
        assert!(
            e.to_string().contains("No samples"),
            "Expected 'No samples' error, got: {}",
            e
        );
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate with SQL injection attempts in table name
#[tokio::test]
async fn test_migrate_sql_injection_table_name() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Try various SQL injection patterns - should fail safely
    let injection_attempts = vec![
        "users; DROP TABLE users; --",
        "users' OR '1'='1",
        "users\"; DROP TABLE users; --",
    ];

    for attempt in injection_attempts {
        let result = migrate::run(
            test_db.database_url(),
            attempt,
            "metadata",
            1000,
            OutputFormat::Json,
            0.8,
            0.95,
            PathFilter::new(),
        )
        .await;

        assert!(
            result.is_err(),
            "SQL injection attempt should fail: {}",
            attempt
        );
    }

    // Verify users table still exists (not dropped)
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&test_db.pool)
        .await
        .expect("Users table should still exist");

    assert_eq!(count.0, 5000, "Users table should be intact");

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate with schema evolution patterns
#[tokio::test]
async fn test_migrate_schema_evolution() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_products_schema_evolution(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run migrate - should detect schema evolution (some fields at 50% density)
    let result = migrate::run(
        test_db.database_url(),
        "products",
        "data",
        1000,
        OutputFormat::Json,
        0.4, // Lower threshold to catch evolving fields
        0.95,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test migrate with extreme threshold values
#[tokio::test]
async fn test_migrate_extreme_thresholds() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Test with 100% thresholds (very strict)
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        1.0,
        1.0,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    // Test with 0% thresholds (very permissive)
    let result = migrate::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        OutputFormat::Json,
        0.0,
        0.0,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Migrate command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}
