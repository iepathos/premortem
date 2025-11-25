---
number: 10
title: Hot Reload and File Watching
category: optimization
priority: low
status: draft
dependencies: [1, 2, 3, 5]
created: 2025-11-25
---

# Specification 010: Hot Reload and File Watching

**Category**: optimization
**Priority**: low
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types, 003 - Validate Trait, 005 - TOML Source]

## Context

Long-running applications benefit from the ability to reload configuration without restarting. The hot reload feature watches configuration files for changes and attempts to reload and validate the new configuration, keeping the old configuration if validation fails.

This is an optional feature gated behind the `watch` feature flag.

## Objective

Implement a file watching system that detects configuration changes, reloads configuration sources, validates the new configuration, and notifies the application of successful reloads or failures.

## Requirements

### Functional Requirements

1. **File Watching**: Monitor file sources for changes
2. **Automatic Reload**: Reload on file modification
3. **Validation on Reload**: Validate new config before applying
4. **Fallback**: Keep old config if new config invalid
5. **Event Notification**: Notify application of reload events
6. **Manual Reload**: API for triggering manual reload
7. **Graceful Shutdown**: Clean watcher termination

### Non-Functional Requirements

- Feature-gated behind `watch` feature
- Debounce rapid file changes
- Cross-platform file watching
- Thread-safe config access

## Acceptance Criteria

- [ ] `build_watched()` returns `(WatchedConfig<T>, ConfigWatcher)`
- [ ] File changes trigger automatic reload attempt
- [ ] Failed validation keeps previous config active
- [ ] `watcher.on_change(callback)` subscribes to events
- [ ] `watcher.reload()` triggers manual reload
- [ ] `watcher.stop()` cleanly terminates watching
- [ ] `watched_config.current()` returns `Arc<T>` for cheap cloning
- [ ] Debouncing prevents reload storms on rapid saves
- [ ] Unit tests for reload logic (with mock file changes)

## Technical Details

### API Design

```rust
/// Configuration that can be hot-reloaded
pub struct WatchedConfig<T> {
    current: Arc<RwLock<Arc<T>>>,
    builder: ConfigBuilder<T>,
}

/// Watcher for configuration file changes
pub struct ConfigWatcher {
    watcher: notify::RecommendedWatcher,
    stop_signal: Arc<AtomicBool>,
    event_tx: broadcast::Sender<ConfigEvent>,
}

/// Events emitted during configuration watching
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    /// Configuration was successfully reloaded
    Reloaded {
        /// Which sources changed
        changed_sources: Vec<String>,
    },
    /// Configuration reload failed validation
    ReloadFailed {
        /// Validation errors
        errors: Vec<ConfigError>,
    },
    /// A source file changed (before reload attempt)
    SourceChanged {
        /// Path of changed source
        path: PathBuf,
    },
    /// Watcher encountered an error
    WatchError {
        message: String,
    },
}

impl<T> WatchedConfig<T> {
    /// Get the current configuration (cheap Arc clone)
    pub fn current(&self) -> Arc<T> {
        self.current.read().unwrap().clone()
    }
}

impl ConfigWatcher {
    /// Subscribe to configuration events
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigEvent> {
        self.event_tx.subscribe()
    }

    /// Register a callback for configuration events
    pub fn on_change<F>(&self, callback: F)
    where
        F: Fn(ConfigEvent) + Send + 'static,
    {
        let mut rx = self.subscribe();
        std::thread::spawn(move || {
            while let Ok(event) = rx.blocking_recv() {
                callback(event);
            }
        });
    }

    /// Manually trigger a reload
    pub fn reload(&self) -> Validation<(), Vec<ConfigError>> {
        // ... trigger reload logic
    }

    /// Stop watching for changes
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
    }
}
```

### Builder Extension

```rust
#[cfg(feature = "watch")]
impl<T: DeserializeOwned + Validate + Send + Sync + 'static> ConfigBuilder<T> {
    /// Build configuration with file watching enabled
    pub fn build_watched(self) -> Validation<(WatchedConfig<T>, ConfigWatcher), Vec<ConfigError>> {
        // First, do initial build
        let config = self.build()?;

        // Create watched config wrapper
        let current = Arc::new(RwLock::new(Arc::new(config.into_inner())));
        let watched = WatchedConfig {
            current: current.clone(),
            builder: self.clone(),
        };

        // Set up file watcher
        let (event_tx, _) = broadcast::channel(16);
        let stop_signal = Arc::new(AtomicBool::new(false));

        let watcher = create_watcher(
            &self.sources,
            current.clone(),
            self.clone(),
            event_tx.clone(),
            stop_signal.clone(),
        )?;

        let config_watcher = ConfigWatcher {
            watcher,
            stop_signal,
            event_tx,
        };

        Validation::success((watched, config_watcher))
    }
}
```

### File Watcher Implementation

```rust
#[cfg(feature = "watch")]
fn create_watcher<T: DeserializeOwned + Validate + Send + Sync + 'static>(
    sources: &[Box<dyn Source>],
    config: Arc<RwLock<Arc<T>>>,
    builder: ConfigBuilder<T>,
    event_tx: broadcast::Sender<ConfigEvent>,
    stop_signal: Arc<AtomicBool>,
) -> Result<notify::RecommendedWatcher, ConfigError> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher, EventKind};
    use std::time::Duration;

    // Collect watchable paths from sources
    let watch_paths: Vec<PathBuf> = sources
        .iter()
        .filter_map(|s| s.watch_path())
        .collect();

    // Debounce channel
    let (debounce_tx, debounce_rx) = std::sync::mpsc::channel();

    // Create file watcher
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                for path in event.paths {
                    debounce_tx.send(path).ok();
                }
            }
        }
    }).map_err(|e| ConfigError::SourceError {
        source_name: "watcher".into(),
        kind: SourceErrorKind::Other { message: e.to_string() },
    })?;

    // Watch all paths
    for path in &watch_paths {
        if path.exists() {
            watcher.watch(path, RecursiveMode::NonRecursive).ok();
        }
    }

    // Spawn reload handler thread
    {
        let event_tx = event_tx.clone();
        let stop_signal = stop_signal.clone();

        std::thread::spawn(move || {
            let debounce_duration = Duration::from_millis(100);
            let mut last_reload = std::time::Instant::now();

            loop {
                if stop_signal.load(Ordering::SeqCst) {
                    break;
                }

                match debounce_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(path) => {
                        // Debounce: ignore if too recent
                        if last_reload.elapsed() < debounce_duration {
                            continue;
                        }

                        // Notify source changed
                        event_tx.send(ConfigEvent::SourceChanged {
                            path: path.clone(),
                        }).ok();

                        // Attempt reload
                        match builder.build() {
                            Validation::Success(new_config) => {
                                // Update config
                                *config.write().unwrap() = Arc::new(new_config.into_inner());
                                event_tx.send(ConfigEvent::Reloaded {
                                    changed_sources: vec![path.display().to_string()],
                                }).ok();
                            }
                            Validation::Failure(errors) => {
                                // Keep old config, notify of failure
                                event_tx.send(ConfigEvent::ReloadFailed { errors }).ok();
                            }
                        }

                        last_reload = std::time::Instant::now();
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
    }

    Ok(watcher)
}
```

### Source Trait Extension

```rust
pub trait Source: Send + Sync {
    // ... existing methods ...

    /// Get the path to watch for this source, if applicable
    #[cfg(feature = "watch")]
    fn watch_path(&self) -> Option<PathBuf> {
        None
    }
}

// In TOML source
#[cfg(feature = "watch")]
impl Source for Toml {
    fn watch_path(&self) -> Option<PathBuf> {
        match &self.source {
            TomlSource::File(path) => Some(path.clone()),
            TomlSource::String { .. } => None,
        }
    }
}
```

### Usage Example

```rust
use premortem::{Config, Toml, Env};

#[tokio::main]
async fn main() {
    let (config, mut watcher) = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build_watched()
        .unwrap_or_exit();

    // Get initial config
    let current = config.current();
    println!("Starting with config: {:?}", current);

    // Subscribe to changes
    watcher.on_change(|event| {
        match event {
            ConfigEvent::Reloaded { changed_sources } => {
                println!("Config reloaded from: {:?}", changed_sources);
            }
            ConfigEvent::ReloadFailed { errors } => {
                eprintln!("Reload failed ({} errors):", errors.len());
                for e in &errors {
                    eprintln!("  {}", e);
                }
            }
            ConfigEvent::SourceChanged { path } => {
                println!("Source changed: {}", path.display());
            }
            ConfigEvent::WatchError { message } => {
                eprintln!("Watch error: {}", message);
            }
        }
    });

    // Application loop
    loop {
        // Always use current() to get latest config
        let cfg = config.current();
        do_work(&cfg).await;
    }
}
```

### Async Support

```rust
#[cfg(all(feature = "watch", feature = "async"))]
impl ConfigWatcher {
    /// Async stream of configuration events
    pub fn events(&self) -> impl Stream<Item = ConfigEvent> {
        BroadcastStream::new(self.subscribe())
            .filter_map(|r| async { r.ok() })
    }

    /// Async reload
    pub async fn reload_async(&self) -> Validation<(), Vec<ConfigError>> {
        // ... async reload implementation
    }
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002, 003, 005
- **Affected Components**: ConfigBuilder, file sources
- **External Dependencies**:
  - `notify` crate for file watching
  - `tokio::sync::broadcast` or `std::sync::mpsc` for events

## Testing Strategy

- **Unit Tests**:
  - Reload logic with mock sources
  - Validation failure handling
  - Event emission
- **Integration Tests**:
  - Actual file modification watching
  - Debounce behavior
  - Graceful shutdown
- **Performance Tests**: Rapid change handling

## Documentation Requirements

- **Code Documentation**: Doc comments with async examples
- **User Documentation**: Hot reload setup guide

## Implementation Notes

- Use `notify` crate for cross-platform file watching
- Debounce duration should be configurable
- Consider using `arc-swap` for lock-free config updates
- Environment variable changes are NOT watched (would require polling)
- Remote sources would need their own polling mechanism

## Migration and Compatibility

Not applicable - new project.
