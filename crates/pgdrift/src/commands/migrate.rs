use crate::output::OutputFormat;
use anyhow::{Context, Result};
use pgdrift_core::analyzer::JsonAnalyzer;
use pgdrift_core::filter::PathFilter;
use pgdrift_core::migration::{
    MigrationCandidate, MigrationConfig, generate_migration_candidates, generate_migration_sql,
};
use pgdrift_db::{ConnectionPool, Sampler};
use serde::Serialize;

/// Result structure for migration command
#[derive(Debug, Serialize)]
pub struct MigrationResult {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub total_samples: u64,
    pub candidates: Vec<MigrationCandidate>,
}

/// Configuration for migration command
pub struct MigrateCommandConfig {
    pub database_url: String,
    pub table: String,
    pub column: String,
    pub sample_size: usize,
    pub format: OutputFormat,
    pub min_density: f64,
    pub min_type_consistency: f64,
    pub filter: PathFilter,
}

impl MigrateCommandConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database_url: impl Into<String>,
        table: impl Into<String>,
        column: impl Into<String>,
        sample_size: usize,
        format: OutputFormat,
        min_density: f64,
        min_type_consistency: f64,
        filter: PathFilter,
    ) -> Self {
        Self {
            database_url: database_url.into(),
            table: table.into(),
            column: column.into(),
            sample_size,
            format,
            min_density,
            min_type_consistency,
            filter,
        }
    }
}

/// Run migration guide generation for a JSONB column
#[allow(clippy::too_many_arguments)]
pub async fn run(
    database_url: &str,
    table: &str,
    column: &str,
    sample_size: usize,
    format: OutputFormat,
    min_density: f64,
    min_type_consistency: f64,
    filter: PathFilter,
) -> Result<()> {
    let config = MigrateCommandConfig::new(
        database_url,
        table,
        column,
        sample_size,
        format,
        min_density,
        min_type_consistency,
        filter,
    );
    run_impl(config).await
}

/// Internal implementation that takes config struct
async fn run_impl(config: MigrateCommandConfig) -> Result<()> {
    let database_url = &config.database_url;
    let table = &config.table;
    let column = &config.column;
    let sample_size = config.sample_size;
    let format = config.format;
    let min_density = config.min_density;
    let min_type_consistency = config.min_type_consistency;
    let filter = config.filter;
    let (schema, table) = parse_table_name(table);

    // Show filter info if patterns are active
    if !filter.patterns().is_empty() {
        println!(
            "Applying {} ignore pattern(s): {}",
            filter.patterns().len(),
            filter.patterns().join(", ")
        );
    }

    let conn = ConnectionPool::new(database_url)
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
        anyhow::bail!("No samples found. Column may be empty or contain only NULL values.");
    }

    println!("Analyzing {} samples...", samples.len());

    // Analyze JSONB structure
    let mut analyzer = JsonAnalyzer::with_filter(filter);
    for sample in &samples {
        analyzer.analyze(sample);
    }
    let stats = analyzer.finalize();
    let field_stats: Vec<_> = stats.values().cloned().collect();

    // Generate migration candidates
    let config = MigrationConfig {
        min_density,
        min_type_consistency,
        ..Default::default()
    };

    let candidates = generate_migration_candidates(&field_stats, column, &config);

    println!(
        "\nFound {} field(s) eligible for migration",
        candidates.len()
    );

    let result = MigrationResult {
        schema: schema.clone(),
        table: table.clone(),
        column: column.to_string(),
        total_samples: samples.len() as u64,
        candidates,
    };

    print_migration_result(&result, &format, &schema, &table, column);

    Ok(())
}

/// Print migration results in the specified format
fn print_migration_result(
    result: &MigrationResult,
    format: &OutputFormat,
    schema: &str,
    table: &str,
    column: &str,
) {
    match format {
        OutputFormat::Table => {
            print_migration_table(result);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&result).unwrap_or_default();
            println!("{}", json);
        }
        OutputFormat::Markdown => {
            print_migration_markdown(result);
        }
    }

    // For table and markdown formats, also show the SQL at the end
    if matches!(format, OutputFormat::Table | OutputFormat::Markdown)
        && !result.candidates.is_empty()
    {
        println!("\n--- Generated Migration SQL ---\n");
        let sql = generate_migration_sql(schema, table, column, &result.candidates);
        println!("{}", sql);
    }
}

/// Print migration candidates as an ASCII table
fn print_migration_table(result: &MigrationResult) {
    use colored::Colorize;
    use tabled::{Table, Tabled};

    if result.candidates.is_empty() {
        println!("\n{}", "No fields meet migration criteria.".yellow());
        println!("  Hint: Lower --min-density or --min-type-consistency thresholds");
        return;
    }

    #[derive(Tabled)]
    struct MigrationRow {
        #[tabled(rename = "Field")]
        field: String,
        #[tabled(rename = "Source Type")]
        source_type: String,
        #[tabled(rename = "Target Type")]
        target_type: String,
        #[tabled(rename = "Density")]
        density: String,
        #[tabled(rename = "Type Consistency")]
        type_consistency: String,
        #[tabled(rename = "Nullable")]
        nullable: String,
        #[tabled(rename = "Warnings")]
        warnings: String,
    }

    let rows: Vec<MigrationRow> = result
        .candidates
        .iter()
        .map(|candidate| {
            let warnings_str = if candidate.warnings.is_empty() {
                "✓".green().to_string()
            } else {
                format!("{} issue(s)", candidate.warnings.len())
                    .yellow()
                    .to_string()
            };

            let nullable_str = if candidate.nullable {
                "Yes".to_string()
            } else {
                "No (NOT NULL)".green().to_string()
            };

            MigrationRow {
                field: candidate.field_path.clone(),
                source_type: format!("{}", candidate.source_type),
                target_type: format!("{}", candidate.target_type),
                density: format!("{:.1}%", candidate.density * 100.0),
                type_consistency: format!("{:.1}%", candidate.type_consistency * 100.0),
                nullable: nullable_str,
                warnings: warnings_str,
            }
        })
        .collect();

    let table = Table::new(rows).to_string();
    println!("\n{}", table);

    // Print warnings details
    for candidate in &result.candidates {
        if !candidate.warnings.is_empty() {
            println!("\n{} {}:", "⚠️ ".yellow(), candidate.field_path.bold());
            for warning in &candidate.warnings {
                println!("  • {} {}", warning.severity, warning.message);
                if !warning.examples.is_empty() {
                    print!("    Examples: ");
                    for (i, example) in warning.examples.iter().enumerate() {
                        if i > 0 {
                            print!(", ");
                        }
                        print!("{}", example);
                    }
                    println!();
                }
            }
        }
    }
}

/// Print migration guide as Markdown
fn print_migration_markdown(result: &MigrationResult) {
    println!(
        "# Migration Guide: {}.{}.{}\n",
        result.schema, result.table, result.column
    );
    println!("Generated: {}\n", chrono::Utc::now().format("%Y-%m-%d"));

    println!("## Summary\n");
    println!(
        "- **{} fields** eligible for migration",
        result.candidates.len()
    );
    let warning_count = result
        .candidates
        .iter()
        .filter(|c| !c.warnings.is_empty())
        .count();
    println!("- **{} fields** with warnings", warning_count);
    println!("- **{} samples** analyzed\n", result.total_samples);

    if result.candidates.is_empty() {
        println!("No fields meet migration criteria.\n");
        println!("**Hint**: Lower `--min-density` or `--min-type-consistency` thresholds");
        return;
    }

    println!("## Recommended Migrations\n");
    println!(
        "| Field | Source Type | Target Type | Density | Type Consistency | Nullable | Warnings |"
    );
    println!(
        "|-------|-------------|-------------|---------|------------------|----------|----------|"
    );

    for candidate in &result.candidates {
        let warnings_str = if candidate.warnings.is_empty() {
            "✅".to_string()
        } else {
            format!("⚠️ {}", candidate.warnings.len())
        };

        let nullable_str = if candidate.nullable { "Yes" } else { "No" };

        println!(
            "| {} | {} | {} | {:.1}% | {:.1}% | {} | {} |",
            candidate.field_path,
            candidate.source_type,
            candidate.target_type,
            candidate.density * 100.0,
            candidate.type_consistency * 100.0,
            nullable_str,
            warnings_str
        );
    }

    // Print warnings section
    let warnings: Vec<_> = result
        .candidates
        .iter()
        .filter(|c| !c.warnings.is_empty())
        .collect();

    if !warnings.is_empty() {
        println!("\n## Warnings\n");
        for candidate in warnings {
            println!("### ⚠️ {}\n", candidate.field_path);
            for warning in &candidate.warnings {
                println!("- **{}**: {}", warning.severity, warning.message);
                if !warning.examples.is_empty() {
                    print!("  - Examples: ");
                    for (i, example) in warning.examples.iter().enumerate() {
                        if i > 0 {
                            print!(", ");
                        }
                        print!("`{}`", example);
                    }
                    println!();
                }
            }
            println!();
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
}
