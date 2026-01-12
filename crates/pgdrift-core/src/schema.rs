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
        // Filter out array paths for now (e.g., "items[].name")
        let non_array_stats: Vec<_> = stats
            .iter()
            .filter(|stat| !stat.path.contains('['))
            .collect();

        // Build nested property tree
        let (root_properties, required_fields) = self.build_property_tree(&non_array_stats);

        JsonSchema {
            schema_version: "https://json-schema.org/draft/2020-12/schema".to_string(),
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

    /// Build a nested property tree from field statistics
    fn build_property_tree(
        &self,
        stats: &[&FieldStats],
    ) -> (HashMap<String, PropertySchema>, Vec<String>) {
        let mut root_properties: HashMap<String, PropertySchema> = HashMap::new();
        let mut required_fields = Vec::new();

        // Group stats by root-level field
        for stat in stats {
            let parts: Vec<&str> = stat.path.split('.').collect();

            if parts.is_empty() {
                continue;
            }

            let root_field = parts[0];

            if parts.len() == 1 {
                // Top-level field
                let property_schema = PropertySchema::from_field_stats(stat, &self.config);
                root_properties.insert(root_field.to_string(), property_schema);

                if stat.density >= self.config.required_threshold {
                    required_fields.push(root_field.to_string());
                }
            } else {
                // Nested field - need to build nested structure
                // Get or create the root property
                let root_prop = root_properties
                    .entry(root_field.to_string())
                    .or_insert_with(|| PropertySchema {
                        property_type: Some(PropertyType::Single("object".to_string())),
                        description: None,
                        format: None,
                        enum_values: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        properties: Some(HashMap::new()),
                        additional_properties: Some(!self.config.strict_additional_properties),
                    });

                // Build nested path
                self.insert_nested_property(
                    root_prop,
                    &parts[1..],
                    stat,
                );
            }
        }

        // Sort required fields
        required_fields.sort();
        required_fields.dedup();

        (root_properties, required_fields)
    }

    /// Recursively insert a nested property into a property schema
    fn insert_nested_property(
        &self,
        parent: &mut PropertySchema,
        path_parts: &[&str],
        stat: &FieldStats,
    ) {
        if path_parts.is_empty() {
            return;
        }

        let field_name = path_parts[0];

        // Ensure parent has properties map
        let properties = parent.properties.get_or_insert_with(HashMap::new);

        if path_parts.len() == 1 {
            // Leaf node - insert the actual field
            let property_schema = PropertySchema::from_field_stats(stat, &self.config);
            properties.insert(field_name.to_string(), property_schema);
        } else {
            // Intermediate node - create/get nested object
            let nested_prop = properties
                .entry(field_name.to_string())
                .or_insert_with(|| PropertySchema {
                    property_type: Some(PropertyType::Single("object".to_string())),
                    description: None,
                    format: None,
                    enum_values: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    properties: Some(HashMap::new()),
                    additional_properties: Some(!self.config.strict_additional_properties),
                });

            // Recurse
            self.insert_nested_property(nested_prop, &path_parts[1..], stat);
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
            schema_version: "https://json-schema.org/draft/2020-12/schema".to_string(),
            title: Some("Test".to_string()),
            description: Some("Test schema".to_string()),
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: vec![],
            additional_properties: true,
        };

        let json_val = schema.to_json();
        assert!(json_val.is_object());
        assert_eq!(json_val["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(json_val["type"], "object");
    }

    // ===== JSON Schema 2020-12 Validation Tests =====

    /// Helper function to validate a generated schema against JSON Schema 2020-12 meta-schema
    fn validate_json_schema(schema: &JsonSchema) -> Result<(), String> {
        // JSON Schema 2020-12 meta-schema (core subset for validation)
        // This validates the structure is a valid JSON Schema
        let meta_schema_json = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "$schema": {"type": "string"},
                "type": {"type": "string"},
                "properties": {"type": "object"},
                "required": {
                    "type": "array",
                    "items": {"type": "string"}
                },
                "additionalProperties": {"type": "boolean"},
                "title": {"type": "string"},
                "description": {"type": "string"}
            }
        });

        let compiled = jsonschema::validator_for(&meta_schema_json)
            .map_err(|e| format!("Failed to compile meta-schema: {}", e))?;

        let schema_json = schema.to_json();

        // The validator returns Result<(), ValidationError>
        // If it fails, we get a single ValidationError
        match compiled.validate(&schema_json) {
            Ok(_) => Ok(()),
            Err(error) => {
                Err(format!("Schema validation failed: {}", error))
            }
        }
    }

    #[test]
    fn test_validate_empty_schema() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let schema = generator.generate_json_schema(&[], None, 0);

        // Validate the schema is valid JSON Schema 2020-12
        assert!(
            validate_json_schema(&schema).is_ok(),
            "Empty schema should be valid JSON Schema"
        );
    }

    #[test]
    fn test_validate_basic_schema() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Email field
        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("test@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        let schema = generator.generate_json_schema(&stats, Some("Test Schema".to_string()), 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Basic schema should be valid JSON Schema 2020-12");

        // Verify structure
        assert_eq!(schema.schema_version, "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.contains_key("email"));
    }

    #[test]
    fn test_validate_schema_with_all_types() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // String field
        let mut string_stats = FieldStats::new("name".to_string(), 0);
        for _ in 0..100 {
            string_stats.record(&json!("Alice"));
        }
        string_stats.finalize(100);
        stats.push(string_stats);

        // Number field
        let mut number_stats = FieldStats::new("age".to_string(), 0);
        for _ in 0..100 {
            number_stats.record(&json!(25));
        }
        number_stats.finalize(100);
        stats.push(number_stats);

        // Boolean field
        let mut bool_stats = FieldStats::new("active".to_string(), 0);
        for _ in 0..100 {
            bool_stats.record(&json!(true));
        }
        bool_stats.finalize(100);
        stats.push(bool_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with multiple types should be valid");

        // Verify all fields are present
        assert!(schema.properties.contains_key("name"));
        assert!(schema.properties.contains_key("age"));
        assert!(schema.properties.contains_key("active"));
    }

    #[test]
    fn test_validate_schema_with_enum() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Enum field with low cardinality
        let mut role_stats = FieldStats::new("role".to_string(), 0);
        for _ in 0..50 {
            role_stats.record(&json!("admin"));
        }
        for _ in 0..30 {
            role_stats.record(&json!("user"));
        }
        for _ in 0..20 {
            role_stats.record(&json!("guest"));
        }
        role_stats.finalize(100);
        stats.push(role_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with enum should be valid");

        // Verify enum is present
        let role_prop = schema.properties.get("role").unwrap();
        assert!(role_prop.enum_values.is_some());
    }

    #[test]
    fn test_validate_schema_with_number_constraints() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Number field with min/max
        let mut score_stats = FieldStats::new("score".to_string(), 0);
        for i in 0..100 {
            score_stats.record(&json!(i));
        }
        score_stats.finalize(100);
        stats.push(score_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with number constraints should be valid");

        // Verify constraints are present
        let score_prop = schema.properties.get("score").unwrap();
        assert!(score_prop.minimum.is_some());
        assert!(score_prop.maximum.is_some());
    }

    #[test]
    fn test_validate_schema_with_format_hints() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Email field
        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("user@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // UUID field
        let mut uuid_stats = FieldStats::new("user_id".to_string(), 0);
        for _ in 0..100 {
            uuid_stats.record(&json!("550e8400-e29b-41d4-a716-446655440000"));
        }
        uuid_stats.finalize(100);
        stats.push(uuid_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with format hints should be valid");

        // Verify format hints are present
        let email_prop = schema.properties.get("email").unwrap();
        assert_eq!(email_prop.format, Some("email".to_string()));

        let uuid_prop = schema.properties.get("user_id").unwrap();
        assert_eq!(uuid_prop.format, Some("uuid".to_string()));
    }

    #[test]
    fn test_validate_schema_with_required_fields() {
        let config = SchemaConfig {
            required_threshold: 0.95,
            ..Default::default()
        };
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // High density field (should be required)
        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("test@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // Low density field (should not be required)
        let mut optional_stats = FieldStats::new("optional".to_string(), 0);
        for _ in 0..80 {
            optional_stats.record(&json!("value"));
        }
        optional_stats.finalize(100);
        stats.push(optional_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with required fields should be valid");

        // Verify required fields
        assert!(schema.required.contains(&"email".to_string()));
        assert!(!schema.required.contains(&"optional".to_string()));
    }

    #[test]
    fn test_validate_schema_strict_mode() {
        let config = SchemaConfig {
            strict_additional_properties: true,
            ..Default::default()
        };
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("test@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Strict mode schema should be valid");

        // Verify strict mode
        assert!(!schema.additional_properties);
    }

    #[test]
    fn test_validate_schema_with_null_values() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Field with some null values
        let mut nullable_stats = FieldStats::new("nickname".to_string(), 0);
        for _ in 0..70 {
            nullable_stats.record(&json!("Bob"));
        }
        for _ in 0..30 {
            nullable_stats.record(&json!(null));
        }
        nullable_stats.finalize(100);
        stats.push(nullable_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with nullable fields should be valid");
    }

    #[test]
    fn test_validate_schema_with_mixed_types() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Field with mixed types (string and number)
        let mut mixed_stats = FieldStats::new("value".to_string(), 0);
        for _ in 0..60 {
            mixed_stats.record(&json!("text"));
        }
        for _ in 0..40 {
            mixed_stats.record(&json!(42));
        }
        mixed_stats.finalize(100);
        stats.push(mixed_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with mixed types should be valid");
    }

    #[test]
    fn test_validate_complex_realistic_schema() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Email (required, with format)
        let mut email_stats = FieldStats::new("email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("user@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // Age (required, with constraints)
        let mut age_stats = FieldStats::new("age".to_string(), 0);
        for i in 18..118 {
            age_stats.record(&json!(i));
        }
        age_stats.finalize(100);
        stats.push(age_stats);

        // Role (enum)
        let mut role_stats = FieldStats::new("role".to_string(), 0);
        for _ in 0..50 {
            role_stats.record(&json!("user"));
        }
        for _ in 0..30 {
            role_stats.record(&json!("admin"));
        }
        for _ in 0..20 {
            role_stats.record(&json!("guest"));
        }
        role_stats.finalize(100);
        stats.push(role_stats);

        // Active (boolean, required)
        let mut active_stats = FieldStats::new("is_active".to_string(), 0);
        for _ in 0..100 {
            active_stats.record(&json!(true));
        }
        active_stats.finalize(100);
        stats.push(active_stats);

        // UUID
        let mut uuid_stats = FieldStats::new("user_id".to_string(), 0);
        for _ in 0..100 {
            uuid_stats.record(&json!("550e8400-e29b-41d4-a716-446655440000"));
        }
        uuid_stats.finalize(100);
        stats.push(uuid_stats);

        // Nullable field
        let mut bio_stats = FieldStats::new("bio".to_string(), 0);
        for _ in 0..70 {
            bio_stats.record(&json!("Software developer"));
        }
        for _ in 0..30 {
            bio_stats.record(&json!(null));
        }
        bio_stats.finalize(100);
        stats.push(bio_stats);

        let schema = generator.generate_json_schema(
            &stats,
            Some("User Schema".to_string()),
            100
        );

        // Validate the complex schema
        validate_json_schema(&schema).expect("Complex realistic schema should be valid JSON Schema 2020-12");

        // Verify key properties
        assert_eq!(schema.schema_version, "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(schema.title, Some("User Schema".to_string()));
        assert_eq!(schema.properties.len(), 6);
        assert!(schema.required.contains(&"email".to_string()));
        assert!(schema.required.contains(&"age".to_string()));
        assert!(schema.required.contains(&"role".to_string()));
        assert!(schema.required.contains(&"is_active".to_string()));
        assert!(schema.required.contains(&"user_id".to_string()));
    }

    // ===== Nested Object Schema Generation Tests =====

    #[test]
    fn test_schema_with_simple_nested_object() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Nested field: user.name
        let mut name_stats = FieldStats::new("user.name".to_string(), 0);
        for _ in 0..100 {
            name_stats.record(&json!("Alice"));
        }
        name_stats.finalize(100);
        stats.push(name_stats);

        // Nested field: user.email
        let mut email_stats = FieldStats::new("user.email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("alice@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with nested objects should be valid");

        // Verify structure
        assert!(schema.properties.contains_key("user"));
        let user_prop = schema.properties.get("user").unwrap();

        // User should be an object with properties
        match &user_prop.property_type {
            Some(PropertyType::Single(t)) => assert_eq!(t, "object"),
            _ => panic!("Expected user to be object type"),
        }

        assert!(user_prop.properties.is_some());
        let user_props = user_prop.properties.as_ref().unwrap();
        assert!(user_props.contains_key("name"));
        assert!(user_props.contains_key("email"));
    }

    #[test]
    fn test_schema_with_deeply_nested_object() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Deeply nested: user.profile.settings.theme
        let mut theme_stats = FieldStats::new("user.profile.settings.theme".to_string(), 0);
        for _ in 0..100 {
            theme_stats.record(&json!("dark"));
        }
        theme_stats.finalize(100);
        stats.push(theme_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Deeply nested schema should be valid");

        // Navigate the nested structure
        let user_prop = schema.properties.get("user").unwrap();
        let user_props = user_prop.properties.as_ref().unwrap();

        let profile_prop = user_props.get("profile").unwrap();
        let profile_props = profile_prop.properties.as_ref().unwrap();

        let settings_prop = profile_props.get("settings").unwrap();
        let settings_props = settings_prop.properties.as_ref().unwrap();

        assert!(settings_props.contains_key("theme"));
    }

    #[test]
    fn test_schema_with_mixed_top_level_and_nested() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Top-level field
        let mut id_stats = FieldStats::new("id".to_string(), 0);
        for _ in 0..100 {
            id_stats.record(&json!(123));
        }
        id_stats.finalize(100);
        stats.push(id_stats);

        // Nested field
        let mut name_stats = FieldStats::new("user.name".to_string(), 0);
        for _ in 0..100 {
            name_stats.record(&json!("Alice"));
        }
        name_stats.finalize(100);
        stats.push(name_stats);

        // Another top-level field
        let mut active_stats = FieldStats::new("active".to_string(), 0);
        for _ in 0..100 {
            active_stats.record(&json!(true));
        }
        active_stats.finalize(100);
        stats.push(active_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Mixed schema should be valid");

        // Verify top-level fields
        assert!(schema.properties.contains_key("id"));
        assert!(schema.properties.contains_key("active"));
        assert!(schema.properties.contains_key("user"));

        // Verify nested structure
        let user_prop = schema.properties.get("user").unwrap();
        let user_props = user_prop.properties.as_ref().unwrap();
        assert!(user_props.contains_key("name"));
    }

    #[test]
    fn test_schema_nested_object_with_multiple_fields() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Multiple fields in same nested object
        let fields = vec![
            ("address.street", json!("123 Main St")),
            ("address.city", json!("New York")),
            ("address.state", json!("NY")),
            ("address.zip", json!("10001")),
        ];

        for (path, value) in fields {
            let mut field_stats = FieldStats::new(path.to_string(), 0);
            for _ in 0..100 {
                field_stats.record(&value);
            }
            field_stats.finalize(100);
            stats.push(field_stats);
        }

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Nested object with multiple fields should be valid");

        // Verify all fields are in the address object
        let address_prop = schema.properties.get("address").unwrap();
        let address_props = address_prop.properties.as_ref().unwrap();
        assert_eq!(address_props.len(), 4);
        assert!(address_props.contains_key("street"));
        assert!(address_props.contains_key("city"));
        assert!(address_props.contains_key("state"));
        assert!(address_props.contains_key("zip"));
    }

    #[test]
    fn test_schema_nested_with_required_fields() {
        let config = SchemaConfig {
            required_threshold: 0.95,
            ..Default::default()
        };
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Top-level required field
        let mut id_stats = FieldStats::new("id".to_string(), 0);
        for _ in 0..100 {
            id_stats.record(&json!(123));
        }
        id_stats.finalize(100);
        stats.push(id_stats);

        // Required nested field (user.email - 100% density)
        let mut email_stats = FieldStats::new("user.email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("user@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // Optional nested field (user.bio - 80% density)
        let mut bio_stats = FieldStats::new("user.bio".to_string(), 0);
        for _ in 0..80 {
            bio_stats.record(&json!("Developer"));
        }
        bio_stats.finalize(100);
        stats.push(bio_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Nested schema with required fields should be valid");

        // Top-level required field
        assert!(schema.required.contains(&"id".to_string()));

        // Note: Nested objects themselves don't become required unless we have stats for the parent object
        // The required logic applies at each level independently
        assert!(schema.properties.contains_key("user"));
    }

    #[test]
    fn test_schema_nested_with_format_detection() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Nested email field
        let mut email_stats = FieldStats::new("contact.email".to_string(), 0);
        for _ in 0..100 {
            email_stats.record(&json!("user@example.com"));
        }
        email_stats.finalize(100);
        stats.push(email_stats);

        // Nested UUID field
        let mut uuid_stats = FieldStats::new("contact.user_id".to_string(), 0);
        for _ in 0..100 {
            uuid_stats.record(&json!("550e8400-e29b-41d4-a716-446655440000"));
        }
        uuid_stats.finalize(100);
        stats.push(uuid_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Nested schema with format hints should be valid");

        // Verify format hints are preserved in nested objects
        let contact_prop = schema.properties.get("contact").unwrap();
        let contact_props = contact_prop.properties.as_ref().unwrap();

        let email_prop = contact_props.get("email").unwrap();
        assert_eq!(email_prop.format, Some("email".to_string()));

        let uuid_prop = contact_props.get("user_id").unwrap();
        assert_eq!(uuid_prop.format, Some("uuid".to_string()));
    }

    #[test]
    fn test_schema_nested_with_enum_values() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Nested enum field
        let mut role_stats = FieldStats::new("permissions.role".to_string(), 0);
        for _ in 0..40 {
            role_stats.record(&json!("admin"));
        }
        for _ in 0..40 {
            role_stats.record(&json!("user"));
        }
        for _ in 0..20 {
            role_stats.record(&json!("guest"));
        }
        role_stats.finalize(100);
        stats.push(role_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Nested schema with enums should be valid");

        // Verify enum is preserved in nested object
        let permissions_prop = schema.properties.get("permissions").unwrap();
        let permissions_props = permissions_prop.properties.as_ref().unwrap();
        let role_prop = permissions_props.get("role").unwrap();

        assert!(role_prop.enum_values.is_some());
        let enum_vals = role_prop.enum_values.as_ref().unwrap();
        assert_eq!(enum_vals.len(), 3);
    }

    #[test]
    fn test_schema_nested_strict_mode() {
        let config = SchemaConfig {
            strict_additional_properties: true,
            ..Default::default()
        };
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        let mut name_stats = FieldStats::new("user.name".to_string(), 0);
        for _ in 0..100 {
            name_stats.record(&json!("Alice"));
        }
        name_stats.finalize(100);
        stats.push(name_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Nested strict schema should be valid");

        // Root should have additionalProperties: false
        assert!(!schema.additional_properties);

        // Nested objects should also respect strict mode
        let user_prop = schema.properties.get("user").unwrap();
        assert_eq!(user_prop.additional_properties, Some(false));
    }

    #[test]
    fn test_schema_multiple_nested_objects() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // First nested object
        let mut user_name_stats = FieldStats::new("user.name".to_string(), 0);
        for _ in 0..100 {
            user_name_stats.record(&json!("Alice"));
        }
        user_name_stats.finalize(100);
        stats.push(user_name_stats);

        // Second nested object
        let mut config_theme_stats = FieldStats::new("config.theme".to_string(), 0);
        for _ in 0..100 {
            config_theme_stats.record(&json!("dark"));
        }
        config_theme_stats.finalize(100);
        stats.push(config_theme_stats);

        // Third nested object
        let mut meta_version_stats = FieldStats::new("metadata.version".to_string(), 0);
        for _ in 0..100 {
            meta_version_stats.record(&json!("1.0"));
        }
        meta_version_stats.finalize(100);
        stats.push(meta_version_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Multiple nested objects should be valid");

        // Verify all three nested objects exist
        assert!(schema.properties.contains_key("user"));
        assert!(schema.properties.contains_key("config"));
        assert!(schema.properties.contains_key("metadata"));
    }

    #[test]
    fn test_schema_filters_array_paths() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Regular nested field
        let mut name_stats = FieldStats::new("user.name".to_string(), 0);
        for _ in 0..100 {
            name_stats.record(&json!("Alice"));
        }
        name_stats.finalize(100);
        stats.push(name_stats);

        // Array path (should be filtered out)
        let mut items_stats = FieldStats::new("items[].id".to_string(), 0);
        for _ in 0..100 {
            items_stats.record(&json!(123));
        }
        items_stats.finalize(100);
        stats.push(items_stats);

        let schema = generator.generate_json_schema(&stats, None, 100);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema should be valid with array paths filtered");

        // Should include user but not items
        assert!(schema.properties.contains_key("user"));
        assert!(!schema.properties.contains_key("items"));
    }
}
