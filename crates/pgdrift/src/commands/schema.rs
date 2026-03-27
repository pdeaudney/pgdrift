use anyhow::{Context, Result};
use pgdrift_core::analyzer::JsonAnalyzer;
use pgdrift_core::filter::PathFilter;
use pgdrift_core::pattern::PatternConfig;
use pgdrift_core::schema::{JsonSchema, SchemaConfig, SchemaGenerator};
use pgdrift_db::{ConnectionPool, Sampler};
use std::str::FromStr;

/// Schema output format
#[derive(Debug, Clone)]
pub enum SchemaFormat {
    JsonSchema,
    PgJsonSchema,
}

impl FromStr for SchemaFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "json-schema" | "json" => Ok(Self::JsonSchema),
            "pg-jsonschema" | "pg" | "sql" => Ok(Self::PgJsonSchema),
            _ => anyhow::bail!(
                "Invalid schema format: {}. Use 'json-schema' or 'pg-jsonschema'",
                s
            ),
        }
    }
}

/// Configuration for schema generation command
pub struct SchemaCommandConfig {
    pub database_url: String,
    pub table: String,
    pub column: String,
    pub sample_size: usize,
    pub format: SchemaFormat,
    pub required_threshold: f64,
    pub strict: bool,
    pub filter: PathFilter,
    pub pattern_config: PatternConfig,
    pub pool_max_lifetime_secs: Option<u64>,
}

impl SchemaCommandConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database_url: impl Into<String>,
        table: impl Into<String>,
        column: impl Into<String>,
        sample_size: usize,
        format: SchemaFormat,
        required_threshold: f64,
        strict: bool,
        filter: PathFilter,
        pattern_config: PatternConfig,
        pool_max_lifetime_secs: Option<u64>,
    ) -> Self {
        Self {
            database_url: database_url.into(),
            table: table.into(),
            column: column.into(),
            sample_size,
            format,
            required_threshold,
            strict,
            filter,
            pattern_config,
            pool_max_lifetime_secs,
        }
    }
}

/// Run JSON schema generation for a JSONB column
#[allow(clippy::too_many_arguments)]
pub async fn run(
    database_url: &str,
    table: &str,
    column: &str,
    sample_size: usize,
    format: SchemaFormat,
    required_threshold: f64,
    strict: bool,
    filter: PathFilter,
    pattern_config: PatternConfig,
    pool_max_lifetime_secs: Option<u64>,
) -> Result<()> {
    let config = SchemaCommandConfig::new(
        database_url,
        table,
        column,
        sample_size,
        format,
        required_threshold,
        strict,
        filter,
        pattern_config,
        pool_max_lifetime_secs,
    );
    run_impl(config).await
}

/// Internal implementation that takes config struct
async fn run_impl(config: SchemaCommandConfig) -> Result<()> {
    let database_url = &config.database_url;
    let table = &config.table;
    let column = &config.column;
    let sample_size = config.sample_size;
    let format = config.format;
    let required_threshold = config.required_threshold;
    let strict = config.strict;
    let filter = config.filter;
    let pattern_config = config.pattern_config;
    let pool_max_lifetime_secs = config.pool_max_lifetime_secs;
    let (schema_name, table_name) = parse_table_name(table);

    // Show filter info if patterns are active
    if !filter.patterns().is_empty() {
        println!(
            "Applying {} ignore pattern(s): {}",
            filter.patterns().len(),
            filter.patterns().join(", ")
        );
    }

    // Show pattern info if custom patterns are configured
    if !pattern_config.patterns().is_empty() {
        println!(
            "Using {} custom pattern(s) for patternProperties",
            pattern_config.patterns().len()
        );
    }

    let conn = ConnectionPool::new_with_max_lifetime_secs(database_url, pool_max_lifetime_secs)
        .await
        .context("Failed to create database connection pool")?;

    conn.test_connection()
        .await
        .context("Failed to connect to the database")?;

    let sampler = Sampler::new(conn.pool(), &schema_name, &table_name, None, sample_size)
        .await
        .context("Failed to create sampler")?
        .show_progress(true);

    println!("\nSampling Strategy: {}", sampler.strategy_info());

    let samples = sampler
        .sample(conn.pool(), &schema_name, &table_name, column)
        .await
        .context("Failed to sample data")?;

    if samples.is_empty() {
        anyhow::bail!("No samples found. Column may be empty or contain only NULL values.");
    }

    println!("Analyzing {} samples...", samples.len());

    // Analyze JSONB structure
    let mut analyzer = JsonAnalyzer::with_filter(filter);
    for sample in &samples {
        analyzer.analyze(sample);
    }
    let is_root_array = analyzer.is_root_array();
    let stats = analyzer.finalize();
    let field_stats: Vec<_> = stats.values().cloned().collect();

    // Generate JSON Schema
    let config = SchemaConfig {
        required_threshold,
        strict_additional_properties: strict,
        ..Default::default()
    };

    let generator = SchemaGenerator::with_patterns(config, pattern_config);
    let json_schema = generator.generate_json_schema(
        &field_stats,
        Some(format!("{}.{}.{} schema", schema_name, table_name, column)),
        samples.len() as u64,
        is_root_array,
    );

    println!(
        "\nGenerated schema with {} properties",
        json_schema.properties.len()
    );
    println!("Required fields: {}", json_schema.required.len());

    print_schema_result(
        &json_schema,
        &format,
        &schema_name,
        &table_name,
        column,
        &generator,
    );

    Ok(())
}

/// Print schema results in the specified format
fn print_schema_result(
    schema: &JsonSchema,
    format: &SchemaFormat,
    db_schema: &str,
    table: &str,
    column: &str,
    generator: &SchemaGenerator,
) {
    match format {
        SchemaFormat::JsonSchema => {
            println!("\n--- JSON Schema (2020-12) ---\n");
            println!("{}", schema.to_json_string());
        }
        SchemaFormat::PgJsonSchema => {
            println!("\n--- pg_jsonschema CHECK Constraint ---\n");
            let sql = generator.generate_pg_check_constraint(schema, db_schema, table, column);
            println!("{}", sql);
        }
    }
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
        assert_eq!(
            parse_table_name("users"),
            ("public".to_string(), "users".to_string())
        );
        assert_eq!(
            parse_table_name("myschema.users"),
            ("myschema".to_string(), "users".to_string())
        );
    }

    #[test]
    fn test_schema_format_from_str() {
        assert!(matches!(
            SchemaFormat::from_str("json-schema").unwrap(),
            SchemaFormat::JsonSchema
        ));
        assert!(matches!(
            SchemaFormat::from_str("json").unwrap(),
            SchemaFormat::JsonSchema
        ));
        assert!(matches!(
            SchemaFormat::from_str("pg-jsonschema").unwrap(),
            SchemaFormat::PgJsonSchema
        ));
        assert!(matches!(
            SchemaFormat::from_str("pg").unwrap(),
            SchemaFormat::PgJsonSchema
        ));
        assert!(matches!(
            SchemaFormat::from_str("sql").unwrap(),
            SchemaFormat::PgJsonSchema
        ));
        assert!(SchemaFormat::from_str("invalid").is_err());
    }
}
