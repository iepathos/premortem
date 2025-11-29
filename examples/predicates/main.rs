//! Example demonstrating stillwater predicate integration with premortem.
//!
//! This example shows how to use stillwater's composable predicates for validation.

use premortem::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
    max_connections: u32,
}

impl Validate for ServerConfig {
    fn validate(&self) -> ConfigValidation<()> {
        // Use stillwater predicates with custom error messages
        let validations = vec![
            validate_with_predicate(&self.host, "host", not_empty(), "host cannot be empty"),
            validate_with_predicate(
                &self.port,
                "port",
                between(1, 65535),
                "port must be between 1 and 65535",
            ),
            validate_with_predicate(
                &self.max_connections,
                "max_connections",
                gt(0),
                "max_connections must be greater than 0",
            ),
        ];

        Validation::all_vec(validations).map(|_| ())
    }
}

#[allow(clippy::result_large_err)]
fn main() -> Result<(), ConfigErrors> {
    // Create mock environment for example
    let env = MockEnv::new().with_file(
        "config.toml",
        r#"
host = "localhost"
port = 8080
max_connections = 100
"#,
    );

    // Build and validate config using predicates
    let config = Config::<ServerConfig>::builder()
        .source(Toml::file("config.toml"))
        .build_with_env(&env)?;

    println!("Server configuration loaded successfully!");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!("  Max connections: {}", config.max_connections);

    Ok(())
}
