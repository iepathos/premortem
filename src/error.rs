//! Error types for the premortem configuration library.
//!
//! This module provides comprehensive error types with source location tracking
//! that integrate with stillwater's `Validation` type and `Semigroup` trait.

use std::collections::BTreeMap;
use std::fmt;

use stillwater::{NonEmptyVec, Semigroup, Validation};

/// Location where a configuration value originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Name of the source (e.g., "config.toml", "env:APP_HOST")
    pub source: String,
    /// Line number in the source (1-indexed), if applicable
    pub line: Option<u32>,
    /// Column number in the source (1-indexed), if applicable
    pub column: Option<u32>,
}

impl SourceLocation {
    /// Create a new source location with just a source name.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            line: None,
            column: None,
        }
    }

    /// Add a line number to this location.
    pub fn with_line(mut self, line: u32) -> Self {
        self.line = Some(line);
        self
    }

    /// Add a column number to this location.
    pub fn with_column(mut self, column: u32) -> Self {
        self.column = Some(column);
        self
    }

    /// Create a location for an environment variable.
    pub fn env(var_name: &str) -> Self {
        Self::new(format!("env:{}", var_name))
    }

    /// Create a location for a file with optional position.
    pub fn file(path: &str, line: Option<u32>, column: Option<u32>) -> Self {
        Self {
            source: path.to_string(),
            line,
            column,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(col)) => write!(f, "{}:{}:{}", self.source, line, col),
            (Some(line), None) => write!(f, "{}:{}", self.source, line),
            _ => write!(f, "{}", self.source),
        }
    }
}

/// Kinds of source loading errors.
#[derive(Debug, Clone)]
pub enum SourceErrorKind {
    /// Source file was not found
    NotFound { path: String },
    /// Source file could not be read
    IoError { message: String },
    /// Source content could not be parsed
    ParseError {
        message: String,
        line: Option<u32>,
        column: Option<u32>,
    },
    /// Remote source failed to connect
    ConnectionError { message: String },
    /// Other source-specific error
    Other { message: String },
}

impl SourceErrorKind {
    /// Add context to this error kind.
    pub fn with_context(self, context: &str) -> Self {
        match self {
            SourceErrorKind::Other { message } => SourceErrorKind::Other {
                message: format!("{} -> {}", context, message),
            },
            other => other,
        }
    }
}

impl fmt::Display for SourceErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceErrorKind::NotFound { path } => write!(f, "file not found: {}", path),
            SourceErrorKind::IoError { message } => write!(f, "I/O error: {}", message),
            SourceErrorKind::ParseError {
                message,
                line,
                column,
            } => {
                write!(f, "parse error: {}", message)?;
                if let Some(l) = line {
                    write!(f, " at line {}", l)?;
                    if let Some(c) = column {
                        write!(f, ", column {}", c)?;
                    }
                }
                Ok(())
            }
            SourceErrorKind::ConnectionError { message } => {
                write!(f, "connection error: {}", message)
            }
            SourceErrorKind::Other { message } => write!(f, "{}", message),
        }
    }
}

/// Errors that can occur during configuration loading and validation.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// A configuration source failed to load
    SourceError {
        source_name: String,
        kind: SourceErrorKind,
    },

    /// A field failed to parse to the expected type
    ParseError {
        path: String,
        source_location: SourceLocation,
        expected_type: String,
        actual_value: String,
        message: String,
    },

    /// A required field is missing
    MissingField {
        path: String,
        searched_sources: Vec<String>,
    },

    /// A validation rule failed
    ValidationError {
        path: String,
        source_location: Option<SourceLocation>,
        value: Option<String>, // None if sensitive
        message: String,
    },

    /// Cross-field validation failed
    CrossFieldError { paths: Vec<String>, message: String },

    /// An unknown field was found (when configured to error)
    UnknownField {
        path: String,
        source_location: SourceLocation,
        did_you_mean: Option<String>,
    },

    /// No sources were provided to the builder
    NoSources,
}

impl ConfigError {
    /// Get the configuration path that this error relates to, if any.
    pub fn path(&self) -> Option<&str> {
        match self {
            ConfigError::ParseError { path, .. } => Some(path),
            ConfigError::MissingField { path, .. } => Some(path),
            ConfigError::ValidationError { path, .. } => Some(path),
            ConfigError::UnknownField { path, .. } => Some(path),
            ConfigError::CrossFieldError { .. } => None,
            ConfigError::SourceError { .. } => None,
            ConfigError::NoSources => None,
        }
    }

    /// Get the source location of this error, if any.
    pub fn source_location(&self) -> Option<&SourceLocation> {
        match self {
            ConfigError::ParseError {
                source_location, ..
            } => Some(source_location),
            ConfigError::ValidationError {
                source_location, ..
            } => source_location.as_ref(),
            ConfigError::UnknownField {
                source_location, ..
            } => Some(source_location),
            _ => None,
        }
    }

    /// Check if this is a validation error.
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            ConfigError::ValidationError { .. } | ConfigError::CrossFieldError { .. }
        )
    }

    /// Get a suggestion for fixing this error, if available.
    pub fn suggestion(&self) -> Option<String> {
        match self {
            ConfigError::UnknownField {
                did_you_mean: Some(s),
                path,
                ..
            } => Some(format!("Change '{}' to '{}'", path, s)),
            ConfigError::MissingField { path, .. } => {
                Some(format!("Add '{}' to your configuration", path))
            }
            _ => None,
        }
    }

    /// Add context to this error for better debugging.
    pub fn with_context(self, context: &str) -> Self {
        match self {
            ConfigError::ValidationError {
                path,
                source_location,
                value,
                message,
            } => ConfigError::ValidationError {
                path,
                source_location,
                value,
                message: format!("{} -> {}", context, message),
            },
            ConfigError::SourceError { source_name, kind } => ConfigError::SourceError {
                source_name,
                kind: kind.with_context(context),
            },
            other => other,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingField {
                path,
                searched_sources,
            } => {
                write!(
                    f,
                    "missing required field '{}' (searched: {})",
                    path,
                    searched_sources.join(", ")
                )
            }
            ConfigError::ParseError {
                path,
                source_location,
                expected_type,
                actual_value,
                message,
            } => {
                write!(
                    f,
                    "[{}] '{}': expected {}, got \"{}\": {}",
                    source_location, path, expected_type, actual_value, message
                )
            }
            ConfigError::ValidationError {
                path,
                source_location,
                message,
                ..
            } => match source_location {
                Some(loc) => write!(f, "[{}] '{}': {}", loc, path, message),
                None => write!(f, "'{}': {}", path, message),
            },
            ConfigError::CrossFieldError { paths, message } => {
                write!(f, "[{}]: {}", paths.join(", "), message)
            }
            ConfigError::UnknownField {
                path,
                source_location,
                did_you_mean,
            } => {
                let mut msg = format!("[{}] unknown field '{}'", source_location, path);
                if let Some(suggestion) = did_you_mean {
                    msg.push_str(&format!("; did you mean '{}'?", suggestion));
                }
                write!(f, "{}", msg)
            }
            ConfigError::SourceError { source_name, kind } => {
                write!(f, "{}: {}", source_name, kind)
            }
            ConfigError::NoSources => {
                write!(f, "no configuration sources provided")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// A non-empty collection of configuration errors.
///
/// Uses `NonEmptyVec` from stillwater to guarantee at least one error exists.
/// This prevents the "empty error list" anti-pattern and enables safe `first()`.
#[derive(Debug, Clone)]
pub struct ConfigErrors(pub NonEmptyVec<ConfigError>);

impl ConfigErrors {
    /// Create from a single error.
    pub fn single(error: ConfigError) -> Self {
        Self(NonEmptyVec::singleton(error))
    }

    /// Create from a non-empty vec.
    pub fn from_nonempty(errors: NonEmptyVec<ConfigError>) -> Self {
        Self(errors)
    }

    /// Try to create from a vec, returning None if empty.
    pub fn from_vec(errors: Vec<ConfigError>) -> Option<Self> {
        NonEmptyVec::from_vec(errors).map(Self)
    }

    /// Get the first error (always exists).
    pub fn first(&self) -> &ConfigError {
        self.0.head()
    }

    /// Get all errors as a slice.
    pub fn as_slice(&self) -> Vec<&ConfigError> {
        self.0.iter().collect()
    }

    /// Number of errors.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if empty (always false, but required for API consistency).
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Iterate over errors.
    pub fn iter(&self) -> impl Iterator<Item = &ConfigError> {
        self.0.iter()
    }

    /// Add context to all errors.
    pub fn with_context(self, context: impl Into<String>) -> Self {
        let context = context.into();
        Self(self.0.map(|e| e.with_context(&context)))
    }
}

impl Semigroup for ConfigErrors {
    fn combine(self, other: Self) -> Self {
        Self(self.0.combine(other.0))
    }
}

impl From<ConfigError> for ConfigErrors {
    fn from(error: ConfigError) -> Self {
        Self::single(error)
    }
}

impl IntoIterator for ConfigErrors {
    type Item = ConfigError;
    type IntoIter = std::vec::IntoIter<ConfigError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
    }
}

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Configuration errors ({}):", self.len())?;
        for error in self.iter() {
            writeln!(f, "  {}", error)?;
        }
        Ok(())
    }
}

/// The standard validation result type for premortem.
///
/// Uses `ConfigErrors` (NonEmptyVec) for guaranteed non-empty error accumulation.
pub type ConfigValidation<T> = Validation<T, ConfigErrors>;

/// Extension trait for creating failing validations easily.
pub trait ConfigValidationExt<T> {
    /// Create a failing validation with a single error.
    fn fail_with(error: ConfigError) -> ConfigValidation<T>;
}

impl<T> ConfigValidationExt<T> for ConfigValidation<T> {
    fn fail_with(error: ConfigError) -> ConfigValidation<T> {
        Validation::Failure(ConfigErrors::single(error))
    }
}

/// Group errors by their source for organized reporting.
pub fn group_by_source(errors: &ConfigErrors) -> BTreeMap<String, Vec<&ConfigError>> {
    let mut groups: BTreeMap<String, Vec<&ConfigError>> = BTreeMap::new();

    for error in errors.iter() {
        let source = error
            .source_location()
            .map(|loc| loc.source.clone())
            .unwrap_or_else(|| "(general)".to_string());

        groups.entry(source).or_default().push(error);
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::new("config.toml");
        assert_eq!(format!("{}", loc), "config.toml");

        let loc = SourceLocation::new("config.toml").with_line(10);
        assert_eq!(format!("{}", loc), "config.toml:10");

        let loc = SourceLocation::new("config.toml")
            .with_line(10)
            .with_column(5);
        assert_eq!(format!("{}", loc), "config.toml:10:5");
    }

    #[test]
    fn test_source_location_env() {
        let loc = SourceLocation::env("APP_HOST");
        assert_eq!(loc.source, "env:APP_HOST");
    }

    #[test]
    fn test_config_error_path() {
        let err = ConfigError::MissingField {
            path: "database.host".to_string(),
            searched_sources: vec!["config.toml".to_string()],
        };
        assert_eq!(err.path(), Some("database.host"));

        let err = ConfigError::NoSources;
        assert_eq!(err.path(), None);
    }

    #[test]
    fn test_config_errors_single() {
        let errors = ConfigErrors::single(ConfigError::NoSources);
        assert_eq!(errors.len(), 1);
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_config_errors_combine() {
        let e1 = ConfigErrors::single(ConfigError::NoSources);
        let e2 = ConfigErrors::single(ConfigError::MissingField {
            path: "host".to_string(),
            searched_sources: vec![],
        });
        let combined = e1.combine(e2);
        assert_eq!(combined.len(), 2);
    }

    #[test]
    fn test_config_validation_fail_with() {
        let result: ConfigValidation<i32> = ConfigValidation::fail_with(ConfigError::NoSources);
        assert!(result.is_failure());
    }

    #[test]
    fn test_group_by_source() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::ParseError {
                path: "port".to_string(),
                source_location: SourceLocation::new("config.toml"),
                expected_type: "integer".to_string(),
                actual_value: "abc".to_string(),
                message: "invalid digit".to_string(),
            },
            ConfigError::ValidationError {
                path: "host".to_string(),
                source_location: Some(SourceLocation::new("config.toml")),
                value: Some("".to_string()),
                message: "cannot be empty".to_string(),
            },
            ConfigError::MissingField {
                path: "database.url".to_string(),
                searched_sources: vec!["config.toml".to_string(), "env".to_string()],
            },
        ])
        .unwrap();

        let grouped = group_by_source(&errors);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.get("config.toml").map(|v| v.len()), Some(2));
        assert_eq!(grouped.get("(general)").map(|v| v.len()), Some(1));
    }
}
