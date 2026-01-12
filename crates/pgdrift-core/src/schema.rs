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
    /// Ghost key threshold - keys with density <= this are considered unstable/dynamic (default: 0.10)
    pub ghost_key_threshold: f64,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            required_threshold: 0.95,
            enum_max_values: 10,
            enum_min_density: 0.8,
            strict_additional_properties: false,
            detect_formats: true,
            ghost_key_threshold: 0.10,
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

    // For arrays
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySchema>>,

    // For dynamic keys
    #[serde(rename = "patternProperties", skip_serializing_if = "Option::is_none")]
    pub pattern_properties: Option<HashMap<String, PropertySchema>>,
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
            items: None,
            pattern_properties: None,
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

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, PropertySchema>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,

    #[serde(rename = "additionalProperties")]
    pub additional_properties: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<PropertySchema>>,

    #[serde(rename = "patternProperties", skip_serializing_if = "Option::is_none")]
    pub pattern_properties: Option<HashMap<String, PropertySchema>>,
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
        is_root_array: bool,
    ) -> JsonSchema {
        if is_root_array {
            // Root is an array - generate array schema with items
            self.generate_root_array_schema(stats, schema_name, total_samples)
        } else {
            // Root is an object - generate object schema with properties
            self.generate_root_object_schema(stats, schema_name, total_samples)
        }
    }

    /// Generate schema when root is an object
    fn generate_root_object_schema(
        &self,
        stats: &[FieldStats],
        schema_name: Option<String>,
        total_samples: u64,
    ) -> JsonSchema {
        // Detect pattern properties at root level (based on ghost keys)
        let (pattern_properties, excluded_keys) = self.detect_pattern_properties(stats, "");

        // Build nested property tree (excluding keys that match patterns)
        let (mut root_properties, required_fields) = self.build_property_tree(stats, &excluded_keys);

        // Apply pattern detection recursively to nested objects
        self.apply_nested_pattern_detection(&mut root_properties, stats);

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
            items: None,
            pattern_properties: if pattern_properties.is_empty() {
                None
            } else {
                Some(pattern_properties)
            },
        }
    }

    /// Generate schema when root is an array
    fn generate_root_array_schema(
        &self,
        stats: &[FieldStats],
        schema_name: Option<String>,
        total_samples: u64,
    ) -> JsonSchema {
        // Find the root array stats (path "[]")
        let root_array_stat = stats.iter().find(|s| s.path == "[]");

        // Determine the array items type
        let items_schema = if let Some(root_stat) = root_array_stat {
            // Check dominant type of array items
            let dominant_type = root_stat
                .types
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(t, _)| *t);

            match dominant_type {
                Some(crate::types::JsonType::Object) => {
                    // Array of objects - build items schema from nested paths
                    self.build_array_items_schema(stats)
                }
                _ => {
                    // Array of primitives (strings, numbers, etc.)
                    Some(Box::new(PropertySchema::from_field_stats(
                        root_stat,
                        &self.config,
                    )))
                }
            }
        } else {
            // No root array stats, try to infer from nested paths
            self.build_array_items_schema(stats)
        };

        JsonSchema {
            schema_version: "https://json-schema.org/draft/2020-12/schema".to_string(),
            title: schema_name,
            description: Some(format!(
                "Auto-generated schema from pgdrift analysis ({} samples)",
                total_samples
            )),
            schema_type: "array".to_string(),
            properties: HashMap::new(),
            required: vec![],
            additional_properties: !self.config.strict_additional_properties,
            items: items_schema,
            pattern_properties: None,
        }
    }

    /// Detect pattern properties at a given path level
    /// Returns (pattern_properties, excluded_keys) where excluded_keys should not appear in regular properties
    fn detect_pattern_properties(
        &self,
        stats: &[FieldStats],
        path_prefix: &str,
    ) -> (HashMap<String, PropertySchema>, Vec<String>) {
        use std::collections::HashMap;

        // Group stats by the next key segment after the prefix
        // Only consider keys that are ghost keys (low density)
        let mut key_groups: HashMap<String, Vec<&FieldStats>> = HashMap::new();
        let mut key_densities: HashMap<String, f64> = HashMap::new();

        for stat in stats {
            // Skip if not at this prefix level
            let key_name = if path_prefix.is_empty() {
                // Root level - first segment is the key
                if let Some(first_part) = stat.path.split('.').next() {
                    if !first_part.contains('[') {
                        Some(first_part.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                // Nested level - check if path starts with prefix
                let prefix_with_dot = format!("{}.", path_prefix);
                if stat.path.starts_with(&prefix_with_dot) {
                    let remaining = &stat.path[prefix_with_dot.len()..];
                    if let Some(next_part) = remaining.split('.').next() {
                        if !next_part.contains('[') {
                            Some(next_part.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(key) = key_name {
                key_groups.entry(key.clone()).or_default().push(stat);

                // Track density - use the stat's density if it's the key itself
                if path_prefix.is_empty() && stat.path == key {
                    key_densities.insert(key.clone(), stat.density);
                } else if !path_prefix.is_empty()
                    && stat.path == format!("{}.{}", path_prefix, key) {
                    key_densities.insert(key.clone(), stat.density);
                }
            }
        }

        // Detect patterns in the keys, but only for ghost keys
        let mut pattern_groups: HashMap<KeyPattern, Vec<(String, Vec<&FieldStats>)>> =
            HashMap::new();

        for (key, stats_list) in key_groups {
            // Check if this is a ghost key (low density)
            let is_ghost_key = key_densities
                .get(&key)
                .map(|d| *d <= self.config.ghost_key_threshold)
                .unwrap_or(false);

            if is_ghost_key {
                if let Some(pattern) = KeyPattern::detect(&key) {
                    pattern_groups
                        .entry(pattern)
                        .or_default()
                        .push((key, stats_list));
                }
            }
        }

        // Build pattern properties for patterns with >= 2 keys
        let mut pattern_properties = HashMap::new();
        let mut excluded_keys = Vec::new();

        for (pattern, key_stats_list) in pattern_groups {
            if key_stats_list.len() >= 2 {
                // Multiple keys match this pattern - use patternProperties
                let regex = pattern.to_regex();

                // Build the value schema from all nested paths
                let value_schema = self.build_pattern_value_schema(stats, path_prefix, &key_stats_list);

                pattern_properties.insert(regex, value_schema);

                // Mark these keys for exclusion from regular properties
                for (key, _) in key_stats_list {
                    excluded_keys.push(key);
                }
            }
        }

        (pattern_properties, excluded_keys)
    }

    /// Apply pattern detection recursively to nested objects
    fn apply_nested_pattern_detection(
        &self,
        properties: &mut HashMap<String, PropertySchema>,
        all_stats: &[FieldStats],
    ) {
        for (prop_name, prop_schema) in properties.iter_mut() {
            // Only process object types with child properties
            if let Some(PropertyType::Single(ref type_name)) = prop_schema.property_type {
                if type_name == "object" {
                    if let Some(ref mut child_props) = prop_schema.properties {
                        // Check if child properties should use pattern properties
                        let (pattern_props, excluded) = self.detect_nested_patterns(
                            child_props,
                            all_stats,
                            prop_name,
                        );

                        if !pattern_props.is_empty() {
                            // Remove excluded properties
                            for key in &excluded {
                                child_props.remove(key);
                            }

                            // Add pattern properties to this nested object
                            prop_schema.pattern_properties = Some(pattern_props);
                        }

                        // Recurse into child properties
                        self.apply_nested_pattern_detection(child_props, all_stats);
                    }
                }
            }
        }
    }

    /// Detect patterns in nested object properties
    fn detect_nested_patterns(
        &self,
        child_properties: &HashMap<String, PropertySchema>,
        all_stats: &[FieldStats],
        parent_path: &str,
    ) -> (HashMap<String, PropertySchema>, Vec<String>) {
        let mut pattern_groups: HashMap<KeyPattern, Vec<String>> = HashMap::new();
        let mut key_densities: HashMap<String, f64> = HashMap::new();

        // Collect densities for each child key
        for (child_key, _child_schema) in child_properties {
            let full_path = format!("{}.{}", parent_path, child_key);

            // Find the stat for this path
            if let Some(stat) = all_stats.iter().find(|s| s.path == full_path) {
                key_densities.insert(child_key.clone(), stat.density);

                // Check if this is a ghost key and matches a pattern
                if stat.density <= self.config.ghost_key_threshold {
                    if let Some(pattern) = KeyPattern::detect(child_key) {
                        pattern_groups
                            .entry(pattern)
                            .or_default()
                            .push(child_key.clone());
                    }
                }
            }
        }

        // Build pattern properties for patterns with >= 2 keys
        let mut pattern_properties = HashMap::new();
        let mut excluded_keys = Vec::new();

        for (pattern, matching_keys) in pattern_groups {
            if matching_keys.len() >= 2 {
                let regex = pattern.to_regex();

                // Build schema from field stats (like root-level patterns)
                let key_stats_list: Vec<(String, Vec<&FieldStats>)> = matching_keys
                    .iter()
                    .map(|key| {
                        let stats: Vec<&FieldStats> = vec![]; // Not used in build_pattern_value_schema
                        (key.clone(), stats)
                    })
                    .collect();

                let merged_schema = self.build_pattern_value_schema(
                    all_stats,
                    parent_path,
                    &key_stats_list,
                );

                pattern_properties.insert(regex, merged_schema);

                // Mark keys for exclusion
                excluded_keys.extend(matching_keys);
            }
        }

        (pattern_properties, excluded_keys)
    }

    /// Build the value schema for a pattern property
    fn build_pattern_value_schema(
        &self,
        all_stats: &[FieldStats],
        path_prefix: &str,
        key_stats_list: &[(String, Vec<&FieldStats>)],
    ) -> PropertySchema {
        // Collect all nested properties across all keys matching the pattern
        let mut nested_properties: HashMap<String, PropertySchema> = HashMap::new();
        let mut nested_required: Vec<String> = Vec::new();

        // For each UUID key, get its nested fields
        for (key, _stats) in key_stats_list {
            let key_prefix = if path_prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", path_prefix, key)
            };

            // Find all stats that are nested under this UUID key
            for stat in all_stats {
                if stat.path.starts_with(&format!("{}.", key_prefix)) {
                    let remaining = &stat.path[key_prefix.len() + 1..];
                    let parts: Vec<&str> = remaining.split('.').collect();

                    if !parts.is_empty() && parts.len() == 1 {
                        // Direct child property
                        let field_name = parts[0];
                        if !field_name.contains('[') {
                            let prop_schema = PropertySchema::from_field_stats(stat, &self.config);
                            nested_properties.insert(field_name.to_string(), prop_schema);

                            if stat.density >= self.config.required_threshold {
                                if !nested_required.contains(&field_name.to_string()) {
                                    nested_required.push(field_name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        nested_required.sort();
        nested_required.dedup();

        PropertySchema {
            property_type: Some(PropertyType::Single("object".to_string())),
            description: Some(format!(
                "Dynamic keys matching pattern ({} keys detected)",
                key_stats_list.len()
            )),
            format: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            pattern: None,
            properties: Some(nested_properties),
            additional_properties: Some(!self.config.strict_additional_properties),
            items: None,
            pattern_properties: None,
        }
    }

    /// Build items schema for root-level array from nested field paths
    fn build_array_items_schema(&self, stats: &[FieldStats]) -> Option<Box<PropertySchema>> {
        // Filter for paths that start with "[]." (nested in root array)
        let nested_stats: Vec<_> = stats
            .iter()
            .filter(|s| s.path.starts_with("[]."))
            .collect();

        if nested_stats.is_empty() {
            // No nested fields, might be array of primitives
            return None;
        }

        // Build properties for the object inside the array
        let mut properties = HashMap::new();
        let mut required_fields = Vec::new();

        for stat in nested_stats {
            // Remove "[]." prefix to get the field name
            let field_path = &stat.path[3..];
            let parts: Vec<&str> = field_path.split('.').collect();

            if parts.is_empty() {
                continue;
            }

            let field_name = parts[0];

            if parts.len() == 1 {
                // Top-level field in the array item
                let property_schema = PropertySchema::from_field_stats(stat, &self.config);
                properties.insert(field_name.to_string(), property_schema);

                if stat.density >= self.config.required_threshold {
                    required_fields.push(field_name.to_string());
                }
            }
            // TODO: Handle nested objects within array items if needed
        }

        required_fields.sort();
        required_fields.dedup();

        Some(Box::new(PropertySchema {
            property_type: Some(PropertyType::Single("object".to_string())),
            description: None,
            format: None,
            enum_values: None,
            minimum: None,
            maximum: None,
            pattern: None,
            properties: Some(properties),
            additional_properties: Some(!self.config.strict_additional_properties),
            items: None,
            pattern_properties: None,
        }))
    }

    /// Build a nested property tree from field statistics
    /// Excluded keys will not appear in the property tree
    fn build_property_tree(
        &self,
        stats: &[FieldStats],
        excluded_keys: &[String],
    ) -> (HashMap<String, PropertySchema>, Vec<String>) {
        let mut root_properties: HashMap<String, PropertySchema> = HashMap::new();
        let mut required_fields = Vec::new();

        // Group stats by root-level field
        for stat in stats {
            let parts: Vec<&str> = stat.path.split('.').collect();

            if parts.is_empty() {
                continue;
            }

            let root_field_raw = parts[0];
            let (root_field, is_array) = self.parse_field_name(root_field_raw);

            // Skip excluded keys (pattern-matched keys)
            if excluded_keys.contains(&root_field) {
                continue;
            }

            if parts.len() == 1 && !is_array {
                // Top-level scalar field
                let property_schema = PropertySchema::from_field_stats(stat, &self.config);
                root_properties.insert(root_field.to_string(), property_schema);

                if stat.density >= self.config.required_threshold {
                    required_fields.push(root_field.to_string());
                }
            } else if parts.len() == 1 && is_array {
                // Top-level array field (e.g., "items[]")
                // Create or get array property
                root_properties
                    .entry(root_field.to_string())
                    .or_insert_with(|| PropertySchema {
                        property_type: Some(PropertyType::Single("array".to_string())),
                        description: Some(format!(
                            "Occurs in {:.1}% of samples",
                            stat.density * 100.0
                        )),
                        format: None,
                        enum_values: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        properties: None,
                        additional_properties: None,
                        items: None,
            pattern_properties: None,
                    });
            } else {
                // Nested field - build nested structure
                if is_array {
                    // Root field is an array (e.g., "items[].name")
                    let root_prop = root_properties
                        .entry(root_field.to_string())
                        .or_insert_with(|| {
                            // Create array with items object
                            let items_schema = PropertySchema {
                                property_type: Some(PropertyType::Single("object".to_string())),
                                description: None,
                                format: None,
                                enum_values: None,
                                minimum: None,
                                maximum: None,
                                pattern: None,
                                properties: Some(HashMap::new()),
                                additional_properties: Some(!self.config.strict_additional_properties),
                                items: None,
            pattern_properties: None,
                            };

                            PropertySchema {
                                property_type: Some(PropertyType::Single("array".to_string())),
                                description: None,
                                format: None,
                                enum_values: None,
                                minimum: None,
                                maximum: None,
                                pattern: None,
                                properties: None,
                                additional_properties: None,
                                items: Some(Box::new(items_schema)),
            pattern_properties: None,
                            }
                        });

                    // Insert into the items object
                    if let Some(ref mut items_box) = root_prop.items {
                        self.insert_nested_property(items_box, &parts[1..], stat);
                    }
                } else {
                    // Root field is an object
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
                            items: None,
            pattern_properties: None,
                        });

                    // Build nested path
                    self.insert_nested_property(root_prop, &parts[1..], stat);
                }
            }
        }

        // Sort required fields
        required_fields.sort();
        required_fields.dedup();

        (root_properties, required_fields)
    }

    /// Parse a field name and detect if it's an array (ends with [])
    /// Returns (field_name, is_array)
    fn parse_field_name<'a>(&self, field: &'a str) -> (String, bool) {
        if field.ends_with("[]") {
            (field[..field.len() - 2].to_string(), true)
        } else {
            (field.to_string(), false)
        }
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

        let field_name_raw = path_parts[0];
        let (field_name, is_array) = self.parse_field_name(field_name_raw);

        // Ensure parent has properties map
        let properties = parent.properties.get_or_insert_with(HashMap::new);

        if path_parts.len() == 1 && !is_array {
            // Leaf node - insert the actual field
            let property_schema = PropertySchema::from_field_stats(stat, &self.config);
            properties.insert(field_name, property_schema);
        } else if path_parts.len() == 1 && is_array {
            // Leaf array field
            properties
                .entry(field_name.clone())
                .or_insert_with(|| PropertySchema {
                    property_type: Some(PropertyType::Single("array".to_string())),
                    description: None,
                    format: None,
                    enum_values: None,
                    minimum: None,
                    maximum: None,
                    pattern: None,
                    properties: None,
                    additional_properties: None,
                    items: None,
            pattern_properties: None,
                });
        } else {
            // Intermediate node
            if is_array {
                // Intermediate array node (e.g., path "user.items[].name")
                let nested_prop = properties
                    .entry(field_name.clone())
                    .or_insert_with(|| PropertySchema {
                        property_type: Some(PropertyType::Single("array".to_string())),
                        description: None,
                        format: None,
                        enum_values: None,
                        minimum: None,
                        maximum: None,
                        pattern: None,
                        properties: None,
                        additional_properties: None,
                        items: Some(Box::new(PropertySchema {
                            property_type: Some(PropertyType::Single("object".to_string())),
                            description: None,
                            format: None,
                            enum_values: None,
                            minimum: None,
                            maximum: None,
                            pattern: None,
                            properties: Some(HashMap::new()),
                            additional_properties: Some(!self.config.strict_additional_properties),
                            items: None,
                            pattern_properties: None,
                        })),
                        pattern_properties: None,
                    });

                // Recurse into the items
                if let Some(ref mut items_box) = nested_prop.items {
                    self.insert_nested_property(items_box, &path_parts[1..], stat);
                }
            } else {
                // Intermediate object node
                let nested_prop = properties
                    .entry(field_name.clone())
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
                        items: None,
            pattern_properties: None,
                    });

                // Recurse
                self.insert_nested_property(nested_prop, &path_parts[1..], stat);
            }
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

/// Detect if a key name follows a known pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyPattern {
    Uuid,
    HexString16, // 16-character hex strings (device IDs, session tokens, etc.)
    // Future patterns: Timestamp, NumericId, etc.
}

impl KeyPattern {
    /// Get the JSON Schema regex pattern for this key pattern
    fn to_regex(&self) -> String {
        match self {
            KeyPattern::Uuid => {
                // UUID pattern (case-insensitive)
                "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$".to_string()
            }
            KeyPattern::HexString16 => {
                // 16-character hex string (device IDs, session tokens)
                "^[0-9a-fA-F]{16}$".to_string()
            }
        }
    }

    /// Detect pattern from a key name
    fn detect(key: &str) -> Option<Self> {
        if is_uuid_format(key) {
            Some(KeyPattern::Uuid)
        } else if is_hex_string_16(key) {
            Some(KeyPattern::HexString16)
        } else {
            None
        }
    }
}

/// Check if a string is a 16-character hex string
fn is_hex_string_16(s: &str) -> bool {
    s.len() == 16 && s.chars().all(|c| c.is_ascii_hexdigit())
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

        let schema = generator.generate_json_schema(&stats, Some("Test Schema".to_string()), 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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
            items: None,
            pattern_properties: None,
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

        let schema = generator.generate_json_schema(&[], None, 0, false);

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

        let schema = generator.generate_json_schema(&stats, Some("Test Schema".to_string()), 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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
            100,
            false
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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

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

        let schema = generator.generate_json_schema(&stats, None, 100, false);

        // Validate the schema
        validate_json_schema(&schema).expect("Multiple nested objects should be valid");

        // Verify all three nested objects exist
        assert!(schema.properties.contains_key("user"));
        assert!(schema.properties.contains_key("config"));
        assert!(schema.properties.contains_key("metadata"));
    }

    #[test]
    fn test_schema_with_array_paths() {
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

        // Array path (should now be included)
        let mut items_stats = FieldStats::new("items[].id".to_string(), 0);
        for _ in 0..100 {
            items_stats.record(&json!(123));
        }
        items_stats.finalize(100);
        stats.push(items_stats);

        let schema = generator.generate_json_schema(&stats, None, 100, false);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema should be valid with array paths");

        // Should include both user and items
        assert!(schema.properties.contains_key("user"));
        assert!(schema.properties.contains_key("items"));

        // items should be an array type
        let items_prop = schema.properties.get("items").unwrap();
        match &items_prop.property_type {
            Some(PropertyType::Single(t)) => assert_eq!(t, "array"),
            _ => panic!("Expected items to be array type"),
        }

        // items should have an items property with nested id field
        assert!(items_prop.items.is_some());
        let items_schema = items_prop.items.as_ref().unwrap();
        assert!(items_schema.properties.is_some());
        let items_properties = items_schema.properties.as_ref().unwrap();
        assert!(items_properties.contains_key("id"));
    }

    #[test]
    fn test_schema_with_uuid_pattern_properties() {
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Simulate 1000 total samples where each UUID appears in only ~5% (50 samples)
        // This makes them ghost keys (density <= 0.10)
        let total_samples = 1000;
        let uuid_sample_count = 50; // 5% density - ghost keys

        // UUID keys with nested properties
        let uuid_keys = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "123e4567-e89b-12d3-a456-426614174000",
        ];

        for uuid in &uuid_keys {
            // Create stats for the UUID key itself (ghost key)
            let mut uuid_stats = FieldStats::new(uuid.to_string(), 0);
            for _ in 0..uuid_sample_count {
                uuid_stats.record(&json!({"name": "Alice", "email": "alice@example.com"}));
            }
            uuid_stats.finalize(total_samples);
            stats.push(uuid_stats);

            // Each UUID has a name field
            let path = format!("{}.name", uuid);
            let mut name_stats = FieldStats::new(path, 0);
            for _ in 0..uuid_sample_count {
                name_stats.record(&json!("Alice"));
            }
            name_stats.finalize(total_samples);
            stats.push(name_stats);

            // Each UUID has an email field
            let path = format!("{}.email", uuid);
            let mut email_stats = FieldStats::new(path, 0);
            for _ in 0..uuid_sample_count {
                email_stats.record(&json!("alice@example.com"));
            }
            email_stats.finalize(total_samples);
            stats.push(email_stats);
        }

        let schema = generator.generate_json_schema(&stats, None, total_samples as u64, false);

        // Print the generated schema for inspection
        println!("\n=== Generated Schema with UUID Pattern Properties ===");
        println!("{}", schema.to_json_string());
        println!("======================================================\n");

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with UUID pattern properties should be valid");

        // Should have patternProperties
        assert!(schema.pattern_properties.is_some());
        let pattern_props = schema.pattern_properties.as_ref().unwrap();

        // Should have UUID pattern
        assert_eq!(pattern_props.len(), 1);
        let uuid_pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";
        assert!(pattern_props.contains_key(uuid_pattern));

        // The UUID pattern should have an object schema with name and email properties
        let uuid_schema = &pattern_props[uuid_pattern];
        assert!(uuid_schema.properties.is_some());
        let uuid_props = uuid_schema.properties.as_ref().unwrap();
        assert!(uuid_props.contains_key("name"));
        assert!(uuid_props.contains_key("email"));

        // UUID keys should NOT be in regular properties (they're excluded)
        assert!(schema.properties.is_empty(), "UUID keys should be excluded from properties when using patternProperties");
    }

    #[test]
    fn test_schema_with_nested_uuid_devices() {
        // Simulate the user's actual case: devices object with UUID keys
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];
        let total_samples = 10000;

        // devices appears in 0.8% of samples (ghost key)
        let mut devices_stats = FieldStats::new("devices".to_string(), 0);
        for _ in 0..78 {
            devices_stats.record(&json!({}));
        }
        devices_stats.finalize(total_samples);
        stats.push(devices_stats);

        // Multiple UUID keys under devices, each appearing in 0.0% (1/10000)
        let uuid_keys = vec![
            "5ED97224-A716-40C5-B795-7337ECBE3FB6",
            "2B9E0537-FAD4-4C92-8DAB-70E87F482745",
            "B9D462E8-D27E-4884-AE98-8653E7BE338B",
        ];

        for uuid in &uuid_keys {
            let path = format!("devices.{}", uuid);
            let mut uuid_stats = FieldStats::new(path.clone(), 0);
            uuid_stats.record(&json!({}));
            uuid_stats.finalize(total_samples);
            stats.push(uuid_stats);

            // Each has a device_no field
            let path = format!("devices.{}.device_no", uuid);
            let mut device_no_stats = FieldStats::new(path, 0);
            device_no_stats.record(&json!(1));
            device_no_stats.finalize(total_samples);
            stats.push(device_no_stats);
        }

        let schema = generator.generate_json_schema(&stats, None, total_samples as u64, false);

        println!("\n=== Schema with nested UUID devices ===");
        println!("{}", schema.to_json_string());
        println!("========================================\n");

        // devices should be in properties
        assert!(schema.properties.contains_key("devices"));
        let devices_prop = schema.properties.get("devices").unwrap();

        // devices should have patternProperties for UUIDs
        assert!(devices_prop.pattern_properties.is_some(), "devices should have patternProperties");
        let pattern_props = devices_prop.pattern_properties.as_ref().unwrap();

        // Should have UUID pattern
        let uuid_pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$";
        assert!(pattern_props.contains_key(uuid_pattern), "devices should have UUID pattern property");

        // Individual UUIDs should NOT be in properties
        assert!(devices_prop.properties.is_none() || devices_prop.properties.as_ref().unwrap().is_empty(),
            "Individual UUID keys should be excluded from properties");

        // The pattern should have device_no property
        let uuid_schema = &pattern_props[uuid_pattern];
        assert!(uuid_schema.properties.is_some());
        let uuid_props = uuid_schema.properties.as_ref().unwrap();
        assert!(uuid_props.contains_key("device_no"), "UUID pattern should include device_no property");
    }

    #[test]
    fn test_schema_with_hex_device_ids() {
        // Test 16-character hex string device IDs (like in user's schema)
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];
        let total_samples = 10000;

        // devices appears in 0.8% of samples
        let mut devices_stats = FieldStats::new("devices".to_string(), 0);
        for _ in 0..84 {
            devices_stats.record(&json!({}));
        }
        devices_stats.finalize(total_samples);
        stats.push(devices_stats);

        // 16-character hex device IDs (lowercase)
        let hex_ids = vec![
            "cacfa794927a8c4b",
            "e79c968dd761ef4f",
            "55108ee4bc11a3cd",
        ];

        for hex_id in &hex_ids {
            let path = format!("devices.{}", hex_id);
            let mut hex_stats = FieldStats::new(path.clone(), 0);
            hex_stats.record(&json!({}));
            hex_stats.finalize(total_samples);
            stats.push(hex_stats);

            // Each has a device_no field
            let path = format!("devices.{}.device_no", hex_id);
            let mut device_no_stats = FieldStats::new(path, 0);
            device_no_stats.record(&json!(1));
            device_no_stats.finalize(total_samples);
            stats.push(device_no_stats);
        }

        let schema = generator.generate_json_schema(&stats, None, total_samples as u64, false);

        println!("\n=== Schema with hex device IDs ===");
        println!("{}", schema.to_json_string());
        println!("===================================\n");

        // devices should be in properties
        assert!(schema.properties.contains_key("devices"));
        let devices_prop = schema.properties.get("devices").unwrap();

        // devices should have patternProperties for hex strings
        assert!(devices_prop.pattern_properties.is_some(), "devices should have patternProperties");
        let pattern_props = devices_prop.pattern_properties.as_ref().unwrap();

        // Should have hex string pattern
        let hex_pattern = "^[0-9a-fA-F]{16}$";
        assert!(pattern_props.contains_key(hex_pattern), "devices should have hex string pattern property");

        // Individual hex IDs should NOT be in properties
        assert!(devices_prop.properties.is_none() || devices_prop.properties.as_ref().unwrap().is_empty(),
            "Individual hex keys should be excluded from properties");

        // The pattern should have device_no property
        let hex_schema = &pattern_props[hex_pattern];
        assert!(hex_schema.properties.is_some());
        let hex_props = hex_schema.properties.as_ref().unwrap();
        assert!(hex_props.contains_key("device_no"), "Hex pattern should include device_no property");
    }

    #[test]
    fn test_schema_with_only_array_paths() {
        // This test reproduces the user's issue where ALL paths are arrays
        let config = SchemaConfig::default();
        let generator = SchemaGenerator::new(config);

        let mut stats = vec![];

        // Only array paths (like tokens[].value, tokens[].type, etc.)
        let fields = vec![
            ("tokens[].id", json!(123)),
            ("tokens[].value", json!("abc123")),
            ("tokens[].type", json!("access")),
            ("tokens[].expires_at", json!("2025-01-12T00:00:00Z")),
        ];

        for (path, value) in fields {
            let mut field_stats = FieldStats::new(path.to_string(), 0);
            for _ in 0..100 {
                field_stats.record(&value);
            }
            field_stats.finalize(100);
            stats.push(field_stats);
        }

        let schema = generator.generate_json_schema(&stats, None, 100, false);

        // Validate the schema
        validate_json_schema(&schema).expect("Schema with only array paths should be valid");

        // Should NOT be empty - should have tokens property
        assert!(!schema.properties.is_empty(), "Schema should not have empty properties");
        assert!(schema.properties.contains_key("tokens"));

        // tokens should be an array
        let tokens_prop = schema.properties.get("tokens").unwrap();
        match &tokens_prop.property_type {
            Some(PropertyType::Single(t)) => assert_eq!(t, "array"),
            _ => panic!("Expected tokens to be array type"),
        }

        // tokens.items should contain object with all the fields
        assert!(tokens_prop.items.is_some());
        let items_schema = tokens_prop.items.as_ref().unwrap();
        assert!(items_schema.properties.is_some());
        let items_properties = items_schema.properties.as_ref().unwrap();

        assert!(items_properties.contains_key("id"));
        assert!(items_properties.contains_key("value"));
        assert!(items_properties.contains_key("type"));
        assert!(items_properties.contains_key("expires_at"));
    }
}
