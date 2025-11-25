---
number: 8
title: Pretty Error Printing
category: foundation
priority: high
status: draft
dependencies: [2]
created: 2025-11-25
---

# Specification 008: Pretty Error Printing

**Category**: foundation
**Priority**: high
**Status**: draft
**Dependencies**: [002 - Error Types]

## Context

Clear, actionable error messages are a key differentiator for premortem. Users should see all configuration errors grouped by source, with helpful suggestions for fixing them. This specification covers the pretty printing and formatting of configuration errors.

### Stillwater Integration

Pretty printing works with `ConfigErrors` (the NonEmptyVec wrapper from spec 002):

```rust
/// Pretty print configuration errors
pub fn pretty_print(errors: &ConfigErrors, options: PrettyPrintOptions) {
    // ConfigErrors guarantees at least one error exists
    // Can safely call errors.first() without Option
}
```

This integrates with the `unwrap_or_exit()` pattern:

```rust
let config = Config::<AppConfig>::builder()
    .source(Toml::file("config.toml"))
    .build()
    .unwrap_or_exit();  // Pretty prints ConfigErrors and exits
```

## Objective

Implement error reporting utilities that format `ConfigErrors` (NonEmptyVec) in a clear, grouped, and colorful way, with support for suggestions and redaction of sensitive values.

## Requirements

### Functional Requirements

1. **Pretty Print**: Format errors for terminal display
2. **Grouping**: Group errors by source file/location
3. **Color Support**: Colorized output (with disable option)
4. **Suggestions**: Show "did you mean?" and fix hints
5. **Redaction**: Hide sensitive values
6. **Error Count**: Show total error count and summary
7. **Truncation**: Optional limit on displayed errors

### Non-Functional Requirements

- Output should be readable in 80-column terminals
- Colors should work on common terminals
- Should handle non-TTY output gracefully
- Fast (errors are shown once at startup)

## Acceptance Criteria

- [ ] `ConfigError::pretty_print()` outputs formatted errors
- [ ] Errors grouped by source file
- [ ] Color support with auto-detection and manual override
- [ ] Suggestions shown for applicable errors
- [ ] Sensitive values shown as `[REDACTED]`
- [ ] Error count header: "Configuration errors (N):"
- [ ] Max errors option with "and N more..." truncation
- [ ] Works with both stdout and stderr
- [ ] Unit tests for formatting

## Technical Details

### API Design

```rust
/// Options for pretty printing errors
#[derive(Debug, Clone)]
pub struct PrettyPrintOptions {
    /// Enable colored output (auto-detected by default)
    pub color: ColorOption,
    /// Group errors by source
    pub group_by_source: bool,
    /// Show fix suggestions
    pub show_suggestions: bool,
    /// Maximum errors to display (None for all)
    pub max_errors: Option<usize>,
    /// Redact sensitive values
    pub redact_sensitive: bool,
    /// Output stream (defaults to stderr)
    pub writer: Box<dyn std::io::Write>,
}

#[derive(Debug, Clone, Copy)]
pub enum ColorOption {
    Auto,
    Always,
    Never,
}

impl Default for PrettyPrintOptions {
    fn default() -> Self {
        Self {
            color: ColorOption::Auto,
            group_by_source: true,
            show_suggestions: true,
            max_errors: Some(20),
            redact_sensitive: true,
            writer: Box::new(std::io::stderr()),
        }
    }
}
```

### Pretty Print Implementation

```rust
impl ConfigError {
    /// Pretty print a slice of errors to stderr
    pub fn pretty_print(errors: &[ConfigError], options: PrettyPrintOptions) {
        let printer = ErrorPrinter::new(options);
        printer.print(errors);
    }

    /// Pretty print to a string (for testing)
    pub fn format(errors: &[ConfigError], options: PrettyPrintOptions) -> String {
        let mut buf = Vec::new();
        let mut options = options;
        options.writer = Box::new(&mut buf);
        let printer = ErrorPrinter::new(options);
        printer.print(errors);
        String::from_utf8(buf).unwrap_or_default()
    }
}

struct ErrorPrinter {
    options: PrettyPrintOptions,
    colors: Colors,
}

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
            error: "\x1b[1;31m",    // bold red
            warning: "\x1b[1;33m",  // bold yellow
            info: "\x1b[1;36m",     // bold cyan
            path: "\x1b[1;37m",     // bold white
            value: "\x1b[33m",      // yellow
            hint: "\x1b[32m",       // green
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
```

### Output Format

```rust
impl ErrorPrinter {
    fn print(&self, errors: &[ConfigError]) {
        let w = &mut self.options.writer;
        let c = &self.colors;

        // Header
        writeln!(w, "\n{}Configuration errors ({}):{}\n",
            c.error, errors.len(), c.reset).ok();

        if self.options.group_by_source {
            self.print_grouped(errors, w);
        } else {
            self.print_flat(errors, w);
        }

        // Suggestions footer
        if self.options.show_suggestions {
            self.print_suggestions(errors, w);
        }
    }

    fn print_grouped(&self, errors: &[ConfigError], w: &mut dyn Write) {
        let groups = group_by_source(errors);
        let c = &self.colors;
        let mut shown = 0;

        for (source, errs) in groups {
            writeln!(w, "  {}{}:{}", c.info, source, c.reset).ok();

            for error in errs {
                if let Some(max) = self.options.max_errors {
                    if shown >= max {
                        let remaining = errors.len() - shown;
                        writeln!(w, "\n  {}...and {} more errors{}\n",
                            c.warning, remaining, c.reset).ok();
                        return;
                    }
                }

                self.print_error(error, w);
                shown += 1;
            }
            writeln!(w).ok();
        }
    }

    fn print_error(&self, error: &ConfigError, w: &mut dyn Write) {
        let c = &self.colors;

        match error {
            ConfigError::MissingField { path, .. } => {
                writeln!(w, "    {}•{} missing required field '{}{}{}'",
                    c.error, c.reset, c.path, path, c.reset).ok();
            }
            ConfigError::ParseError { path, expected_type, actual_value, message, .. } => {
                let display_value = self.maybe_redact(actual_value, path);
                writeln!(w, "    {}•{} '{}{}{}': expected {}, got \"{}{}{}\": {}",
                    c.error, c.reset,
                    c.path, path, c.reset,
                    expected_type,
                    c.value, display_value, c.reset,
                    message).ok();
            }
            ConfigError::ValidationError { path, value, message, .. } => {
                let display_value = value.as_ref()
                    .map(|v| self.maybe_redact(v, path))
                    .unwrap_or_else(|| "[REDACTED]".to_string());
                writeln!(w, "    {}•{} '{}{}{}' = {}{}{}: {}",
                    c.error, c.reset,
                    c.path, path, c.reset,
                    c.value, display_value, c.reset,
                    message).ok();
            }
            ConfigError::CrossFieldError { paths, message } => {
                let paths_str = paths.iter()
                    .map(|p| format!("{}{}{}", c.path, p, c.reset))
                    .collect::<Vec<_>>()
                    .join(", ");
                writeln!(w, "    {}•{} [{}]: {}",
                    c.error, c.reset, paths_str, message).ok();
            }
            ConfigError::UnknownField { path, did_you_mean, .. } => {
                let mut msg = format!("    {}•{} unknown field '{}{}{}'",
                    c.warning, c.reset, c.path, path, c.reset);
                if let Some(suggestion) = did_you_mean {
                    msg.push_str(&format!("; did you mean '{}{}{}'?",
                        c.hint, suggestion, c.reset));
                }
                writeln!(w, "{}", msg).ok();
            }
            ConfigError::SourceError { source_name, kind } => {
                writeln!(w, "    {}•{} {}: {}",
                    c.error, c.reset, source_name, kind).ok();
            }
        }
    }

    fn print_suggestions(&self, errors: &[ConfigError], w: &mut dyn Write) {
        let c = &self.colors;
        let suggestions: Vec<_> = errors.iter()
            .filter_map(|e| e.suggestion())
            .take(3)
            .collect();

        if !suggestions.is_empty() {
            writeln!(w, "{}Hints:{}", c.hint, c.reset).ok();
            for suggestion in suggestions {
                writeln!(w, "  • {}", suggestion).ok();
            }
            writeln!(w).ok();
        }
    }

    fn maybe_redact(&self, value: &str, _path: &str) -> String {
        // TODO: Check if path is marked sensitive
        if self.options.redact_sensitive && is_sensitive_path(_path) {
            "[REDACTED]".to_string()
        } else {
            value.to_string()
        }
    }
}

fn is_sensitive_path(path: &str) -> bool {
    let sensitive_patterns = ["password", "secret", "key", "token", "credential"];
    let lower = path.to_lowercase();
    sensitive_patterns.iter().any(|p| lower.contains(p))
}
```

### Example Output

```
Configuration errors (4):

  config.toml:
    • missing required field 'database.host'
    • 'database.pool_size' = -5: value must be >= 1
    • 'server.timeout_seconds' = 999: value 999 is not in range 1..=300

  env:APP_DATABASE_PORT:
    • 'database.port': expected integer, got "abc": invalid digit

Hints:
  • Add 'database.host' to your configuration
  • Change 'database.pool_size' to a positive integer
```

### Convenience Methods

```rust
/// Trait extension for easy error handling
pub trait ValidationExt<T, E> {
    /// Unwrap or pretty print errors and exit
    fn unwrap_or_exit(self) -> T;

    /// Unwrap or pretty print errors and exit with custom options
    fn unwrap_or_exit_with(self, options: PrettyPrintOptions) -> T;
}

impl<T> ValidationExt<T, Vec<ConfigError>> for Validation<T, Vec<ConfigError>> {
    fn unwrap_or_exit(self) -> T {
        self.unwrap_or_exit_with(PrettyPrintOptions::default())
    }

    fn unwrap_or_exit_with(self, options: PrettyPrintOptions) -> T {
        match self {
            Validation::Success(value) => value,
            Validation::Failure(errors) => {
                ConfigError::pretty_print(&errors, options);
                std::process::exit(1);
            }
        }
    }
}
```

## Dependencies

- **Prerequisites**: Spec 002 (Error Types)
- **Affected Components**: User-facing error display
- **External Dependencies**:
  - `atty` or `is-terminal` for TTY detection (optional)

## Testing Strategy

- **Unit Tests**:
  - Formatting each error type
  - Grouping logic
  - Color stripping
  - Truncation
  - Redaction
- **Integration Tests**: Manual visual inspection with example configs

## Documentation Requirements

- **Code Documentation**: Doc comments with output examples
- **User Documentation**: Error message interpretation guide

## Implementation Notes

- Consider using `termcolor` or `colored` crate for cross-platform colors
- TTY detection should use `std::io::IsTerminal` (Rust 1.70+) or `atty` crate
- Keep output width under 80 chars when possible
- Consider structured output option (JSON) for tooling

## Migration and Compatibility

Not applicable - new project.
