use clap::{Parser, Subcommand};

mod commands;
mod output;

#[derive(Parser)]
#[command(
    name = "pgdrift",
    about = "A tool to detect and manage schema drift in PostgreSQL databases."
)]
#[command(author, version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all jsonb columns in the database
    Discover {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: output::OutputFormat,
    },

    /// Analyze a jsonb column for schema drift
    Analyze {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Table name
        // #[arg(short, long)]
        table: String,

        /// Column name
        // #[arg(short, long)]
        column: String,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: output::OutputFormat,

        /// Number of samples to analyze
        #[arg(short, long, default_value = "5000")]
        sample_size: usize,

        /// JSON paths to ignore (can be specified multiple times)
        #[arg(long = "ignore-path", value_name = "PATTERN")]
        ignore_paths: Vec<String>,

        /// Path to ignore config file (default: .pgdrift-ignore.toml)
        #[arg(long = "ignore-config", value_name = "FILE")]
        ignore_config: Option<String>,
    },

    /// Generate index recommendations for a jsonb column
    Index {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Table name
        table: String,

        /// Column name
        column: String,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: output::OutputFormat,

        /// Number of samples to analyze
        #[arg(short, long, default_value = "5000")]
        sample_size: usize,

        /// JSON paths to ignore (can be specified multiple times)
        #[arg(long = "ignore-path", value_name = "PATTERN")]
        ignore_paths: Vec<String>,

        /// Path to ignore config file (default: .pgdrift-ignore.toml)
        #[arg(long = "ignore-config", value_name = "FILE")]
        ignore_config: Option<String>,
    },

    /// Scan all jsonb columns in the database for drift
    ScanAll {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: output::OutputFormat,

        /// Number of samples to analyze per column
        #[arg(short, long, default_value = "5000")]
        sample_size: usize,

        /// Only scan columns in this schema
        #[arg(long = "schema", value_name = "SCHEMA")]
        schema: Option<String>,

        /// Only scan columns in this table
        #[arg(long = "table", value_name = "TABLE")]
        table: Option<String>,

        /// JSON paths to ignore (can be specified multiple times)
        #[arg(long = "ignore-path", value_name = "PATTERN")]
        ignore_paths: Vec<String>,

        /// Path to ignore config file (default: .pgdrift-ignore.toml)
        #[arg(long = "ignore-config", value_name = "FILE")]
        ignore_config: Option<String>,
    },

    /// Generate migration guide to extract JSONB fields to native columns
    Migrate {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Table name
        table: String,

        /// Column name
        column: String,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: output::OutputFormat,

        /// Number of samples to analyze
        #[arg(short, long, default_value = "5000")]
        sample_size: usize,

        /// Minimum field density to consider for migration (0.0-1.0)
        #[arg(long, default_value = "0.8")]
        min_density: f64,

        /// Minimum type consistency to consider safe (0.0-1.0)
        #[arg(long, default_value = "0.95")]
        min_type_consistency: f64,

        /// JSON paths to ignore (can be specified multiple times)
        #[arg(long = "ignore-path", value_name = "PATTERN")]
        ignore_paths: Vec<String>,

        /// Path to ignore config file (default: .pgdrift-ignore.toml)
        #[arg(long = "ignore-config", value_name = "FILE")]
        ignore_config: Option<String>,
    },

    /// Generate JSON Schema for a JSONB column
    Schema {
        /// DB connection URL
        #[arg(short, long, env = "DATABASE_URL")]
        database_url: String,

        /// Table name
        table: String,

        /// Column name
        column: String,

        /// Schema output format: json-schema or pg-jsonschema
        #[arg(short = 'f', long, default_value = "json-schema")]
        format: String,

        /// Number of samples to analyze
        #[arg(short, long, default_value = "5000")]
        sample_size: usize,

        /// Minimum field density to be marked as required (0.0-1.0)
        #[arg(long, default_value = "0.95")]
        required_threshold: f64,

        /// Strict mode: set additionalProperties to false
        #[arg(long)]
        strict: bool,

        /// JSON paths to ignore (can be specified multiple times)
        #[arg(long = "ignore-path", value_name = "PATTERN")]
        ignore_paths: Vec<String>,

        /// Path to ignore config file (default: .pgdrift-ignore.toml)
        #[arg(long = "ignore-config", value_name = "FILE")]
        ignore_config: Option<String>,

        /// Custom regex patterns for patternProperties (can be specified multiple times)
        #[arg(long = "pattern-regex", value_name = "REGEX")]
        pattern_regexes: Vec<String>,

        /// Path to pattern config file (TOML format)
        #[arg(long = "pattern-config", value_name = "FILE")]
        pattern_config: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Discover {
            database_url,
            format,
        } => {
            commands::discover::run(&database_url, format).await?;
        }
        Commands::Analyze {
            database_url,
            table,
            column,
            sample_size,
            format,
            ignore_paths,
            ignore_config,
        } => {
            let filter = commands::load_path_filter(ignore_paths, ignore_config)?;
            commands::analyze::run(&database_url, &table, &column, sample_size, format, filter)
                .await?;
        }
        Commands::Index {
            database_url,
            table,
            column,
            sample_size,
            format,
            ignore_paths,
            ignore_config,
        } => {
            let filter = commands::load_path_filter(ignore_paths, ignore_config)?;
            commands::index::run(&database_url, &table, &column, sample_size, format, filter)
                .await?;
        }
        Commands::ScanAll {
            database_url,
            sample_size,
            format,
            schema,
            table,
            ignore_paths,
            ignore_config,
        } => {
            let filter = commands::load_path_filter(ignore_paths, ignore_config)?;
            commands::scan_all::run(&database_url, sample_size, format, schema, table, filter)
                .await?;
        }
        Commands::Migrate {
            database_url,
            table,
            column,
            sample_size,
            format,
            min_density,
            min_type_consistency,
            ignore_paths,
            ignore_config,
        } => {
            let filter = commands::load_path_filter(ignore_paths, ignore_config)?;
            commands::migrate::run(
                &database_url,
                &table,
                &column,
                sample_size,
                format,
                min_density,
                min_type_consistency,
                filter,
            )
            .await?;
        }
        Commands::Schema {
            database_url,
            table,
            column,
            format,
            sample_size,
            required_threshold,
            strict,
            ignore_paths,
            ignore_config,
            pattern_regexes,
            pattern_config,
        } => {
            let filter = commands::load_path_filter(ignore_paths, ignore_config)?;
            let pattern_cfg = commands::load_pattern_config(pattern_regexes, pattern_config)?;
            let schema_format = commands::schema::SchemaFormat::from_str(&format)?;
            commands::schema::run(
                &database_url,
                &table,
                &column,
                sample_size,
                schema_format,
                required_threshold,
                strict,
                filter,
                pattern_cfg,
            )
            .await?;
        }
    }
    Ok(())
}
