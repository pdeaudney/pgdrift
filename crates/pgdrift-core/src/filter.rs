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
    /// Supports two types of patterns:
    /// - Exact match: "user.email" matches only "user.email"
    /// - Prefix match: "user.internal.*" matches "user.internal" and all its children
    ///   like "user.internal.id", "user.internal.token", etc.
    pub fn should_ignore(&self, path: &str) -> bool {
        self.ignore_patterns.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix(".*") {
                // Prefix match: pattern ends with .*
                // Match the prefix itself OR paths that start with "prefix."
                path == prefix || path.starts_with(&format!("{}.", prefix))
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
}
