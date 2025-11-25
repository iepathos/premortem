//! Pretty printing for configuration errors.
//!
//! This module provides colorized, grouped error output with support for
//! suggestions, redaction of sensitive values, and truncation.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{ConfigErrors, PrettyPrintOptions};
//!
//! let errors: ConfigErrors = /* ... */;
//! errors.pretty_print(PrettyPrintOptions::default());
//! ```
//!
//! # Output Format
//!
//! ```text
//! Configuration errors (4):
//!
//!   config.toml:
//!     • missing required field 'database.host'
//!     • 'database.pool_size' = -5: value must be >= 1
//!
//!   env:APP_DATABASE_PORT:
//!     • 'database.port': expected integer, got "abc": invalid digit
//!
//! Hints:
//!   • Add 'database.host' to your configuration
//! ```

use std::io::Write;

use crate::error::{group_by_source, ConfigError, ConfigErrors, ConfigValidation};
use stillwater::Validation;

/// Options for pretty printing errors.
#[derive(Debug, Clone)]
pub struct PrettyPrintOptions {
    /// Enable colored output (auto-detected by default).
    pub color: ColorOption,
    /// Group errors by source.
    pub group_by_source: bool,
    /// Show fix suggestions.
    pub show_suggestions: bool,
    /// Maximum errors to display (None for all).
    pub max_errors: Option<usize>,
    /// Redact sensitive values.
    pub redact_sensitive: bool,
}

impl Default for PrettyPrintOptions {
    fn default() -> Self {
        Self {
            color: ColorOption::Auto,
            group_by_source: true,
            show_suggestions: true,
            max_errors: Some(20),
            redact_sensitive: true,
        }
    }
}

impl PrettyPrintOptions {
    /// Create options with colors disabled.
    pub fn no_color() -> Self {
        Self {
            color: ColorOption::Never,
            ..Default::default()
        }
    }

    /// Create options that show all errors (no truncation).
    pub fn show_all() -> Self {
        Self {
            max_errors: None,
            ..Default::default()
        }
    }

    /// Set the color option.
    pub fn with_color(mut self, color: ColorOption) -> Self {
        self.color = color;
        self
    }

    /// Set whether to group by source.
    pub fn with_grouping(mut self, group: bool) -> Self {
        self.group_by_source = group;
        self
    }

    /// Set whether to show suggestions.
    pub fn with_suggestions(mut self, show: bool) -> Self {
        self.show_suggestions = show;
        self
    }

    /// Set the maximum number of errors to display.
    pub fn with_max_errors(mut self, max: Option<usize>) -> Self {
        self.max_errors = max;
        self
    }

    /// Set whether to redact sensitive values.
    pub fn with_redaction(mut self, redact: bool) -> Self {
        self.redact_sensitive = redact;
        self
    }
}

/// Color output option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOption {
    /// Auto-detect based on terminal capability.
    Auto,
    /// Always use colors.
    Always,
    /// Never use colors.
    Never,
}

/// ANSI color codes for terminal output.
struct Colors {
    error: &'static str,
    warning: &'static str,
    info: &'static str,
    path: &'static str,
    value: &'static str,
    hint: &'static str,
    reset: &'static str,
}

impl Colors {
    fn enabled() -> Self {
        Self {
            error: "\x1b[1;31m",   // bold red
            warning: "\x1b[1;33m", // bold yellow
            info: "\x1b[1;36m",    // bold cyan
            path: "\x1b[1;37m",    // bold white
            value: "\x1b[33m",     // yellow
            hint: "\x1b[32m",      // green
            reset: "\x1b[0m",
        }
    }

    fn disabled() -> Self {
        Self {
            error: "",
            warning: "",
            info: "",
            path: "",
            value: "",
            hint: "",
            reset: "",
        }
    }
}

/// Internal error printer that handles formatting.
struct ErrorPrinter<'a> {
    options: &'a PrettyPrintOptions,
    colors: Colors,
}

impl<'a> ErrorPrinter<'a> {
    fn new(options: &'a PrettyPrintOptions, use_color: bool) -> Self {
        let colors = if use_color {
            Colors::enabled()
        } else {
            Colors::disabled()
        };
        Self { options, colors }
    }

    fn print(&self, errors: &ConfigErrors, writer: &mut dyn Write) {
        let c = &self.colors;

        // Header
        writeln!(
            writer,
            "\n{}Configuration errors ({}):{}\n",
            c.error,
            errors.len(),
            c.reset
        )
        .ok();

        if self.options.group_by_source {
            self.print_grouped(errors, writer);
        } else {
            self.print_flat(errors, writer);
        }

        // Suggestions footer
        if self.options.show_suggestions {
            self.print_suggestions(errors, writer);
        }
    }

    fn print_grouped(&self, errors: &ConfigErrors, writer: &mut dyn Write) {
        let groups = group_by_source(errors);
        let c = &self.colors;
        let mut shown = 0;

        for (source, errs) in groups {
            writeln!(writer, "  {}{}:{}", c.info, source, c.reset).ok();

            for error in errs {
                if let Some(max) = self.options.max_errors {
                    if shown >= max {
                        let remaining = errors.len() - shown;
                        writeln!(
                            writer,
                            "\n  {}...and {} more errors{}\n",
                            c.warning, remaining, c.reset
                        )
                        .ok();
                        return;
                    }
                }

                self.print_error(error, writer);
                shown += 1;
            }
            writeln!(writer).ok();
        }
    }

    fn print_flat(&self, errors: &ConfigErrors, writer: &mut dyn Write) {
        let c = &self.colors;

        for (shown, error) in errors.iter().enumerate() {
            if let Some(max) = self.options.max_errors {
                if shown >= max {
                    let remaining = errors.len() - shown;
                    writeln!(
                        writer,
                        "\n  {}...and {} more errors{}\n",
                        c.warning, remaining, c.reset
                    )
                    .ok();
                    return;
                }
            }

            self.print_error(error, writer);
        }
        writeln!(writer).ok();
    }

    fn print_error(&self, error: &ConfigError, writer: &mut dyn Write) {
        let c = &self.colors;

        match error {
            ConfigError::MissingField { path, .. } => {
                writeln!(
                    writer,
                    "    {}•{} missing required field '{}{}{}'",
                    c.error, c.reset, c.path, path, c.reset
                )
                .ok();
            }
            ConfigError::ParseError {
                path,
                expected_type,
                actual_value,
                message,
                ..
            } => {
                let display_value = self.maybe_redact(actual_value, path);
                writeln!(
                    writer,
                    "    {}•{} '{}{}{}': expected {}, got \"{}{}{}\": {}",
                    c.error,
                    c.reset,
                    c.path,
                    path,
                    c.reset,
                    expected_type,
                    c.value,
                    display_value,
                    c.reset,
                    message
                )
                .ok();
            }
            ConfigError::ValidationError {
                path,
                value,
                message,
                ..
            } => {
                let display_value = value
                    .as_ref()
                    .map(|v| self.maybe_redact(v, path))
                    .unwrap_or_else(|| "[no value]".to_string());
                writeln!(
                    writer,
                    "    {}•{} '{}{}{}' = {}{}{}: {}",
                    c.error,
                    c.reset,
                    c.path,
                    path,
                    c.reset,
                    c.value,
                    display_value,
                    c.reset,
                    message
                )
                .ok();
            }
            ConfigError::CrossFieldError { paths, message } => {
                let paths_str = paths
                    .iter()
                    .map(|p| format!("{}{}{}", c.path, p, c.reset))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(
                    writer,
                    "    {}•{} [{}]: {}",
                    c.error, c.reset, paths_str, message
                )
                .ok();
            }
            ConfigError::UnknownField {
                path, did_you_mean, ..
            } => {
                let mut msg = format!(
                    "    {}•{} unknown field '{}{}{}'",
                    c.warning, c.reset, c.path, path, c.reset
                );
                if let Some(suggestion) = did_you_mean {
                    msg.push_str(&format!(
                        "; did you mean '{}{}{}'?",
                        c.hint, suggestion, c.reset
                    ));
                }
                writeln!(writer, "{}", msg).ok();
            }
            ConfigError::SourceError { source_name, kind } => {
                writeln!(
                    writer,
                    "    {}•{} {}: {}",
                    c.error, c.reset, source_name, kind
                )
                .ok();
            }
            ConfigError::NoSources => {
                writeln!(
                    writer,
                    "    {}•{} no configuration sources provided",
                    c.error, c.reset
                )
                .ok();
            }
        }
    }

    fn print_suggestions(&self, errors: &ConfigErrors, writer: &mut dyn Write) {
        let c = &self.colors;
        let suggestions: Vec<_> = errors
            .iter()
            .filter_map(|e| e.suggestion())
            .take(3)
            .collect();

        if !suggestions.is_empty() {
            writeln!(writer, "{}Hints:{}", c.hint, c.reset).ok();
            for suggestion in suggestions {
                writeln!(writer, "  • {}", suggestion).ok();
            }
            writeln!(writer).ok();
        }
    }

    fn maybe_redact(&self, value: &str, path: &str) -> String {
        if self.options.redact_sensitive && is_sensitive_path(path) {
            "[REDACTED]".to_string()
        } else {
            value.to_string()
        }
    }
}

/// Check if a config path appears to contain sensitive data.
fn is_sensitive_path(path: &str) -> bool {
    let sensitive_patterns = [
        "password",
        "secret",
        "key",
        "token",
        "credential",
        "api_key",
    ];
    let lower = path.to_lowercase();
    sensitive_patterns.iter().any(|p| lower.contains(p))
}

/// Detect if stdout/stderr is a TTY for color support.
fn should_use_color(color_option: ColorOption) -> bool {
    match color_option {
        ColorOption::Always => true,
        ColorOption::Never => false,
        ColorOption::Auto => {
            // Use std::io::IsTerminal (Rust 1.70+)
            use std::io::IsTerminal;
            std::io::stderr().is_terminal()
        }
    }
}

impl ConfigErrors {
    /// Pretty print errors to stderr.
    ///
    /// # Stillwater Integration
    ///
    /// `ConfigErrors` wraps `NonEmptyVec<ConfigError>`, guaranteeing at least
    /// one error exists. This eliminates empty-error-list edge cases.
    pub fn pretty_print(&self, options: &PrettyPrintOptions) {
        let use_color = should_use_color(options.color);
        let printer = ErrorPrinter::new(options, use_color);
        let mut stderr = std::io::stderr();
        printer.print(self, &mut stderr);
    }

    /// Pretty print to a string (for testing).
    pub fn format(&self, options: &PrettyPrintOptions) -> String {
        let use_color = match options.color {
            ColorOption::Always => true,
            ColorOption::Never => false,
            ColorOption::Auto => false, // Default to no color for string formatting
        };
        let printer = ErrorPrinter::new(options, use_color);
        let mut buf = Vec::new();
        printer.print(self, &mut buf);
        String::from_utf8(buf).unwrap_or_default()
    }

    /// Pretty print with default options.
    pub fn pretty_print_default(&self) {
        self.pretty_print(&PrettyPrintOptions::default());
    }
}

/// Trait extension for easy error handling with pretty printing.
///
/// # Stillwater Integration
///
/// Works with `ConfigValidation<T>` (alias for `Validation<T, ConfigErrors>`).
/// Uses `ConfigErrors` (NonEmptyVec wrapper) for type-safe error handling.
pub trait ValidationExt<T> {
    /// Unwrap or pretty print errors and exit with code 1.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use premortem::{Config, ValidationExt};
    ///
    /// let config = Config::<AppConfig>::builder()
    ///     .source(Toml::file("config.toml"))
    ///     .build()
    ///     .unwrap_or_exit();  // Prints all errors and exits if validation fails
    /// ```
    fn unwrap_or_exit(self) -> T;

    /// Unwrap or pretty print errors with custom options and exit.
    fn unwrap_or_exit_with(self, options: &PrettyPrintOptions) -> T;

    /// Convert to Result, pretty printing on error but not exiting.
    fn unwrap_or_print(self) -> Result<T, ConfigErrors>;
}

impl<T> ValidationExt<T> for ConfigValidation<T> {
    fn unwrap_or_exit(self) -> T {
        self.unwrap_or_exit_with(&PrettyPrintOptions::default())
    }

    fn unwrap_or_exit_with(self, options: &PrettyPrintOptions) -> T {
        match self {
            Validation::Success(value) => value,
            Validation::Failure(errors) => {
                errors.pretty_print(options);
                std::process::exit(1);
            }
        }
    }

    fn unwrap_or_print(self) -> Result<T, ConfigErrors> {
        match self {
            Validation::Success(value) => Ok(value),
            Validation::Failure(errors) => {
                errors.pretty_print_default();
                Err(errors)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceLocation;

    fn create_test_errors() -> ConfigErrors {
        ConfigErrors::from_vec(vec![
            ConfigError::MissingField {
                path: "database.host".to_string(),
                searched_sources: vec!["config.toml".to_string(), "env".to_string()],
            },
            ConfigError::ParseError {
                path: "database.port".to_string(),
                source_location: SourceLocation::new("config.toml").with_line(5),
                expected_type: "integer".to_string(),
                actual_value: "abc".to_string(),
                message: "invalid digit".to_string(),
            },
            ConfigError::ValidationError {
                path: "server.timeout".to_string(),
                source_location: Some(SourceLocation::new("config.toml")),
                value: Some("0".to_string()),
                message: "must be positive".to_string(),
            },
        ])
        .unwrap()
    }

    #[test]
    fn test_format_errors_contains_header() {
        let errors = create_test_errors();
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("Configuration errors (3):"));
    }

    #[test]
    fn test_format_errors_contains_all_errors() {
        let errors = create_test_errors();
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("database.host"));
        assert!(output.contains("database.port"));
        assert!(output.contains("server.timeout"));
    }

    #[test]
    fn test_format_missing_field() {
        let errors = ConfigErrors::single(ConfigError::MissingField {
            path: "host".to_string(),
            searched_sources: vec!["config.toml".to_string()],
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("missing required field 'host'"));
    }

    #[test]
    fn test_format_parse_error() {
        let errors = ConfigErrors::single(ConfigError::ParseError {
            path: "port".to_string(),
            source_location: SourceLocation::new("config.toml"),
            expected_type: "integer".to_string(),
            actual_value: "abc".to_string(),
            message: "invalid digit".to_string(),
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("'port': expected integer, got \"abc\": invalid digit"));
    }

    #[test]
    fn test_format_validation_error() {
        let errors = ConfigErrors::single(ConfigError::ValidationError {
            path: "timeout".to_string(),
            source_location: None,
            value: Some("0".to_string()),
            message: "must be positive".to_string(),
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("'timeout' = 0: must be positive"));
    }

    #[test]
    fn test_format_cross_field_error() {
        let errors = ConfigErrors::single(ConfigError::CrossFieldError {
            paths: vec!["start_date".to_string(), "end_date".to_string()],
            message: "start_date must be before end_date".to_string(),
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("[start_date, end_date]"));
        assert!(output.contains("start_date must be before end_date"));
    }

    #[test]
    fn test_format_unknown_field_with_suggestion() {
        let errors = ConfigErrors::single(ConfigError::UnknownField {
            path: "hoost".to_string(),
            source_location: SourceLocation::new("config.toml"),
            did_you_mean: Some("host".to_string()),
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("unknown field 'hoost'"));
        assert!(output.contains("did you mean 'host'?"));
    }

    #[test]
    fn test_format_source_error() {
        let errors = ConfigErrors::single(ConfigError::SourceError {
            source_name: "config.toml".to_string(),
            kind: crate::error::SourceErrorKind::NotFound {
                path: "/etc/config.toml".to_string(),
            },
        });
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("config.toml: file not found"));
    }

    #[test]
    fn test_format_no_sources() {
        let errors = ConfigErrors::single(ConfigError::NoSources);
        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("no configuration sources provided"));
    }

    #[test]
    fn test_redaction_of_sensitive_values() {
        let errors = ConfigErrors::single(ConfigError::ValidationError {
            path: "database.password".to_string(),
            source_location: None,
            value: Some("super_secret_123".to_string()),
            message: "invalid format".to_string(),
        });
        let output = errors.format(&PrettyPrintOptions::default());

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("super_secret_123"));
    }

    #[test]
    fn test_no_redaction_when_disabled() {
        let errors = ConfigErrors::single(ConfigError::ValidationError {
            path: "database.password".to_string(),
            source_location: None,
            value: Some("super_secret_123".to_string()),
            message: "invalid format".to_string(),
        });
        let options = PrettyPrintOptions::no_color().with_redaction(false);
        let output = errors.format(&options);

        assert!(!output.contains("[REDACTED]"));
        assert!(output.contains("super_secret_123"));
    }

    #[test]
    fn test_truncation_with_max_errors() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::MissingField {
                path: "field1".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field2".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field3".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field4".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field5".to_string(),
                searched_sources: vec![],
            },
        ])
        .unwrap();

        let options = PrettyPrintOptions::no_color().with_max_errors(Some(3));
        let output = errors.format(&options);

        assert!(output.contains("field1"));
        assert!(output.contains("field2"));
        assert!(output.contains("field3"));
        assert!(!output.contains("field4"));
        assert!(!output.contains("field5"));
        assert!(output.contains("...and 2 more errors"));
    }

    #[test]
    fn test_no_truncation_when_disabled() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::MissingField {
                path: "field1".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field2".to_string(),
                searched_sources: vec![],
            },
            ConfigError::MissingField {
                path: "field3".to_string(),
                searched_sources: vec![],
            },
        ])
        .unwrap();

        let output = errors.format(&PrettyPrintOptions::show_all());

        assert!(output.contains("field1"));
        assert!(output.contains("field2"));
        assert!(output.contains("field3"));
        assert!(!output.contains("...and"));
    }

    #[test]
    fn test_suggestions_shown() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::MissingField {
                path: "database.host".to_string(),
                searched_sources: vec![],
            },
            ConfigError::UnknownField {
                path: "hoost".to_string(),
                source_location: SourceLocation::new("config.toml"),
                did_you_mean: Some("host".to_string()),
            },
        ])
        .unwrap();

        let output = errors.format(&PrettyPrintOptions::no_color());

        assert!(output.contains("Hints:"));
        assert!(output.contains("Add 'database.host' to your configuration"));
        assert!(output.contains("Change 'hoost' to 'host'"));
    }

    #[test]
    fn test_no_suggestions_when_disabled() {
        let errors = ConfigErrors::single(ConfigError::MissingField {
            path: "database.host".to_string(),
            searched_sources: vec![],
        });

        let options = PrettyPrintOptions::no_color().with_suggestions(false);
        let output = errors.format(&options);

        assert!(!output.contains("Hints:"));
    }

    #[test]
    fn test_grouping_by_source() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::ParseError {
                path: "port".to_string(),
                source_location: SourceLocation::new("config.toml"),
                expected_type: "integer".to_string(),
                actual_value: "abc".to_string(),
                message: "invalid".to_string(),
            },
            ConfigError::ValidationError {
                path: "host".to_string(),
                source_location: Some(SourceLocation::new("config.toml")),
                value: Some("".to_string()),
                message: "empty".to_string(),
            },
            ConfigError::ParseError {
                path: "timeout".to_string(),
                source_location: SourceLocation::new("env:APP_TIMEOUT"),
                expected_type: "integer".to_string(),
                actual_value: "xyz".to_string(),
                message: "invalid".to_string(),
            },
        ])
        .unwrap();

        let options = PrettyPrintOptions::no_color().with_grouping(true);
        let output = errors.format(&options);

        // Should have source headers
        assert!(output.contains("config.toml:"));
        assert!(output.contains("env:APP_TIMEOUT:"));
    }

    #[test]
    fn test_flat_output_without_grouping() {
        let errors = ConfigErrors::from_vec(vec![
            ConfigError::ParseError {
                path: "port".to_string(),
                source_location: SourceLocation::new("config.toml"),
                expected_type: "integer".to_string(),
                actual_value: "abc".to_string(),
                message: "invalid".to_string(),
            },
            ConfigError::ValidationError {
                path: "host".to_string(),
                source_location: Some(SourceLocation::new("env:APP_HOST")),
                value: Some("".to_string()),
                message: "empty".to_string(),
            },
        ])
        .unwrap();

        let options = PrettyPrintOptions::no_color().with_grouping(false);
        let output = errors.format(&options);

        // Should have errors but no source headers for grouping
        assert!(output.contains("port"));
        assert!(output.contains("host"));
    }

    #[test]
    fn test_is_sensitive_path() {
        assert!(is_sensitive_path("password"));
        assert!(is_sensitive_path("database.password"));
        assert!(is_sensitive_path("api_key"));
        assert!(is_sensitive_path("API_KEY"));
        assert!(is_sensitive_path("secret_token"));
        assert!(is_sensitive_path("aws_secret_access_key"));
        assert!(is_sensitive_path("credential"));

        assert!(!is_sensitive_path("host"));
        assert!(!is_sensitive_path("port"));
        assert!(!is_sensitive_path("timeout"));
    }

    #[test]
    fn test_color_option_always() {
        let errors = ConfigErrors::single(ConfigError::NoSources);
        let options = PrettyPrintOptions::default().with_color(ColorOption::Always);
        let output = errors.format(&options);

        // Should contain ANSI escape codes
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_color_option_never() {
        let errors = ConfigErrors::single(ConfigError::NoSources);
        let options = PrettyPrintOptions::default().with_color(ColorOption::Never);
        let output = errors.format(&options);

        // Should not contain ANSI escape codes
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_pretty_print_options_builder() {
        let options = PrettyPrintOptions::default()
            .with_color(ColorOption::Always)
            .with_grouping(false)
            .with_suggestions(false)
            .with_max_errors(Some(10))
            .with_redaction(false);

        assert_eq!(options.color, ColorOption::Always);
        assert!(!options.group_by_source);
        assert!(!options.show_suggestions);
        assert_eq!(options.max_errors, Some(10));
        assert!(!options.redact_sensitive);
    }

    #[test]
    fn test_validation_ext_unwrap_or_print_success() {
        let validation: ConfigValidation<i32> = Validation::Success(42);
        let result = validation.unwrap_or_print();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_validation_ext_unwrap_or_print_failure() {
        let validation: ConfigValidation<i32> =
            Validation::Failure(ConfigErrors::single(ConfigError::NoSources));
        let result = validation.unwrap_or_print();
        assert!(result.is_err());
    }
}
