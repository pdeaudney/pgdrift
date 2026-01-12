use crate::stats::FieldStats;
use crate::types::JsonType;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Configuration for schema generation
#[derive(Debug, Clone)]
pub struct SchemaConfig {
    /// Minimum density for a field to be marked as required (default: 0.95)
    pub required_threshold: f64,
    /// Maximum number of distinct values to generate enum (default: 10)
    pub enum_max_values: usize,
    /// Minimum density for enum detection (default: 0.8)
    pub enum_min_density: f64,
    /// Whether to set additionalProperties: false (default: false)
    pub strict_additional_properties: bool,
    /// Whether to detect and set format hints (default: true)
    pub detect_formats: bool,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            required_threshold: 0.95,
            enum_max_values: 10,
            enum_min_density: 0.8,
            strict_additional_properties: false,
            detect_formats: true,
        }
    }
}

/// Property type in JSON Schema
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum PropertyType {
    /// Single type (e.g., "string")
    Single(String),
    /// Multiple types (e.g., ["string", "null"])
    Multiple(Vec<String>),
}

impl PropertyType {
    fn from_json_types(types: &HashMap<JsonType, u64>, allow_null: bool) -> Self {
        let mut type_names: Vec<String> = types
            .keys()
            .filter(|t| !matches!(t, JsonType::Null))
            .map(|t| t.to_json_schema_type())
            .collect();

        type_names.sort();
        type_names.dedup();

        if type_names.len() == 1 {
            if allow_null {
                PropertyType::Multiple(vec![type_names[0].clone(), "null".to_string()])
            } else {
                PropertyType::Single(type_names[0].clone())
            }
        } else {
            if allow_null && !type_names.contains(&"null".to_string()) {
                type_names.push("null".to_string());
            }
            PropertyType::Multiple(type_names)
        }
    }
}

/// Property schema definition
#[derive(Debug, Clone, Serialize)]
pub struct PropertySchema {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub property_type: Option<PropertyType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    // For nested objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, PropertySchema>>,

    #[serde(rename = "additionalProperties", skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<bool>,
}

impl PropertySchema {
    fn from_field_stats(stats: &FieldStats, config: &SchemaConfig) -> Self {
        let dominant_type = stats
            .types
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(t, _)| *t);

        let allow_null = stats.null_count > 0;
        let property_type = Some(PropertyType::from_json_types(&stats.types, allow_null));

        // Generate description
        let description = Some(format!(
            "Occurs in {:.1}% of samples ({}/{})",
            stats.density * 100.0,
            stats.occurrences,
            stats.total_samples
        ));

        // Detect enum values
        let enum_values = if let Some(ref distinct_values) = stats.distinct_values {
            if distinct_values.len() <= config.enum_max_values
                && stats.density >= config.enum_min_density
            {
                let mut values: Vec<Value> = distinct_values
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect();
                values.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                Some(values)
            } else {
                None
            }
        } else {
            None
        };

        // Detect format hints for strings
        let format = if config.detect_formats {
            if let Some(JsonType::String) = dominant_type {
                detect_string_format(&stats.path, &stats.examples)
            } else {
                None
            }
        } else {
            None
        };

        // Number constraints
        let (minimum, maximum) = if matches!(dominant_type, Some(JsonType::Number)) {
            (stats.min_number, stats.max_number)
        } else {
            (None, None)
        };

        Self {
            property_type,
            description,
            format,
            enum_values,
            minimum,
            maximum,
            pattern: None,
            properties: None,
            additional_properties: None,
        }
    }
}

/// Complete JSON Schema definition
#[derive(Debug, Clone, Serialize)]
pub struct JsonSchema {
    #[serde(rename = "$schema")]
    pub schema_version: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "type")]
    pub schema_type: String,

    pub properties: HashMap<String, PropertySchema>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,

    #[serde(rename = "additionalProperties")]
    pub additional_properties: bool,
}

impl JsonSchema {
    /// Convert to JSON Value for serialization
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }

    /// Convert to pretty-printed JSON string
    pub fn to_json_string(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::new())
    }
}

/// Schema generator
pub struct SchemaGenerator {
    config: SchemaConfig,
}

impl SchemaGenerator {
    pub fn new(config: SchemaConfig) -> Self {
        Self { config }
    }

    /// Generate JSON Schema from field statistics
    pub fn generate_json_schema(
        &self,
        stats: &[FieldStats],
        schema_name: Option<String>,
        total_samples: u64,
    ) -> JsonSchema {
        // Build property tree
        let mut root_properties: HashMap<String, PropertySchema> = HashMap::new();
        let mut required_fields = Vec::new();

        for stat in stats {
            // Only include top-level fields for now (no nested objects)
            if !stat.path.contains('.') && !stat.path.contains('[') {
                let property_schema = PropertySchema::from_field_stats(stat, &self.config);
                root_properties.insert(stat.path.clone(), property_schema);

                // Mark as required if density meets threshold
                if stat.density >= self.config.required_threshold {
                    required_fields.push(stat.path.clone());
                }
            }
        }

        required_fields.sort();

        JsonSchema {
            schema_version: "https://json-schema.org/draft-07/schema#".to_string(),
            title: schema_name,
            description: Some(format!(
                "Auto-generated schema from pgdrift analysis ({} samples)",
                total_samples
            )),
            schema_type: "object".to_string(),
            properties: root_properties,
            required: required_fields,
            additional_properties: !self.config.strict_additional_properties,
        }
    }

    /// Generate pg_jsonschema CHECK constraint SQL
    pub fn generate_pg_check_constraint(
        &self,
        schema: &JsonSchema,
        db_schema: &str,
        table: &str,
        column: &str,
    ) -> String {
        let mut sql = String::new();

        sql.push_str("-- Generated by pgdrift - Schema enforcement using pg_jsonschema\n");
        sql.push_str("--\n");
        sql.push_str("-- ⚠️  WARNING: Review this SQL before executing.\n");
        sql.push_str("-- ⚠️  Test the constraint on a staging environment first.\n");
        sql.push_str("-- ⚠️  This may reject existing data that violates the schema.\n");
        sql.push_str("--\n");
        sql.push_str("-- Install pg_jsonschema extension (if not already installed)\n");
        sql.push_str("-- CREATE EXTENSION IF NOT EXISTS pg_jsonschema;\n\n");

        let schema_json = schema.to_json_string();
        let constraint_name = format!("{}_{}_schema_check", table, column);

        sql.push_str(&format!(
            "ALTER TABLE {}.{}\n",
            quote_identifier(db_schema),
            quote_identifier(table)
        ));
        sql.push_str(&format!("  ADD CONSTRAINT {}\n", quote_identifier(&constraint_name)));
        sql.push_str("  CHECK (\n");
        sql.push_str("    json_matches_schema(\n");
        sql.push_str(&format!("      '{}'::json,\n", schema_json.replace('\'', "''")));
        sql.push_str(&format!("      {}\n", quote_identifier(column)));
        sql.push_str("    )\n");
        sql.push_str("  );\n\n");

        sql.push_str("-- Test the constraint (examples)\n");
        sql.push_str(&format!(
            "-- Valid: INSERT INTO {}.{} (..., {}) VALUES (..., '{{...}}');\n",
            db_schema, table, column
        ));
        sql.push_str(&format!(
            "-- Invalid: INSERT INTO {}.{} (..., {}) VALUES (..., '{{\"invalid\": ...}}');\n",
            db_schema, table, column
        ));

        sql
    }
}

/// Detect string format hints (email, uuid, date-time, etc.)
fn detect_string_format(field_path: &str, examples: &[Value]) -> Option<String> {
    // Check field name for common patterns
    let lower_path = field_path.to_lowercase();

    if lower_path.contains("email") {
        return Some("email".to_string());
    }

    if lower_path.contains("uuid") || lower_path.contains("guid") {
        return Some("uuid".to_string());
    }

    // Check examples for ISO 8601 date-time format
    let has_datetime_examples = examples.iter().any(|v| {
        if let Value::String(s) = v {
            s.contains('T') && s.len() > 10 // Basic ISO 8601 check
        } else {
            false
        }
    });

    // Detect date-time format from field name or examples
    if (lower_path.contains("date") || lower_path.contains("time") || lower_path.contains("timestamp") || lower_path.ends_with("_at"))
        && has_datetime_examples
    {
        return Some("date-time".to_string());
    }

    if lower_path.contains("url") || lower_path.contains("uri") {
        return Some("uri".to_string());
    }

    // Check examples for UUID pattern
    if examples.iter().all(|v| {
        if let Value::String(s) = v {
            is_uuid_format(s)
        } else {
            false
        }
    }) && !examples.is_empty()
    {
        return Some("uuid".to_string());
    }

    None
}

/// Check if a string matches UUID format
fn is_uuid_format(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts[0].len() == 8
        && parts[1].len() == 4
        && parts[2].len() == 4
        && parts[3].len() == 4
        && parts[4].len() == 12
        && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Quote a PostgreSQL identifier
fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

impl JsonType {
    /// Convert to JSON Schema type name
    fn to_json_schema_type(&self) -> String {
        match self {
            JsonType::Null => "null".to_string(),
            JsonType::Boolean => "boolean".to_string(),
            JsonType::Number => "number".to_string(),
            JsonType::String => "string".to_string(),
            JsonType::Array => "array".to_string(),
            JsonType::Object => "object".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_detect_email_format() {
        let examples = vec![json!("test@example.com"), json!("user@domain.org")];
        assert_eq!(
            detect_string_format("user_email", &examples),
            Some("email".to_string())
        );
    }

    #[test]
    fn test_detect_uuid_format_from_name() {
        let examples = vec![];
        assert_eq!(
            detect_string_format("user_uuid", &examples),
            Some("uuid".to_string())
        );
    }

    #[test]
    fn test_detect_uuid_format_from_value() {
        let examples = vec![
            json!("550e8400-e29b-41d4-a716-446655440000"),
            json!("6ba7b810-9dad-11d1-80b4-00c04fd430c8"),
        ];
        assert_eq!(
            detect_string_format("id", &examples),
            Some("uuid".to_string())
        );
    }

    #[test]
    fn test_detect_datetime_format() {
        let examples = vec![
            json!("2024-01-12T10:30:00Z"),
            json!("2024-01-13T15:45:00+00:00"),
        ];
        assert_eq!(
            detect_string_format("created_at", &examples),
            Some("date-time".to_string())
        );
    }

    #[test]
    fn test_is_uuid_format() {
        assert!(is_uuid_format("550e8400-e29b-41d4-a716-446655440000"));
        assert!(is_uuid_format("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
        assert!(!is_uuid_format("not-a-uuid"));
        assert!(!is_uuid_format("550e8400-e29b-41d4-a716"));
    }

    #[test]
    fn test_property_type_single() {
        let mut types = HashMap::new();
        types.insert(JsonType::String, 100);

        let prop_type = PropertyType::from_json_types(&types, false);
        match prop_type {
            PropertyType::Single(t) => assert_eq!(t, "string"),
            _ => panic!("Expected Single type"),
        }
    }

    #[test]
    fn test_property_type_with_null() {
        let mut types = HashMap::new();
        types.insert(JsonType::String, 90);
        types.insert(JsonType::Null, 10);

        let prop_type = PropertyType::from_json_types(&types, true);
        match prop_type {
            PropertyType::Multiple(types) => {
                assert_eq!(types.len(), 2);
                assert!(types.contains(&"string".to_string()));
                assert!(types.contains(&"null".to_string()));
            }
            _ => panic!("Expected Multiple types"),
        }
    }

    #[test]
    fn test_schema_generator_basic() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Email field - 100% density
        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("test@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // Age field - 80% density
        let mut age_stats = FieldStats::new("age".to_string(), 0);
        for _ in 0..80 {
            age_stats.record(&json!(25));
        }
        age_stats.finalize(100);
        stats.push(age_stats);

        let schema = generator.generate_json_schema(&stats, Some("Test Schema".to_string()), 100);

        assert_eq!(schema.schema_type, "object");
        assert_eq!(schema.properties.len(), 2);
        assert!(schema.properties.contains_key("email"));
        assert!(schema.properties.contains_key("age"));

        // Email should be required (100% density >= 95% threshold)
        assert!(schema.required.contains(&"email".to_string()));

        // Age should not be required (80% density < 95% threshold)
        assert!(!schema.required.contains(&"age".to_string()));
    }

    #[test]
    fn test_schema_enum_detection() {
        let mut config = SchemaConfig::default();
        config.enum_max_values = 3;
        config.enum_min_density = 0.8;

        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Role field with 3 distinct values
        let mut role_stats = FieldStats::new("role".to_string(), 0);
        for _ in 0..30 {
            role_stats.record(&json!("admin"));
        }
        for _ in 0..30 {
            role_stats.record(&json!("user"));
        }
        for _ in 0..40 {
            role_stats.record(&json!("guest"));
        }
        role_stats.finalize(100);
        stats.push(role_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        let role_prop = schema.properties.get("role").unwrap();
        assert!(role_prop.enum_values.is_some());
        let enum_vals = role_prop.enum_values.as_ref().unwrap();
        assert_eq!(enum_vals.len(), 3);
    }

    #[test]
    fn test_json_schema_to_json() {
        let schema = JsonSchema {
            schema_version: "https://json-schema.org/draft-07/schema#".to_string(),
            title: Some("Test".to_string()),
            description: Some("Test schema".to_string()),
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: vec![],
            additional_properties: true,
        };

        let json_val = schema.to_json();
        assert!(json_val.is_object());
        assert_eq!(json_val["$schema"], "https://json-schema.org/draft-07/schema#");
        assert_eq!(json_val["type"], "object");
    }
}
