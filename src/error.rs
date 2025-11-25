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

    /// Add a path prefix to this error.
    ///
    /// For errors with a path, prepends the prefix with a dot separator.
    /// For example, if path is "port" and prefix is "database", the result is "database.port".
    /// If the path starts with "[", treats it as an array index and doesn't add a dot.
    pub fn with_path_prefix(self, prefix: &str) -> Self {
        match self {
            ConfigError::ParseError {
                path,
                source_location,
                expected_type,
                actual_value,
                message,
            } => ConfigError::ParseError {
                path: prefix_path(prefix, &path),
                source_location,
                expected_type,
                actual_value,
                message,
            },
            ConfigError::MissingField {
                path,
                searched_sources,
            } => ConfigError::MissingField {
                path: prefix_path(prefix, &path),
                searched_sources,
            },
            ConfigError::ValidationError {
                path,
                source_location,
                value,
                message,
            } => ConfigError::ValidationError {
                path: prefix_path(prefix, &path),
                source_location,
                value,
                message,
            },
            ConfigError::CrossFieldError { paths, message } => ConfigError::CrossFieldError {
                paths: paths.into_iter().map(|p| prefix_path(prefix, &p)).collect(),
                message,
            },
            ConfigError::UnknownField {
                path,
                source_location,
                did_you_mean,
            } => ConfigError::UnknownField {
                path: prefix_path(prefix, &path),
                source_location,
                did_you_mean,
            },
            other => other, // SourceError and NoSources don't have paths
        }
    }
}

/// Helper function to prefix a path with a parent path.
fn prefix_path(prefix: &str, path: &str) -> String {
    if path.is_empty() {
        prefix.to_string()
    } else if path.starts_with('[') {
        // Array index - no dot needed
        format!("{}{}", prefix, path)
    } else {
        format!("{}.{}", prefix, path)
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

    /// Add a path prefix to all error paths.
    ///
    /// This is used for nested validation to add parent context to error paths.
    /// For example, if an error has path "port" and prefix is "database",
    /// the resulting path will be "database.port".
    pub fn with_path_prefix(self, prefix: &str) -> Self {
        Self(self.0.map(|e| e.with_path_prefix(prefix)))
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

    // Test Semigroup associativity law: (a <> b) <> c == a <> (b <> c)
    #[test]
    fn test_config_errors_semigroup_associativity() {
        let a = ConfigErrors::single(ConfigError::NoSources);
        let b = ConfigErrors::single(ConfigError::MissingField {
            path: "host".to_string(),
            searched_sources: vec!["config.toml".to_string()],
        });
        let c = ConfigErrors::single(ConfigError::MissingField {
            path: "port".to_string(),
            searched_sources: vec!["env".to_string()],
        });

        let left = a.clone().combine(b.clone()).combine(c.clone());
        let right = a.combine(b.combine(c));

        assert_eq!(left.len(), right.len());
        assert_eq!(left.len(), 3);
    }

    #[test]
    fn test_source_error_kind_display() {
        let not_found = SourceErrorKind::NotFound {
            path: "/etc/config.toml".to_string(),
        };
        assert_eq!(format!("{}", not_found), "file not found: /etc/config.toml");

        let io_err = SourceErrorKind::IoError {
            message: "permission denied".to_string(),
        };
        assert_eq!(format!("{}", io_err), "I/O error: permission denied");

        let parse_err = SourceErrorKind::ParseError {
            message: "unexpected token".to_string(),
            line: Some(10),
            column: Some(5),
        };
        assert_eq!(
            format!("{}", parse_err),
            "parse error: unexpected token at line 10, column 5"
        );

        let parse_err_no_pos = SourceErrorKind::ParseError {
            message: "invalid syntax".to_string(),
            line: None,
            column: None,
        };
        assert_eq!(
            format!("{}", parse_err_no_pos),
            "parse error: invalid syntax"
        );

        let conn_err = SourceErrorKind::ConnectionError {
            message: "timeout".to_string(),
        };
        assert_eq!(format!("{}", conn_err), "connection error: timeout");

        let other = SourceErrorKind::Other {
            message: "custom error".to_string(),
        };
        assert_eq!(format!("{}", other), "custom error");
    }

    #[test]
    fn test_config_error_display_all_variants() {
        // MissingField
        let err = ConfigError::MissingField {
            path: "db.host".to_string(),
            searched_sources: vec!["config.toml".to_string(), "env".to_string()],
        };
        assert_eq!(
            format!("{}", err),
            "missing required field 'db.host' (searched: config.toml, env)"
        );

        // ParseError
        let err = ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::new("config.toml").with_line(5),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid digit".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "[config.toml:5] 'port': expected integer, got \"abc\": invalid digit"
        );

        // ValidationError with location
        let err = ConfigError::ValidationError {
            path: "timeout".to_string(),
            source_location: Some(SourceLocation::new("config.toml")),
            value: Some("0".to_string()),
            message: "must be positive".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "[config.toml] 'timeout': must be positive"
        );

        // ValidationError without location
        let err = ConfigError::ValidationError {
            path: "timeout".to_string(),
            source_location: None,
            value: Some("0".to_string()),
            message: "must be positive".to_string(),
        };
        assert_eq!(format!("{}", err), "'timeout': must be positive");

        // CrossFieldError
        let err = ConfigError::CrossFieldError {
            paths: vec!["start_date".to_string(), "end_date".to_string()],
            message: "start_date must be before end_date".to_string(),
        };
        assert_eq!(
            format!("{}", err),
            "[start_date, end_date]: start_date must be before end_date"
        );

        // UnknownField without suggestion
        let err = ConfigError::UnknownField {
            path: "hoost".to_string(),
            source_location: SourceLocation::new("config.toml").with_line(3),
            did_you_mean: None,
        };
        assert_eq!(format!("{}", err), "[config.toml:3] unknown field 'hoost'");

        // UnknownField with suggestion
        let err = ConfigError::UnknownField {
            path: "hoost".to_string(),
            source_location: SourceLocation::new("config.toml").with_line(3),
            did_you_mean: Some("host".to_string()),
        };
        assert_eq!(
            format!("{}", err),
            "[config.toml:3] unknown field 'hoost'; did you mean 'host'?"
        );

        // SourceError
        let err = ConfigError::SourceError {
            source_name: "config.toml".to_string(),
            kind: SourceErrorKind::NotFound {
                path: "/etc/config.toml".to_string(),
            },
        };
        assert_eq!(
            format!("{}", err),
            "config.toml: file not found: /etc/config.toml"
        );

        // NoSources
        let err = ConfigError::NoSources;
        assert_eq!(format!("{}", err), "no configuration sources provided");
    }

    #[test]
    fn test_config_error_source_location() {
        let err = ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::new("config.toml"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid digit".to_string(),
        };
        assert!(err.source_location().is_some());
        assert_eq!(err.source_location().unwrap().source, "config.toml");

        let err = ConfigError::ValidationError {
            path: "timeout".to_string(),
            source_location: None,
            value: None,
            message: "must be positive".to_string(),
        };
        assert!(err.source_location().is_none());

        let err = ConfigError::MissingField {
            path: "host".to_string(),
            searched_sources: vec![],
        };
        assert!(err.source_location().is_none());
    }

    #[test]
    fn test_config_error_is_validation_error() {
        let validation_err = ConfigError::ValidationError {
            path: "port".to_string(),
            source_location: None,
            value: None,
            message: "must be positive".to_string(),
        };
        assert!(validation_err.is_validation_error());

        let cross_field_err = ConfigError::CrossFieldError {
            paths: vec!["start".to_string(), "end".to_string()],
            message: "invalid range".to_string(),
        };
        assert!(cross_field_err.is_validation_error());

        let parse_err = ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::new("config.toml"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid digit".to_string(),
        };
        assert!(!parse_err.is_validation_error());
    }

    #[test]
    fn test_config_error_suggestion() {
        let unknown_field = ConfigError::UnknownField {
            path: "hoost".to_string(),
            source_location: SourceLocation::new("config.toml"),
            did_you_mean: Some("host".to_string()),
        };
        assert_eq!(
            unknown_field.suggestion(),
            Some("Change 'hoost' to 'host'".to_string())
        );

        let missing_field = ConfigError::MissingField {
            path: "database.url".to_string(),
            searched_sources: vec![],
        };
        assert_eq!(
            missing_field.suggestion(),
            Some("Add 'database.url' to your configuration".to_string())
        );

        let parse_err = ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::new("config.toml"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid digit".to_string(),
        };
        assert!(parse_err.suggestion().is_none());
    }

    #[test]
    fn test_config_error_with_context() {
        let err = ConfigError::ValidationError {
            path: "port".to_string(),
            source_location: None,
            value: Some("0".to_string()),
            message: "must be positive".to_string(),
        };
        let with_ctx = err.with_context("while validating server config");
        match with_ctx {
            ConfigError::ValidationError { message, .. } => {
                assert_eq!(
                    message,
                    "while validating server config -> must be positive"
                );
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_source_error_kind_with_context() {
        let err = SourceErrorKind::Other {
            message: "custom error".to_string(),
        };
        let with_ctx = err.with_context("loading config");
        match with_ctx {
            SourceErrorKind::Other { message } => {
                assert_eq!(message, "loading config -> custom error");
            }
            _ => panic!("Expected Other variant"),
        }

        // Non-Other variants should pass through unchanged
        let err = SourceErrorKind::NotFound {
            path: "/etc/config".to_string(),
        };
        let with_ctx = err.with_context("loading config");
        match with_ctx {
            SourceErrorKind::NotFound { path } => {
                assert_eq!(path, "/etc/config");
            }
            _ => panic!("Expected NotFound variant"),
        }
    }

    #[test]
    fn test_config_errors_from_vec() {
        // Empty vec returns None
        let empty: Vec<ConfigError> = vec![];
        assert!(ConfigErrors::from_vec(empty).is_none());

        // Non-empty vec returns Some
        let errors = ConfigErrors::from_vec(vec![ConfigError::NoSources]);
        assert!(errors.is_some());
        assert_eq!(errors.unwrap().len(), 1);
    }

    #[test]
    fn test_config_errors_iter() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::NoSources,
            ConfigError::MissingField {
                path: "host".to_string(),
                searched_sources: vec![],
            },
        ])
        .unwrap();

        let paths: Vec<_> = errors.iter().filter_map(|e| e.path()).collect();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "host");
    }

    #[test]
    fn test_config_errors_into_iter() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::NoSources,
            ConfigError::MissingField {
                path: "host".to_string(),
                searched_sources: vec![],
            },
        ])
        .unwrap();

        let collected: Vec<_> = errors.into_iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_config_errors_with_context() {
        let errors = ConfigErrors::from_vec(vec![ConfigError::ValidationError {
            path: "port".to_string(),
            source_location: None,
            value: Some("0".to_string()),
            message: "must be positive".to_string(),
        }])
        .unwrap();

        let with_ctx = errors.with_context("validating server");
        let first = with_ctx.first();
        match first {
            ConfigError::ValidationError { message, .. } => {
                assert!(message.contains("validating server"));
            }
            _ => panic!("Expected ValidationError"),
        }
    }

    #[test]
    fn test_config_errors_display() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::NoSources,
            ConfigError::MissingField {
                path: "host".to_string(),
                searched_sources: vec!["config.toml".to_string()],
            },
        ])
        .unwrap();

        let display = format!("{}", errors);
        assert!(display.contains("Configuration errors (2):"));
        assert!(display.contains("no configuration sources provided"));
        assert!(display.contains("missing required field 'host'"));
    }

    #[test]
    fn test_config_errors_from_single_error() {
        let errors: ConfigErrors = ConfigError::NoSources.into();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_source_location_file() {
        let loc = SourceLocation::file("config.toml", Some(10), Some(5));
        assert_eq!(loc.source, "config.toml");
        assert_eq!(loc.line, Some(10));
        assert_eq!(loc.column, Some(5));
    }

    #[test]
    fn test_config_error_with_path_prefix() {
        // ValidationError
        let err = ConfigError::ValidationError {
            path: "port".to_string(),
            source_location: None,
            value: None,
            message: "must be positive".to_string(),
        };
        let prefixed = err.with_path_prefix("database");
        assert_eq!(prefixed.path(), Some("database.port"));

        // MissingField
        let err = ConfigError::MissingField {
            path: "host".to_string(),
            searched_sources: vec![],
        };
        let prefixed = err.with_path_prefix("server");
        assert_eq!(prefixed.path(), Some("server.host"));

        // ParseError
        let err = ConfigError::ParseError {
            path: "timeout".to_string(),
            source_location: SourceLocation::new("config.toml"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid".to_string(),
        };
        let prefixed = err.with_path_prefix("connection");
        assert_eq!(prefixed.path(), Some("connection.timeout"));

        // CrossFieldError
        let err = ConfigError::CrossFieldError {
            paths: vec!["start".to_string(), "end".to_string()],
            message: "invalid range".to_string(),
        };
        let prefixed = err.with_path_prefix("schedule");
        match prefixed {
            ConfigError::CrossFieldError { paths, .. } => {
                assert_eq!(paths, vec!["schedule.start", "schedule.end"]);
            }
            _ => panic!("Expected CrossFieldError"),
        }

        // NoSources should remain unchanged
        let err = ConfigError::NoSources;
        let prefixed = err.with_path_prefix("any");
        assert!(prefixed.path().is_none());
    }

    #[test]
    fn test_config_error_with_path_prefix_array_index() {
        // Array index paths should not get a dot separator
        let err = ConfigError::ValidationError {
            path: "[0]".to_string(),
            source_location: None,
            value: None,
            message: "invalid".to_string(),
        };
        let prefixed = err.with_path_prefix("items");
        assert_eq!(prefixed.path(), Some("items[0]"));
    }

    #[test]
    fn test_config_error_with_path_prefix_empty_path() {
        // Empty path should just become the prefix
        let err = ConfigError::ValidationError {
            path: "".to_string(),
            source_location: None,
            value: None,
            message: "invalid".to_string(),
        };
        let prefixed = err.with_path_prefix("config");
        assert_eq!(prefixed.path(), Some("config"));
    }

    #[test]
    fn test_config_errors_with_path_prefix() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::ValidationError {
                path: "host".to_string(),
                source_location: None,
                value: None,
                message: "empty".to_string(),
            },
            ConfigError::ValidationError {
                path: "port".to_string(),
                source_location: None,
                value: None,
                message: "invalid".to_string(),
            },
        ])
        .unwrap();

        let prefixed = errors.with_path_prefix("database");
        let paths: Vec<_> = prefixed.iter().filter_map(|e| e.path()).collect();
        assert_eq!(paths, vec!["database.host", "database.port"]);
    }
}
