//! Hot reload and file watching support.
//!
//! This module provides configuration hot-reloading through file watching.
//! When configuration files change, the new configuration is automatically
//! loaded, validated, and applied - keeping the old configuration if
//! validation fails.
//!
//! This module is only available with the `watch` feature enabled.
//!
//! # Example
//!
//! ```ignore
//! use premortem::{Config, Toml, Env};
//!
//! let (config, watcher) = Config::<AppConfig>::builder()
//!     .source(Toml::file("config.toml"))
//!     .source(Env::prefix("APP_"))
//!     .build_watched()
//!     .expect("Failed to load config");
//!
//! // Get current config (cheap Arc clone)
//! let current = config.current();
//! println!("Starting with config: {:?}", current);
//!
//! // Subscribe to changes
//! watcher.on_change(|event| {
//!     match event {
//!         ConfigEvent::Reloaded { changed_sources } => {
//!             println!("Config reloaded from: {:?}", changed_sources);
//!         }
//!         ConfigEvent::ReloadFailed { errors } => {
//!             eprintln!("Reload failed: {:?}", errors);
//!         }
//!         _ => {}
//!     }
//! });
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, RwLock};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::de::DeserializeOwned;

use crate::config::ConfigBuilder;
use crate::env::{ConfigEnv, RealEnv};
use crate::error::{ConfigError, ConfigErrors, SourceErrorKind};
use crate::source::Source;
use crate::validate::Validate;

/// Events emitted during configuration watching.
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    /// Configuration was successfully reloaded.
    Reloaded {
        /// Which sources changed.
        changed_sources: Vec<String>,
    },
    /// Configuration reload failed validation.
    ReloadFailed {
        /// Validation errors that prevented reload.
        errors: Vec<String>,
    },
    /// A source file changed (before reload attempt).
    SourceChanged {
        /// Path of the changed source.
        path: PathBuf,
    },
    /// Watcher encountered an error.
    WatchError {
        /// Error message.
        message: String,
    },
}

/// A sender for configuration events.
///
/// Uses mpsc channels to allow multiple receivers to subscribe.
#[derive(Clone)]
struct EventSender {
    senders: Arc<RwLock<Vec<mpsc::Sender<ConfigEvent>>>>,
}

impl EventSender {
    fn new() -> Self {
        Self {
            senders: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn send(&self, event: ConfigEvent) {
        let senders = self.senders.read().unwrap();
        // Send to all subscribers, removing closed channels
        for sender in senders.iter() {
            let _ = sender.send(event.clone());
        }
    }

    fn subscribe(&self) -> mpsc::Receiver<ConfigEvent> {
        let (tx, rx) = mpsc::channel();
        self.senders.write().unwrap().push(tx);
        rx
    }
}

/// Configuration that can be hot-reloaded.
///
/// Wraps the configuration in an Arc for cheap cloning and thread-safe access.
/// Use `current()` to get the latest configuration value.
pub struct WatchedConfig<T> {
    current: Arc<RwLock<Arc<T>>>,
}

impl<T> WatchedConfig<T> {
    /// Get the current configuration (cheap Arc clone).
    ///
    /// This returns an `Arc<T>` that can be cloned cheaply and used
    /// across threads. The underlying configuration is only replaced
    /// when a successful reload occurs.
    pub fn current(&self) -> Arc<T> {
        self.current.read().unwrap().clone()
    }
}

impl<T> Clone for WatchedConfig<T> {
    fn clone(&self) -> Self {
        Self {
            current: Arc::clone(&self.current),
        }
    }
}

/// Type-erased reload function.
type ReloadFn = Box<dyn Fn() -> Result<(), ConfigErrors> + Send + Sync>;

/// Watcher for configuration file changes.
///
/// Monitors configuration source files and triggers reloads when changes
/// are detected. Provides event subscription and manual reload capabilities.
pub struct ConfigWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    stop_signal: Arc<AtomicBool>,
    event_sender: EventSender,
    reload_fn: ReloadFn,
}

impl ConfigWatcher {
    /// Subscribe to configuration events.
    ///
    /// Returns a receiver that will receive all configuration events
    /// (reloads, failures, source changes, errors).
    pub fn subscribe(&self) -> mpsc::Receiver<ConfigEvent> {
        self.event_sender.subscribe()
    }

    /// Register a callback for configuration events.
    ///
    /// Spawns a thread that calls the callback for each event.
    /// The callback should be lightweight to avoid blocking event delivery.
    ///
    /// # Example
    ///
    /// ```ignore
    /// watcher.on_change(|event| {
    ///     match event {
    ///         ConfigEvent::Reloaded { .. } => println!("Config reloaded!"),
    ///         ConfigEvent::ReloadFailed { errors } => eprintln!("Reload failed: {:?}", errors),
    ///         _ => {}
    ///     }
    /// });
    /// ```
    pub fn on_change<F>(&self, callback: F)
    where
        F: Fn(ConfigEvent) + Send + 'static,
    {
        let rx = self.subscribe();
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                callback(event);
            }
        });
    }

    /// Stop watching for changes.
    ///
    /// After calling this, no more events will be emitted and the
    /// watcher thread will terminate.
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
    }

    /// Manually trigger a configuration reload.
    ///
    /// This reloads all configuration sources and validates the new
    /// configuration. If validation succeeds, the new configuration
    /// becomes active. If validation fails, the old configuration
    /// is preserved and errors are returned.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Configuration was successfully reloaded
    /// * `Err(ConfigErrors)` - Reload failed, old configuration preserved
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Trigger a manual reload
    /// match watcher.reload() {
    ///     Ok(()) => println!("Configuration reloaded successfully"),
    ///     Err(errors) => eprintln!("Reload failed: {:?}", errors),
    /// }
    /// ```
    pub fn reload(&self) -> Result<(), ConfigErrors> {
        (self.reload_fn)()
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Internal state for the reload handler.
struct ReloadState<T> {
    current: Arc<RwLock<Arc<T>>>,
    sources: Vec<Box<dyn Source>>,
    event_sender: EventSender,
    stop_signal: Arc<AtomicBool>,
    debounce_duration: Duration,
}

/// Shared reloader that performs the actual config reload.
///
/// This is used both by the background watcher thread and for manual reloads.
struct Reloader<T> {
    current: Arc<RwLock<Arc<T>>>,
    sources: Arc<Vec<Box<dyn Source>>>,
    event_sender: EventSender,
}

impl<T> Clone for Reloader<T> {
    fn clone(&self) -> Self {
        Self {
            current: Arc::clone(&self.current),
            sources: Arc::clone(&self.sources),
            event_sender: self.event_sender.clone(),
        }
    }
}

impl<T> Reloader<T>
where
    T: DeserializeOwned + Validate + Send + Sync + 'static,
{
    /// Perform a reload of the configuration.
    ///
    /// Returns `Ok(())` if the reload succeeded, or `Err` with the errors if it failed.
    /// On failure, the old configuration is preserved.
    fn reload(&self) -> Result<(), ConfigErrors> {
        let env = RealEnv::new();
        let mut builder = ConfigBuilder::<T>::new();
        for source in self.sources.iter() {
            builder = builder.source(SourceWrapper(source.clone_box()));
        }

        match builder.build_with_env(&env) {
            Ok(new_config) => {
                // Update config
                *self.current.write().unwrap() = Arc::new(new_config.into_inner());
                self.event_sender.send(ConfigEvent::Reloaded {
                    changed_sources: vec!["manual".to_string()],
                });
                Ok(())
            }
            Err(errors) => {
                // Keep old config, notify of failure
                let error_strings: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                self.event_sender.send(ConfigEvent::ReloadFailed {
                    errors: error_strings,
                });
                Err(errors)
            }
        }
    }
}

/// Build a watched configuration.
///
/// This is the main entry point for hot-reloadable configuration.
pub fn build_watched<T>(
    sources: Vec<Box<dyn Source>>,
    env: &dyn ConfigEnv,
) -> Result<(WatchedConfig<T>, ConfigWatcher), ConfigErrors>
where
    T: DeserializeOwned + Validate + Send + Sync + 'static,
{
    // Clone sources for reload handler - we need to keep copies
    let sources_for_reload: Vec<Box<dyn Source>> = sources.iter().map(|s| s.clone_box()).collect();

    // First, do initial build using the original sources
    let mut builder = ConfigBuilder::<T>::new();
    for source in sources {
        builder = builder.source(SourceWrapper(source));
    }

    let config = builder.build_with_env(env)?;

    // Create watched config wrapper
    let current = Arc::new(RwLock::new(Arc::new(config.into_inner())));
    let watched = WatchedConfig {
        current: Arc::clone(&current),
    };

    // Set up event sender
    let event_sender = EventSender::new();
    let stop_signal = Arc::new(AtomicBool::new(false));

    // Collect watchable paths from sources
    let watch_paths: Vec<PathBuf> = sources_for_reload
        .iter()
        .filter_map(|s| s.watch_path())
        .collect();

    // Create debounce channel
    let (debounce_tx, debounce_rx) = mpsc::channel();

    // Create file watcher
    let watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
            ) {
                for path in event.paths {
                    let _ = debounce_tx.send(path);
                }
            }
        }
    })
    .map_err(|e| {
        ConfigErrors::single(ConfigError::SourceError {
            source_name: "watcher".into(),
            kind: SourceErrorKind::Other {
                message: e.to_string(),
            },
        })
    })?;

    // Watch all paths
    let mut watcher = watcher;
    for path in &watch_paths {
        if path.exists() {
            if let Err(e) = watcher.watch(path, RecursiveMode::NonRecursive) {
                // Log but don't fail - the file might appear later
                event_sender.send(ConfigEvent::WatchError {
                    message: format!("Failed to watch {}: {}", path.display(), e),
                });
            }
        }
    }

    // Create shared reloader for both manual and automatic reloads
    let sources_arc = Arc::new(sources_for_reload);
    let reloader = Reloader {
        current: Arc::clone(&current),
        sources: Arc::clone(&sources_arc),
        event_sender: event_sender.clone(),
    };

    // Spawn reload handler thread
    let state = ReloadState {
        current,
        sources: sources_arc.iter().map(|s| s.clone_box()).collect(),
        event_sender: event_sender.clone(),
        stop_signal: Arc::clone(&stop_signal),
        debounce_duration: Duration::from_millis(100),
    };

    spawn_reload_handler(state, debounce_rx);

    // Create the reload function for manual reloads
    let reload_fn: ReloadFn = Box::new(move || reloader.reload());

    let config_watcher = ConfigWatcher {
        watcher,
        stop_signal,
        event_sender,
        reload_fn,
    };

    Ok((watched, config_watcher))
}

/// Spawn the reload handler thread.
fn spawn_reload_handler<T>(state: ReloadState<T>, debounce_rx: mpsc::Receiver<PathBuf>)
where
    T: DeserializeOwned + Validate + Send + Sync + 'static,
{
    std::thread::spawn(move || {
        let mut last_reload = Instant::now();

        loop {
            if state.stop_signal.load(Ordering::SeqCst) {
                break;
            }

            match debounce_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(path) => {
                    // Debounce: ignore if too recent
                    if last_reload.elapsed() < state.debounce_duration {
                        continue;
                    }

                    // Notify source changed
                    state
                        .event_sender
                        .send(ConfigEvent::SourceChanged { path: path.clone() });

                    // Attempt reload
                    let env = RealEnv::new();
                    let mut builder = ConfigBuilder::<T>::new();
                    for source in &state.sources {
                        builder = builder.source(SourceWrapper(source.clone_box()));
                    }

                    match builder.build_with_env(&env) {
                        Ok(new_config) => {
                            // Update config
                            *state.current.write().unwrap() = Arc::new(new_config.into_inner());
                            state.event_sender.send(ConfigEvent::Reloaded {
                                changed_sources: vec![path.display().to_string()],
                            });
                        }
                        Err(errors) => {
                            // Keep old config, notify of failure
                            let error_strings: Vec<String> =
                                errors.iter().map(|e| e.to_string()).collect();
                            state.event_sender.send(ConfigEvent::ReloadFailed {
                                errors: error_strings,
                            });
                        }
                    }

                    last_reload = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

/// Wrapper to allow using boxed sources.
struct SourceWrapper(Box<dyn Source>);

impl Source for SourceWrapper {
    fn load(&self, env: &dyn ConfigEnv) -> Result<crate::source::ConfigValues, ConfigErrors> {
        self.0.load(env)
    }

    fn name(&self) -> &str {
        self.0.name()
    }

    fn watch_path(&self) -> Option<PathBuf> {
        self.0.watch_path()
    }

    fn clone_box(&self) -> Box<dyn Source> {
        self.0.clone_box()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test config struct
    #[derive(Debug, Clone, serde::Deserialize)]
    struct TestConfig {
        host: String,
        port: i64,
    }

    #[test]
    fn test_config_event_debug() {
        let event = ConfigEvent::Reloaded {
            changed_sources: vec!["config.toml".to_string()],
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("Reloaded"));
        assert!(debug.contains("config.toml"));
    }

    #[test]
    fn test_config_event_clone() {
        let event = ConfigEvent::SourceChanged {
            path: PathBuf::from("/test/config.toml"),
        };
        let cloned = event.clone();
        match cloned {
            ConfigEvent::SourceChanged { path } => {
                assert_eq!(path, PathBuf::from("/test/config.toml"));
            }
            _ => panic!("Expected SourceChanged"),
        }
    }

    #[test]
    fn test_watched_config_current() {
        let config = TestConfig {
            host: "localhost".to_string(),
            port: 8080,
        };
        let watched = WatchedConfig {
            current: Arc::new(RwLock::new(Arc::new(config))),
        };

        let current = watched.current();
        assert_eq!(current.host, "localhost");
        assert_eq!(current.port, 8080);
    }

    #[test]
    fn test_watched_config_clone() {
        let config = TestConfig {
            host: "localhost".to_string(),
            port: 8080,
        };
        let watched = WatchedConfig {
            current: Arc::new(RwLock::new(Arc::new(config))),
        };

        let cloned = watched.clone();
        assert_eq!(cloned.current().host, "localhost");
    }

    #[test]
    fn test_event_sender_subscribe() {
        let sender = EventSender::new();
        let rx = sender.subscribe();

        sender.send(ConfigEvent::Reloaded {
            changed_sources: vec!["test.toml".to_string()],
        });

        let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        match event {
            ConfigEvent::Reloaded { changed_sources } => {
                assert_eq!(changed_sources, vec!["test.toml"]);
            }
            _ => panic!("Expected Reloaded event"),
        }
    }

    #[test]
    fn test_event_sender_multiple_subscribers() {
        let sender = EventSender::new();
        let rx1 = sender.subscribe();
        let rx2 = sender.subscribe();

        sender.send(ConfigEvent::WatchError {
            message: "test error".to_string(),
        });

        // Both receivers should get the event
        let event1 = rx1.recv_timeout(Duration::from_millis(100)).unwrap();
        let event2 = rx2.recv_timeout(Duration::from_millis(100)).unwrap();

        match event1 {
            ConfigEvent::WatchError { message } => assert_eq!(message, "test error"),
            _ => panic!("Expected WatchError"),
        }
        match event2 {
            ConfigEvent::WatchError { message } => assert_eq!(message, "test error"),
            _ => panic!("Expected WatchError"),
        }
    }

    #[test]
    fn test_reloader_success() {
        use crate::sources::Defaults;
        use crate::validate::Validate;
        use serde::Serialize;
        use stillwater::Validation;

        // Make TestConfig serializable for Defaults
        #[derive(Debug, Clone, serde::Deserialize, Serialize)]
        struct SerializableConfig {
            host: String,
            port: i64,
        }

        impl Validate for SerializableConfig {
            fn validate(&self) -> crate::ConfigValidation<()> {
                Validation::Success(())
            }
        }

        // Create a Defaults source
        let defaults = Defaults::from(SerializableConfig {
            host: "localhost".to_string(),
            port: 8080,
        });
        let sources: Vec<Box<dyn Source>> = vec![Box::new(defaults)];

        // Create the shared state
        let config = SerializableConfig {
            host: "initial".to_string(),
            port: 0,
        };
        let current = Arc::new(RwLock::new(Arc::new(config)));
        let event_sender = EventSender::new();
        let rx = event_sender.subscribe();

        let reloader: Reloader<SerializableConfig> = Reloader {
            current: Arc::clone(&current),
            sources: Arc::new(sources),
            event_sender,
        };

        // Perform reload
        let result = reloader.reload();
        assert!(result.is_ok());

        // Check that config was updated
        let new_config = current.read().unwrap().clone();
        assert_eq!(new_config.host, "localhost");
        assert_eq!(new_config.port, 8080);

        // Check that event was sent
        let event = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        match event {
            ConfigEvent::Reloaded { changed_sources } => {
                assert_eq!(changed_sources, vec!["manual"]);
            }
            _ => panic!("Expected Reloaded event"),
        }
    }

    #[test]
    fn test_build_watched_with_file_change() {
        use crate::sources::Toml;
        use crate::validate::Validate;
        use crate::Config;
        use std::io::Write;
        use stillwater::Validation;

        #[derive(Debug, Clone, serde::Deserialize)]
        struct WatchTestConfig {
            host: String,
            port: i64,
        }

        impl Validate for WatchTestConfig {
            fn validate(&self) -> crate::ConfigValidation<()> {
                Validation::Success(())
            }
        }

        // Create temp directory and config file
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");

        // Write initial config
        let initial_config = r#"
host = "localhost"
port = 8080
"#;
        std::fs::write(&config_path, initial_config).expect("Failed to write initial config");

        // Build watched config
        let (watched, watcher) = Config::<WatchTestConfig>::builder()
            .source(Toml::file(&config_path))
            .build_watched()
            .expect("Failed to build watched config");

        // Verify initial values
        let current = watched.current();
        assert_eq!(current.host, "localhost");
        assert_eq!(current.port, 8080);

        // Subscribe to events
        let rx = watcher.subscribe();

        // Wait a bit for the watcher to be fully set up
        std::thread::sleep(Duration::from_millis(100));

        // Modify the config file
        let updated_config = r#"
host = "127.0.0.1"
port = 9000
"#;
        // Use atomic write pattern: write to temp then rename
        let temp_path = config_path.with_extension("tmp");
        {
            let mut file = std::fs::File::create(&temp_path).expect("Failed to create temp config");
            file.write_all(updated_config.as_bytes())
                .expect("Failed to write");
            file.sync_all().expect("Failed to sync");
        }
        std::fs::rename(&temp_path, &config_path).expect("Failed to rename");

        // Wait for reload event with timeout
        let mut reloaded = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);

        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(ConfigEvent::Reloaded { .. }) => {
                    reloaded = true;
                    break;
                }
                Ok(ConfigEvent::SourceChanged { .. }) => {
                    // File change detected, continue waiting for reload
                    continue;
                }
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        assert!(reloaded, "Config should have been reloaded");

        // Verify config was updated
        let updated = watched.current();
        assert_eq!(updated.host, "127.0.0.1");
        assert_eq!(updated.port, 9000);

        // Stop the watcher
        watcher.stop();
    }

    #[test]
    fn test_build_watched_reload_failure_keeps_old_config() {
        use crate::error::ConfigValidationExt;
        use crate::sources::Toml;
        use crate::validate::Validate;
        use crate::Config;
        use std::io::Write;
        use stillwater::Validation;

        #[derive(Debug, Clone, serde::Deserialize)]
        struct ValidatedConfig {
            host: String,
            port: i64,
        }

        impl Validate for ValidatedConfig {
            fn validate(&self) -> crate::ConfigValidation<()> {
                if self.port > 0 {
                    Validation::Success(())
                } else {
                    Validation::fail_with(crate::ConfigError::ValidationError {
                        path: "port".to_string(),
                        source_location: None,
                        value: Some(self.port.to_string()),
                        message: "port must be positive".to_string(),
                    })
                }
            }
        }

        // Create temp directory and config file
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("config.toml");

        // Write initial valid config
        let initial_config = r#"
host = "localhost"
port = 8080
"#;
        std::fs::write(&config_path, initial_config).expect("Failed to write initial config");

        // Build watched config
        let (watched, watcher) = Config::<ValidatedConfig>::builder()
            .source(Toml::file(&config_path))
            .build_watched()
            .expect("Failed to build watched config");

        // Subscribe to events
        let rx = watcher.subscribe();

        // Wait a bit for the watcher to be fully set up
        std::thread::sleep(Duration::from_millis(100));

        // Write invalid config (port = 0 fails validation)
        let invalid_config = r#"
host = "invalid-host"
port = 0
"#;
        let temp_path = config_path.with_extension("tmp");
        {
            let mut file = std::fs::File::create(&temp_path).expect("Failed to create temp config");
            file.write_all(invalid_config.as_bytes())
                .expect("Failed to write");
            file.sync_all().expect("Failed to sync");
        }
        std::fs::rename(&temp_path, &config_path).expect("Failed to rename");

        // Wait for reload failure event
        let mut reload_failed = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);

        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(ConfigEvent::ReloadFailed { errors }) => {
                    assert!(!errors.is_empty(), "Should have validation errors");
                    reload_failed = true;
                    break;
                }
                Ok(_) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        assert!(reload_failed, "Should have received ReloadFailed event");

        // Verify OLD config is still in place (not the invalid one)
        let current = watched.current();
        assert_eq!(current.host, "localhost", "Should keep old host");
        assert_eq!(current.port, 8080, "Should keep old port");

        watcher.stop();
    }
}
