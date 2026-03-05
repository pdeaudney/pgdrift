use crate::output::{AnalysisResult, OutputFormat, print_analysis};
use anyhow::{Context, Result};
use pgdrift_core::analyzer::JsonAnalyzer;
use pgdrift_core::drift::{DriftConfig, detect_drift};
use pgdrift_core::filter::PathFilter;
use pgdrift_db::{ConnectionPool, Sampler};

/// run performs analysis of a specified jsonb column in a PostgreSQL database
#[allow(dead_code)]
pub async fn run(
    database_url: &str,
    table: &str,
    column: &str,
    sample_size: usize,
    format: OutputFormat,
    filter: PathFilter,
) -> Result<()> {
    run_with_pool_lifetime(
        database_url,
        table,
        column,
        sample_size,
        format,
        filter,
        None,
    )
    .await
}

/// Run analysis with explicit pool lifetime override.
pub async fn run_with_pool_lifetime(
    database_url: &str,
    table: &str,
    column: &str,
    sample_size: usize,
    format: OutputFormat,
    filter: PathFilter,
    pool_max_lifetime_secs: Option<u64>,
) -> Result<()> {
    let result = run_with_result(
        database_url,
        table,
        column,
        sample_size,
        filter,
        pool_max_lifetime_secs,
    )
    .await?;
    print_analysis(&result, &format);
    Ok(())
}

/// Run analysis and return structured results without printing.
pub async fn run_with_result(
    database_url: &str,
    table: &str,
    column: &str,
    sample_size: usize,
    filter: PathFilter,
    pool_max_lifetime_secs: Option<u64>,
) -> Result<AnalysisResult> {
    let (schema, table) = parse_table_name(table);

    // Show filter info if patterns are active
    if !filter.patterns().is_empty() {
        println!(
            "Applying {} ignore pattern(s): {}",
            filter.patterns().len(),
            filter.patterns().join(", ")
        );
    }

    let conn = ConnectionPool::new_with_max_lifetime_secs(database_url, pool_max_lifetime_secs)
        .await
        .context("Failed to create database connection pool")?;

    conn.test_connection()
        .await
        .context("Failed to connect to the database")?;

    let sampler = Sampler::new(conn.pool(), &schema, &table, None, sample_size)
        .await
        .context("Failed to create sampler")?
        .show_progress(true);

    println!("\nSampling Strategy: {}", sampler.strategy_info());

    let samples = sampler
        .sample(conn.pool(), &schema, &table, column)
        .await
        .context("Failed to sample data")?;

    if samples.is_empty() {
        anyhow::bail!("No samples found. Column may be empty or NUILL.");
    }

    println!("Analyzing {} samples ...", samples.len());

    let mut analyzer = JsonAnalyzer::with_filter(filter);
    for sample in &samples {
        analyzer.analyze(sample)
    }
    let stats = analyzer.finalize();
    let mut field_stats: Vec<_> = stats.values().cloned().collect();
    field_stats.sort_by(|a, b| a.path.cmp(&b.path));

    let config = DriftConfig::default();
    let drift_issues = detect_drift(&stats, &config);

    let result = AnalysisResult {
        table: table.to_string(),
        column: column.to_string(),
        samples_analyzed: samples.len() as u64,
        field_stats,
        drift_issues,
    };

    Ok(result)
}

/// Parse table name into schema and table components
fn parse_table_name(table: &str) -> (String, String) {
    match table.split_once('.') {
        Some((schema, table)) => (schema.to_string(), table.to_string()),
        None => ("public".to_string(), table.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table_name() {
        let (schema, table) = parse_table_name("myschema.mytable");
        assert_eq!(schema, "myschema");
        assert_eq!(table, "mytable");

        let (schema, table) = parse_table_name("mytable");
        assert_eq!(schema, "public");
        assert_eq!(table, "mytable");

        assert_eq!(
            parse_table_name("users"),
            ("public".to_string(), "users".to_string())
        );

        assert_eq!(
            parse_table_name("myschema.users"),
            ("myschema".to_string(), "users".to_string())
        );

        assert_eq!(
            parse_table_name("public.orders"),
            ("public".to_string(), "orders".to_string())
        );
    }
}
