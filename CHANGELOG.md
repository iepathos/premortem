# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2025-11-30

### Added

- **Environment Variable Validation Ergonomics** - Declarative required environment variable validation
  - `.require(var_name)` - Mark a single environment variable as required
  - `.require_all(&[...])` - Mark multiple environment variables as required at once
  - Source-level presence validation separate from value-level constraints
  - Error accumulation for ALL missing required variables (not fail-fast)
  - Clear error messages with full environment variable names including prefix
- Environment validation example (`examples/env-validation/`) demonstrating 90+ line reduction
- Performance benchmarks for environment variable validation (`benches/env_validation.rs`)
- Integration tests for required environment variables (`tests/env_required_integration.rs`)

### Changed

- Environment source now validates required variables during `load()` before deserialization
- Environment variable errors include full variable name with prefix for clarity

### Documentation

- Added "Required Environment Variables" section to README
- Added env-validation example to examples table
- Documented separation of source-level (presence) vs value-level (constraint) validation
- Updated CLAUDE.md with comprehensive environment variable validation patterns

## [0.5.0] - 2025-11-29

### Added

- **Stillwater 0.13.0 Predicate Integration** - Support for composable validation using stillwater predicates
  - `from_predicate()` - Convert stillwater predicates to premortem validators
  - `validate_with_predicate()` - Validate fields using predicates with custom error messages
  - Re-exported predicate combinators from stillwater in prelude
  - Predicate examples demonstrating validation composition

### Changed

- Updated stillwater dependency from 0.11.0 to 0.13.0

### Fixed

- Suppressed clippy `result_large_err` warning in predicates example

## [0.4.0] - 2025-11-27

### Changed

- Updated stillwater dependency from 0.8 to 0.11.0
- Updated remote sources spec for latest stillwater patterns

### Fixed

- README accuracy for remote sources documentation

## [0.3.0] - 2025-11-25

### Added

- **YAML Configuration Source** - Full YAML file support with `Yaml::file()` and `Yaml::string()` (`yaml` feature flag)
- YAML source includes line number tracking for error messages
- Support for YAML anchors and aliases
- `Yaml` type exported in prelude (matching `Json` and `Toml` pattern)
- YAML configuration example (`examples/yaml/`)
- Watch example demonstrating hot reload functionality (`examples/watch/`)
- Array path parsing for config value JSON reconstruction
- Integration tests for `build_watched` file watching

### Changed

- Refactored helper functions extracted from parse_validator for better modularity

### Fixed

- Documentation accuracy improvements across the codebase

## [0.2.0] - 2025-11-25

### Added

- Source location tracking for all error types including `MissingField`
- Source location propagation to validation errors for better debugging
- JSON line tracking for source location consistency

### Fixed

- Environment source prefix handling and type inference
- Source location lookup for nested struct validation

### Changed

- Improved demo example by removing contrived empty env var

## [0.1.1] - 2025-11-25

### Added

- README for premortem-derive crate
- MIT license file
- GitHub CI workflows
- Security, CI, and coverage badges to READMEs
- `deny.toml` for cargo-deny auditing

### Fixed

- Stillwater local references updated to use published 0.8.0
- Circular dev-dependencies removed
- Version specifiers added for all dependencies

### Changed

- Documentation clarifications for `get_path` behavior with empty paths

## [0.1.0] - 2025-11-25

### Added

- **Core Config Builder** - Fluent builder pattern for constructing configuration
- **Error Types and Source Location** - Comprehensive error types with `ConfigError`, `ConfigErrors`, and `SourceLocation` for tracking where values came from
- **Validate Trait** - `Validate` trait for custom validation with error accumulation using stillwater's `Validation` type
- **Validate Derive Macro** - `#[derive(Validate)]` macro with declarative validation attributes
- **TOML File Source** - TOML configuration file support (`toml` feature flag)
- **Environment Variable Source** - Environment variable configuration with prefix support
- **Defaults Source** - Programmatic default values source
- **Pretty Error Printing** - Human-readable error formatting with source locations
- **Value Tracing and Origin Tracking** - Track where each configuration value originated
- **Hot Reload and File Watching** - `watch` feature for live configuration updates
- **Prelude Module** - Convenient re-exports of commonly used types
- **JSON Configuration Source** - JSON file support (`json` feature flag)
- **ConfigEnv Trait** - Testable I/O abstraction with `MockEnv` for unit testing
- **Property-Based Testing** - Comprehensive property tests for error handling
- **Integration Examples** - Full working examples demonstrating library usage

### Dependencies

- `stillwater` 0.8 - Functional patterns (Validation, NonEmptyVec, Semigroup)
- `serde` 1.0 - Serialization/deserialization
- `serde_json` 1.0 - JSON support

[Unreleased]: https://github.com/iepathos/premortem/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/iepathos/premortem/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/iepathos/premortem/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/iepathos/premortem/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/iepathos/premortem/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/iepathos/premortem/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/iepathos/premortem/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/iepathos/premortem/releases/tag/v0.1.0
