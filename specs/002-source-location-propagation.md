---
number: 2
title: Source Location Propagation to Validation Errors
category: foundation
priority: critical
status: draft
dependencies: []
created: 2025-11-25
---

# Specification 002: Source Location Propagation to Validation Errors

**Category**: foundation
**Priority**: critical
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

However, the current implementation has two critical gaps:

### Gap 1: TOML Parser Doesn't Capture Line Numbers

In `src/sources/toml_source.rs`, the parser creates `ConfigValue` without line numbers:

```rust
// Current (broken):
ConfigValue::new(toml_to_value(value), SourceLocation::new(source_name));
//                                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//                                      Only stores "config.toml", no line number!
```

### Gap 2: Validation Loses Source Locations Entirely

The data flow in `src/config.rs` loses source locations during deserialization:

```
ConfigValues (has source locations per path)
    ↓ to_json()
JSON (source locations completely lost!)
    ↓ serde_json::from_value()
T struct (no source info available)
    ↓ config.validate()
ValidationError { source_location: None }  // Cannot know where value came from!
```

This means even if we fix Gap 1, validation errors would still lack source locations because the derive macro's validation runs on the deserialized struct with no access to where values originated.

### Stillwater Philosophy Alignment

This fix must follow stillwater's core principles:

1. **Pure Core, Imperative Shell**: Parsing with line tracking is pure transformation
2. **Errors Should Tell Stories**: `[config.toml:8]` tells exactly where the problem is
3. **Composition Over Complexity**: Small functions that compose cleanly
4. **Pragmatism Over Purity**: Use practical solutions (thread-local or context) over theoretical purity

## Objective

Ensure all configuration errors include accurate source locations (file:line) by:
1. Capturing line numbers during TOML parsing
2. Propagating source locations through to validation errors
3. Producing error output that matches the documented README format

## Requirements

### Functional Requirements

1. **TOML Line Number Tracking**
   - Use `toml_edit` crate to parse TOML with span information
   - Extract line numbers for each key-value pair during parsing
   - Store line numbers in `SourceLocation` via `with_line()`

2. **Source Location Registry**
   - Create a `SourceLocationMap` type: `HashMap<String, SourceLocation>`
   - Populate during config loading from `ConfigValues`
   - Make available to validation system

3. **Validation Context**
   - Create `ValidationContext` struct holding source location lookup
   - Pass context through validation without breaking existing API
   - Use thread-local storage for pragmatic access in derive macro

4. **Derive Macro Updates**
   - Update generated validation code to lookup source locations
   - Attach source locations to `ValidationError` variants
   - Fall back to `None` when no location available

5. **Error Output Format**
   - Ensure `Display` for errors shows `[source:line]` prefix
   - Match README example format exactly

### Non-Functional Requirements

1. **Performance**: Line tracking should add minimal overhead (< 5% parsing time)
2. **Memory**: Source location map size proportional to config key count (acceptable)
3. **Backwards Compatibility**: Existing code without context should still work
4. **Testability**: All new code should be pure functions where possible

## Acceptance Criteria

- [ ] `toml_edit` added as dependency for TOML parsing
- [ ] TOML parser captures line numbers for each value
- [ ] `SourceLocation` stored with line numbers in `ConfigValues`
- [ ] `ValidationContext` created with source location lookup
- [ ] Thread-local or context mechanism for derive macro access
- [ ] Derive macro generates code that attaches source locations
- [ ] `cargo run --example error-demo` shows `[config.toml:X]` prefixes
- [ ] Error output matches README format exactly
- [ ] All existing tests pass
- [ ] New tests cover source location propagation
- [ ] No hardcoded line numbers in examples

## Technical Details

### Implementation Approach

#### Phase 1: Add toml_edit and Capture Line Numbers

1. Add `toml_edit` to Cargo.toml:
```toml
[dependencies]
toml_edit = "0.22"
```

2. Update `parse_toml()` in `src/sources/toml_source.rs`:
```rust
use toml_edit::{DocumentMut, Item};

fn parse_toml(content: &str, source_name: &str) -> Result<ConfigValues, ConfigErrors> {
    let doc: DocumentMut = content.parse().map_err(|e| /* ... */)?;

    let mut values = ConfigValues::empty();
    flatten_document(&doc, "", source_name, content, &mut values);
    Ok(values)
}

fn flatten_document(
    item: &Item,
    prefix: &str,
    source_name: &str,
    content: &str,  // Original content for line calculation
    values: &mut ConfigValues,
) {
    match item {
        Item::Value(v) => {
            let span = v.span();
            let line = span.map(|s| line_from_offset(content, s.start));
            let mut loc = SourceLocation::new(source_name);
            if let Some(l) = line {
                loc = loc.with_line(l);
            }
            values.insert(prefix.to_string(), ConfigValue::new(value_to_value(v), loc));
        }
        Item::Table(t) => {
            for (key, val) in t.iter() {
                let path = if prefix.is_empty() { key.to_string() } else { format!("{}.{}", prefix, key) };
                flatten_document(val, &path, source_name, content, values);
            }
        }
        // ... arrays, etc.
    }
}

fn line_from_offset(content: &str, offset: usize) -> u32 {
    content[..offset.min(content.len())].lines().count() as u32
}
```

#### Phase 2: Create ValidationContext

1. Add to `src/validate.rs`:
```rust
use std::cell::RefCell;
use std::collections::HashMap;
use crate::error::SourceLocation;

/// Map from config path to source location
pub type SourceLocationMap = HashMap<String, SourceLocation>;

/// Context for validation with source location lookup
#[derive(Debug, Default)]
pub struct ValidationContext {
    locations: SourceLocationMap,
}

impl ValidationContext {
    pub fn new(locations: SourceLocationMap) -> Self {
        Self { locations }
    }

    pub fn location_for(&self, path: &str) -> Option<&SourceLocation> {
        self.locations.get(path)
    }
}

// Thread-local for derive macro access
thread_local! {
    static VALIDATION_CONTEXT: RefCell<Option<ValidationContext>> = RefCell::new(None);
}

pub fn with_validation_context<F, R>(ctx: ValidationContext, f: F) -> R
where
    F: FnOnce() -> R,
{
    VALIDATION_CONTEXT.with(|cell| {
        *cell.borrow_mut() = Some(ctx);
    });
    let result = f();
    VALIDATION_CONTEXT.with(|cell| {
        *cell.borrow_mut() = None;
    });
    result
}

pub fn current_source_location(path: &str) -> Option<SourceLocation> {
    VALIDATION_CONTEXT.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|ctx| ctx.location_for(path).cloned())
    })
}
```

#### Phase 3: Update ConfigBuilder

1. In `src/config.rs`, update `build_with_env()`:
```rust
pub fn build_with_env(self, env: &dyn ConfigEnv) -> Result<Config<T>, ConfigErrors>
where
    T: DeserializeOwned + Validate,
{
    // ... existing loading code ...

    let merged = merge_config_values(all_values);

    // Build source location map from merged values
    let locations: SourceLocationMap = merged
        .iter()
        .map(|(path, cv)| (path.clone(), cv.source.clone()))
        .collect();

    let config = deserialize_config::<T>(&merged, &source_names)?;

    // Run validation with context
    let ctx = ValidationContext::new(locations);
    let validation_result = with_validation_context(ctx, || config.validate());

    match validation_result {
        Validation::Success(()) => Ok(Config::new(config)),
        Validation::Failure(errors) => Err(errors),
    }
}
```

#### Phase 4: Update Derive Macro

1. In `premortem-derive/src/lib.rs`, update generated validation code:
```rust
// Generated code should use:
let source_location = premortem::validate::current_source_location(#path);

ConfigError::ValidationError {
    path: #path.to_string(),
    source_location,  // Now populated!
    value: Some(value.to_string()),
    message: #message.to_string(),
}
```

### Architecture Changes

```
                    BEFORE                              AFTER
                    ======                              =====

TOML File ──→ toml::Value ──→ ConfigValues    TOML File ──→ toml_edit::Document ──→ ConfigValues
              (no spans)      (no lines)                    (has spans!)            (has lines!)
                   ↓                                              ↓
              to_json()                                      to_json()
                   ↓                                              ↓
              JSON Value                                    JSON Value
                   ↓                                              ↓
              deserialize                      SourceLocationMap stored separately
                   ↓                                              ↓
              T struct                                       T struct
                   ↓                                              ↓
              validate()                       validate() with thread-local context
                   ↓                                              ↓
              ValidationError                            ValidationError
              { source_location: None }        { source_location: Some([file:line]) }
```

### Data Structures

**New types in `src/validate.rs`:**
```rust
pub type SourceLocationMap = HashMap<String, SourceLocation>;

pub struct ValidationContext {
    locations: SourceLocationMap,
}
```

**Modified in `src/error.rs`:**
No changes needed - `SourceLocation` already has `line` field.

### APIs and Interfaces

**New public API:**
```rust
// In premortem::validate
pub fn current_source_location(path: &str) -> Option<SourceLocation>;
pub fn with_validation_context<F, R>(ctx: ValidationContext, f: F) -> R;
```

**Internal API (not public):**
```rust
fn line_from_offset(content: &str, offset: usize) -> u32;
```

## Dependencies

- **Prerequisites**: None (Spec 001 added source_location to MissingField, but this is independent)
- **Affected Components**:
  - `Cargo.toml` - add `toml_edit` dependency
  - `src/sources/toml_source.rs` - line number extraction
  - `src/validate.rs` - ValidationContext and thread-local
  - `src/config.rs` - context setup during build
  - `premortem-derive/src/lib.rs` - use context in generated code
  - `examples/error-demo/` - remove hardcoded errors, use real validation
- **External Dependencies**:
  - `toml_edit = "0.22"` (or latest compatible)

## Testing Strategy

- **Unit Tests**:
  - `line_from_offset()` correctly calculates line numbers
  - `flatten_document()` preserves spans from toml_edit
  - `ValidationContext` lookup works correctly
  - Thread-local context set/get/clear works

- **Integration Tests**:
  - TOML source produces `ConfigValues` with line numbers
  - Validation errors include source locations from parsed config
  - Multi-source config preserves correct source per value
  - Error Display shows `[file:line]` prefix

- **End-to-End Tests**:
  - `cargo run --example error-demo` produces expected output
  - Output matches README format exactly
  - No hardcoded line numbers in examples

## Documentation Requirements

- **Code Documentation**: Rustdoc for new public types and functions
- **User Documentation**: Update README if output format changes
- **Architecture Updates**: None needed (internal implementation change)

## Implementation Notes

### Why Thread-Local Instead of Trait Change?

The `Validate` trait is public API:
```rust
pub trait Validate {
    fn validate(&self) -> ConfigValidation<()>;
}
```

Changing to `fn validate(&self, ctx: &ValidationContext)` would be a breaking change for all users implementing custom validation.

Thread-local is pragmatic (stillwater principle #6):
- No API breakage
- Derive macro can access context invisibly
- Manual implementations still work (just get `None` for source_location)
- Future: could add `validate_with_context()` method with default impl

### Performance Considerations

- `toml_edit` is slightly slower than `toml` for parsing
- But config parsing is typically done once at startup
- The overhead is negligible for typical config file sizes
- Memory: one `SourceLocation` per config path (small)

### Thread Safety

Thread-local storage is safe:
- Each thread has its own context
- Context is set, validation runs, context is cleared
- No cross-thread access issues
- Async-safe when validation is not moved between threads (typical)

## Migration and Compatibility

**Breaking Changes**: None

**Behavioral Changes**:
- Validation errors will now include `source_location: Some(...)` where previously they had `None`
- Error Display output will include `[file:line]` prefixes
- This is the documented/expected behavior, so it's a bug fix not a breaking change

**Migration Path**: None needed - existing code will automatically benefit
