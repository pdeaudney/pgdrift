pub mod analyze;
pub mod discover;
pub mod index;
pub mod scan_all;

use anyhow::{Context, Result};
use pgdrift_core::filter::PathFilter;
use std::path::Path;

/// Load path filter from TOML config and CLI args
///
/// This function merges patterns from a TOML configuration file
/// with patterns provided via CLI arguments.
///
/// # Arguments
/// * `cli_patterns` - Patterns provided via --ignore-path CLI flags
/// * `config_path` - Optional path to TOML config file (defaults to .pgdrift-ignore.toml)
///
/// # Returns
/// A `PathFilter` containing all patterns from both sources
///
/// # Errors
/// Returns an error if the TOML file exists but cannot be parsed
pub fn load_path_filter(
    cli_patterns: Vec<String>,
    config_path: Option<String>,
) -> Result<PathFilter> {
    let mut filter = PathFilter::new();

    // Try to load from TOML file
    let toml_path = config_path.unwrap_or_else(|| ".pgdrift-ignore.toml".to_string());

    if Path::new(&toml_path).exists() {
        filter = PathFilter::from_file(&toml_path)
            .with_context(|| format!("Failed to parse ignore config: {}", toml_path))?;
    }

    // Add CLI patterns (merge with TOML)
    if !cli_patterns.is_empty() {
        filter.add_patterns(cli_patterns);
    }

    Ok(filter)
}
