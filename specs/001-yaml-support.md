---
number: 1
title: YAML Configuration Source Support
category: compatibility
priority: medium
status: draft
dependencies: []
created: 2025-11-25
---

# Specification 001: YAML Configuration Source Support

**Category**: compatibility
**Priority**: medium
**Status**: draft
**Dependencies**: None

## Context

Premortem currently supports TOML and JSON configuration sources, but YAML is a widely-used configuration format, especially in DevOps, Kubernetes, and cloud-native ecosystems. Adding YAML support would make premortem more accessible to users who prefer or are required to use YAML configuration files.

The `yaml` feature flag already exists in `Cargo.toml` but is not yet implemented. This specification defines the implementation of a `Yaml` source that follows the same patterns established by `Toml` and `Json` sources.

## Objective

Implement a `Yaml` configuration source that:
1. Loads configuration from YAML files or strings
2. Supports required/optional file modes
3. Provides rich error reporting with source location (line numbers)
4. Follows the existing `Source` trait implementation patterns
5. Uses dependency injection via `ConfigEnv` for testability
6. Handles all YAML data types including anchors, aliases, and multi-document files

## Requirements

### Functional Requirements

1. **File Loading**: Load YAML configuration from file paths via `ConfigEnv::read_file`
2. **String Loading**: Load YAML configuration from string content
3. **Required/Optional Modes**: Support both required (error on missing) and optional (empty values on missing) file modes
4. **Custom Naming**: Allow custom source names for error messages via `.named()`
5. **Value Type Conversion**: Convert all YAML types to premortem's `Value` enum:
   - Strings -> `Value::String`
   - Integers -> `Value::Integer`
   - Floats -> `Value::Float`
   - Booleans -> `Value::Bool`
   - Null -> `Value::Null`
   - Sequences -> `Value::Array` with `[n]` indexed paths
   - Mappings -> Nested dot-notation paths
6. **Array Handling**: Store array length metadata as `{path}.__len`
7. **Nested Structures**: Flatten nested mappings using dot notation (e.g., `database.pool.max_size`)
8. **Source Location Tracking**: Track line numbers for each value to enable precise error messages

### Non-Functional Requirements

1. **Pure Core Pattern**: Separate I/O (file reading) from pure parsing logic
2. **Error Accumulation**: Return `ConfigErrors` for all error conditions
3. **Feature Flag**: Only compile when `yaml` feature is enabled
4. **Consistent API**: Match the API patterns of `Toml` and `Json` sources
5. **Watch Support**: Implement `watch_path` and `clone_box` for the watch feature

## Acceptance Criteria

- [ ] `Yaml::file("config.yaml")` loads YAML from a file path
- [ ] `Yaml::string("...")` loads YAML from a string
- [ ] `.optional()` makes missing files return empty `ConfigValues` instead of error
- [ ] `.required()` makes missing files return `SourceError::NotFound`
- [ ] `.named("...")` sets custom source name for error messages
- [ ] Nested mappings are flattened to dot notation (e.g., `database.host`)
- [ ] Sequences are indexed with bracket notation (e.g., `hosts[0]`)
- [ ] Sequence length is stored as `{path}.__len`
- [ ] All scalar types are correctly converted to `Value` variants
- [ ] Parse errors include line and column information
- [ ] I/O errors (permission denied, etc.) are properly reported as `SourceError::IoError`
- [ ] Line numbers are tracked for each value in `SourceLocation`
- [ ] Feature flag `yaml` controls compilation
- [ ] `Yaml` is re-exported from `premortem::sources` and `premortem` root
- [ ] All existing test patterns from TOML/JSON sources are replicated for YAML
- [ ] Documentation examples use `ignore` attribute for doctests

## Technical Details

### Implementation Approach

1. **Add serde_yaml Dependency**
   ```toml
   # Cargo.toml
   serde_yaml = { version = "0.9", optional = true }

   [features]
   yaml = ["dep:serde_yaml"]
   ```

2. **Create yaml_source.rs Module**
   - Follow the exact structure of `json_source.rs` as a template
   - Use `serde_yaml::from_str` for parsing
   - Use `serde_yaml::Value` for intermediate representation

3. **Source Structure**
   ```rust
   enum YamlSource {
       File(PathBuf),
       String { content: String, name: String },
   }

   pub struct Yaml {
       source: YamlSource,
       required: bool,
       name: Option<String>,
   }
   ```

4. **Line Number Tracking**
   - `serde_yaml` doesn't directly expose line numbers like `toml_edit`
   - Use the same `find_key_line` approach as `json_source.rs`
   - Search for key patterns in content to determine approximate line numbers

5. **Value Conversion**
   ```rust
   fn yaml_to_value(yaml: &serde_yaml::Value) -> Value {
       match yaml {
           serde_yaml::Value::Null => Value::Null,
           serde_yaml::Value::Bool(b) => Value::Bool(*b),
           serde_yaml::Value::Number(n) => {
               if let Some(i) = n.as_i64() {
                   Value::Integer(i)
               } else if let Some(f) = n.as_f64() {
                   Value::Float(f)
               } else {
                   Value::String(n.to_string())
               }
           }
           serde_yaml::Value::String(s) => Value::String(s.clone()),
           serde_yaml::Value::Sequence(arr) => Value::Array(...),
           serde_yaml::Value::Mapping(map) => Value::Table(...),
           serde_yaml::Value::Tagged(tagged) => yaml_to_value(&tagged.value),
       }
   }
   ```

### Architecture Changes

1. **New File**: `src/sources/yaml_source.rs`
2. **Module Registration**: Add to `src/sources/mod.rs`:
   ```rust
   #[cfg(feature = "yaml")]
   mod yaml_source;

   #[cfg(feature = "yaml")]
   pub use yaml_source::Yaml;
   ```
3. **Root Re-export**: Add to `src/lib.rs`:
   ```rust
   #[cfg(feature = "yaml")]
   pub use sources::Yaml;
   ```

### Data Structures

Uses existing structures:
- `ConfigValues` - container for flattened key-value pairs
- `ConfigValue` - value with source location metadata
- `SourceLocation` - tracks source file and line number
- `Value` - intermediate representation enum

### APIs and Interfaces

Public API following existing patterns:

```rust
// Builder methods
impl Yaml {
    pub fn file(path: impl Into<PathBuf>) -> Self;
    pub fn string(content: impl Into<String>) -> Self;
    pub fn optional(self) -> Self;
    pub fn required(self) -> Self;
    pub fn named(self, name: impl Into<String>) -> Self;
}

// Source trait implementation
impl Source for Yaml {
    fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors>;
    fn name(&self) -> &str;

    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf>;

    #[cfg(feature = "watch")]
    fn clone_box(&self) -> Box<dyn Source>;
}
```

## Dependencies

- **Prerequisites**: None (TOML and JSON patterns already established)
- **Affected Components**:
  - `Cargo.toml` - new dependency
  - `src/sources/mod.rs` - module registration
  - `src/lib.rs` - re-export
- **External Dependencies**: `serde_yaml` crate

## Testing Strategy

### Unit Tests

All tests should follow the established patterns from `toml_source.rs` and `json_source.rs`:

1. **Basic Loading**
   - `test_yaml_file_load` - Load from file via MockEnv
   - `test_yaml_string_load` - Load from string content

2. **Error Handling**
   - `test_yaml_file_missing_required` - Missing required file returns NotFound
   - `test_yaml_file_missing_optional` - Missing optional file returns empty values
   - `test_yaml_file_permission_denied` - Unreadable file returns IoError
   - `test_yaml_parse_error_with_location` - Invalid YAML reports line/column

3. **Data Types**
   - `test_yaml_all_value_types` - String, int, float, bool, null
   - `test_yaml_nested_objects` - Nested mappings with dot notation
   - `test_yaml_arrays` - Sequences with bracket notation
   - `test_yaml_array_of_objects` - Complex nested structures

4. **Source Tracking**
   - `test_yaml_source_location_tracking` - Source name in metadata
   - `test_yaml_line_number_tracking` - Line numbers for values
   - `test_yaml_nested_object_line_tracking` - Lines in nested structures
   - `test_yaml_custom_name` - Custom source naming

5. **API Methods**
   - `test_yaml_required_method` - Toggle required state
   - `test_yaml_optional_method` - Toggle optional state

### Integration Tests

- Test YAML source in combination with other sources (layering)
- Test deserialization into typed structs via ConfigBuilder

### Performance Tests

Not required for initial implementation.

### User Acceptance

- YAML files commonly used in Kubernetes and cloud configurations should parse correctly
- Error messages should clearly identify the source file and line number

## Documentation Requirements

### Code Documentation

- Module-level documentation with examples (using `ignore` attribute)
- All public methods documented with examples
- Internal functions documented for maintainability

### User Documentation

- Add YAML examples to CLAUDE.md usage section
- Include YAML in feature flag documentation

### Architecture Updates

None required - follows existing source pattern.

## Implementation Notes

### YAML-Specific Considerations

1. **Anchors and Aliases**: `serde_yaml` automatically resolves anchors (`&`) and aliases (`*`) during parsing, so no special handling is needed.

2. **Multi-Document Files**: Standard `serde_yaml::from_str` only parses the first document. If multi-document support is needed later, use `serde_yaml::Deserializer::from_str` with iteration.

3. **Tagged Values**: YAML supports custom tags (e.g., `!include`). For initial implementation, extract the inner value from `serde_yaml::Value::Tagged` and convert normally.

4. **Duplicate Keys**: YAML allows duplicate keys (last wins). This is handled automatically by `serde_yaml`.

5. **Binary Data**: YAML supports binary data via `!!binary` tag. For initial implementation, convert to base64 string representation.

### Error Message Format

Follow established patterns:
```
[config.yaml:10] 'database.port': expected integer, got string
```

### Feature Flag Testing

Run tests with:
```bash
cargo test --features yaml
cargo test --all-features
```

## Migration and Compatibility

### Breaking Changes

None - this is a new feature addition.

### Migration Requirements

None - users opt-in by enabling the `yaml` feature flag.

### Compatibility Considerations

- The `yaml` feature should work independently and in combination with `toml` and `json` features
- All existing tests must continue to pass
- The `full` feature already includes `yaml` in its list
