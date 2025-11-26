# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2025-11-25

### Added

- YAML configuration source support (`yaml` feature flag)
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

[Unreleased]: https://github.com/iepathos/premortem/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/iepathos/premortem/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/iepathos/premortem/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/iepathos/premortem/releases/tag/v0.1.0
