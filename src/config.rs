//! Core Config type and ConfigBuilder.
//!
//! This module provides the main entry point for configuring and loading
//! application configuration using the builder pattern.

use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use stillwater::Validation;

use crate::env::{ConfigEnv, RealEnv};
use crate::error::{ConfigError, ConfigErrors};
use crate::source::{merge_config_values, ConfigValues, Source};
use crate::trace::{TraceBuilder, TracedConfig};
use crate::validate::{with_validation_context, Validate, ValidationContext};

/// Wrapper around a validated configuration value.
///
/// This type ensures that the configuration has been loaded and validated.
/// It implements `Deref` to provide transparent access to the inner type.
#[derive(Debug, Clone)]
pub struct Config<T> {
    inner: T,
}

impl<T> Config<T> {
    /// Create a new Config wrapping an already-validated value.
    ///
    /// This is primarily used internally after successful validation.
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    /// Get a reference to the inner configuration value.
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Consume this Config and return the inner value.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Create a builder for this configuration type.
    pub fn builder() -> ConfigBuilder<T> {
        ConfigBuilder::new()
    }
}

impl<T> std::ops::Deref for Config<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> AsRef<T> for Config<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

/// Builder for loading and validating configuration.
///
/// # Example
///
/// ```ignore
/// use premortem::Config;
///
/// let config = Config::<AppConfig>::builder()
///     .source(Toml::file("config.toml"))
///     .source(Env::new().prefix("APP"))
///     .build()
///     .expect("Failed to load config");
/// ```
pub struct ConfigBuilder<T> {
    sources: Vec<Box<dyn Source>>,
    _marker: PhantomData<T>,
}

impl<T> Default for ConfigBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ConfigBuilder<T> {
    /// Create a new empty config builder.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Add a configuration source.
    ///
    /// Sources are applied in order, with later sources overriding earlier ones.
    pub fn source<S: Source + 'static>(mut self, source: S) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Build the configuration using the real environment.
    ///
    /// This is the main entry point for production use.
    pub fn build(self) -> Result<Config<T>, ConfigErrors>
    where
        T: DeserializeOwned + Validate,
    {
        self.build_with_env(&RealEnv::new())
    }

    /// Build the configuration with a custom environment.
    ///
    /// This enables dependency injection for testing.
    pub fn build_with_env(self, env: &dyn ConfigEnv) -> Result<Config<T>, ConfigErrors>
    where
        T: DeserializeOwned + Validate,
    {
        // Check for empty sources first (pure validation)
        if self.sources.is_empty() {
            return Err(ConfigErrors::single(ConfigError::NoSources));
        }

        // Collect source names for error messages
        let source_names: Vec<String> = self.sources.iter().map(|s| s.name().to_string()).collect();

        // Load from all sources, accumulating errors
        let mut all_values = Vec::with_capacity(self.sources.len());
        let mut all_errors = Vec::new();

        for source in &self.sources {
            match source.load(env) {
                Ok(values) => all_values.push(values),
                Err(errors) => all_errors.extend(errors.into_iter()),
            }
        }

        // If any source failed, return all errors
        if !all_errors.is_empty() {
            return Err(ConfigErrors::from_vec(all_errors).unwrap());
        }

        // Merge all values (pure function)
        let merged = merge_config_values(all_values);

        // Build source location map from merged values for validation context
        let locations = merged
            .iter()
            .map(|(path, cv)| (path.clone(), cv.source.clone()))
            .collect();

        // Deserialize (pure function)
        let config = deserialize_config::<T>(&merged, &source_names)?;

        // Validate with context (source locations available for error messages)
        let ctx = ValidationContext::new(locations);
        let validation_result = with_validation_context(ctx, || config.validate());

        match validation_result {
            Validation::Success(()) => Ok(Config::new(config)),
            Validation::Failure(errors) => Err(errors),
        }
    }

    /// Build the configuration with value tracing enabled.
    ///
    /// This is like `build()` but also collects tracing information
    /// showing where each configuration value came from and its
    /// override history across sources.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let traced = Config::<AppConfig>::builder()
    ///     .source(Defaults::from(AppConfig::default()))
    ///     .source(Toml::file("config.toml"))
    ///     .source(Env::prefix("APP_"))
    ///     .build_traced()?;
    ///
    /// // Check where a value came from
    /// if let Some(trace) = traced.trace("database.host") {
    ///     println!("From: {}", trace.final_value.source);
    /// }
    ///
    /// // Generate a debug report
    /// println!("{}", traced.trace_report());
    /// ```
    pub fn build_traced(self) -> Result<TracedConfig<T>, ConfigErrors>
    where
        T: DeserializeOwned + Validate,
    {
        self.build_traced_with_env(&RealEnv::new())
    }

    /// Build the configuration with file watching enabled.
    ///
    /// This enables hot-reloading of configuration when source files change.
    /// The returned `WatchedConfig` provides thread-safe access to the current
    /// configuration, and the `ConfigWatcher` allows subscribing to change events.
    ///
    /// Only available with the `watch` feature enabled.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (config, watcher) = Config::<AppConfig>::builder()
    ///     .source(Toml::file("config.toml"))
    ///     .source(Env::prefix("APP_"))
    ///     .build_watched()?;
    ///
    /// // Get current config (cheap Arc clone)
    /// let current = config.current();
    ///
    /// // Subscribe to changes
    /// watcher.on_change(|event| {
    ///     match event {
    ///         ConfigEvent::Reloaded { .. } => println!("Config reloaded!"),
    ///         ConfigEvent::ReloadFailed { errors } => eprintln!("Reload failed: {:?}", errors),
    ///         _ => {}
    ///     }
    /// });
    /// ```
    #[cfg(feature = "watch")]
    pub fn build_watched(
        self,
    ) -> Result<(crate::watch::WatchedConfig<T>, crate::watch::ConfigWatcher), ConfigErrors>
    where
        T: DeserializeOwned + Validate + Send + Sync + 'static,
    {
        self.build_watched_with_env(&RealEnv::new())
    }

    /// Build the configuration with file watching using a custom environment.
    ///
    /// This enables dependency injection for testing watched builds.
    #[cfg(feature = "watch")]
    pub fn build_watched_with_env(
        self,
        env: &dyn ConfigEnv,
    ) -> Result<(crate::watch::WatchedConfig<T>, crate::watch::ConfigWatcher), ConfigErrors>
    where
        T: DeserializeOwned + Validate + Send + Sync + 'static,
    {
        // Check for empty sources first
        if self.sources.is_empty() {
            return Err(ConfigErrors::single(ConfigError::NoSources));
        }

        crate::watch::build_watched(self.sources, env)
    }

    /// Build the configuration with tracing using a custom environment.
    ///
    /// This enables dependency injection for testing traced builds.
    pub fn build_traced_with_env(self, env: &dyn ConfigEnv) -> Result<TracedConfig<T>, ConfigErrors>
    where
        T: DeserializeOwned + Validate,
    {
        // Check for empty sources first (pure validation)
        if self.sources.is_empty() {
            return Err(ConfigErrors::single(ConfigError::NoSources));
        }

        // Collect source names for error messages
        let source_names: Vec<String> = self.sources.iter().map(|s| s.name().to_string()).collect();

        // Load from all sources, accumulating errors and trace data
        let mut all_values = Vec::with_capacity(self.sources.len());
        let mut all_errors = Vec::new();
        let mut trace_builder = TraceBuilder::new();

        for source in &self.sources {
            match source.load(env) {
                Ok(values) => {
                    // Record trace data for each value
                    for (path, config_value) in values.iter() {
                        trace_builder.add_value(
                            path.clone(),
                            config_value.value.clone(),
                            config_value.source.clone(),
                        );
                    }
                    all_values.push(values);
                }
                Err(errors) => all_errors.extend(errors.into_iter()),
            }
        }

        // If any source failed, return all errors
        if !all_errors.is_empty() {
            return Err(ConfigErrors::from_vec(all_errors).unwrap());
        }

        // Merge all values (pure function)
        let merged = merge_config_values(all_values);

        // Build source location map from merged values for validation context
        let locations = merged
            .iter()
            .map(|(path, cv)| (path.clone(), cv.source.clone()))
            .collect();

        // Deserialize (pure function)
        let config = deserialize_config::<T>(&merged, &source_names)?;

        // Validate with context (source locations available for error messages)
        let ctx = ValidationContext::new(locations);
        let validation_result = with_validation_context(ctx, || config.validate());

        match validation_result {
            Validation::Success(()) => {
                let traces = trace_builder.build();
                Ok(TracedConfig::new(Config::new(config), traces))
            }
            Validation::Failure(errors) => Err(errors),
        }
    }
}

/// Pure function: deserialize ConfigValues into target type T.
fn deserialize_config<T: DeserializeOwned>(
    values: &ConfigValues,
    source_names: &[String],
) -> Result<T, ConfigErrors> {
    // Convert ConfigValues to JSON for serde deserialization
    let json_value = values.to_json();

    serde_json::from_value(json_value).map_err(|e| {
        // Try to extract the path from the serde error
        let message = e.to_string();

        // Check if this is a missing field error
        if message.contains("missing field") {
            // Extract field name from error message
            if let Some(start) = message.find('`') {
                if let Some(end) = message[start + 1..].find('`') {
                    let field = &message[start + 1..start + 1 + end];
                    return ConfigErrors::single(ConfigError::MissingField {
                        path: field.to_string(),
                        source_location: None,
                        searched_sources: source_names.to_vec(),
                    });
                }
            }
        }

        ConfigErrors::single(ConfigError::ParseError {
            path: "(root)".to_string(),
            source_location: crate::error::SourceLocation::new("merged config"),
            expected_type: std::any::type_name::<T>().to_string(),
            actual_value: "(complex)".to_string(),
            message,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MockEnv;
    use crate::error::{ConfigValidation, SourceLocation};
    use crate::value::Value;

    // A simple test source that returns static values
    #[derive(Clone)]
    struct StaticSource {
        name: String,
        values: ConfigValues,
    }

    impl StaticSource {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                values: ConfigValues::default(),
            }
        }

        fn with_value(mut self, path: &str, value: impl Into<Value>) -> Self {
            self.values.insert(
                path.to_string(),
                crate::value::ConfigValue::new(value, SourceLocation::new(&self.name)),
            );
            self
        }
    }

    impl Source for StaticSource {
        fn load(&self, _env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
            Ok(self.values.clone())
        }

        fn name(&self) -> &str {
            &self.name
        }

        #[cfg(feature = "watch")]
        fn watch_path(&self) -> Option<std::path::PathBuf> {
            None
        }

        #[cfg(feature = "watch")]
        fn clone_box(&self) -> Box<dyn Source> {
            Box::new(self.clone())
        }
    }

    // Simple config struct for testing
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct SimpleConfig {
        host: String,
        port: i64,
    }

    impl Validate for SimpleConfig {
        fn validate(&self) -> ConfigValidation<()> {
            Validation::Success(())
        }
    }

    #[test]
    fn test_config_builder_no_sources() {
        let result = Config::<SimpleConfig>::builder().build();
        assert!(result.is_err());

        if let Err(errors) = result {
            assert!(matches!(errors.first(), ConfigError::NoSources));
        }
    }

    #[test]
    fn test_config_builder_single_source() {
        let source = StaticSource::new("test")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source)
            .build_with_env(&env);

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_config_builder_merging() {
        let source1 = StaticSource::new("base")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let source2 = StaticSource::new("override").with_value("host", "production.example.com");

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .build_with_env(&env);

        assert!(result.is_ok());
        let config = result.unwrap();
        // source2 should override host
        assert_eq!(config.host, "production.example.com");
        // port should remain from source1
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_config_deref() {
        let config = Config::new(SimpleConfig {
            host: "test".to_string(),
            port: 80,
        });

        // Test Deref
        assert_eq!(config.host, "test");
        assert_eq!(config.port, 80);

        // Test get()
        assert_eq!(config.get().host, "test");

        // Test into_inner()
        let inner = config.into_inner();
        assert_eq!(inner.host, "test");
    }

    // Test with validation
    #[derive(Debug, serde::Deserialize)]
    struct ValidatedConfig {
        port: i64,
    }

    impl Validate for ValidatedConfig {
        fn validate(&self) -> ConfigValidation<()> {
            if self.port > 0 && self.port < 65536 {
                Validation::Success(())
            } else {
                Validation::Failure(ConfigErrors::single(ConfigError::ValidationError {
                    path: "port".to_string(),
                    source_location: None,
                    value: Some(self.port.to_string()),
                    message: "port must be between 1 and 65535".to_string(),
                }))
            }
        }
    }

    #[test]
    fn test_config_validation_pass() {
        let source = StaticSource::new("test").with_value("port", 8080i64);

        let env = MockEnv::new();
        let result = Config::<ValidatedConfig>::builder()
            .source(source)
            .build_with_env(&env);

        assert!(result.is_ok());
    }

    #[test]
    fn test_config_validation_fail() {
        let source = StaticSource::new("test").with_value("port", 70000i64);

        let env = MockEnv::new();
        let result = Config::<ValidatedConfig>::builder()
            .source(source)
            .build_with_env(&env);

        assert!(result.is_err());
        if let Err(errors) = result {
            assert!(matches!(
                errors.first(),
                ConfigError::ValidationError { .. }
            ));
        }
    }

    // Test error accumulation from multiple sources
    #[derive(Clone)]
    struct FailingSource {
        name: String,
    }

    impl FailingSource {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl Source for FailingSource {
        fn load(&self, _env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
            use crate::error::SourceErrorKind;
            Err(ConfigErrors::single(ConfigError::SourceError {
                source_name: self.name.clone(),
                kind: SourceErrorKind::NotFound {
                    path: format!("{}.toml", self.name),
                },
            }))
        }

        fn name(&self) -> &str {
            &self.name
        }

        #[cfg(feature = "watch")]
        fn watch_path(&self) -> Option<std::path::PathBuf> {
            None
        }

        #[cfg(feature = "watch")]
        fn clone_box(&self) -> Box<dyn Source> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn test_error_accumulation_from_sources() {
        let source1 = FailingSource::new("source1");
        let source2 = FailingSource::new("source2");

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .build_with_env(&env);

        assert!(result.is_err());
        if let Err(errors) = result {
            // Both source errors should be accumulated
            assert_eq!(errors.len(), 2);
        }
    }

    #[test]
    fn test_missing_field_error() {
        let source = StaticSource::new("test").with_value("host", "localhost");
        // Missing "port" field

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source)
            .build_with_env(&env);

        assert!(result.is_err());
        if let Err(errors) = result {
            assert!(matches!(
                errors.first(),
                ConfigError::MissingField { path, .. } if path == "port"
            ));
        }
    }

    // ========== build_traced() tests ==========

    #[test]
    fn test_build_traced_single_source() {
        let source = StaticSource::new("test")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source)
            .build_traced_with_env(&env);

        assert!(result.is_ok());
        let traced = result.unwrap();

        // Check config values
        assert_eq!(traced.host, "localhost");
        assert_eq!(traced.port, 8080);

        // Check traces exist
        assert!(traced.trace("host").is_some());
        assert!(traced.trace("port").is_some());

        // Single source means no overrides
        assert!(!traced.was_overridden("host"));
        assert!(!traced.was_overridden("port"));
    }

    #[test]
    fn test_build_traced_multiple_sources_with_override() {
        let source1 = StaticSource::new("defaults")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let source2 = StaticSource::new("override").with_value("host", "production.example.com");

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .build_traced_with_env(&env);

        assert!(result.is_ok());
        let traced = result.unwrap();

        // Check final values
        assert_eq!(traced.host, "production.example.com");
        assert_eq!(traced.port, 8080);

        // host was overridden, port was not
        assert!(traced.was_overridden("host"));
        assert!(!traced.was_overridden("port"));

        // Check trace history for host
        let host_trace = traced.trace("host").unwrap();
        assert_eq!(host_trace.history.len(), 2);
        assert_eq!(
            host_trace.final_value.value.as_str(),
            Some("production.example.com")
        );
        assert_eq!(host_trace.final_value.source.source, "override");
    }

    #[test]
    fn test_build_traced_trace_report() {
        let source1 = StaticSource::new("defaults")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let source2 = StaticSource::new("env").with_value("host", "prod-server");

        let env = MockEnv::new();
        let traced = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .build_traced_with_env(&env)
            .unwrap();

        let report = traced.trace_report();

        // Report should contain path names
        assert!(report.contains("host"));
        assert!(report.contains("port"));

        // Report should show sources
        assert!(report.contains("defaults"));
        assert!(report.contains("env"));

        // Report should show override markers
        assert!(report.contains("overridden"));
    }

    #[test]
    fn test_build_traced_overridden_paths() {
        let source1 = StaticSource::new("defaults")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let source2 = StaticSource::new("env").with_value("host", "prod-server");

        let env = MockEnv::new();
        let traced = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .build_traced_with_env(&env)
            .unwrap();

        let overridden: Vec<&str> = traced.overridden_paths().collect();
        assert_eq!(overridden.len(), 1);
        assert!(overridden.contains(&"host"));
    }

    #[test]
    fn test_build_traced_no_sources_error() {
        let result = Config::<SimpleConfig>::builder().build_traced();

        assert!(result.is_err());
        if let Err(errors) = result {
            assert!(matches!(errors.first(), ConfigError::NoSources));
        }
    }

    #[test]
    fn test_build_traced_validation_failure() {
        let source = StaticSource::new("test").with_value("port", 70000i64);

        let env = MockEnv::new();
        let result = Config::<ValidatedConfig>::builder()
            .source(source)
            .build_traced_with_env(&env);

        assert!(result.is_err());
        if let Err(errors) = result {
            assert!(matches!(
                errors.first(),
                ConfigError::ValidationError { .. }
            ));
        }
    }

    #[test]
    fn test_build_traced_source_error() {
        let source = FailingSource::new("bad_source");

        let env = MockEnv::new();
        let result = Config::<SimpleConfig>::builder()
            .source(source)
            .build_traced_with_env(&env);

        assert!(result.is_err());
        if let Err(errors) = result {
            assert!(matches!(errors.first(), ConfigError::SourceError { .. }));
        }
    }

    #[test]
    fn test_build_traced_into_inner() {
        let source = StaticSource::new("test")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let env = MockEnv::new();
        let traced = Config::<SimpleConfig>::builder()
            .source(source)
            .build_traced_with_env(&env)
            .unwrap();

        let inner = traced.into_inner();
        assert_eq!(inner.host, "localhost");
        assert_eq!(inner.port, 8080);
    }

    #[test]
    fn test_build_traced_into_config() {
        let source = StaticSource::new("test")
            .with_value("host", "localhost")
            .with_value("port", 8080i64);

        let env = MockEnv::new();
        let traced = Config::<SimpleConfig>::builder()
            .source(source)
            .build_traced_with_env(&env)
            .unwrap();

        let config = traced.into_config();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 8080);
    }

    #[test]
    fn test_build_traced_three_sources() {
        let source1 = StaticSource::new("defaults")
            .with_value("host", "localhost")
            .with_value("port", 80i64);

        let source2 = StaticSource::new("config")
            .with_value("host", "staging")
            .with_value("port", 8080i64);

        let source3 = StaticSource::new("env").with_value("host", "prod");

        let env = MockEnv::new();
        let traced = Config::<SimpleConfig>::builder()
            .source(source1)
            .source(source2)
            .source(source3)
            .build_traced_with_env(&env)
            .unwrap();

        // Final values
        assert_eq!(traced.host, "prod");
        assert_eq!(traced.port, 8080);

        // host trace should have 3 entries
        let host_trace = traced.trace("host").unwrap();
        assert_eq!(host_trace.history.len(), 3);
        assert_eq!(host_trace.history[0].source.source, "defaults");
        assert_eq!(host_trace.history[1].source.source, "config");
        assert_eq!(host_trace.history[2].source.source, "env");

        // port trace should have 2 entries
        let port_trace = traced.trace("port").unwrap();
        assert_eq!(port_trace.history.len(), 2);
    }

    // ========== Array reconstruction tests ==========

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct ConfigWithVec {
        name: String,
        hosts: Vec<String>,
        ports: Vec<u16>,
    }

    impl Validate for ConfigWithVec {
        fn validate(&self) -> ConfigValidation<()> {
            Validation::Success(())
        }
    }

    #[test]
    fn test_defaults_with_vec_roundtrip() {
        use crate::sources::Defaults;

        let original = ConfigWithVec {
            name: "test-app".to_string(),
            hosts: vec![
                "host1".to_string(),
                "host2".to_string(),
                "host3".to_string(),
            ],
            ports: vec![8080, 8081],
        };

        let env = MockEnv::new();
        let result = Config::<ConfigWithVec>::builder()
            .source(Defaults::from(original.clone()))
            .build_with_env(&env);

        assert!(result.is_ok(), "Build failed: {:?}", result.err());
        let config = result.unwrap();

        assert_eq!(config.name, "test-app");
        assert_eq!(config.hosts, vec!["host1", "host2", "host3"]);
        assert_eq!(config.ports, vec![8080, 8081]);
    }

    #[test]
    fn test_defaults_with_empty_vec_roundtrip() {
        use crate::sources::Defaults;

        let original = ConfigWithVec {
            name: "empty-app".to_string(),
            hosts: vec![],
            ports: vec![],
        };

        let env = MockEnv::new();
        let result = Config::<ConfigWithVec>::builder()
            .source(Defaults::from(original.clone()))
            .build_with_env(&env);

        assert!(result.is_ok(), "Build failed: {:?}", result.err());
        let config = result.unwrap();

        assert_eq!(config.name, "empty-app");
        assert!(config.hosts.is_empty());
        assert!(config.ports.is_empty());
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct ServerEntry {
        host: String,
        port: u16,
    }

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
    struct ConfigWithNestedVec {
        name: String,
        servers: Vec<ServerEntry>,
    }

    impl Validate for ConfigWithNestedVec {
        fn validate(&self) -> ConfigValidation<()> {
            Validation::Success(())
        }
    }

    #[test]
    fn test_defaults_with_vec_of_structs_roundtrip() {
        use crate::sources::Defaults;

        let original = ConfigWithNestedVec {
            name: "multi-server".to_string(),
            servers: vec![
                ServerEntry {
                    host: "server1.example.com".to_string(),
                    port: 8080,
                },
                ServerEntry {
                    host: "server2.example.com".to_string(),
                    port: 8081,
                },
            ],
        };

        let env = MockEnv::new();
        let result = Config::<ConfigWithNestedVec>::builder()
            .source(Defaults::from(original.clone()))
            .build_with_env(&env);

        assert!(result.is_ok(), "Build failed: {:?}", result.err());
        let config = result.unwrap();

        assert_eq!(config.name, "multi-server");
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].host, "server1.example.com");
        assert_eq!(config.servers[0].port, 8080);
        assert_eq!(config.servers[1].host, "server2.example.com");
        assert_eq!(config.servers[1].port, 8081);
    }

    #[test]
    fn test_defaults_with_vec_merged_with_toml() {
        use crate::sources::{Defaults, Toml};

        let defaults = ConfigWithVec {
            name: "default-app".to_string(),
            hosts: vec!["default-host".to_string()],
            ports: vec![80],
        };

        let toml_content = r#"
            name = "overridden-app"
            hosts = ["host1", "host2"]
            ports = [8080, 8081, 8082]
        "#;

        let env = MockEnv::new().with_file("config.toml", toml_content);
        let result = Config::<ConfigWithVec>::builder()
            .source(Defaults::from(defaults))
            .source(Toml::file("config.toml"))
            .build_with_env(&env);

        assert!(result.is_ok(), "Build failed: {:?}", result.err());
        let config = result.unwrap();

        // TOML should override defaults
        assert_eq!(config.name, "overridden-app");
        assert_eq!(config.hosts, vec!["host1", "host2"]);
        assert_eq!(config.ports, vec![8080, 8081, 8082]);
    }
}
