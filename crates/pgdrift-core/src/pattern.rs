use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for custom regex patterns used in patternProperties
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PatternConfig {
    custom_patterns: Vec<CustomPattern>,
}

/// A custom regex pattern for matching dynamic property keys
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomPattern {
    /// The regex pattern (JSON Schema compatible)
    pub regex: String,
    /// Optional description of what this pattern matches
    pub description: Option<String>,
}

impl PatternConfig {
    /// Create an empty pattern config with no custom patterns
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config with the given patterns
    pub fn with_patterns(patterns: Vec<String>) -> Self {
        Self {
            custom_patterns: patterns
                .into_iter()
                .map(|regex| CustomPattern {
                    regex,
                    description: None,
                })
                .collect(),
        }
    }

    /// Load pattern configuration from a TOML file
    ///
    /// Example TOML file format:
    /// ```toml
    /// [[patterns]]
    /// regex = "^[0-9]{6}$"
    /// description = "6-digit numeric IDs"
    ///
    /// [[patterns]]
    /// regex = "^session_[0-9a-f]{32}$"
    /// description = "Session tokens"
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: PatternConfigFile = toml::from_str(&contents)?;
        Ok(Self {
            custom_patterns: config.patterns,
        })
    }

    /// Add additional patterns to the config
    pub fn add_patterns(&mut self, patterns: Vec<String>) {
        self.custom_patterns.extend(
            patterns.into_iter().map(|regex| CustomPattern {
                regex,
                description: None,
            }),
        );
    }

    /// Add a custom pattern with a description
    pub fn add_pattern(&mut self, regex: String, description: Option<String>) {
        self.custom_patterns.push(CustomPattern { regex, description });
    }

    /// Get all configured custom patterns
    pub fn patterns(&self) -> &[CustomPattern] {
        &self.custom_patterns
    }

    /// Check if a key matches any of the configured patterns
    pub fn matches(&self, key: &str) -> Option<&CustomPattern> {
        self.custom_patterns.iter().find(|pattern| {
            // Try to compile and match the regex
            if let Ok(re) = regex::Regex::new(&pattern.regex) {
                re.is_match(key)
            } else {
                false
            }
        })
    }

    /// Get the regex string for a pattern, or None if not found
    pub fn get_regex(&self, key: &str) -> Option<&str> {
        self.matches(key).map(|p| p.regex.as_str())
    }
}

/// Internal structure for deserializing TOML configuration
#[derive(Debug, Deserialize)]
struct PatternConfigFile {
    patterns: Vec<CustomPattern>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_empty_config() {
        let config = PatternConfig::new();
        assert_eq!(config.patterns().len(), 0);
        assert!(config.matches("anything").is_none());
    }

    #[test]
    fn test_with_patterns() {
        let patterns = vec![
            "^[0-9]{6}$".to_string(),
            "^session_[0-9a-f]{32}$".to_string(),
        ];
        let config = PatternConfig::with_patterns(patterns);

        assert_eq!(config.patterns().len(), 2);
        assert!(config.matches("123456").is_some());
        assert!(config.matches("session_abcdef0123456789abcdef0123456789").is_some());
        assert!(config.matches("invalid").is_none());
    }

    #[test]
    fn test_add_patterns() {
        let mut config = PatternConfig::new();
        config.add_patterns(vec!["^[0-9]{6}$".to_string()]);

        assert_eq!(config.patterns().len(), 1);
        assert!(config.matches("123456").is_some());
        assert!(config.matches("12345").is_none());
    }

    #[test]
    fn test_add_pattern_with_description() {
        let mut config = PatternConfig::new();
        config.add_pattern(
            "^[0-9]{6}$".to_string(),
            Some("6-digit IDs".to_string()),
        );

        let patterns = config.patterns();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].regex, "^[0-9]{6}$");
        assert_eq!(patterns[0].description, Some("6-digit IDs".to_string()));
    }

    #[test]
    fn test_matches() {
        let config = PatternConfig::with_patterns(vec![
            "^[0-9]{6}$".to_string(),
            "^user_[a-z]+$".to_string(),
        ]);

        // Should match first pattern
        let m1 = config.matches("123456");
        assert!(m1.is_some());
        assert_eq!(m1.unwrap().regex, "^[0-9]{6}$");

        // Should match second pattern
        let m2 = config.matches("user_admin");
        assert!(m2.is_some());
        assert_eq!(m2.unwrap().regex, "^user_[a-z]+$");

        // Should not match
        assert!(config.matches("12345").is_none());
        assert!(config.matches("user_123").is_none());
    }

    #[test]
    fn test_get_regex() {
        let config = PatternConfig::with_patterns(vec!["^[0-9]{6}$".to_string()]);

        assert_eq!(config.get_regex("123456"), Some("^[0-9]{6}$"));
        assert_eq!(config.get_regex("invalid"), None);
    }

    #[test]
    fn test_from_file() {
        let toml_content = r#"
[[patterns]]
regex = "^[0-9]{6}$"
description = "6-digit numeric IDs"

[[patterns]]
regex = "^session_[0-9a-f]{32}$"
description = "Session tokens"

[[patterns]]
regex = "^api_key_[A-Za-z0-9]{40}$"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "{}", toml_content).unwrap();

        let config = PatternConfig::from_file(temp_file.path()).unwrap();

        assert_eq!(config.patterns().len(), 3);

        // Check first pattern
        assert_eq!(config.patterns()[0].regex, "^[0-9]{6}$");
        assert_eq!(
            config.patterns()[0].description,
            Some("6-digit numeric IDs".to_string())
        );

        // Check second pattern
        assert_eq!(config.patterns()[1].regex, "^session_[0-9a-f]{32}$");
        assert_eq!(
            config.patterns()[1].description,
            Some("Session tokens".to_string())
        );

        // Check third pattern (no description)
        assert_eq!(config.patterns()[2].regex, "^api_key_[A-Za-z0-9]{40}$");
        assert_eq!(config.patterns()[2].description, None);

        // Test matching
        assert!(config.matches("123456").is_some());
        assert!(config.matches("session_abcdef0123456789abcdef0123456789").is_some());
        assert!(config
            .matches("api_key_abcdefghijklmnopqrstuvwxyz1234567890ABCD")
            .is_some());
    }

    #[test]
    fn test_case_sensitive_matching() {
        let config = PatternConfig::with_patterns(vec!["^User_[0-9]+$".to_string()]);

        assert!(config.matches("User_123").is_some());
        assert!(config.matches("user_123").is_none()); // lowercase 'u'
        assert!(config.matches("USER_123").is_none()); // uppercase 'USER'
    }

    #[test]
    fn test_invalid_regex_ignored() {
        let config = PatternConfig::with_patterns(vec![
            "^[0-9]{6}$".to_string(),  // Valid
            "[invalid(".to_string(),    // Invalid regex
        ]);

        // Valid pattern should still work
        assert!(config.matches("123456").is_some());

        // Invalid regex should not crash, just not match
        assert!(config.matches("[invalid(").is_none());
    }

    #[test]
    fn test_complex_patterns() {
        let config = PatternConfig::with_patterns(vec![
            // ISO 8601 timestamp pattern
            r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$".to_string(),
            // Email-like pattern
            r"^[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}$".to_string(),
        ]);

        assert!(config.matches("2024-01-12T10:30:00Z").is_some());
        assert!(config.matches("user@example.com").is_some());
        assert!(config.matches("invalid").is_none());
    }

    #[test]
    fn test_multiple_patterns_first_match() {
        let config = PatternConfig::with_patterns(vec![
            "^[0-9]+$".to_string(),  // Matches any number
            "^[0-9]{6}$".to_string(), // Matches 6-digit number
        ]);

        // Should match the first pattern
        let m = config.matches("123456");
        assert!(m.is_some());
        assert_eq!(m.unwrap().regex, "^[0-9]+$");
    }
}
