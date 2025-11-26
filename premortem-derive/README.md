# premortem-derive

[![CI](https://github.com/iepathos/premortem/actions/workflows/ci.yml/badge.svg)](https://github.com/iepathos/premortem/actions/workflows/ci.yml)
[![Coverage](https://github.com/iepathos/premortem/actions/workflows/coverage.yml/badge.svg)](https://github.com/iepathos/premortem/actions/workflows/coverage.yml)
[![Security](https://github.com/iepathos/premortem/actions/workflows/security.yml/badge.svg)](https://github.com/iepathos/premortem/actions/workflows/security.yml)
[![Crates.io](https://img.shields.io/crates/v/premortem-derive.svg)](https://crates.io/crates/premortem-derive)
[![Documentation](https://docs.rs/premortem-derive/badge.svg)](https://docs.rs/premortem-derive)
[![License](https://img.shields.io/badge/license-MIT)](../LICENSE)

Derive macros for the [premortem](https://crates.io/crates/premortem) configuration validation library.

## Usage

This crate is typically used through the main `premortem` crate with the `derive` feature (enabled by default):

```toml
[dependencies]
premortem = "0.1"
```

```rust
use premortem::Validate;
use serde::Deserialize;

#[derive(Debug, Deserialize, Validate)]
struct AppConfig {
    #[validate(non_empty)]
    pub host: String,

    #[validate(range(1..=65535))]
    pub port: u16,
}
```

## Direct Dependency

If you need to depend on this crate directly:

```toml
[dependencies]
premortem-derive = "0.1"
```

## Documentation

See the [premortem documentation](https://docs.rs/premortem) for full usage details and available validators.

## License

MIT
