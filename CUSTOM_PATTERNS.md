# Custom Pattern Properties

## Overview

pgdrift supports custom regex patterns for JSON Schema `patternProperties`. When generating schemas, keys with low density (≤ 10%, "ghost keys") that match custom patterns will be consolidated into `patternProperties` instead of being listed individually.

This is useful for dynamic keys like:
- Session tokens
- API keys
- User/Order IDs with prefixes
- Timestamp-based keys
- Any other pattern-based dynamic keys

## Built-in Patterns

pgdrift automatically detects these patterns:
- **UUID**: `^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$`
- **16-char hex**: `^[0-9a-fA-F]{16}$`

## Adding Custom Patterns

### Method 1: Command Line Arguments

Use `--pattern-regex` to specify patterns directly:

```bash
pgdrift schema users metadata --pattern-regex '^[0-9]{6}$'
pgdrift schema users metadata --pattern-regex '^session_[0-9a-f]{32}$'
```

Multiple patterns can be specified:

```bash
pgdrift schema users metadata \
  --pattern-regex '^user_[0-9]+$' \
  --pattern-regex '^api_key_[A-Za-z0-9]{40}$'
```

### Method 2: Configuration File

Create a TOML configuration file (e.g., `.pgdrift-patterns.toml`):

```toml
[[patterns]]
regex = "^[0-9]{6}$"
description = "6-digit numeric identifiers"

[[patterns]]
regex = "^session_[0-9a-f]{32}$"
description = "Session tokens"

[[patterns]]
regex = "^api_key_[A-Za-z0-9]{40}$"
description = "API keys"
```

Use it with:

```bash
pgdrift schema users metadata --pattern-config .pgdrift-patterns.toml
```

### Method 3: Combine Both

You can combine a config file with additional CLI patterns:

```bash
pgdrift schema users metadata \
  --pattern-config .pgdrift-patterns.toml \
  --pattern-regex '^custom_[0-9]{4}$'
```

## How It Works

### 1. Ghost Key Detection

Keys with density ≤ 10% are considered "ghost keys" (unstable/dynamic).

Example:
```json
{
  "550e8400-e29b-41d4-a716-446655440000": {"name": "Alice"},
  "6ba7b810-9dad-11d1-80b4-00c04fd430c8": {"name": "Bob"}
}
```

In a database with 10,000 rows, if each UUID appears in only ~1% of rows, they're ghost keys.

### 2. Pattern Matching

When 2+ ghost keys match the same pattern, they're consolidated:

**Before** (without pattern properties):
```json
{
  "properties": {
    "550e8400-e29b-41d4-a716-446655440000": {"type": "object", ...},
    "6ba7b810-9dad-11d1-80b4-00c04fd430c8": {"type": "object", ...},
    "123e4567-e89b-12d3-a456-426614174000": {"type": "object", ...}
  }
}
```

**After** (with pattern properties):
```json
{
  "properties": {},
  "patternProperties": {
    "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$": {
      "type": "object",
      "description": "Dynamic keys matching pattern (3 keys detected)",
      "properties": {
        "name": {"type": "string"}
      }
    }
  }
}
```

### 3. Priority Order

1. **Custom patterns** (checked first)
2. **Built-in patterns** (UUID, hex strings)
3. **No match** (key listed individually if not a ghost key)

## Examples

### Example 1: Session Tokens

Data:
```json
{
  "sessions": {
    "session_abc123def456...": {"expires": "2025-01-12"},
    "session_fed789cba012...": {"expires": "2025-01-13"}
  }
}
```

Command:
```bash
pgdrift schema users data \
  --pattern-regex '^session_[0-9a-f]{32}$'
```

Result:
```json
{
  "properties": {
    "sessions": {
      "type": "object",
      "patternProperties": {
        "^session_[0-9a-f]{32}$": {
          "type": "object",
          "properties": {
            "expires": {"type": "string", "format": "date"}
          }
        }
      }
    }
  }
}
```

### Example 2: Multiple Pattern Types

Data:
```json
{
  "user_123": {"name": "Alice"},
  "user_456": {"name": "Bob"},
  "api_key_abc...": {"permissions": ["read"]},
  "api_key_xyz...": {"permissions": ["write"]}
}
```

Config file (`.pgdrift-patterns.toml`):
```toml
[[patterns]]
regex = "^user_[0-9]+$"

[[patterns]]
regex = "^api_key_[A-Za-z0-9]{40}$"
```

Command:
```bash
pgdrift schema users data --pattern-config .pgdrift-patterns.toml
```

Result:
```json
{
  "patternProperties": {
    "^user_[0-9]+$": {
      "type": "object",
      "properties": {
        "name": {"type": "string"}
      }
    },
    "^api_key_[A-Za-z0-9]{40}$": {
      "type": "object",
      "properties": {
        "permissions": {"type": "array"}
      }
    }
  }
}
```

### Example 3: Nested Pattern Properties

Data:
```json
{
  "devices": {
    "abc123def4567890": {"device_no": 1},
    "fedcba9876543210": {"device_no": 2}
  }
}
```

Command (using built-in 16-char hex pattern):
```bash
pgdrift schema users metadata
```

Result:
```json
{
  "properties": {
    "devices": {
      "type": "object",
      "properties": {},
      "patternProperties": {
        "^[0-9a-fA-F]{16}$": {
          "type": "object",
          "properties": {
            "device_no": {"type": "number"}
          }
        }
      }
    }
  }
}
```

## Regex Tips

### Common Patterns

```toml
# 6-digit numbers
[[patterns]]
regex = "^[0-9]{6}$"

# User IDs with prefix
[[patterns]]
regex = "^user_[0-9]+$"

# Session tokens (32 hex chars)
[[patterns]]
regex = "^session_[0-9a-f]{32}$"

# API keys (40 alphanumeric)
[[patterns]]
regex = "^api_key_[A-Za-z0-9]{40}$"

# ISO 8601 timestamps
[[patterns]]
regex = "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}Z$"

# Email addresses as keys
[[patterns]]
regex = "^[a-z0-9._%+-]+@[a-z0-9.-]+\\.[a-z]{2,}$"

# Order IDs
[[patterns]]
regex = "^ORD-[A-Z0-9]{8}$"
```

### Regex Syntax

- Use JSON Schema compatible regex (no lookahead/lookbehind)
- Escape backslashes in TOML: `\d` becomes `\\d`
- Use `^` and `$` for exact matches
- Character classes: `[0-9]`, `[a-z]`, `[A-Za-z0-9]`
- Quantifiers: `{6}` (exactly 6), `{2,10}` (2 to 10), `+` (1 or more)

## Troubleshooting

### Pattern not matching

Check that:
1. Keys have ≤ 10% density (ghost keys)
2. At least 2 keys match the pattern
3. Regex syntax is correct (test with regex tools)
4. Pattern uses `^` and `$` for exact matching

### Seeing individual keys instead of pattern

The pattern might not be matching. Verify:
```bash
# Test the regex
echo "your_key_here" | grep -E '^your_regex$'
```

### Custom pattern not used

Custom patterns are checked first, then built-in. If a built-in pattern matches, verify your custom pattern is more specific.

## Performance

Pattern matching is performed during schema generation only, not during data scanning. Custom patterns add minimal overhead (regex compilation is cached).

## See Also

- [JSON Schema patternProperties](https://json-schema.org/understanding-json-schema/reference/object#patternproperties)
- [pgdrift ignore patterns](README.md#ignore-patterns) - for excluding paths from analysis
- Example config: `.pgdrift-patterns.toml.example`
