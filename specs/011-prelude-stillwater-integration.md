---
number: 11
title: Prelude Module and Stillwater Integration
category: foundation
priority: high
status: draft
dependencies: [1, 2, 3]
created: 2025-11-25
---

# Specification 011: Prelude Module and Stillwater Integration

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types, 003 - Validate Trait]

## Context

Premortem builds on stillwater's functional programming primitives (`Validation`, `Effect`, `Semigroup`, `NonEmptyVec`). Users shouldn't need to know about stillwater to use premortem effectively—the prelude module re-exports everything needed for common usage.

This spec also ensures consistent use of stillwater patterns across the entire codebase.

### Design Philosophy

Following stillwater's pragmatism principle:
- **Hide complexity** - Users import one module, get everything
- **Expose power** - Advanced users can access stillwater directly
- **Consistent types** - Use `ConfigErrors` and `ConfigValidation<T>` everywhere

## Objective

Create a prelude module that provides ergonomic access to premortem's API and stillwater's types, ensuring users can write configuration code with a single import.

## Requirements

### Functional Requirements

1. **Prelude Module**: Single import for common usage
2. **Stillwater Re-exports**: Key types available without direct stillwater dependency
3. **Type Consistency**: All public APIs use `ConfigErrors` and `ConfigValidation<T>`
4. **Layered Access**: Basic, intermediate, and advanced import patterns

### Non-Functional Requirements

- Zero additional runtime cost (just re-exports)
- Clear documentation showing import patterns
- Backward compatible if stillwater types change (wrapper if needed)

## Acceptance Criteria

- [ ] `use premortem::prelude::*` provides all common types
- [ ] `Validation`, `Effect`, `Semigroup`, `NonEmptyVec` available from prelude
- [ ] `Validation::all()` and `Validation::traverse()` accessible
- [ ] All source `load()` methods return consistent types
- [ ] All error handling uses `ConfigErrors` not `Vec<ConfigError>`
- [ ] Documentation shows recommended import patterns
- [ ] Unit tests verify re-exports work correctly

## Technical Details

### Prelude Module Structure

```rust
// src/prelude.rs

//! Convenient re-exports for common premortem usage.
//!
//! # Example
//!
//! ```rust
//! use premortem::prelude::*;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize, Validate)]
//! struct AppConfig {
//!     #[validate(non_empty)]
//!     name: String,
//!     #[validate(range(1..=65535))]
//!     port: u16,
//! }
//!
//! fn main() {
//!     let config = Config::<AppConfig>::builder()
//!         .source(Toml::file("config.toml"))
//!         .source(Env::prefix("APP_"))
//!         .build()
//!         .unwrap_or_exit();
//!
//!     println!("Running on port {}", config.get().port);
//! }
//! ```

// ============================================================================
// Stillwater re-exports (core functional programming types)
// ============================================================================

/// Result type with error accumulation. Use `Validation::all()` to combine
/// multiple validations and collect ALL errors.
pub use stillwater::Validation;

/// Deferred computation with environment. Used by Sources for I/O operations.
pub use stillwater::Effect;

/// Trait for combining values. `ConfigErrors` implements this for error accumulation.
pub use stillwater::Semigroup;

/// Guaranteed non-empty collection. Underlying type for `ConfigErrors`.
pub use stillwater::NonEmptyVec;

// ============================================================================
// Error types
// ============================================================================

pub use crate::error::{
    /// Individual configuration error with source location.
    ConfigError,
    /// Non-empty collection of errors. Implements `Semigroup` for accumulation.
    ConfigErrors,
    /// Type alias: `Validation<T, ConfigErrors>`. The standard result type.
    ConfigValidation,
    /// Location where a configuration value originated.
    SourceLocation,
    /// Kinds of source loading errors.
    SourceErrorKind,
};

// ============================================================================
// Core config types
// ============================================================================

pub use crate::config::{
    /// The main configuration container wrapping validated config.
    Config,
    /// Builder for constructing configuration from multiple sources.
    ConfigBuilder,
};

// ============================================================================
// Sources
// ============================================================================

pub use crate::source::{
    /// Trait for configuration sources. Implement for custom sources.
    Source,
    /// Intermediate representation of configuration values.
    ConfigValues,
    /// TOML file configuration source.
    Toml,
    /// Environment variable configuration source.
    Env,
    /// Default values configuration source.
    Defaults,
    /// Partial defaults builder for specific paths.
    PartialDefaults,
};

// ============================================================================
// Validation
// ============================================================================

pub use crate::validate::{
    /// Trait for types that can be validated. Usually derived.
    Validate,
    /// Trait for individual validators.
    Validator,
};

/// Derive macro for `Validate` trait.
pub use premortem_derive::Validate;

// ============================================================================
// Common validators (for programmatic use)
// ============================================================================

pub use crate::validators::{
    NonEmpty,
    MinLength,
    MaxLength,
    Length,
    Range,
    Positive,
    Negative,
    NonZero,
    Pattern,
    Email,
    Url,
    Each,
};

// ============================================================================
// Convenience extensions
// ============================================================================

pub use crate::ext::ValidationExt;

// ============================================================================
// Optional features
// ============================================================================

#[cfg(feature = "watch")]
pub use crate::watch::{
    WatchedConfig,
    ConfigWatcher,
    ConfigEvent,
};

#[cfg(feature = "trace")]
pub use crate::trace::{
    TracedConfig,
    ValueTrace,
    TracedValue,
};
```

### Import Patterns Documentation

```rust
// src/lib.rs

//! # Import Patterns
//!
//! ## Quick Start (Recommended)
//!
//! For most users, import the prelude:
//!
//! ```rust
//! use premortem::prelude::*;
//! ```
//!
//! ## Selective Imports
//!
//! Import only what you need:
//!
//! ```rust
//! use premortem::{Config, Toml, Env, Validate};
//! use premortem::error::ConfigErrors;
//! ```
//!
//! ## Advanced: Direct Stillwater Access
//!
//! For custom sources or advanced patterns:
//!
//! ```rust
//! use premortem::prelude::*;
//! use stillwater::{Effect, IO};  // Direct stillwater access
//! ```
```

### Type Consistency Requirements

All public APIs must use these types:

| Type | Usage |
|------|-------|
| `ConfigError` | Individual error |
| `ConfigErrors` | Collection of errors (never `Vec<ConfigError>`) |
| `ConfigValidation<T>` | Result type (alias for `Validation<T, ConfigErrors>`) |
| `Effect<T, ConfigErrors, E>` | I/O operations in sources |

### Stillwater Pattern Re-exports

Ensure these patterns are accessible:

```rust
// Validation::all() - combine validations with error accumulation
let result = Validation::all((
    validate_host(&config.host),
    validate_port(config.port),
    validate_pool_size(config.pool_size),
));

// Validation::traverse() - validate collections
let validated: ConfigValidation<Vec<Server>> = Validation::traverse(
    servers.iter(),
    |server| server.validate()
);

// Effect combinators
let effect = source.load()
    .context("Loading configuration")
    .map(|values| transform(values));
```

### Module Structure

```
src/
├── lib.rs              # Crate root, re-exports prelude
├── prelude.rs          # All common re-exports
├── config.rs           # Config, ConfigBuilder
├── error.rs            # ConfigError, ConfigErrors, ConfigValidation
├── source/
│   ├── mod.rs          # Source trait, ConfigValues
│   ├── toml.rs         # Toml source
│   ├── env.rs          # Env source
│   └── defaults.rs     # Defaults source
├── validate/
│   ├── mod.rs          # Validate trait
│   └── validators.rs   # Built-in validators
├── ext.rs              # Extension traits (ValidationExt)
├── watch.rs            # Hot reload (feature-gated)
└── trace.rs            # Value tracing (feature-gated)
```

### Library Root

```rust
// src/lib.rs

//! # premortem
//!
//! Configuration loading with comprehensive validation and error accumulation.
//!
//! Built on [stillwater](https://crates.io/crates/stillwater) for functional
//! programming patterns.
//!
//! ## Quick Start
//!
//! ```rust
//! use premortem::prelude::*;
//! use serde::Deserialize;
//!
//! #[derive(Debug, Deserialize, Validate)]
//! struct Config {
//!     #[validate(non_empty)]
//!     database_url: String,
//! }
//!
//! let config = Config::<Config>::builder()
//!     .source(Toml::file("config.toml"))
//!     .build()
//!     .unwrap_or_exit();
//! ```

pub mod prelude;
pub mod config;
pub mod error;
pub mod source;
pub mod validate;
pub mod validators;
mod ext;

#[cfg(feature = "watch")]
pub mod watch;

#[cfg(feature = "trace")]
pub mod trace;

// Also export at crate root for convenience
pub use config::{Config, ConfigBuilder};
pub use error::{ConfigError, ConfigErrors, ConfigValidation};
pub use source::{Source, Toml, Env, Defaults};
pub use validate::Validate;
pub use premortem_derive::Validate;
```

## Dependencies

- **Prerequisites**: Specs 001, 002, 003
- **Affected Components**: All public API surfaces
- **External Dependencies**:
  - `stillwater` crate (re-exported types)

## Testing Strategy

- **Unit Tests**:
  - Verify all re-exports compile and are accessible
  - Test that `use premortem::prelude::*` provides expected types
  - Verify type aliases work correctly
- **Documentation Tests**:
  - All examples in module docs must compile
  - Import pattern examples must work

## Documentation Requirements

- **Code Documentation**: Comprehensive module-level docs with examples
- **User Documentation**: Getting started guide using prelude
- **Migration Guide**: If changing from direct imports

## Implementation Notes

- Consider `pub use stillwater::prelude::*` if stillwater adds one
- Watch for stillwater breaking changes—may need wrapper types
- Feature flags should conditionally include watch/trace in prelude

## Migration and Compatibility

Not applicable - new project.
