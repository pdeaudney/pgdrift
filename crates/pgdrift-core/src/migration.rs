use crate::drift::Severity;
use crate::stats::FieldStats;
use crate::types::JsonType;
use serde::Serialize;
use serde_json::Value;
use std::fmt;

/// Configuration for migration candidate analysis
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Minimum field density to be considered for migration (default: 0.8)
    pub min_density: f64,
    /// Minimum type consistency to be considered safe (default: 0.95)
    pub min_type_consistency: f64,
    /// Maximum string length before recommending TEXT over VARCHAR (default: 255)
    pub varchar_threshold: usize,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            min_density: 0.8,
            min_type_consistency: 0.95,
            varchar_threshold: 255,
        }
    }
}

/// PostgreSQL type for native column migration
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "UPPERCASE")]
pub enum PostgresType {
    Text,
    Varchar { length: u32 },
    Integer,
    BigInt,
    Numeric { precision: u8, scale: u8 },
    DoublePrecision,
    Boolean,
    Timestamp,
    TimestampTz,
    Date,
}

impl fmt::Display for PostgresType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PostgresType::Text => write!(f, "TEXT"),
            PostgresType::Varchar { length } => write!(f, "VARCHAR({})", length),
            PostgresType::Integer => write!(f, "INTEGER"),
            PostgresType::BigInt => write!(f, "BIGINT"),
            PostgresType::Numeric { precision, scale } => {
                write!(f, "NUMERIC({},{})", precision, scale)
            }
            PostgresType::DoublePrecision => write!(f, "DOUBLE PRECISION"),
            PostgresType::Boolean => write!(f, "BOOLEAN"),
            PostgresType::Timestamp => write!(f, "TIMESTAMP"),
            PostgresType::TimestampTz => write!(f, "TIMESTAMPTZ"),
            PostgresType::Date => write!(f, "DATE"),
        }
    }
}

impl PostgresType {
    /// Infer PostgreSQL type from JSON field statistics
    pub fn infer_from_stats(stats: &FieldStats) -> Option<Self> {
        // Get dominant JSON type
        let dominant_type = stats
            .types
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| t)?;

        match dominant_type {
            JsonType::Boolean => Some(PostgresType::Boolean),
            JsonType::String => Self::infer_string_type(stats),
            JsonType::Number => Self::infer_number_type(stats),
            JsonType::Null | JsonType::Array | JsonType::Object => None,
        }
    }

    /// Infer string-based PostgreSQL type
    fn infer_string_type(stats: &FieldStats) -> Option<Self> {
        // Check max length
        if let Some(max_len) = stats.max_string_length {
            if max_len <= 255 {
                return Some(PostgresType::Varchar {
                    length: max_len.max(1) as u32,
                });
            }
        }

        // Default to TEXT for longer strings or unknown length
        Some(PostgresType::Text)
    }

    /// Infer number-based PostgreSQL type
    fn infer_number_type(stats: &FieldStats) -> Option<Self> {
        let has_decimals = stats.has_decimals.unwrap_or(false);

        if has_decimals {
            // Use NUMERIC for precision or DOUBLE PRECISION
            // For now, default to DOUBLE PRECISION
            return Some(PostgresType::DoublePrecision);
        }

        // Integer types - use range to determine size
        let min = stats.min_number.unwrap_or(0.0);
        let max = stats.max_number.unwrap_or(0.0);

        // Check if it fits in INTEGER range (-2^31 to 2^31-1)
        const INT_MIN: f64 = -2147483648.0;
        const INT_MAX: f64 = 2147483647.0;

        if min >= INT_MIN && max <= INT_MAX {
            Some(PostgresType::Integer)
        } else {
            // Use BIGINT for larger numbers
            Some(PostgresType::BigInt)
        }
    }

    /// Get the SQL cast expression for this type
    pub fn cast_expression(&self, json_path_expr: &str) -> String {
        match self {
            PostgresType::Text => json_path_expr.to_string(),
            PostgresType::Varchar { .. } => json_path_expr.to_string(),
            PostgresType::Boolean => format!("({})::{}", json_path_expr, self),
            PostgresType::Integer
            | PostgresType::BigInt
            | PostgresType::Numeric { .. }
            | PostgresType::DoublePrecision => format!("({})::{}", json_path_expr, self),
            PostgresType::Timestamp | PostgresType::TimestampTz | PostgresType::Date => {
                format!("({})::{}", json_path_expr, self)
            }
        }
    }
}

/// Warning for a migration candidate
#[derive(Debug, Clone, Serialize)]
pub struct MigrationWarning {
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Value>,
}

/// A candidate field for migration from JSONB to native column
#[derive(Debug, Clone, Serialize)]
pub struct MigrationCandidate {
    pub field_path: String,
    pub source_type: JsonType,
    pub target_type: PostgresType,
    pub density: f64,
    pub type_consistency: f64,
    pub nullable: bool,
    pub warnings: Vec<MigrationWarning>,

    // SQL fragments
    pub add_column_sql: String,
    pub backfill_sql: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint_sql: Option<String>,
}

impl MigrationCandidate {
    /// Generate a safe column name from a field path
    fn generate_column_name(field_path: &str) -> String {
        // Replace dots and array brackets with underscores
        field_path
            .replace('.', "_")
            .replace("[]", "_array")
            .replace('[', "_")
            .replace(']', "")
            .to_lowercase()
    }

    /// Get JSON path accessor for PostgreSQL
    fn json_accessor(jsonb_column: &str, field_path: &str, target_type: &PostgresType) -> String {
        // For text extraction, use ->> operator
        // For typed extraction, use -> then cast
        match target_type {
            PostgresType::Text | PostgresType::Varchar { .. } => {
                format!("{}->>'{}'", jsonb_column, field_path)
            }
            _ => format!("{}->'{}'", jsonb_column, field_path),
        }
    }

    /// Generate SQL fragments for this migration candidate
    pub fn new(
        stats: &FieldStats,
        jsonb_column: &str,
        config: &MigrationConfig,
    ) -> Option<Self> {
        // Check if field is scalar (not Array or Object)
        let dominant_type = stats
            .types
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| *t)?;

        if matches!(dominant_type, JsonType::Array | JsonType::Object) {
            return None; // Skip non-scalar types
        }

        // Calculate type consistency
        let total_typed = stats
            .types
            .iter()
            .filter(|(t, _)| !matches!(t, JsonType::Null))
            .map(|(_, count)| count)
            .sum::<u64>();

        let dominant_count = stats.types.get(&dominant_type).unwrap_or(&0);
        let type_consistency = if total_typed > 0 {
            *dominant_count as f64 / total_typed as f64
        } else {
            0.0
        };

        // Check if meets migration criteria
        if stats.density < config.min_density {
            return None;
        }

        // Infer target PostgreSQL type
        let target_type = PostgresType::infer_from_stats(stats)?;

        // Generate column name
        let column_name = Self::generate_column_name(&stats.path);

        // Generate SQL fragments
        let add_column_sql = format!("ADD COLUMN {} {}", column_name, target_type);

        let json_path_expr = Self::json_accessor(jsonb_column, &stats.path, &target_type);
        let cast_expr = target_type.cast_expression(&json_path_expr);
        let backfill_sql = format!("{} = {}", column_name, cast_expr);

        // Determine nullability
        let nullable = stats.null_count > 0 || stats.density < 1.0;

        // Generate constraint SQL for NOT NULL if appropriate
        let constraint_sql = if !nullable && stats.density >= 0.99 {
            Some(format!("ALTER COLUMN {} SET NOT NULL", column_name))
        } else {
            None
        };

        // Generate warnings
        let mut warnings = Vec::new();

        // Type inconsistency warning
        if type_consistency < config.min_type_consistency {
            let minority_percentage = (1.0 - type_consistency) * 100.0;
            warnings.push(MigrationWarning {
                severity: if type_consistency < 0.90 {
                    Severity::Critical
                } else {
                    Severity::Warning
                },
                message: format!(
                    "Type inconsistency: {:.1}% of values are not {}",
                    minority_percentage,
                    dominant_type
                ),
                examples: stats
                    .examples
                    .iter()
                    .filter(|v| JsonType::from_value(v) != dominant_type)
                    .take(3)
                    .cloned()
                    .collect(),
            });
        }

        // Low density warning
        if stats.density < 0.95 {
            warnings.push(MigrationWarning {
                severity: Severity::Info,
                message: format!(
                    "Field appears in only {:.1}% of samples ({}/{})",
                    stats.density * 100.0,
                    stats.occurrences,
                    stats.total_samples
                ),
                examples: vec![],
            });
        }

        Some(Self {
            field_path: stats.path.clone(),
            source_type: dominant_type,
            target_type,
            density: stats.density,
            type_consistency,
            nullable,
            warnings,
            add_column_sql,
            backfill_sql,
            constraint_sql,
        })
    }
}

/// Generate migration candidates from field statistics
pub fn generate_migration_candidates(
    stats: &[FieldStats],
    jsonb_column: &str,
    config: &MigrationConfig,
) -> Vec<MigrationCandidate> {
    stats
        .iter()
        .filter_map(|stat| MigrationCandidate::new(stat, jsonb_column, config))
        .collect()
}

/// Generate complete migration SQL script
pub fn generate_migration_sql(
    schema: &str,
    table: &str,
    jsonb_column: &str,
    candidates: &[MigrationCandidate],
) -> String {
    let mut sql = String::new();

    // Header
    sql.push_str(&format!(
        "-- Migration Guide for {}.{}.{}\n",
        schema, table, jsonb_column
    ));
    sql.push_str(&format!(
        "-- Generated by pgdrift on {}\n",
        chrono::Utc::now().format("%Y-%m-%d")
    ));
    sql.push_str("--\n");
    sql.push_str("-- ⚠️  WARNING: This is a generated migration guide.\n");
    sql.push_str("-- ⚠️  REVIEW CAREFULLY before executing any statements.\n");
    sql.push_str("-- ⚠️  Test on a staging environment first.\n");
    sql.push_str("--\n");

    // Summary
    let warning_count = candidates.iter().filter(|c| !c.warnings.is_empty()).count();
    sql.push_str("-- Summary:\n");
    sql.push_str(&format!("--   - {} fields eligible for migration\n", candidates.len()));
    sql.push_str(&format!("--   - {} fields with warnings\n", warning_count));
    sql.push_str("--\n\n");

    if candidates.is_empty() {
        sql.push_str("-- No fields meet migration criteria.\n");
        return sql;
    }

    // Step 1: Add columns
    sql.push_str("-- Step 1: Add new native columns\n");
    sql.push_str(&format!("ALTER TABLE {}.{}\n", quote_identifier(schema), quote_identifier(table)));
    for (i, candidate) in candidates.iter().enumerate() {
        if i > 0 {
            sql.push_str(",\n");
        }
        sql.push_str(&format!("  {}", candidate.add_column_sql));
    }
    sql.push_str(";\n\n");

    // Step 2: Backfill data
    sql.push_str("-- Step 2: Backfill data from JSONB\n");
    sql.push_str(&format!("UPDATE {}.{} SET\n", quote_identifier(schema), quote_identifier(table)));
    for (i, candidate) in candidates.iter().enumerate() {
        if i > 0 {
            sql.push_str(",\n");
        }
        sql.push_str(&format!("  {}", candidate.backfill_sql));
    }
    sql.push_str(";\n\n");

    // Step 3: Add constraints
    let constraints: Vec<_> = candidates
        .iter()
        .filter_map(|c| c.constraint_sql.as_ref())
        .collect();

    if !constraints.is_empty() {
        sql.push_str("-- Step 3: Add NOT NULL constraints (for high-density fields)\n");
        sql.push_str(&format!("ALTER TABLE {}.{}\n", quote_identifier(schema), quote_identifier(table)));
        for (i, constraint) in constraints.iter().enumerate() {
            if i > 0 {
                sql.push_str(",\n");
            }
            sql.push_str(&format!("  {}", constraint));
        }
        sql.push_str(";\n\n");
    }

    // Warnings section
    for candidate in candidates {
        if !candidate.warnings.is_empty() {
            sql.push_str(&format!("\n-- WARNING: Field '{}' has issues:\n", candidate.field_path));
            for warning in &candidate.warnings {
                sql.push_str(&format!("--   {}: {}\n", warning.severity, warning.message));
                if !warning.examples.is_empty() {
                    sql.push_str("--   Examples: ");
                    for (i, example) in warning.examples.iter().enumerate() {
                        if i > 0 {
                            sql.push_str(", ");
                        }
                        sql.push_str(&format!("{}", example));
                    }
                    sql.push_str("\n");
                }
            }
        }
    }

    sql
}

/// Quote a PostgreSQL identifier (schema/table/column name)
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_postgres_type_display() {
        assert_eq!(PostgresType::Text.to_string(), "TEXT");
        assert_eq!(PostgresType::Varchar { length: 50 }.to_string(), "VARCHAR(50)");
        assert_eq!(PostgresType::Integer.to_string(), "INTEGER");
        assert_eq!(PostgresType::BigInt.to_string(), "BIGINT");
        assert_eq!(
            PostgresType::Numeric {
                precision: 10,
                scale: 2
            }
            .to_string(),
            "NUMERIC(10,2)"
        );
        assert_eq!(PostgresType::Boolean.to_string(), "BOOLEAN");
    }

    #[test]
    fn test_infer_boolean_type() {
        let mut stats = FieldStats::new("is_active".to_string(), 0);
        stats.record(&json!(true));
        stats.record(&json!(false));
        stats.finalize(2);

        let pg_type = PostgresType::infer_from_stats(&stats);
        assert_eq!(pg_type, Some(PostgresType::Boolean));
    }

    #[test]
    fn test_infer_varchar_type() {
        let mut stats = FieldStats::new("email".to_string(), 0);
        stats.record(&json!("test@example.com"));
        stats.record(&json!("user@domain.org"));
        stats.finalize(2);

        let pg_type = PostgresType::infer_from_stats(&stats);
        // Max length is 16, should use VARCHAR
        assert!(matches!(pg_type, Some(PostgresType::Varchar { .. })));
    }

    #[test]
    fn test_infer_text_type_for_long_strings() {
        let mut stats = FieldStats::new("description".to_string(), 0);
        let long_string = "a".repeat(300);
        stats.record(&json!(long_string));
        stats.finalize(1);

        let pg_type = PostgresType::infer_from_stats(&stats);
        assert_eq!(pg_type, Some(PostgresType::Text));
    }

    #[test]
    fn test_infer_integer_type() {
        let mut stats = FieldStats::new("age".to_string(), 0);
        stats.record(&json!(25));
        stats.record(&json!(30));
        stats.record(&json!(45));
        stats.finalize(3);

        let pg_type = PostgresType::infer_from_stats(&stats);
        assert_eq!(pg_type, Some(PostgresType::Integer));
    }

    #[test]
    fn test_infer_bigint_type() {
        let mut stats = FieldStats::new("large_id".to_string(), 0);
        stats.record(&json!(9_000_000_000_i64));
        stats.finalize(1);

        let pg_type = PostgresType::infer_from_stats(&stats);
        assert_eq!(pg_type, Some(PostgresType::BigInt));
    }

    #[test]
    fn test_infer_double_precision_type() {
        let mut stats = FieldStats::new("price".to_string(), 0);
        stats.record(&json!(19.99));
        stats.record(&json!(29.50));
        stats.finalize(2);

        let pg_type = PostgresType::infer_from_stats(&stats);
        assert_eq!(pg_type, Some(PostgresType::DoublePrecision));
    }

    #[test]
    fn test_generate_column_name() {
        assert_eq!(
            MigrationCandidate::generate_column_name("user.email"),
            "user_email"
        );
        assert_eq!(
            MigrationCandidate::generate_column_name("items[].name"),
            "items_array_name"
        );
        assert_eq!(
            MigrationCandidate::generate_column_name("metadata.nested.field"),
            "metadata_nested_field"
        );
    }

    #[test]
    fn test_quote_identifier() {
        assert_eq!(quote_identifier("simple"), "\"simple\"");
        assert_eq!(quote_identifier("with\"quote"), "\"with\"\"quote\"");
        assert_eq!(quote_identifier("public"), "\"public\"");
    }

    #[test]
    fn test_migration_candidate_high_density() {
        let mut stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            stats.record(&json!("test@example.com"));
        }
        stats.finalize(100);

        let config = MigrationConfig::default();
        let candidate = MigrationCandidate::new(&stats, "metadata", &config);

        assert!(candidate.is_some());
        let candidate = candidate.unwrap();
        assert_eq!(candidate.field_path, "email");
        assert_eq!(candidate.density, 1.0);
        assert!(!candidate.nullable);
    }

    #[test]
    fn test_migration_candidate_low_density_excluded() {
        let mut stats = FieldStats::new("optional_field".to_string(), 0);
        for i in 0..100 {
            if i < 50 {
                stats.record(&json!("value"));
            }
        }
        stats.finalize(100);

        let config = MigrationConfig::default();
        let candidate = MigrationCandidate::new(&stats, "metadata", &config);

        // Should be None due to low density (50% < 80%)
        assert!(candidate.is_none());
    }

    #[test]
    fn test_migration_candidate_type_inconsistency_warning() {
        let mut stats = FieldStats::new("age".to_string(), 0);
        // 90 numbers, 10 strings
        for _ in 0..90 {
            stats.record(&json!(25));
        }
        for _ in 0..10 {
            stats.record(&json!("unknown"));
        }
        stats.finalize(100);

        let config = MigrationConfig::default();
        let candidate = MigrationCandidate::new(&stats, "metadata", &config);

        assert!(candidate.is_some());
        let candidate = candidate.unwrap();
        assert!(!candidate.warnings.is_empty());
        assert!(candidate.warnings[0]
            .message
            .contains("Type inconsistency"));
    }
}
