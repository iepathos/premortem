---
number: 9
title: Value Tracing and Origin Tracking
category: foundation
priority: medium
status: draft
dependencies: [1, 2]
created: 2025-11-25
---

# Specification 009: Value Tracing and Origin Tracking

**Category**: foundation
**Priority**: medium
**Status**: draft
**Dependencies**: [001 - Core Config Builder, 002 - Error Types]

## Context

When debugging configuration issues, especially in production, it's crucial to know where each value came from. With multiple sources (defaults, file, environment), a value might be set in one place and overridden in another. Value tracing shows the complete history of each configuration path.

## Objective

Implement value tracing that tracks the source and override history of each configuration value, with APIs to query and display this information for debugging.

## Requirements

### Functional Requirements

1. **Origin Tracking**: Record source for each value
2. **Override History**: Track all values from all sources, not just final
3. **Trace Query**: Query trace for specific config path
4. **Trace Report**: Generate full trace report
5. **Override Detection**: Identify which values were overridden

### Non-Functional Requirements

- Tracing is opt-in (not required for basic usage)
- Minimal overhead when tracing is disabled
- Thread-safe trace data

## Acceptance Criteria

- [ ] `build_traced()` returns `TracedConfig<T>` with trace data
- [ ] `traced.trace("path")` returns trace for specific path
- [ ] Trace shows all sources that provided a value
- [ ] Trace shows which source "won" (final value)
- [ ] `traced.was_overridden("path")` checks for overrides
- [ ] `traced.trace_report()` generates human-readable report
- [ ] Regular `build()` has no tracing overhead
- [ ] Unit tests for trace queries

## Technical Details

### API Design

```rust
/// Configuration with tracing information
pub struct TracedConfig<T> {
    config: Config<T>,
    traces: BTreeMap<String, ValueTrace>,
}

/// Trace of a single configuration value
#[derive(Debug, Clone)]
pub struct ValueTrace {
    /// The final value (from highest priority source)
    pub final_value: TracedValue,
    /// All values from all sources, in priority order (lowest first)
    pub history: Vec<TracedValue>,
}

/// A value with its source information
#[derive(Debug, Clone)]
pub struct TracedValue {
    /// The value at this source
    pub value: Value,
    /// Where this value came from
    pub source: SourceLocation,
    /// Whether this value was used (not overridden)
    pub is_final: bool,
}

impl<T> TracedConfig<T> {
    /// Get reference to the configuration
    pub fn value(&self) -> &T {
        self.config.get()
    }

    /// Consume and return the configuration
    pub fn into_inner(self) -> T {
        self.config.into_inner()
    }

    /// Get the trace for a specific path
    pub fn trace(&self, path: &str) -> Option<&ValueTrace> {
        self.traces.get(path)
    }

    /// Check if a path was overridden by a higher-priority source
    pub fn was_overridden(&self, path: &str) -> bool {
        self.traces.get(path)
            .map(|t| t.history.len() > 1)
            .unwrap_or(false)
    }

    /// Get all traces
    pub fn traces(&self) -> impl Iterator<Item = (&str, &ValueTrace)> {
        self.traces.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Get paths that were overridden
    pub fn overridden_paths(&self) -> impl Iterator<Item = &str> {
        self.traces.iter()
            .filter(|(_, t)| t.history.len() > 1)
            .map(|(k, _)| k.as_str())
    }

    /// Generate a human-readable trace report
    pub fn trace_report(&self) -> String {
        let mut report = String::new();

        for (path, trace) in &self.traces {
            report.push_str(&format!("{} = {:?}\n", path, trace.final_value.value));

            for (i, val) in trace.history.iter().enumerate() {
                let marker = if val.is_final { "✓" } else { "○" };
                let override_note = if !val.is_final { " <- overridden" } else { "" };
                report.push_str(&format!(
                    "  {} [{}] {:?}{}\n",
                    marker, val.source, val.value, override_note
                ));
            }
            report.push('\n');
        }

        report
    }
}
```

### Display Implementation

```rust
impl std::fmt::Display for ValueTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Final: {:?} (from {})",
            self.final_value.value,
            self.final_value.source)?;

        if self.history.len() > 1 {
            writeln!(f, "History:")?;
            for val in &self.history {
                let marker = if val.is_final { "→" } else { " " };
                writeln!(f, "  {} [{}] {:?}", marker, val.source, val.value)?;
            }
        }

        Ok(())
    }
}
```

### Builder Extension

```rust
impl<T: DeserializeOwned + Validate> ConfigBuilder<T> {
    /// Build configuration with value tracing enabled
    pub fn build_traced(self) -> Validation<TracedConfig<T>, Vec<ConfigError>> {
        // Load all sources, keeping all values
        let mut all_values: BTreeMap<String, Vec<TracedValue>> = BTreeMap::new();
        let mut errors = Vec::new();

        for source in &self.sources {
            match source.load() {
                Validation::Success(values) => {
                    for (path, config_value) in values.iter() {
                        all_values.entry(path.clone()).or_default().push(TracedValue {
                            value: config_value.value.clone(),
                            source: config_value.source.clone(),
                            is_final: false, // Will be set later
                        });
                    }
                }
                Validation::Failure(errs) => {
                    errors.extend(errs);
                }
            }
        }

        if !errors.is_empty() {
            return Validation::fail(errors);
        }

        // Build traces, marking final values
        let mut traces = BTreeMap::new();
        for (path, mut history) in all_values {
            if let Some(last) = history.last_mut() {
                last.is_final = true;
            }
            let final_value = history.last().cloned().unwrap();
            traces.insert(path, ValueTrace {
                final_value,
                history,
            });
        }

        // Merge for final config (same as regular build)
        let merged = self.merge_sources()?;

        // Deserialize and validate
        let config = self.deserialize_and_validate(merged)?;

        Validation::success(TracedConfig { config, traces })
    }
}
```

### Trace Query Examples

```rust
let traced = Config::<AppConfig>::builder()
    .source(Defaults::from(AppConfig::default()))
    .source(Toml::file("config.toml"))
    .source(Env::prefix("APP_"))
    .build_traced()?;

// Query specific path
if let Some(trace) = traced.trace("database.host") {
    println!("database.host = {:?}", trace.final_value.value);
    println!("  from: {}", trace.final_value.source);

    if trace.history.len() > 1 {
        println!("  (overridden from {} sources)", trace.history.len() - 1);
    }
}

// Check for overrides
if traced.was_overridden("database.host") {
    println!("Warning: database.host was overridden");
}

// Full report
let report = traced.trace_report();
std::fs::write("config-trace.txt", report)?;

// List all overridden values
println!("Overridden values:");
for path in traced.overridden_paths() {
    let trace = traced.trace(path).unwrap();
    println!("  {} ({} sources)", path, trace.history.len());
}
```

### Example Trace Report Output

```
database.host = "prod-db.example.com"
  ○ [defaults] "localhost" <- overridden
  ○ [config.toml:12] "staging-db.example.com" <- overridden
  ✓ [env:APP_DATABASE_HOST] "prod-db.example.com"

database.port = 5432
  ✓ [config.toml:13] 5432

server.timeout_seconds = 30
  ○ [defaults] 60 <- overridden
  ✓ [config.toml:5] 30

cache.enabled = false
  ✓ [defaults] false
```

### Serialization Support

```rust
impl<T: Serialize> TracedConfig<T> {
    /// Export traces as JSON for tooling
    pub fn traces_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();

        for (path, trace) in &self.traces {
            let entry = serde_json::json!({
                "final": {
                    "value": trace.final_value.value,
                    "source": trace.final_value.source.to_string(),
                },
                "history": trace.history.iter().map(|v| {
                    serde_json::json!({
                        "value": v.value,
                        "source": v.source.to_string(),
                        "overridden": !v.is_final,
                    })
                }).collect::<Vec<_>>(),
            });
            map.insert(path.clone(), entry);
        }

        serde_json::Value::Object(map)
    }
}
```

## Dependencies

- **Prerequisites**: Specs 001, 002
- **Affected Components**: ConfigBuilder, all sources
- **External Dependencies**: None additional

## Testing Strategy

- **Unit Tests**:
  - Single source tracing
  - Multi-source override detection
  - Trace query API
  - Report generation
- **Integration Tests**:
  - Real multi-source configuration tracing
  - JSON export

## Documentation Requirements

- **Code Documentation**: Doc comments with trace examples
- **User Documentation**: Debugging guide using traces

## Implementation Notes

- Traces are stored separately from the config to avoid runtime overhead when not using tracing
- Consider making trace storage lazy (computed on first query)
- Arc<ValueTrace> could reduce cloning for large configs
- Consider adding timestamp to traces for debugging reload issues

## Migration and Compatibility

Not applicable - new project.
