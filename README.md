# pgdrift

pgdrift is a command-line tool for detecting schema drift in PostgreSQL JSONB columns. It analyzes semi-structured data, identifies inconsistencies, and helps maintain data quality in production systems.

## What It Does

When you store JSON documents in PostgreSQL JSONB columns, the schema isn't enforced at the database level. Over time, this leads to drift: fields changing types, optional fields appearing inconsistently, deprecated fields lingering in old records. pgdrift scans your JSONB columns and surfaces these issues before they cause runtime errors.

**Key capabilities:**

- Discover all JSONB columns in a database
- Detect type inconsistencies (field appears as both string and number)
- Identify ghost keys (deprecated fields present in <10% of records)
- Find sparse fields (optional fields present in 10-80% of records)
- Detect missing required fields (expected fields present in 80-95% of records)
- Analyze schema evolution patterns
- **Generate PostgreSQL index recommendations** for JSONB fields (B-tree, GIN, Partial)
- **Generate migration guides** to extract JSONB fields to native PostgreSQL columns
- **Generate JSON Schema** definitions with pg_jsonschema CHECK constraints
- **Scan all JSONB columns** at once for database-wide drift analysis
- Generate reports in multiple formats (table, JSON, markdown)

## Getting Started

### Prerequisites

- PostgreSQL 12 or later
- Access to a PostgreSQL database with JSONB columns
- (Optional) Rust 1.75 or later if building from source

### Installation

#### Option 1: Install from crates.io (recommended)

```bash
cargo install pgdrift
```

This downloads and compiles the latest stable release from crates.io.

#### Option 2: Download pre-built binaries

Download the latest release for your platform from the [GitHub Releases page](https://github.com/capybarastack/pgdrift/releases):

**Linux:**
```bash
curl -L https://github.com/capybarastack/pgdrift/releases/latest/download/pgdrift-x86_64-unknown-linux-gnu -o pgdrift
chmod +x pgdrift
sudo mv pgdrift /usr/local/bin/
```

**macOS:**
```bash
curl -L https://github.com/capybarastack/pgdrift/releases/latest/download/pgdrift-x86_64-apple-darwin -o pgdrift
chmod +x pgdrift
sudo mv pgdrift /usr/local/bin/
```

**Windows:**
Download `pgdrift-x86_64-pc-windows-msvc.exe` from the releases page.

#### Option 3: Build from source

Clone the repository and build:

```bash
git clone https://github.com/capybarastack/pgdrift.git
cd pgdrift
cargo build --release
```

The compiled binary will be at `target/release/pgdrift`.

Or install directly from the local clone:

```bash
cargo install --path crates/pgdrift
```

## Usage

pgdrift provides six main commands: `discover`, `analyze`, `scan-all`, `index`, `migrate`, and `schema`.

### Discovering JSONB Columns

List all JSONB columns in your database:

```bash
pgdrift discover --database-url "postgres://user:pass@localhost/mydb"
```

You can also set the database URL via environment variable:

```bash
export DATABASE_URL="postgres://user:pass@localhost/mydb"
pgdrift discover
```

**Example output:**

```
JSONB Columns Found:
┌────────┬───────────┬──────────┬────────────────┐
│ Schema │ Table     │ Column   │ Approx Rows    │
├────────┼───────────┼──────────┼────────────────┤
│ public │ users     │ metadata │ 1,245,892      │
│ public │ events    │ payload  │ 8,932,441      │
│ public │ sessions  │ context  │ 445,201        │
└────────┴───────────┴──────────┴────────────────┘
```

### Analyzing a JSONB Column

Run drift detection on a specific table and column:

```bash
pgdrift analyze users metadata --database-url $DATABASE_URL
```

By default, pgdrift samples 5,000 rows. You can adjust this:

```bash
pgdrift analyze users metadata --sample-size 10000
```

**Example output:**

```
Analyzing public.users.metadata (5000 samples)
[████████████████████] 5000/5000 (00:00:03)

Schema Analysis Summary:
  Total unique paths: 47
  Maximum nesting depth: 5
  Drift issues found: 2 critical, 3 warnings, 8 info

Critical Issues:
┌──────────────────────┬──────────┬─────────────────────────────────────────────────────────┐
│ Path                 │ Severity │ Issue                                                   │
├──────────────────────┼──────────┼─────────────────────────────────────────────────────────┤
│ user.age             │ Critical │ Type inconsistency (minority: 8.0%: string:92.0, num... │
│ user.email           │ Critical │ Missing key: 15.00% missing (750/5000 samples missi...  │
└──────────────────────┴──────────┴─────────────────────────────────────────────────────────┘

Warnings:
┌──────────────────────┬──────────┬─────────────────────────────────────────────────────────┐
│ Path                 │ Severity │ Issue                                                   │
├──────────────────────┼──────────┼─────────────────────────────────────────────────────────┤
│ user.phone           │ Warning  │ Missing key: 8.00% missing (400/5000 samples missing... │
│ prefs.theme          │ Warning  │ Schema evolution: deprecated field 'old_theme' → 't...  │
└──────────────────────┴──────────┴─────────────────────────────────────────────────────────┘

Info:
┌──────────────────────┬──────────┬─────────────────────────────────────────────────────────┐
│ Path                 │ Severity │ Issue                                                   │
├──────────────────────┼──────────┼─────────────────────────────────────────────────────────┤
│ legacy.deprecated_id │ Info     │ Ghost key: 0.80% present (40/5000 samples)              │
│ user.nickname        │ Info     │ Sparse field: 45.00% present (2250/5000 samples)        │
└──────────────────────┴──────────┴─────────────────────────────────────────────────────────┘
```

### Scanning All JSONB Columns

Analyze all JSONB columns in your database at once:

```bash
pgdrift scan-all --database-url $DATABASE_URL
```

This command discovers all JSONB columns and runs drift analysis on each one, providing a summary of issues across your entire database.

**Example output:**

```
Discovered 3 JSONB columns. Starting analysis...

Analyzing column: public.metadata (table: users)
Analysis complete for public.users.metadata - Samples Analyzed: 5000, Issues Found: 5 (Critical: 2, Warning: 2, Info: 1)

Analyzing column: public.payload (table: events)
Analysis complete for public.events.payload - Samples Analyzed: 5000, Issues Found: 12 (Critical: 0, Warning: 4, Info: 8)

Analyzing column: public.context (table: sessions)
Analysis complete for public.sessions.context - Samples Analyzed: 5000, Issues Found: 0 (Critical: 0, Warning: 0, Info: 0)

Scan All Complete - Scanned 3 column(s)

Overall Summary:
  Total samples analyzed: 15000
  Total issues found: 17
    Critical: 2
    Warning: 6
    Info: 9

Column Details:
╭────────┬──────────┬──────────┬─────────┬──────────┬─────────┬──────┬──────────────╮
│ Schema │ Table    │ Column   │ Samples │ Critical │ Warning │ Info │ Total Issues │
├────────┼──────────┼──────────┼─────────┼──────────┼─────────┼──────┼──────────────┤
│ public │ users    │ metadata │ 5000    │ 2        │ 2       │ 1    │ 5            │
│ public │ events   │ payload  │ 5000    │ 0        │ 4       │ 8    │ 12           │
│ public │ sessions │ context  │ 5000    │ 0        │ 0       │ 0    │ 0            │
╰────────┴──────────┴──────────┴─────────┴──────────┴─────────┴──────┴──────────────╯

* Columns with critical issues:
  • public.users.metadata
```

#### Filtering by Schema or Table

You can filter scan-all to analyze only specific schemas or tables:

**Filter by schema:**
```bash
pgdrift scan-all --database-url $DATABASE_URL --schema public
```

**Filter by table:**
```bash
pgdrift scan-all --database-url $DATABASE_URL --table users
```

**Filter by both schema and table:**
```bash
pgdrift scan-all --database-url $DATABASE_URL --schema public --table users
```

This is useful when you have a large database and only want to analyze specific tables or schemas, or when you want to focus on a particular area of your database.

### Generating Index Recommendations

Get PostgreSQL index recommendations for JSONB fields:

```bash
pgdrift index users metadata --database-url $DATABASE_URL
```

The index command analyzes field density, cardinality, and access patterns to recommend appropriate index types.

**Example output:**

```
Index Recommendations for users.metadata

Summary:
  Total recommendations: 3
  High priority: 1
  Medium priority: 2

Recommendations:
╭─────────────────┬────────────┬──────────┬──────────────────────────────────────────────────────╮
│ Field Path      │ Index Type │ Priority │ Reason                                               │
├─────────────────┼────────────┼──────────┼──────────────────────────────────────────────────────┤
│ user.email      │ B-tree     │ High     │ High density (98.5%), scalar type (string)           │
│ user.tags       │ GIN        │ Medium   │ Array type, suitable for containment queries         │
│ prefs.theme     │ Partial    │ Medium   │ Low density (15.2%), create index WHERE field exists │
╰─────────────────┴────────────┴──────────┴──────────────────────────────────────────────────────╯

SQL Commands:

1 - user.email
-- High density scalar field - B-tree index recommended
CREATE INDEX idx_users_metadata_user_email
  ON users ((metadata->'user'->>'email'));
Benefit: Speeds up equality and range queries on user.email

2 - user.tags
-- Array field - GIN index recommended for containment queries
CREATE INDEX idx_users_metadata_user_tags
  ON users USING GIN ((metadata->'user'->'tags'));
Benefit: Enables fast @>, @<, && containment queries on arrays

3 - prefs.theme
-- Sparse field - Partial index recommended
CREATE INDEX idx_users_metadata_prefs_theme
  ON users ((metadata->'prefs'->>'theme'))
  WHERE metadata->'prefs'->>'theme' IS NOT NULL;
Benefit: Reduces index size by only indexing rows where field exists
```

### Generating Migration Guides

Generate SQL migration scripts to extract stable JSONB fields to native PostgreSQL columns:

```bash
pgdrift migrate users metadata --database-url $DATABASE_URL
```

The migrate command analyzes JSONB fields and identifies candidates suitable for extraction to native columns based on density (how often the field appears) and type consistency. This improves query performance and enables better indexing.

**Example output:**

```
Analyzing 5000 samples...

Found 5 field(s) eligible for migration

╭───────────────┬─────────────┬─────────────┬─────────┬──────────────────┬──────────┬──────────╮
│ Field         │ Source Type │ Target Type │ Density │ Type Consistency │ Nullable │ Warnings │
├───────────────┼─────────────┼─────────────┼─────────┼──────────────────┼──────────┼──────────┤
│ email         │ String      │ TEXT        │ 100.0%  │ 100.0%           │ No       │ ✓        │
│ age           │ Number      │ INTEGER     │ 98.0%   │ 95.2%            │ Yes      │ ⚠️ 1     │
│ is_active     │ Boolean     │ BOOLEAN     │ 100.0%  │ 100.0%           │ No       │ ✓        │
│ created_at    │ String      │ TIMESTAMP   │ 99.0%   │ 99.0%            │ Yes      │ ✓        │
│ score         │ Number      │ NUMERIC     │ 85.0%   │ 100.0%           │ Yes      │ ✓        │
╰───────────────┴─────────────┴─────────────┴─────────┴──────────────────┴──────────┴──────────╯

⚠️ age:
  • Warning Type inconsistency: 4.8% of values are STRING
    Examples: "unknown", "N/A"

--- Generated Migration SQL ---

-- ⚠️  WARNING: This is a generated migration guide.
-- ⚠️  REVIEW CAREFULLY before executing any statements.
-- ⚠️  Test on a staging environment first.

-- Step 1: Add new native columns
ALTER TABLE public.users
  ADD COLUMN email TEXT,
  ADD COLUMN age INTEGER,
  ADD COLUMN is_active BOOLEAN DEFAULT false,
  ADD COLUMN created_at TIMESTAMP,
  ADD COLUMN score NUMERIC(10,2);

-- Step 2: Backfill data from JSONB
UPDATE public.users SET
  email = metadata->>'email',
  age = (metadata->>'age')::INTEGER,
  is_active = (metadata->>'is_active')::BOOLEAN,
  created_at = (metadata->>'created_at')::TIMESTAMP,
  score = (metadata->>'score')::NUMERIC;

-- Step 3: Add NOT NULL constraints
ALTER TABLE public.users
  ALTER COLUMN email SET NOT NULL,
  ALTER COLUMN is_active SET NOT NULL;

-- Step 4: Create indexes on native columns
CREATE INDEX idx_users_email ON public.users(email);
CREATE INDEX idx_users_created_at ON public.users(created_at);

-- WARNING: Field 'age' has type inconsistencies
--   Recommendation: Clean data before migration
```

#### Migration Criteria

Fields are considered good migration candidates when they meet these criteria:

- **High density** (≥80% occurrence by default) - Field appears consistently across records
- **Type consistent** (≥95% same type by default) - Safe to map to a native PostgreSQL type
- **Scalar type** (String, Number, Boolean) - Not Object or Array
- **Not filtered** - Not excluded via `--ignore-path` patterns

You can adjust these thresholds:

```bash
# Lower thresholds to include more fields
pgdrift migrate users metadata --min-density 0.6 --min-type-consistency 0.90

# Stricter thresholds for high-confidence migrations only
pgdrift migrate users metadata --min-density 0.95 --min-type-consistency 0.99
```

#### Type Mapping

pgdrift automatically infers the best PostgreSQL type based on JSON type and observed data:

| JSON Type | PostgreSQL Type | Decision Logic |
|-----------|----------------|----------------|
| String | TEXT or VARCHAR(n) | VARCHAR if max length < 255 chars |
| Number | INTEGER, BIGINT, NUMERIC, DOUBLE | Based on range and decimal detection |
| Boolean | BOOLEAN | Direct mapping |
| Null | Nullable column | Adds NULL constraint |

#### Output Formats

Migration guides can be generated in multiple formats:

**SQL format** (default): Executable migration SQL with warnings as comments

```bash
pgdrift migrate users metadata --format table > migration.sql
```

**JSON format**: Machine-readable for programmatic processing

```bash
pgdrift migrate users metadata --format json > migration.json
```

**Markdown format**: Human-readable guide for documentation

```bash
pgdrift migrate users metadata --format markdown > migration-guide.md
```

**Important:** The migrate command only generates SQL output files. It never executes migrations or modifies your database. Always review and test the generated SQL on a staging environment before applying to production.

### Generating JSON Schemas

Generate JSON Schema definitions (2020-12) with optional pg_jsonschema CHECK constraints:

```bash
pgdrift schema users metadata --database-url $DATABASE_URL
```

The schema command analyzes JSONB structure and generates a JSON Schema that describes the observed data structure, types, required fields, and validation rules.

**Example output:**

```
Sampling Strategy: ReservoirPK (5000 samples via primary key)

Analyzing 5000 samples...

Generated schema with 8 properties
Required fields: 2

--- JSON Schema (2020-12) ---

{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "public.users.metadata schema",
  "description": "Auto-generated from 5000 samples",
  "type": "object",
  "properties": {
    "email": {
      "type": "string",
      "format": "email",
      "description": "Occurs in 100% of samples (5000/5000)"
    },
    "age": {
      "type": "integer",
      "minimum": 18,
      "maximum": 95,
      "description": "Occurs in 98% of samples (4900/5000)"
    },
    "is_active": {
      "type": "boolean",
      "description": "Occurs in 100% of samples (5000/5000)"
    },
    "role": {
      "type": "string",
      "enum": ["admin", "user", "guest"],
      "description": "Occurs in 95% of samples (4750/5000). Detected 3 distinct values."
    },
    "user_id": {
      "type": "string",
      "format": "uuid",
      "description": "Occurs in 100% of samples (5000/5000)"
    },
    "created_at": {
      "type": "string",
      "format": "date-time",
      "description": "Occurs in 99% of samples (4950/5000)"
    },
    "preferences": {
      "type": "object",
      "properties": {
        "theme": {
          "type": "string",
          "enum": ["light", "dark"],
          "description": "Occurs in 80% of samples (4000/5000)"
        }
      },
      "additionalProperties": true
    }
  },
  "required": ["email", "is_active"],
  "additionalProperties": false
}
```

#### Schema Inference Rules

pgdrift automatically infers schema rules based on observed data:

| Rule | Condition | JSON Schema Output |
|------|-----------|-------------------|
| Required field | Density ≥ 95% (default) | Added to `required` array |
| Type constraint | Type consistency ≥ 95% | Set `type` field |
| Multiple types | Type consistency < 95% | Use `oneOf` or `anyOf` |
| Null allowed | null_count > 0 | Add `null` to type array |
| Enum values | ≤ 10 distinct values | Set `enum` array |
| Email pattern | Field named "email" + valid emails | Set `format: "email"` |
| UUID pattern | String matching UUID format | Set `format: "uuid"` |
| Date-time pattern | ISO 8601 timestamps | Set `format: "date-time"` |
| URI pattern | Valid HTTP/HTTPS URLs | Set `format: "uri"` |
| Number range | Numeric min/max observed | Set `minimum`, `maximum` |

#### pg_jsonschema Integration

Generate PostgreSQL CHECK constraints using the [pg_jsonschema](https://github.com/supabase/pg_jsonschema) extension:

```bash
pgdrift schema users metadata --format pg-jsonschema
```

**Example output:**

```
--- pg_jsonschema CHECK Constraint ---

-- ⚠️  WARNING: Review this SQL before executing.
-- ⚠️  Test the constraint on a staging environment first.
-- ⚠️  This may reject existing data that violates the schema.

-- Install pg_jsonschema extension (if not already installed)
-- CREATE EXTENSION IF NOT EXISTS pg_jsonschema;

-- Add CHECK constraint to enforce schema
ALTER TABLE public.users
  ADD CONSTRAINT users_metadata_schema_check
  CHECK (
    json_matches_schema(
      '{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
          "email": {"type": "string", "format": "email"},
          "is_active": {"type": "boolean"}
        },
        "required": ["email", "is_active"],
        "additionalProperties": false
      }'::json,
      metadata
    )
  );
```

This CHECK constraint validates all new and updated JSONB data against the schema, preventing drift from occurring in the future.

#### Schema Options

**Adjust required threshold:**

```bash
# Stricter: only mark fields present in 99%+ of records as required
pgdrift schema users metadata --required-threshold 0.99

# Relaxed: mark fields present in 80%+ as required
pgdrift schema users metadata --required-threshold 0.8
```

**Strict mode** (reject additional properties):

```bash
pgdrift schema users metadata --strict
```

This sets `additionalProperties: false`, meaning the schema will reject any fields not explicitly defined in the properties list.

**Save to file:**

```bash
# Save JSON Schema
pgdrift schema users metadata > users-metadata-schema.json

# Save pg_jsonschema SQL
pgdrift schema users metadata --format pg-jsonschema > add-schema-check.sql
```

#### Installing pg_jsonschema

To use the generated CHECK constraints, install the [pg_jsonschema extension](https://github.com/supabase/pg_jsonschema):

```sql
-- Requires PostgreSQL 12+
CREATE EXTENSION IF NOT EXISTS pg_jsonschema;
```

**Note:** pg_jsonschema must be installed by a superuser. For cloud PostgreSQL services (AWS RDS, Azure, etc.), check if the extension is available or contact support.

**Important:** The schema command only generates JSON Schema and SQL files. It never creates CHECK constraints or modifies your database. Always test generated schemas against existing data before applying constraints.

#### Practical Usage Examples

**Example 1: Enforce user metadata schema**

```bash
# Generate schema for user metadata
pgdrift schema users metadata --strict > user-schema.json

# Generate and apply pg_jsonschema constraint
pgdrift schema users metadata --format pg-jsonschema --strict > add-user-schema.sql

# Review the SQL, then apply (on staging first!)
psql -h staging-db -f add-user-schema.sql

# Test with valid data (should succeed)
INSERT INTO users (name, metadata) VALUES (
  'Alice',
  '{"email": "alice@example.com", "is_active": true}'
);

# Test with invalid data (should fail)
INSERT INTO users (name, metadata) VALUES (
  'Bob',
  '{"email": "not-an-email", "extra_field": "not allowed"}'  -- Fails: invalid format, strict mode
);
```

**Example 2: Gradual schema migration**

```bash
# Start with relaxed schema (no additionalProperties restriction)
pgdrift schema orders metadata > orders-schema-v1.json

# After cleanup, switch to strict mode
pgdrift schema orders metadata --strict --required-threshold 0.99 > orders-schema-v2.json

# Compare schemas to understand the difference
diff orders-schema-v1.json orders-schema-v2.json
```

**Example 3: Combine with path filtering**

```bash
# Generate schema excluding internal/debug fields
pgdrift schema logs data \
  --ignore-path "_internal.*" \
  --ignore-path "debug.*" \
  --format pg-jsonschema > logs-schema.sql
```

### Output Formats

pgdrift supports three output formats:

**Table format** (default): Human-readable ASCII tables with color coding

```bash
pgdrift analyze users metadata --format table
```

**JSON format**: Machine-readable output for programmatic processing

```bash
pgdrift analyze users metadata --format json > drift-report.json
```

**Markdown format**: Copy-paste into GitHub issues or documentation

```bash
pgdrift analyze users metadata --format markdown > DRIFT_REPORT.md
```

### Filtering JSON Paths

pgdrift allows you to exclude specific JSON paths from analysis, which is useful for ignoring sensitive data, debug fields, or internal metadata that you don't want to track.

#### Using CLI Flags

Ignore specific paths using the `--ignore-path` flag (can be used multiple times):

```bash
# Ignore a single path
pgdrift analyze users metadata --ignore-path "user.internal.token"

# Ignore multiple paths
pgdrift analyze users metadata \
  --ignore-path "user.internal.*" \
  --ignore-path "debug.logs" \
  --ignore-path "temp.session_data"
```

Works with all commands (analyze, index, migrate, schema, scan-all):

```bash
# Generate index recommendations, ignoring internal fields
pgdrift index users metadata --ignore-path "internal.*"

# Generate migration guide, ignoring sensitive fields
pgdrift migrate users metadata --ignore-path "internal.*" --ignore-path "debug.*"

# Generate JSON schema, excluding temporary fields
pgdrift schema users metadata --ignore-path "temp.*"

# Scan all columns, ignoring debug and temporary data
pgdrift scan-all --database-url $DATABASE_URL \
  --ignore-path "debug.*" \
  --ignore-path "temp.*"
```

#### Using a Configuration File

Create a `.pgdrift-ignore.toml` file in your project directory:

```toml
# .pgdrift-ignore.toml
ignore_paths = [
    "user.internal.*",      # Ignore all internal user data
    "metadata.debug.*",     # Ignore debug metadata
    "temp.session_data",    # Ignore specific temp field
    "analytics.raw_events", # Ignore raw analytics data
]
```

pgdrift automatically loads this file if it exists:

```bash
# Uses .pgdrift-ignore.toml automatically
pgdrift analyze users metadata
```

You can also specify a custom config file location:

```bash
pgdrift analyze users metadata --ignore-config ./custom-ignore.toml
```

#### Pattern Matching

pgdrift supports two types of patterns:

**Exact match**: Matches only the specific path

```toml
ignore_paths = ["user.email"]
```

- Filters: `user.email`
- Keeps: `user.email.domain`, `user.name`

**Prefix match with wildcard**: Matches a path and all its children

```toml
ignore_paths = ["user.internal.*"]
```

- Filters: `user.internal`, `user.internal.id`, `user.internal.token`, `user.internal.metadata.secret`
- Keeps: `user.email`, `user.name`, `user.external`

**Important notes:**

- Patterns are case-sensitive
- Array paths like `items[].name` use different notation and won't match `items.*`
- CLI flags and TOML patterns are merged (not replaced)

#### Common Use Cases

**Ignoring sensitive data:**

```toml
ignore_paths = [
    "user.ssn",
    "payment.credit_card",
    "credentials.*",
]
```

**Ignoring debug/internal fields:**

```toml
ignore_paths = [
    "debug.*",
    "internal.*",
    "_metadata.*",
]
```

**Ignoring temporary or cache data:**

```toml
ignore_paths = [
    "temp.*",
    "cache.*",
    "session_data",
]
```

**Combining CLI and file:**

```bash
# .pgdrift-ignore.toml has: ["internal.*", "debug.*"]
# This adds "temp.*" to the filter list
pgdrift analyze users metadata --ignore-path "temp.*"
```

### Adaptive Sampling Strategies

pgdrift uses adaptive sampling strategies based on table size:

- **Small tables** (< 100k rows): Random sampling with `ORDER BY random()`
- **Medium tables** (100k-10M rows): Reservoir sampling via primary key index
- **Large tables** (> 10M rows): PostgreSQL native `TABLESAMPLE` (no table locks)

For very large tables, pgdrift automatically selects the safest sampling method to minimize performance impact.

### Row Count Accuracy

pgdrift uses PostgreSQL's internal statistics (`pg_stat_user_tables.n_live_tup`) for estimated row counts. These estimates are fast but can be slightly inaccurate (typically off by 1-2 rows) if the statistics are stale.

If you notice row count discrepancies, update PostgreSQL's statistics by running:

```sql
ANALYZE your_table_name;
```

Or for all tables:

```sql
ANALYZE;
```

This is a lightweight operation that updates metadata without locking tables or affecting performance.

**Future consideration**: If estimate accuracy proves problematic in practice, we may switch to `COUNT(*)` queries for exact counts, trading performance for accuracy.

### Drift Detection Logic

pgdrift categorizes fields based on how frequently they appear across sampled records. The detection logic uses intelligent thresholds to distinguish between different types of schema issues:

#### Field Presence Thresholds

- **Ghost Keys** (<10% present): Deprecated or rarely-used fields that appear in less than 10% of records. These are typically legacy fields that should be cleaned up. Severity: **Info**

- **Sparse Fields** (10-80% present): Optional fields with moderate presence. These represent legitimate optional data that appears in some but not most records. Severity: **Info**

- **Missing Keys** (80-95% present): Fields that appear to be required (high presence) but have unexpected gaps. These likely indicate missing data or incomplete migrations. Severity: **Warning** (90-95%) or **Critical** (<90%)

- **Normal Fields** (≥95% present): Fields consistently present across nearly all records. No issues reported.

#### Type Inconsistency Detection

When a field appears with multiple data types (e.g., sometimes a string, sometimes a number), pgdrift flags it based on the minority type percentage:

- **Critical**: Minority type ≥10% (significant inconsistency)
- **Warning**: Minority type 5-10% (moderate inconsistency)
- **Info**: Minority type <5% (minor inconsistency)

#### Schema Evolution Patterns

pgdrift automatically detects common schema evolution patterns:

- **Version markers**: Fields like `version`, `schema_version`, `api_version`
- **Deprecated naming**: Fields prefixed with `old_`, `legacy_`, `deprecated_`
- **Mutually exclusive fields**: Related fields that never appear together (e.g., `address_v1` and `address_v2`)

All schema evolution detections are reported at **Warning** level.

#### Severity Levels Summary

- **Critical**: Requires immediate attention (major type inconsistencies, missing required fields)
- **Warning**: Should be reviewed (minor type inconsistencies, schema evolution, missing semi-required fields)
- **Info**: Informational (ghost keys, sparse fields, minor issues)

## Testing

pgdrift has comprehensive test coverage across unit and integration tests.

### Running Tests

Run unit tests (no Docker required):

```bash
cargo test --lib
```

Run all tests including integration tests (requires Docker):

```bash
cargo test
```

Run tests for a specific crate:

```bash
cargo test -p pgdrift-core
cargo test -p pgdrift-db
cargo test -p pgdrift
```

### Test Coverage

Current test suite includes:

- **228 total tests** (118 unit tests + 110 integration tests)
- Unit tests for JSON analysis, drift detection, sampling strategies, index recommendations, migration analysis, schema generation, and field categorization
- Integration tests against real PostgreSQL databases using testcontainers
- Edge case testing for SQL injection, Unicode handling, extreme nesting, and mixed types

Run integration tests separately:

```bash
cargo test --test integration_test
cargo test --test analyze_integration_test
cargo test --test discover_integration_test
cargo test --test index_integration_test
cargo test --test migrate_integration_test
cargo test --test schema_integration_test
cargo test --test scan_all_integration_test
```

Integration tests automatically spin up PostgreSQL containers and populate them with fixture data representing common drift scenarios.

## Architecture

pgdrift is structured as a Cargo workspace with three crates:

- **pgdrift-core**: Analysis engine, drift detection algorithms, and core types
- **pgdrift-db**: Database layer, connection pooling, and sampling strategies
- **pgdrift**: Command-line interface and output formatting

This separation allows the analysis engine to be used as a library in other tools.

### How It Works

1. **Discovery**: Query PostgreSQL system catalogs to find all JSONB columns
2. **Sampling**: Select a representative sample using adaptive strategies
3. **Analysis**: Recursively traverse each JSON document, building statistics for every path
4. **Detection**: Apply drift detection algorithms to identify issues
5. **Reporting**: Format and display results with severity levels

The JSON analyzer handles nested objects, arrays, and mixed types. Field paths use dot notation for nesting (`user.profile.email`) and bracket notation for arrays (`addresses[].city`).

## Development Hygiene

Before committing code, ensure you've taken the following steps:

- Run `cargo fmt` to format your code
- Run `cargo clippy` to catch common mistakes
- Run `cargo test` and ensure all tests pass
- Update tests if you've added new functionality

### Code Style

- Follow standard Rust conventions (rustfmt default configuration)
- Write unit tests for new algorithms
- Add integration tests for new commands or database interactions
- Keep functions focused and well-named
- Document public APIs with rustdoc comments

## Configuration

pgdrift reads the database connection string from either:

1. The `--database-url` flag
2. The `DATABASE_URL` environment variable

PostgreSQL connection strings follow the standard format:

```
postgres://username:password@hostname:port/database
```

For local development:

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/dev_db"
```

For production (read-only recommended):

```bash
export DATABASE_URL="postgres://readonly_user:pass@prod.example.com:5432/prod_db"
pgdrift analyze users metadata
```

## Performance

pgdrift is designed to handle large-scale databases efficiently:

- **Sampling performance**: 10,000 samples analyzed in under 10 seconds for most schemas
- **Memory usage**: Peak memory typically under 500MB
- **Minimal impact**: Uses read-only queries and adaptive sampling to avoid production load

Benchmark on a table with 5M rows and moderately complex JSONB (20-30 fields, nesting depth 3):

```
Sampling 10,000 rows: ~2 seconds
Analysis: ~3 seconds
Total: ~5 seconds
```

## Roadmap

pgdrift is under active development. Completed and planned features:

**v0.1.0 - Released** ✅

- ✅ JSONB column discovery
- ✅ Drift detection and analysis
- ✅ Index recommendation engine
- ✅ Scan-all command for database-wide analysis
- ✅ Multiple output formats (table, JSON, markdown)

**v0.2.0 - Current** ✅

- ✅ Migration guide generation (JSONB → native columns)
- ✅ JSON Schema generation (2020-12)
- ✅ pg_jsonschema CHECK constraint support
- ✅ Path filtering with .pgdrift-ignore.toml config

**Future Releases**

- CI/CD integration mode for drift detection in pipelines
- Web dashboard for visual analysis
- Support for additional database types (MySQL JSON, MongoDB)
- Schema evolution tracking and diff generation

## Contributing

Contributions are welcome. Please ensure:

- New features include tests
- Code passes `cargo clippy` and `cargo fmt`
- Integration tests pass locally
- Documentation is updated for user-facing changes

When reporting issues, include:

- PostgreSQL version
- Sample database schema (if possible)
- Steps to reproduce
- Expected vs actual behavior

## License

MIT License. See LICENSE file for details.

## Acknowledgments

Built with:

- [sqlx](https://github.com/launchbadge/sqlx) - Async PostgreSQL driver
- [clap](https://github.com/clap-rs/clap) - Command line argument parsing
- [tabled](https://github.com/zhiburt/tabled) - ASCII table formatting
- [serde_json](https://github.com/serde-rs/json) - JSON parsing and manipulation
- [jsonschema](https://github.com/Stranger6667/jsonschema-rs) - JSON Schema validation (testing)

Inspired by the need for better tooling around semi-structured data in relational databases.
