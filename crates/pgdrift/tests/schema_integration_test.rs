use pgdrift::commands::schema;
use pgdrift_core::filter::PathFilter;
use pgdrift_db::fixtures;
use pgdrift_db::test_utils::TestDb;

/// Test end-to-end schema command with consistent schema
#[tokio::test]
async fn test_schema_consistent_schema() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema command - JSON Schema format
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command generates pg_jsonschema format
#[tokio::test]
async fn test_schema_pg_jsonschema_format() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema command - pg_jsonschema format
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::PgJsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with strict mode
#[tokio::test]
async fn test_schema_strict_mode() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema command with strict mode (additionalProperties: false)
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        true, // strict mode
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with relaxed mode
#[tokio::test]
async fn test_schema_relaxed_mode() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema command with relaxed mode (additionalProperties: true)
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false, // relaxed mode
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with different required thresholds
#[tokio::test]
async fn test_schema_required_threshold() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_ghost_keys(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // With high required threshold (0.95) - fewer required fields
    let result = schema::run(
        test_db.database_url(),
        "users_sparse",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    // With low required threshold (0.8) - more required fields
    let result = schema::run(
        test_db.database_url(),
        "users_sparse",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.8,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with schema.table format
#[tokio::test]
async fn test_schema_with_schema_prefix() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema with explicit schema
    let result = schema::run(
        test_db.database_url(),
        "public.users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command handles type inconsistencies
#[tokio::test]
async fn test_schema_type_inconsistency() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_type_inconsistency(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema - should handle type inconsistency (age field has both string and number)
    let result = schema::run(
        test_db.database_url(),
        "users_mixed_types",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command handles deeply nested structures
#[tokio::test]
async fn test_schema_handles_deep_nesting() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_nested(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema - should handle deeply nested structures
    let result = schema::run(
        test_db.database_url(),
        "users_nested",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with path filter
#[tokio::test]
async fn test_schema_with_path_filter() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Create filter to ignore preferences fields
    let mut filter = PathFilter::new();
    filter.add_patterns(vec!["preferences.*".to_string()]);

    // Run schema with filter
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        filter,
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command fails gracefully with invalid table
#[tokio::test]
async fn test_schema_invalid_table() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Try to generate schema for non-existent table
    let result = schema::run(
        test_db.database_url(),
        "nonexistent_table",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid table");

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command fails gracefully with invalid column
#[tokio::test]
async fn test_schema_invalid_column() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Try to generate schema for non-existent column
    let result = schema::run(
        test_db.database_url(),
        "users",
        "nonexistent_column",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid column");

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema command with empty column
#[tokio::test]
async fn test_schema_empty_column() {
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

    // Try to generate schema for empty column
    let result = schema::run(
        test_db.database_url(),
        "empty_table",
        "data",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
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

/// Test schema with invalid database URL
#[tokio::test]
async fn test_schema_invalid_database_url() {
    let result = schema::run(
        "postgres://invalid:invalid@localhost:9999/invalid",
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_err(), "Expected error for invalid database URL");
}

/// Test schema command with all NULL JSONB values
#[tokio::test]
async fn test_schema_all_null_column() {
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

    // Try to generate schema - should fail with "No samples" error
    let result = schema::run(
        test_db.database_url(),
        "null_table",
        "data",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
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

/// Test schema with SQL injection attempts in table name
#[tokio::test]
async fn test_schema_sql_injection_table_name() {
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
        let result = schema::run(
            test_db.database_url(),
            attempt,
            "metadata",
            1000,
            schema::SchemaFormat::JsonSchema,
            0.95,
            false,
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

/// Test schema with empty JSONB objects
#[tokio::test]
async fn test_schema_empty_json_objects() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    sqlx::query(
        "CREATE TABLE empty_objects (
            id SERIAL PRIMARY KEY,
            data JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert empty objects
    for _ in 0..100 {
        sqlx::query("INSERT INTO empty_objects (data) VALUES ($1)")
            .bind(serde_json::json!({}))
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert empty object");
    }

    // Should handle empty objects (generates schema with no properties)
    let result = schema::run(
        test_db.database_url(),
        "empty_objects",
        "data",
        100,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(
        result.is_ok(),
        "Should handle empty objects: {:?}",
        result.err()
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema with schema evolution patterns
#[tokio::test]
async fn test_schema_schema_evolution() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_products_schema_evolution(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Run schema - should detect schema evolution (some fields at 50% density)
    let result = schema::run(
        test_db.database_url(),
        "products",
        "data",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.4, // Lower threshold to include evolving fields
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema with extreme threshold values
#[tokio::test]
async fn test_schema_extreme_thresholds() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Test with 100% threshold (very strict)
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        1.0,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    // Test with 0% threshold (very permissive)
    let result = schema::run(
        test_db.database_url(),
        "users",
        "metadata",
        1000,
        schema::SchemaFormat::JsonSchema,
        0.0,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(result.is_ok(), "Schema command failed: {:?}", result.err());

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema format parsing
#[test]
fn test_schema_format_from_str() {
    // Test valid formats
    assert!(matches!(
        schema::SchemaFormat::from_str("json-schema").unwrap(),
        schema::SchemaFormat::JsonSchema
    ));
    assert!(matches!(
        schema::SchemaFormat::from_str("json").unwrap(),
        schema::SchemaFormat::JsonSchema
    ));
    assert!(matches!(
        schema::SchemaFormat::from_str("pg-jsonschema").unwrap(),
        schema::SchemaFormat::PgJsonSchema
    ));
    assert!(matches!(
        schema::SchemaFormat::from_str("pg").unwrap(),
        schema::SchemaFormat::PgJsonSchema
    ));
    assert!(matches!(
        schema::SchemaFormat::from_str("sql").unwrap(),
        schema::SchemaFormat::PgJsonSchema
    ));

    // Test invalid format
    assert!(schema::SchemaFormat::from_str("invalid").is_err());
}

/// Test schema with field type mutation (object -> primitive -> array)
#[tokio::test]
async fn test_schema_field_type_mutation() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    sqlx::query(
        "CREATE TABLE type_mutation (
            id SERIAL PRIMARY KEY,
            data JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Same field appears as different JSON types
    let test_cases = vec![
        serde_json::json!({"field": {"nested": "object"}}), // Object
        serde_json::json!({"field": "string value"}),       // String
        serde_json::json!({"field": 123}),                  // Number
        serde_json::json!({"field": true}),                 // Boolean
        serde_json::json!({"field": [1, 2, 3]}),            // Array
        serde_json::json!({"field": null}),                 // Null
    ];

    for case in test_cases {
        sqlx::query("INSERT INTO type_mutation (data) VALUES ($1)")
            .bind(case)
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert mutation data");
    }

    // Should detect extreme type inconsistency and handle it
    let result = schema::run(
        test_db.database_url(),
        "type_mutation",
        "data",
        100,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(
        result.is_ok(),
        "Should handle type mutations: {:?}",
        result.err()
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

/// Test schema detects common format patterns
#[tokio::test]
async fn test_schema_format_detection() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    sqlx::query(
        "CREATE TABLE format_test (
            id SERIAL PRIMARY KEY,
            data JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert data with various detectable formats
    for i in 0..100 {
        let data = serde_json::json!({
            "email": format!("user{}@example.com", i),
            "user_id": format!("550e8400-e29b-41d4-a716-{:012}", i),  // UUID format
            "created_at": "2025-01-12T10:30:00Z",  // date-time format
            "website": "https://example.com",       // uri format
            "age": 25 + i
        });

        sqlx::query("INSERT INTO format_test (data) VALUES ($1)")
            .bind(data)
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert data");
    }

    // Run schema generation - should detect email, uuid, date-time, uri formats
    let result = schema::run(
        test_db.database_url(),
        "format_test",
        "data",
        100,
        schema::SchemaFormat::JsonSchema,
        0.95,
        false,
        PathFilter::new(),
    )
    .await;

    assert!(
        result.is_ok(),
        "Should detect formats: {:?}",
        result.err()
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}
