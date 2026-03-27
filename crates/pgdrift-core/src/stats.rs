use crate::types::JsonType;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
pub struct FieldStats {
    #[serde(serialize_with = "serialize_arc_str")]
    pub path: Arc<str>,
    pub occurrences: u64,
    pub total_samples: u64,
    pub density: f64,
    pub null_count: u64,
    pub types: HashMap<JsonType, u64>,
    pub examples: Vec<Value>,
    pub depth: usize,

    // Number statistics for type inference and schema generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_number: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_decimals: Option<bool>,

    // String statistics for enum detection and schema generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distinct_values: Option<HashSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_string_length: Option<usize>,
}

impl FieldStats {
    pub fn new(path: Arc<str>, depth: usize) -> Self {
        Self {
            path,
            occurrences: 0,
            total_samples: 0,
            density: 0.0,
            null_count: 0,
            types: HashMap::new(),
            examples: Vec::new(),
            depth,
            min_number: None,
            max_number: None,
            has_decimals: None,
            distinct_values: None,
            max_string_length: None,
        }
    }

    /// Record and occurence of this field with its value
    pub fn record(&mut self, value: &Value) {
        self.occurrences += 1;

        let json_type = JsonType::from_value(value);
        *self.types.entry(json_type).or_insert(0) += 1;

        if matches!(value, Value::Null) {
            self.null_count += 1;
        }

        // Track number statistics
        if let Value::Number(num) = value
            && let Some(n) = num.as_f64()
        {
            // Track min/max
            self.min_number = Some(self.min_number.map_or(n, |min| min.min(n)));
            self.max_number = Some(self.max_number.map_or(n, |max| max.max(n)));

            // Track if we've seen any decimals
            if !num.is_i64() && !num.is_u64() {
                self.has_decimals = Some(true);
            } else if self.has_decimals.is_none() {
                self.has_decimals = Some(false);
            }
        }

        // Track distinct string values (up to 100 for enum detection)
        if let Value::String(s) = value {
            // Track max string length
            self.max_string_length = Some(
                self.max_string_length
                    .map_or(s.len(), |max| max.max(s.len())),
            );

            // Track distinct values (limit to 100 to avoid memory issues)
            let distinct_values = self.distinct_values.get_or_insert_with(HashSet::new);
            if distinct_values.len() < 100 {
                distinct_values.insert(s.clone());
            }
        }

        // Store examples - max 10
        if self.examples.len() < 10 {
            self.examples.push(value.clone());
        }
    }

    pub fn finalize(&mut self, total_samples: u64) {
        self.total_samples = total_samples;

        if self.total_samples > 0 {
            self.density = self.occurrences as f64 / self.total_samples as f64;
        }
    }
}

/// Custom serializer for Arc<str> to serialize as a String
fn serialize_arc_str<S>(arc: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(arc)
}
