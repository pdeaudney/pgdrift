use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for filtering JSON paths during analysis
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PathFilter {
    ignore_patterns: Vec<String>,
}

impl PathFilter {
    /// Create an empty filter with no patterns
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a filter with the given patterns
    pub fn with_patterns(patterns: Vec<String>) -> Self {
        Self {
            ignore_patterns: patterns,
        }
    }

    /// Load filter configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: IgnoreConfig = toml::from_str(&contents)?;
        Ok(Self {
            ignore_patterns: config.ignore_paths,
        })
    }

    /// Add additional patterns to the filter
    pub fn add_patterns(&mut self, patterns: Vec<String>) {
        self.ignore_patterns.extend(patterns);
    }

    /// Check if a given path should be ignored based on the configured patterns
    ///
    /// Supports three types of patterns:
    /// - Exact match: "user.email" matches only "user.email"
    /// - Suffix wildcard: "user.internal.*" matches "user.internal" and all its children
    ///   like "user.internal.id", "user.internal.token", etc.
    /// - Prefix wildcard: "*.date_created" matches any path ending with "date_created"
    ///   like "uuid-123.date_created", "user_456.date_created", etc.
    pub fn should_ignore(&self, path: &str) -> bool {
        self.ignore_patterns.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix(".*") {
                // Suffix wildcard: pattern ends with .*
                // Match the prefix itself OR paths that start with "prefix."
                path == prefix || path.starts_with(&format!("{}.", prefix))
            } else if let Some(suffix) = pattern.strip_prefix("*.") {
                // Prefix wildcard: pattern starts with *.
                // Match any path that ends with ".suffix"
                path == suffix || path.ends_with(&format!(".{}", suffix))
            } else {
                // Exact match
                path == pattern
            }
        })
    }

    /// Get all configured patterns
    pub fn patterns(&self) -> &[String] {
        &self.ignore_patterns
    }
}

/// Internal structure for deserializing TOML configuration
#[derive(Debug, Deserialize)]
struct IgnoreConfig {
    ignore_paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let filter = PathFilter::with_patterns(vec!["user.email".to_string()]);

        assert!(filter.should_ignore("user.email"));
        assert!(!filter.should_ignore("user.email.domain"));
        assert!(!filter.should_ignore("user"));
        assert!(!filter.should_ignore("user.name"));
    }

    #[test]
    fn test_prefix_match() {
        let filter = PathFilter::with_patterns(vec!["user.internal.*".to_string()]);

        // Should match the prefix itself
        assert!(filter.should_ignore("user.internal"));
        // Should match children
        assert!(filter.should_ignore("user.internal.id"));
        assert!(filter.should_ignore("user.internal.data"));
        assert!(filter.should_ignore("user.internal.metadata.secret"));

        // Should NOT match partial prefix
        assert!(!filter.should_ignore("user.int"));
        assert!(!filter.should_ignore("user"));
        // Should NOT match sibling paths
        assert!(!filter.should_ignore("user.external"));
        assert!(!filter.should_ignore("user.email"));
    }

    #[test]
    fn test_multiple_patterns() {
        let filter = PathFilter::with_patterns(vec![
            "user.internal.*".to_string(),
            "metadata.debug".to_string(),
            "temp.*".to_string(),
        ]);

        assert!(filter.should_ignore("user.internal.id"));
        assert!(filter.should_ignore("metadata.debug"));
        assert!(filter.should_ignore("temp.session"));
        assert!(!filter.should_ignore("user.email"));
    }

    #[test]
    fn test_empty_filter() {
        let filter = PathFilter::new();

        assert!(!filter.should_ignore("any.path"));
        assert!(!filter.should_ignore("user.email"));
    }

    #[test]
    fn test_add_patterns() {
        let mut filter = PathFilter::with_patterns(vec!["first.*".to_string()]);
        filter.add_patterns(vec!["second.*".to_string(), "third".to_string()]);

        assert!(filter.should_ignore("first.value"));
        assert!(filter.should_ignore("second.value"));
        assert!(filter.should_ignore("third"));
        assert!(!filter.should_ignore("fourth"));
    }

    #[test]
    fn test_array_notation_not_matched_by_prefix() {
        let filter = PathFilter::with_patterns(vec!["addresses.*".to_string()]);

        // "addresses.*" should NOT match "addresses[].city"
        // because the bracket notation creates a different path structure
        assert!(!filter.should_ignore("addresses[].city"));

        // But it should match nested object fields
        assert!(filter.should_ignore("addresses.primary"));
        assert!(filter.should_ignore("addresses.primary.city"));
    }

    #[test]
    fn test_patterns_accessor() {
        let patterns = vec!["user.internal.*".to_string(), "debug".to_string()];
        let filter = PathFilter::with_patterns(patterns.clone());

        assert_eq!(filter.patterns(), &patterns);
    }

    #[test]
    fn test_edge_case_empty_path() {
        let filter = PathFilter::with_patterns(vec!["user.*".to_string()]);

        assert!(!filter.should_ignore(""));
    }

    #[test]
    fn test_edge_case_root_level_prefix() {
        let filter = PathFilter::with_patterns(vec!["metadata.*".to_string()]);

        // Should match the prefix itself and children
        assert!(filter.should_ignore("metadata"));
        assert!(filter.should_ignore("metadata.debug"));
        assert!(filter.should_ignore("metadata.internal.secret"));
        assert!(!filter.should_ignore("data"));
    }

    #[test]
    fn test_case_sensitive() {
        let filter = PathFilter::with_patterns(vec!["User.Email".to_string()]);

        assert!(filter.should_ignore("User.Email"));
        assert!(!filter.should_ignore("user.email"));
        assert!(!filter.should_ignore("USER.EMAIL"));
    }

    // ===== Prefix Wildcard Tests =====

    #[test]
    fn test_prefix_wildcard_basic() {
        let filter = PathFilter::with_patterns(vec!["*.date_created".to_string()]);

        // Should match paths ending with date_created
        assert!(filter.should_ignore("user.date_created"));
        assert!(filter.should_ignore("550e8400-e29b-41d4-a716-446655440000.date_created"));
        assert!(filter.should_ignore("record_123.date_created"));

        // Should match the exact suffix without prefix
        assert!(filter.should_ignore("date_created"));

        // Should NOT match paths that don't end with date_created
        assert!(!filter.should_ignore("user.date_updated"));
        assert!(!filter.should_ignore("date_created.something"));
        assert!(!filter.should_ignore("user.date_created_at"));
    }

    #[test]
    fn test_prefix_wildcard_with_nested_paths() {
        let filter = PathFilter::with_patterns(vec!["*.timestamp".to_string()]);

        // Should match nested paths ending with timestamp
        assert!(filter.should_ignore("user.metadata.timestamp"));
        assert!(filter.should_ignore("events.payload.timestamp"));
        assert!(filter.should_ignore("a.b.c.d.timestamp"));

        // Should NOT match if timestamp is not the last segment
        assert!(!filter.should_ignore("user.timestamp.value"));
        assert!(!filter.should_ignore("timestamp.created"));
    }

    #[test]
    fn test_prefix_wildcard_with_uuids() {
        let filter =
            PathFilter::with_patterns(vec!["*.created_at".to_string(), "*.updated_at".to_string()]);

        // Should match UUID-keyed fields with these suffixes
        assert!(filter.should_ignore("550e8400-e29b-41d4-a716-446655440000.created_at"));
        assert!(filter.should_ignore("6ba7b810-9dad-11d1-80b4-00c04fd430c8.updated_at"));
        assert!(filter.should_ignore("user_abc123.created_at"));

        // Should NOT match different suffixes
        assert!(!filter.should_ignore("550e8400-e29b-41d4-a716-446655440000.deleted_at"));
        assert!(!filter.should_ignore("user_abc123.name"));
    }

    #[test]
    fn test_prefix_wildcard_case_sensitive() {
        let filter = PathFilter::with_patterns(vec!["*.CreatedAt".to_string()]);

        assert!(filter.should_ignore("user.CreatedAt"));
        assert!(!filter.should_ignore("user.createdat"));
        assert!(!filter.should_ignore("user.CREATEDAT"));
        assert!(!filter.should_ignore("user.created_at"));
    }

    #[test]
    fn test_combined_suffix_and_prefix_wildcards() {
        let filter = PathFilter::with_patterns(vec![
            "internal.*".to_string(),  // Suffix wildcard
            "*.timestamp".to_string(), // Prefix wildcard
            "user.email".to_string(),  // Exact match
        ]);

        // Suffix wildcard matches
        assert!(filter.should_ignore("internal.secret"));
        assert!(filter.should_ignore("internal.data.key"));

        // Prefix wildcard matches
        assert!(filter.should_ignore("event.timestamp"));
        assert!(filter.should_ignore("uuid-123.timestamp"));

        // Exact match
        assert!(filter.should_ignore("user.email"));

        // No matches
        assert!(!filter.should_ignore("external.data"));
        assert!(!filter.should_ignore("user.name"));
        assert!(!filter.should_ignore("event.created"));
    }

    #[test]
    fn test_prefix_wildcard_with_special_characters() {
        let filter = PathFilter::with_patterns(vec!["*.user_id".to_string()]);

        // Should match paths with underscores
        assert!(filter.should_ignore("record.user_id"));
        assert!(filter.should_ignore("abc-123.user_id"));

        // Should NOT match partial matches
        assert!(!filter.should_ignore("record.user_identifier"));
        assert!(!filter.should_ignore("user_id_value"));
    }

    #[test]
    fn test_prefix_wildcard_empty_prefix() {
        let filter = PathFilter::with_patterns(vec!["*.id".to_string()]);

        // Should match any path ending with .id
        assert!(filter.should_ignore("user.id"));
        assert!(filter.should_ignore("a.b.c.id"));

        // Should match just "id" (no prefix)
        assert!(filter.should_ignore("id"));

        // Should NOT match paths where id is not the last segment
        assert!(!filter.should_ignore("id.value"));
    }

    #[test]
    fn test_prefix_wildcard_matches_array_notation() {
        let filter = PathFilter::with_patterns(vec!["*.name".to_string()]);

        // Should match regular nested paths
        assert!(filter.should_ignore("user.name"));
        assert!(filter.should_ignore("record.name"));

        // Should also match array notation paths ending with .name
        assert!(filter.should_ignore("items[].name"));
        assert!(filter.should_ignore("users[0].name"));

        // Should NOT match if name is not the last segment
        assert!(!filter.should_ignore("items[].name.first"));
    }

    #[test]
    fn test_multiple_prefix_wildcards() {
        let filter = PathFilter::with_patterns(vec![
            "*.created_at".to_string(),
            "*.updated_at".to_string(),
            "*.deleted_at".to_string(),
        ]);

        // All timestamp fields should be ignored
        assert!(filter.should_ignore("user.created_at"));
        assert!(filter.should_ignore("record.updated_at"));
        assert!(filter.should_ignore("item.deleted_at"));
        assert!(filter.should_ignore("550e8400-e29b-41d4-a716-446655440000.created_at"));

        // Other fields should not be ignored
        assert!(!filter.should_ignore("user.name"));
        assert!(!filter.should_ignore("record.status"));
    }

    #[test]
    fn test_prefix_wildcard_overlapping_patterns() {
        let filter = PathFilter::with_patterns(vec![
            "*.metadata".to_string(),
            "user.metadata".to_string(), // Both should match user.metadata
        ]);

        // Should match via both patterns
        assert!(filter.should_ignore("user.metadata"));

        // Should match via prefix wildcard only
        assert!(filter.should_ignore("record.metadata"));
        assert!(filter.should_ignore("abc.metadata"));
    }
}
