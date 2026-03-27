use crate::filter::PathFilter;
use crate::interner::StringInterner;
use crate::stats::FieldStats;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct JsonAnalyzer {
    stats: HashMap<Arc<str>, FieldStats>,
    interner: StringInterner,
    total_samples: u64,
    filter: PathFilter,
    root_is_array: Option<bool>,
}

impl Default for JsonAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonAnalyzer {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            interner: StringInterner::with_common_paths(),
            total_samples: 0,
            filter: PathFilter::new(),
            root_is_array: None,
        }
    }

    /// Create a new analyzer with a path filter
    pub fn with_filter(filter: PathFilter) -> Self {
        Self {
            stats: HashMap::new(),
            interner: StringInterner::with_common_paths(),
            total_samples: 0,
            filter,
            root_is_array: None,
        }
    }

    /// Analyze a sing json document
    pub fn analyze(&mut self, value: &Value) {
        self.total_samples += 1;

        // Detect if root is an array
        if self.root_is_array.is_none() {
            self.root_is_array = Some(value.is_array());
        }

        // For root-level arrays, record the array itself
        if value.is_array() {
            let array_path = self.interner.intern("[]");
            self.record_field(array_path, value, 0);
        }

        let empty_path = self.interner.intern("");
        self.walk(empty_path, value, 0);
    }

    /// Recursive walk
    fn walk(&mut self, path: Arc<str>, value: &Value, depth: usize) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    // Build path once and intern it
                    let field_path = if path.is_empty() {
                        self.interner.intern(key)
                    } else {
                        let full_path = format!("{}.{}", path, key);
                        self.interner.intern(&full_path)
                    };

                    // Check if this path should be ignored
                    if self.filter.should_ignore(&field_path) {
                        continue; // Skip this path and its children
                    }

                    self.record_field(Arc::clone(&field_path), val, depth + 1);

                    self.walk(field_path, val, depth + 1);
                }
            }
            Value::Array(arr) => {
                let array_path = if path.is_empty() {
                    self.interner.intern("[]")
                } else {
                    let full_path = format!("{}[]", path);
                    self.interner.intern(&full_path)
                };

                for item in arr {
                    self.walk(Arc::clone(&array_path), item, depth + 1);
                }
            }
            _ => {
                // Leaf node recorded by parent
            }
        }
    }

    fn record_field(&mut self, path: Arc<str>, value: &Value, depth: usize) {
        self.stats
            .entry(Arc::clone(&path))
            .or_insert_with(|| FieldStats::new(path, depth))
            .record(value);
    }

    /// Check if the root JSON value is an array
    pub fn is_root_array(&self) -> bool {
        self.root_is_array.unwrap_or(false)
    }

    pub fn finalize(mut self) -> HashMap<Arc<str>, FieldStats> {
        for stats in self.stats.values_mut() {
            stats.finalize(self.total_samples);
        }
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JsonType;
    use serde_json::json;

    #[test]
    fn test_flat_object() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({
            "name": "Alice",
            "age": 30
        }));

        let stats = analyzer.finalize();

        assert_eq!(stats.len(), 2);
        assert!(stats.contains_key("name"));
        assert!(stats.contains_key("age"));

        let name_stats = &stats["name"];
        assert_eq!(name_stats.occurrences, 1);
        assert_eq!(name_stats.density, 1.0);
        assert_eq!(name_stats.depth, 1);
    }

    #[test]
    fn test_nested_object() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({
            "user": {
                "profile": {
                    "email": "test@example.com"
                }
            }
        }));

        let stats = analyzer.finalize();

        assert!(stats.contains_key("user"));
        assert!(stats.contains_key("user.profile"));
        assert!(stats.contains_key("user.profile.email"));

        assert_eq!(stats["user.profile.email"].depth, 3);
    }

    #[test]
    fn test_array_with_objects() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({
            "addresses": [
                {"city": "Brisbane", "zip": "4000"},
                {"city": "Sydney", "zip": "2000"}
            ]
        }));

        let stats = analyzer.finalize();

        // Should have: addresses, addresses[], addresses[].city, addresses[].zip
        assert!(stats.contains_key("addresses"));
        assert!(stats.contains_key("addresses[].city"));
        assert!(stats.contains_key("addresses[].zip"));

        // Each item in array was seen once, but we analyzed 1 document
        let city_stats = &stats["addresses[].city"];
        assert_eq!(city_stats.occurrences, 2); // Appears in both array items
    }

    #[test]
    fn test_multiple_documents_density() {
        let mut analyzer = JsonAnalyzer::new();

        // 3 documents, "nickname" only in 1
        analyzer.analyze(&json!({"name": "Alice", "nickname": "Al"}));
        analyzer.analyze(&json!({"name": "Bob"}));
        analyzer.analyze(&json!({"name": "Carol"}));

        let stats = analyzer.finalize();

        let name_stats = &stats["name"];
        assert_eq!(name_stats.occurrences, 3);
        assert_eq!(name_stats.density, 1.0); // 100%

        let nickname_stats = &stats["nickname"];
        assert_eq!(nickname_stats.occurrences, 1);
        assert!((nickname_stats.density - 0.333).abs() < 0.01); // ~33%
    }

    #[test]
    fn test_type_inconsistency() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({"age": "25"})); // String
        analyzer.analyze(&json!({"age": 30})); // Number
        analyzer.analyze(&json!({"age": "35"})); // String

        let stats = analyzer.finalize();
        let age_stats = &stats["age"];

        // Should track both types
        assert_eq!(age_stats.types.get(&JsonType::String), Some(&2));
        assert_eq!(age_stats.types.get(&JsonType::Number), Some(&1));
    }

    #[test]
    fn test_null_tracking() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({"optional": null}));
        analyzer.analyze(&json!({"optional": "value"}));
        analyzer.analyze(&json!({"optional": null}));

        let stats = analyzer.finalize();
        let optional_stats = &stats["optional"];

        assert_eq!(optional_stats.null_count, 2);
        assert_eq!(optional_stats.occurrences, 3);
    }

    #[test]
    fn test_empty_array() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({"items": []}));

        let stats = analyzer.finalize();

        // Should record the array field itself
        assert!(stats.contains_key("items"));

        // But no items in the array
        assert!(!stats.contains_key("items[]"));
    }

    #[test]
    fn test_deep_nesting() {
        let mut analyzer = JsonAnalyzer::new();

        analyzer.analyze(&json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "value": "deep"
                        }
                    }
                }
            }
        }));

        let stats = analyzer.finalize();

        assert_eq!(stats["level1"].depth, 1);
        assert_eq!(stats["level1.level2"].depth, 2);
        assert_eq!(stats["level1.level2.level3"].depth, 3);
        assert_eq!(stats["level1.level2.level3.level4"].depth, 4);
        assert_eq!(stats["level1.level2.level3.level4.value"].depth, 5);
    }

    #[test]
    fn test_examples_collection() {
        let mut analyzer = JsonAnalyzer::new();

        for i in 0..15 {
            analyzer.analyze(&json!({"value": i}));
        }

        let stats = analyzer.finalize();
        let value_stats = &stats["value"];

        // Should limit to 10 examples
        assert_eq!(value_stats.examples.len(), 10);
    }

    #[test]
    fn test_filter_exact_match() {
        use crate::filter::PathFilter;

        let filter = PathFilter::with_patterns(vec!["user.email".to_string()]);
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "user": {
                "name": "Alice",
                "email": "alice@example.com",
                "age": 30
            }
        }));

        let stats = analyzer.finalize();

        // Should include user and user.name and user.age
        assert!(stats.contains_key("user"));
        assert!(stats.contains_key("user.name"));
        assert!(stats.contains_key("user.age"));

        // Should NOT include user.email (exact match filtered)
        assert!(!stats.contains_key("user.email"));
    }

    #[test]
    fn test_filter_prefix_match() {
        use crate::filter::PathFilter;

        let filter = PathFilter::with_patterns(vec!["user.internal.*".to_string()]);
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "user": {
                "name": "Alice",
                "internal": {
                    "id": 123,
                    "token": "secret"
                },
                "email": "alice@example.com"
            }
        }));

        let stats = analyzer.finalize();

        // Should include user, user.name, user.email
        assert!(stats.contains_key("user"));
        assert!(stats.contains_key("user.name"));
        assert!(stats.contains_key("user.email"));

        // Should NOT include user.internal or its children (prefix match filtered)
        assert!(!stats.contains_key("user.internal"));
        assert!(!stats.contains_key("user.internal.id"));
        assert!(!stats.contains_key("user.internal.token"));
    }

    #[test]
    fn test_filter_multiple_patterns() {
        use crate::filter::PathFilter;

        let filter =
            PathFilter::with_patterns(vec!["debug.*".to_string(), "temp.session".to_string()]);
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "user": "Alice",
            "debug": {
                "logs": "verbose",
                "trace": true
            },
            "temp": {
                "session": "xyz",
                "cache": "data"
            }
        }));

        let stats = analyzer.finalize();

        // Should include user and temp.cache
        assert!(stats.contains_key("user"));
        assert!(stats.contains_key("temp"));
        assert!(stats.contains_key("temp.cache"));

        // Should NOT include debug.* (prefix) or temp.session (exact)
        assert!(!stats.contains_key("debug"));
        assert!(!stats.contains_key("debug.logs"));
        assert!(!stats.contains_key("debug.trace"));
        assert!(!stats.contains_key("temp.session"));
    }

    #[test]
    fn test_filter_nested_filtering() {
        use crate::filter::PathFilter;

        let filter = PathFilter::with_patterns(vec!["metadata.internal.*".to_string()]);
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "metadata": {
                "title": "Document",
                "internal": {
                    "draft": true,
                    "private": {
                        "notes": "confidential"
                    }
                }
            }
        }));

        let stats = analyzer.finalize();

        // Should include metadata and metadata.title
        assert!(stats.contains_key("metadata"));
        assert!(stats.contains_key("metadata.title"));

        // Should NOT include metadata.internal or ANY of its descendants
        assert!(!stats.contains_key("metadata.internal"));
        assert!(!stats.contains_key("metadata.internal.draft"));
        assert!(!stats.contains_key("metadata.internal.private"));
        assert!(!stats.contains_key("metadata.internal.private.notes"));
    }

    #[test]
    fn test_filter_with_specific_paths() {
        use crate::filter::PathFilter;

        // Test that filtering is applied to specific paths only
        let filter = PathFilter::with_patterns(vec!["items.metadata".to_string()]);
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "items": [
                {"name": "Item 1"},
                {"name": "Item 2"}
            ],
            "items.metadata": "should be filtered"
        }));

        let stats = analyzer.finalize();

        // items and items[].name should NOT be filtered
        assert!(stats.contains_key("items"));
        assert!(stats.contains_key("items[].name"));

        // items.metadata SHOULD be filtered (exact match)
        assert!(!stats.contains_key("items.metadata"));
    }

    #[test]
    fn test_empty_filter() {
        use crate::filter::PathFilter;

        let filter = PathFilter::new();
        let mut analyzer = JsonAnalyzer::with_filter(filter);

        analyzer.analyze(&json!({
            "name": "Alice",
            "email": "alice@example.com",
            "nested": {
                "value": 123
            }
        }));

        let stats = analyzer.finalize();

        // With empty filter, all fields should be included
        assert_eq!(stats.len(), 4); // name, email, nested, nested.value
        assert!(stats.contains_key("name"));
        assert!(stats.contains_key("email"));
        assert!(stats.contains_key("nested"));
        assert!(stats.contains_key("nested.value"));
    }
}
