---
number: 1
title: Source Location Tracking for All Error Types
category: foundation
priority: high
status: draft
dependencies: []
created: 2025-11-25
---

# Specification 001: Source Location Tracking for All Error Types

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: none

## Context

The premortem README promises detailed error output with source location tracking:

```
$ ./myapp
Configuration errors (3):
  [config.toml:8] missing required field 'database.host'
  [env:APP_PORT] value "abc" is not a valid integer
  [config.toml:10] 'pool_size' value -5 must be >= 1
```

However, the current implementation has gaps:

1. **`MissingField` error** lacks a `source_location` field - it cannot show `[config.toml:8]`
2. **Display implementations** produce inconsistent formats across error types
3. **Error messages** don't match the README's promised format

Users need source location information for effective debugging and traceback. When configuration fails, knowing exactly which file and line caused the problem is essential for quick resolution.

## Objective

Ensure all error types that can be traced to a source location include that information, and provide consistent Display formatting that matches the README's documented output format.

## Requirements

### Functional Requirements

1. **Add source_location to MissingField**
   - Add `source_location: Option<SourceLocation>` field to `MissingField` variant
   - When a field is required but missing, track where the parser expected to find it
   - For files: include filename and line number where the parent object was defined
   - For environment variables: include the expected variable name

2. **Consistent Display Format**
   - All errors with source location should display as: `[source:line] message`
   - Errors without source location should omit the prefix
   - Format must match README examples exactly

3. **Source Location Propagation**
   - TOML parser should capture line numbers during parsing
   - Environment source should create `SourceLocation::env(var_name)` for each variable
   - Missing fields should inherit location from their parent container

### Non-Functional Requirements

1. **Performance**: Source location tracking should have negligible overhead
2. **Memory**: SourceLocation is small (String + 2 Option<u32>) - acceptable overhead
3. **Backwards Compatibility**: Existing code using `MissingField` without source_location should still compile (Option makes it optional)

## Acceptance Criteria

- [ ] `MissingField` variant includes `source_location: Option<SourceLocation>` field
- [ ] `Display` impl for `MissingField` shows `[source:line]` prefix when location is present
- [ ] `Display` impl for `ParseError` matches format: `[source] value "x" is not a valid type`
- [ ] `Display` impl for `ValidationError` matches format: `[source:line] 'path' value X message`
- [ ] All error Display formats are consistent (location prefix, then message)
- [ ] TOML source captures line numbers for missing required fields
- [ ] Environment source creates proper SourceLocation for each variable
- [ ] Example exists demonstrating the documented error output format
- [ ] All existing tests pass after changes
- [ ] New tests cover source location in MissingField errors

## Technical Details

### Implementation Approach

#### Phase 1: Update Error Types

Update `MissingField` variant in `src/error.rs`:

```rust
/// A required field is missing
MissingField {
    path: String,
    source_location: Option<SourceLocation>,  // NEW
    searched_sources: Vec<String>,
},
```

Update `ConfigError::source_location()` method to return the new field:

```rust
pub fn source_location(&self) -> Option<&SourceLocation> {
    match self {
        ConfigError::MissingField { source_location, .. } => source_location.as_ref(),
        // ... existing matches
    }
}
```

#### Phase 2: Update Display Implementations

Standardize Display format for all error types:

```rust
// MissingField - NEW format
ConfigError::MissingField { path, source_location, .. } => {
    match source_location {
        Some(loc) => write!(f, "[{}] missing required field '{}'", loc, path),
        None => write!(f, "missing required field '{}'", path),
    }
}

// ParseError - update format to match README
ConfigError::ParseError { path, source_location, actual_value, message, .. } => {
    write!(f, "[{}] value \"{}\" {}", source_location, actual_value, message)
}

// ValidationError - update format to match README
ConfigError::ValidationError { path, source_location, value, message, .. } => {
    match (source_location, value) {
        (Some(loc), Some(val)) => write!(f, "[{}] '{}' value {} {}", loc, path, val, message),
        (Some(loc), None) => write!(f, "[{}] '{}' {}", loc, path, message),
        (None, Some(val)) => write!(f, "'{}' value {} {}", path, val, message),
        (None, None) => write!(f, "'{}' {}", path, message),
    }
}
```

#### Phase 3: Update Source Implementations

**TOML Source** (`src/sources/toml.rs`):
- Track line numbers during TOML parsing using `toml::Spanned` or similar
- When creating MissingField errors, include the line where parent table was defined

**Environment Source** (`src/sources/env.rs`):
- Already creates `SourceLocation::env(var_name)` for parse errors
- Extend to missing field errors when a required env var is not set

#### Phase 4: Create Example

Create `examples/error-output/` demonstrating:
1. A config.toml with intentional problems (missing field, invalid pool_size)
2. An environment setup with invalid APP_PORT
3. A main.rs that loads config and shows error output matching README

### Architecture Changes

No architectural changes required. This is an enhancement to existing error types.

### Data Structures

Modify `ConfigError::MissingField`:

```rust
// Before
MissingField {
    path: String,
    searched_sources: Vec<String>,
}

// After
MissingField {
    path: String,
    source_location: Option<SourceLocation>,
    searched_sources: Vec<String>,
}
```

### APIs and Interfaces

Public API changes:
- `ConfigError::MissingField` struct variant gains a field (source-breaking if destructured)
- Consider using `#[non_exhaustive]` on ConfigError if not already present

## Dependencies

- **Prerequisites**: None
- **Affected Components**:
  - `src/error.rs` - ConfigError enum and Display impl
  - `src/sources/toml.rs` - TOML parsing with line tracking
  - `src/sources/env.rs` - Environment variable source
  - `src/pretty.rs` - Pretty printing (may need format updates)
- **External Dependencies**: None new

## Testing Strategy

- **Unit Tests**:
  - Test Display output for each error variant with/without source_location
  - Test SourceLocation formatting (file only, file:line, file:line:col)
  - Test MissingField with source_location propagation

- **Integration Tests**:
  - Test TOML source creates MissingField with line numbers
  - Test Env source creates MissingField with env var names
  - Test error accumulation preserves all source locations

- **User Acceptance**:
  - Example produces output matching README exactly
  - Error messages are clear and actionable

## Documentation Requirements

- **Code Documentation**: Update rustdoc for modified error types
- **User Documentation**: README already documents the format - ensure implementation matches
- **Architecture Updates**: None needed

## Implementation Notes

### Line Number Tracking in TOML

The `toml` crate provides `toml::Spanned<T>` for tracking source positions. However, this requires parsing into Spanned types first, then converting. Alternative approach:

1. Parse TOML to `toml::Value` first
2. Track table positions during traversal
3. When deserializing fails for a missing field, use the parent table's position

### Backwards Compatibility

Adding a field to an enum variant is a breaking change for code that destructures it. Options:
1. Accept the breaking change (minor version bump)
2. Add `#[non_exhaustive]` to prevent future breaks
3. Use a builder pattern for error construction (overkill)

Recommendation: Accept breaking change with minor version bump. The new field is `Option<_>` so existing logic can use `None`.

### Pretty Printer Updates

The `src/pretty.rs` module has its own formatting logic. Ensure it's updated to match the new Display formats for consistency between `Display` trait and pretty printing.

## Migration and Compatibility

**Breaking Changes**:
- `ConfigError::MissingField` variant gains a field
- Code that pattern-matches on MissingField must be updated

**Migration Path**:
1. Update pattern matches to include `source_location` (or use `..` to ignore)
2. When constructing MissingField, provide `source_location: None` for backwards-compatible behavior
3. Optionally update to provide actual source locations

**Version Impact**: Minor version bump (0.1.x -> 0.2.0) due to enum variant change
