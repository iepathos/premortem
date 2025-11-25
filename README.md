# premortem

> Know how your app will die—before it does.

[![Crates.io](https://img.shields.io/crates/v/premortem.svg)](https://crates.io/crates/premortem)
[![Documentation](https://docs.rs/premortem/badge.svg)](https://docs.rs/premortem)
[![License](https://img.shields.io/crates/l/premortem.svg)](LICENSE)

A configuration library that performs a **premortem** on your app's config—finding all the ways it would die before it ever runs.

## Why "premortem"?

The name is a bit tongue-in-cheek—but only a bit. Configuration errors are one of the leading causes of production outages. Bad config doesn't just cause bugs; it causes *incidents*, *pages*, and *3am debugging sessions*.

A **postmortem** is what you do *after* something dies—gathering everyone to analyze what went wrong. Traditional config libraries give you the postmortem experience:

```
$ ./myapp
Error: missing field `database.host`

$ ./myapp  # fixed it, try again
Error: invalid port value

$ ./myapp  # fixed that too
Error: pool_size must be positive

# Three deaths to find three problems
```

**premortem** gives you all the fatal issues upfront:

```
$ ./myapp
Configuration errors (3):
  [config.toml:8] missing required field 'database.host'
  [env:APP_PORT] value "abc" is not a valid integer
  [config.toml:10] 'pool_size' value -5 must be >= 1
```

One run. All errors. Know how your app would die—before it does.

## Features

- **Accumulate all errors** — Never stop at the first problem
- **Trace value origins** — Know exactly which source provided each value
- **Multi-source loading** — Files, environment, CLI args, remote sources
- **Holistic validation** — Type, range, format, cross-field, and business rules
- **Derive macro** — Declarative validation with `#[derive(Validate)]`
- **Hot reload** — Watch for config changes (optional feature)

## Quick Start

```rust
use premortem::{Config, Toml, Env, Validate};
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(non_empty)]
    pub host: String,

    #[validate(range(1..=65535))]
    pub port: u16,

    #[validate(range(1..=100))]
    pub pool_size: u32,
}

fn main() {
    let config = Config::<AppConfig>::builder()
        .source(Toml::file("config.toml"))
        .source(Env::prefix("APP_"))
        .build()
        .unwrap_or_else(|errors| {
            eprintln!("Configuration errors ({}):", errors.len());
            for e in &errors {
                eprintln!("  {}", e);
            }
            std::process::exit(1);
        });

    println!("Starting server on {}:{}", config.host, config.port);
}
```

## Installation

```toml
[dependencies]
premortem = "0.1"
```

With optional features:

```toml
[dependencies]
premortem = { version = "0.1", features = ["json", "yaml", "watch"] }
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `toml` | TOML file support (default) |
| `json` | JSON file support |
| `yaml` | YAML file support |
| `watch` | Hot reload / file watching |
| `remote` | Remote sources (Consul, etcd, Vault, etc.) |
| `full` | All features |

## License

MIT Glen Baker <iepathos@gmail.com>
