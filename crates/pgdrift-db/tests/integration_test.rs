use pgdrift_db::discover_jsonb_columns;
use pgdrift_db::test_utils::TestDb;
use pgdrift_db::{Sampler, SamplingStrategy};

#[tokio::test]
async fn test_discover_consistent_schema() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create consistent fixture");

    let columns = discover_jsonb_columns(&test_db.pool)
        .await
        .expect("Failed to discover JSONB columns");

    // Should find the users.metadata column
    let users_metadata = columns
        .iter()
        .find(|col| col.table == "users" && col.column == "metadata");

    assert!(
        users_metadata.is_some(),
        "Failed to discover users.metadata column"
    );

    let col = users_metadata.unwrap();
    assert_eq!(col.schema, "public");
    // estimated_rows is from pg_stat_user_tables which isn't immediately updated
    // Just check it exists and is a reasonable number
    assert!(col.estimated_rows.is_some());
    assert!(col.estimated_rows.unwrap() > 0);

    test_db
        .cleanup()
        .await
        .expect("Failed to cleanup test database");
}

#[tokio::test]
async fn test_discover_multiple_tables() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create users fixture");

    pgdrift_db::fixtures::create_products_schema_evolution(&test_db.pool)
        .await
        .expect("Failed to create products fixture");

    let columns = discover_jsonb_columns(&test_db.pool)
        .await
        .expect("Failed to discover JSONB columns");

    // Should find both users.metadata and products.data
    assert!(
        columns.iter().any(|col| col.table == "users"),
        "Failed to find users table"
    );
    assert!(
        columns.iter().any(|col| col.table == "products"),
        "Failed to find products table"
    );

    test_db
        .cleanup()
        .await
        .expect("Failed to cleanup test database");
}

#[tokio::test]
async fn test_discover_sparse_schema() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_ghost_keys(&test_db.pool)
        .await
        .expect("Failed to create sparse fixture");

    let columns = discover_jsonb_columns(&test_db.pool)
        .await
        .expect("Failed to discover JSONB columns");

    let users_sparse = columns
        .iter()
        .find(|col| col.table == "users_sparse" && col.column == "metadata");

    assert!(
        users_sparse.is_some(),
        "Failed to discover users_sparse.metadata column"
    );

    test_db
        .cleanup()
        .await
        .expect("Failed to cleanup test database");
}

// ===== Sampler Integration Tests =====

#[tokio::test]
async fn test_sampler_auto_select_random_strategy() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Small table (5000 rows) should use Random strategy
    // Request fewer samples than rows to trigger Random sampling
    let sampler = Sampler::new(&test_db.pool, "public", "users", Some(5000), 100)
        .await
        .expect("Failed to create sampler");

    let info = sampler.strategy_info();
    assert!(
        info.contains("Random sampling"),
        "Expected Random strategy, got: {}",
        info
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_random_sampling() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Use Random strategy explicitly
    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 100 }).show_progress(false); // Disable progress bar for tests

    let samples = sampler
        .sample(&test_db.pool, "public", "users", "metadata")
        .await
        .expect("Failed to sample");

    // Should get samples (up to 100)
    assert!(!samples.is_empty(), "Expected samples but got none");
    assert!(samples.len() <= 100, "Got more samples than limit");

    // Each sample should be valid JSON
    for sample in &samples {
        assert!(sample.is_object(), "Expected JSON object in sample");
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_with_type_inconsistency() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_type_inconsistency(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 200 }).show_progress(false);

    let samples = sampler
        .sample(&test_db.pool, "public", "users_mixed_types", "metadata")
        .await
        .expect("Failed to sample");

    assert!(
        !samples.is_empty(),
        "Expected samples from mixed type table"
    );

    // Verify we got samples with the age field
    let has_age = samples.iter().any(|s| s.get("age").is_some());

    assert!(has_age, "Expected to find samples with 'age' field");

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_reservoir_strategy_selection() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Medium table (500k rows estimated) should use Reservoir strategy
    let sampler = Sampler::new(&test_db.pool, "public", "users", Some(500_000), 10_000)
        .await
        .expect("Failed to create sampler");

    let info = sampler.strategy_info();
    // Should use Reservoir (if PK found) or Random (fallback)
    assert!(
        info.contains("Reservoir") || info.contains("Random"),
        "Expected Reservoir or Random strategy, got: {}",
        info
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_tablesample_strategy_selection() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    // Large table (20M rows estimated) should use TABLESAMPLE
    let sampler = Sampler::new(&test_db.pool, "public", "users", Some(20_000_000), 10_000)
        .await
        .expect("Failed to create sampler");

    let info = sampler.strategy_info();
    assert!(
        info.contains("TABLESAMPLE"),
        "Expected TABLESAMPLE strategy, got: {}",
        info
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_handles_null_values() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with some NULL JSONB values
    sqlx::query(
        "CREATE TABLE test_nulls (
            id SERIAL PRIMARY KEY,
            data JSONB
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert mix of NULL and valid JSON
    for i in 0..10 {
        if i % 3 == 0 {
            sqlx::query("INSERT INTO test_nulls (data) VALUES (NULL)")
                .execute(&test_db.pool)
                .await
                .expect("Failed to insert NULL");
        } else {
            sqlx::query("INSERT INTO test_nulls (data) VALUES ($1)")
                .bind(serde_json::json!({"value": i}))
                .execute(&test_db.pool)
                .await
                .expect("Failed to insert data");
        }
    }

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 100 }).show_progress(false);

    let samples = sampler
        .sample(&test_db.pool, "public", "test_nulls", "data")
        .await
        .expect("Failed to sample");

    // Should only get non-NULL values (IS NOT NULL in query)
    assert!(!samples.is_empty(), "Expected non-null samples");
    assert!(samples.len() < 10, "Should filter out NULL values");

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_with_text_primary_key() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with TEXT primary key to test fallback from ReservoirPK to TABLESAMPLE
    sqlx::query(
        "CREATE TABLE test_text_pk (
            slug TEXT PRIMARY KEY,
            metadata JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert some test data
    for i in 0..100 {
        sqlx::query("INSERT INTO test_text_pk (slug, metadata) VALUES ($1, $2)")
            .bind(format!("item-{:04}", i))
            .bind(serde_json::json!({
                "id": i,
                "name": format!("Item {}", i),
                "value": i * 10
            }))
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert data");
    }

    // Simulate medium-sized table (would trigger ReservoirPK if PK was numeric)
    // With text PK, should fallback to TABLESAMPLE to avoid ORDER BY random() timeouts
    let sampler = Sampler::new(&test_db.pool, "public", "test_text_pk", Some(500_000), 50)
        .await
        .expect("Failed to create sampler");

    let info = sampler.strategy_info();
    // Should use TABLESAMPLE strategy since text PK can't be used for ReservoirPK
    assert!(
        info.contains("TABLESAMPLE"),
        "Expected TABLESAMPLE strategy for text PK in medium table, got: {}",
        info
    );

    // Verify sampling actually works without errors
    let samples = sampler
        .sample(&test_db.pool, "public", "test_text_pk", "metadata")
        .await
        .expect("Failed to sample table with text primary key");

    assert!(!samples.is_empty(), "Expected samples from text PK table");
    assert!(samples.len() <= 50, "Got more samples than limit");

    // Verify samples are valid JSON
    for sample in &samples {
        assert!(sample.is_object(), "Expected JSON object in sample");
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sampler_with_uuid_primary_key() {
    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with UUID primary key to test fallback from ReservoirPK to TABLESAMPLE
    sqlx::query(
        "CREATE TABLE test_uuid_pk (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            metadata JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert some test data
    for i in 0..100 {
        sqlx::query("INSERT INTO test_uuid_pk (metadata) VALUES ($1)")
            .bind(serde_json::json!({
                "item_id": i,
                "name": format!("Item {}", i),
                "value": i * 10
            }))
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert data");
    }

    // Simulate medium-sized table (would trigger ReservoirPK if PK was numeric)
    // With UUID PK, should fallback to TABLESAMPLE to avoid ORDER BY random() timeouts
    let sampler = Sampler::new(&test_db.pool, "public", "test_uuid_pk", Some(500_000), 50)
        .await
        .expect("Failed to create sampler");

    let info = sampler.strategy_info();
    // Should use TABLESAMPLE strategy since UUID PK can't be used for ReservoirPK
    assert!(
        info.contains("TABLESAMPLE"),
        "Expected TABLESAMPLE strategy for UUID PK in medium table, got: {}",
        info
    );

    // Verify sampling actually works without errors
    let samples = sampler
        .sample(&test_db.pool, "public", "test_uuid_pk", "metadata")
        .await
        .expect("Failed to sample table with UUID primary key");

    assert!(!samples.is_empty(), "Expected samples from UUID PK table");
    assert!(samples.len() <= 50, "Got more samples than limit");

    // Verify samples are valid JSON
    for sample in &samples {
        assert!(sample.is_object(), "Expected JSON object in sample");
    }

    test_db.cleanup().await.expect("Failed to cleanup");
}

// ===== Streaming Sampler Tests =====

#[tokio::test]
async fn test_sample_stream_returns_batches() {
    use futures::StreamExt;

    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 2500 }).show_progress(false);

    let mut stream = sampler.sample_stream(&test_db.pool, "public", "users", "metadata");

    let mut batch_count = 0;
    let mut total_samples = 0;

    while let Some(batch_result) = stream.next().await {
        let batch = batch_result.expect("Failed to get batch");

        assert!(!batch.is_empty(), "Batch should not be empty");
        assert!(
            batch.len() <= 1000,
            "Batch size should be at most 1000, got {}",
            batch.len()
        );

        // Each sample should be valid JSON
        for sample in &batch {
            assert!(sample.is_object(), "Expected JSON object in sample");
        }

        total_samples += batch.len();
        batch_count += 1;
    }

    // Should have gotten samples
    assert!(total_samples > 0, "Expected to receive samples");

    // With 2500 samples and batch size 1000, should have 3 batches
    // (1000, 1000, 500)
    assert!(
        batch_count >= 2,
        "Expected at least 2 batches for 2500 samples, got {}",
        batch_count
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sample_stream_backward_compatibility() {
    use futures::StreamExt;

    let test_db = TestDb::new().await.expect("Failed to create test database");

    pgdrift_db::fixtures::create_users_consistent(&test_db.pool)
        .await
        .expect("Failed to create fixture");

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 100 }).show_progress(false);

    // Collect all samples from stream
    let mut stream = sampler.sample_stream(&test_db.pool, "public", "users", "metadata");
    let mut stream_samples = Vec::new();

    while let Some(batch_result) = stream.next().await {
        let batch = batch_result.expect("Failed to get batch");
        stream_samples.extend(batch);
    }

    // Get samples using old API
    let sampler2 =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 100 }).show_progress(false);

    let old_api_samples = sampler2
        .sample(&test_db.pool, "public", "users", "metadata")
        .await
        .expect("Failed to sample");

    // Both should return similar number of samples
    // (exact count may vary due to randomness, but should be close)
    assert!(!stream_samples.is_empty(), "Stream should return samples");
    assert!(!old_api_samples.is_empty(), "Old API should return samples");

    // Both should be within reasonable range of the limit
    assert!(
        stream_samples.len() <= 100,
        "Stream samples should not exceed limit"
    );
    assert!(
        old_api_samples.len() <= 100,
        "Old API samples should not exceed limit"
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sample_stream_empty_table() {
    use futures::StreamExt;

    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create empty table
    sqlx::query(
        "CREATE TABLE empty_test (
            id SERIAL PRIMARY KEY,
            data JSONB
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 100 }).show_progress(false);

    let mut stream = sampler.sample_stream(&test_db.pool, "public", "empty_test", "data");

    let mut total_samples = 0;
    while let Some(batch_result) = stream.next().await {
        let batch = batch_result.expect("Failed to get batch");
        total_samples += batch.len();
    }

    // Empty table should return 0 samples
    assert_eq!(total_samples, 0, "Empty table should return no samples");

    test_db.cleanup().await.expect("Failed to cleanup");
}

#[tokio::test]
async fn test_sample_stream_large_dataset() {
    use futures::StreamExt;

    let test_db = TestDb::new().await.expect("Failed to create test database");

    // Create table with many rows to test batch behavior
    sqlx::query(
        "CREATE TABLE large_test (
            id SERIAL PRIMARY KEY,
            data JSONB NOT NULL
        )",
    )
    .execute(&test_db.pool)
    .await
    .expect("Failed to create table");

    // Insert 3500 rows
    for i in 0..3500 {
        sqlx::query("INSERT INTO large_test (data) VALUES ($1)")
            .bind(serde_json::json!({
                "id": i,
                "value": i * 2
            }))
            .execute(&test_db.pool)
            .await
            .expect("Failed to insert data");
    }

    let sampler =
        Sampler::with_strategy(SamplingStrategy::Random { limit: 3500 }).show_progress(false);

    let mut stream = sampler.sample_stream(&test_db.pool, "public", "large_test", "data");

    let mut batches = Vec::new();
    while let Some(batch_result) = stream.next().await {
        let batch = batch_result.expect("Failed to get batch");
        batches.push(batch);
    }

    // Should have multiple batches (1000, 1000, 1000, 500)
    assert!(batches.len() >= 3, "Expected at least 3 batches");

    // First batches should be full (1000 samples)
    for (i, batch) in batches.iter().enumerate().take(batches.len() - 1) {
        assert!(
            batch.len() >= 900,
            "Batch {} should have ~1000 samples, got {}",
            i,
            batch.len()
        );
    }

    // Last batch might be partial
    let last_batch = batches.last().unwrap();
    assert!(
        last_batch.len() <= 1000,
        "Last batch should not exceed 1000 samples"
    );

    test_db.cleanup().await.expect("Failed to cleanup");
}
