//! Axum web server integration example.
//!
//! This example demonstrates how to use premortem to validate web server
//! configuration before starting an Axum server. It shows:
//!
//! - Web server configuration patterns (host, port, TLS, timeouts)
//! - Validation of network-related settings
//! - Cross-field validation (e.g., TLS cert requires TLS key)
//! - Integration with async runtime
//! - Graceful error reporting before server startup
//!
//! Run with:
//!   cargo run
//!
//! Override with environment variables:
//!   SERVER_PORT=8080 SERVER_HOST=0.0.0.0 cargo run

use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use premortem::prelude::*;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};

/// Web server configuration with comprehensive validation.
///
/// This configuration demonstrates common web server settings that should
/// be validated before the server starts to prevent runtime failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerConfig {
    /// Server hostname - must be a valid IP or hostname
    host: String,

    /// Server port - must be valid port number
    port: u16,

    /// Optional TLS certificate path
    tls_cert: Option<String>,

    /// Optional TLS key path
    tls_key: Option<String>,

    /// Maximum request body size in megabytes
    max_body_size_mb: u32,

    /// Request timeout in seconds
    request_timeout_secs: u64,

    /// Maximum concurrent connections
    max_connections: u32,

    /// Connection idle timeout in seconds
    idle_timeout_secs: u64,

    /// API path prefix
    api_prefix: String,

    /// CORS allowed origins
    cors_allowed_origins: Vec<String>,

    /// Rate limit: requests per window
    rate_limit_requests: u32,

    /// Rate limit: window duration in seconds
    rate_limit_window_secs: u64,

    /// Logging level
    log_level: String,

    /// Logging format
    log_format: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            tls_cert: None,
            tls_key: None,
            max_body_size_mb: 10,
            request_timeout_secs: 30,
            max_connections: 1000,
            idle_timeout_secs: 60,
            api_prefix: "/api/v1".to_string(),
            cors_allowed_origins: vec![],
            rate_limit_requests: 100,
            rate_limit_window_secs: 60,
            log_level: "info".to_string(),
            log_format: "json".to_string(),
        }
    }
}

impl Validate for ServerConfig {
    fn validate(&self) -> ConfigValidation<()> {
        use stillwater::Validation;

        let mut errors = Vec::new();

        // Validate host is not empty
        if self.host.is_empty() {
            errors.push(ConfigError::ValidationError {
                path: "host".to_string(),
                source_location: None,
                value: Some(self.host.clone()),
                message: "host cannot be empty".to_string(),
            });
        }

        // Validate port is in valid range
        if self.port == 0 {
            errors.push(ConfigError::ValidationError {
                path: "port".to_string(),
                source_location: None,
                value: Some(self.port.to_string()),
                message: "port must be between 1 and 65535".to_string(),
            });
        }

        // Cross-field validation: TLS cert requires TLS key and vice versa
        match (&self.tls_cert, &self.tls_key) {
            (Some(_), None) => {
                errors.push(ConfigError::CrossFieldError {
                    paths: vec!["tls_cert".to_string(), "tls_key".to_string()],
                    message: "TLS certificate provided but TLS key is missing".to_string(),
                });
            }
            (None, Some(_)) => {
                errors.push(ConfigError::CrossFieldError {
                    paths: vec!["tls_cert".to_string(), "tls_key".to_string()],
                    message: "TLS key provided but TLS certificate is missing".to_string(),
                });
            }
            _ => {}
        }

        // Validate max body size is reasonable
        if self.max_body_size_mb == 0 {
            errors.push(ConfigError::ValidationError {
                path: "max_body_size_mb".to_string(),
                source_location: None,
                value: Some(self.max_body_size_mb.to_string()),
                message: "max body size must be at least 1 MB".to_string(),
            });
        }
        if self.max_body_size_mb > 1024 {
            errors.push(ConfigError::ValidationError {
                path: "max_body_size_mb".to_string(),
                source_location: None,
                value: Some(self.max_body_size_mb.to_string()),
                message: "max body size should not exceed 1024 MB (1 GB)".to_string(),
            });
        }

        // Validate request timeout
        if self.request_timeout_secs == 0 {
            errors.push(ConfigError::ValidationError {
                path: "request_timeout_secs".to_string(),
                source_location: None,
                value: Some(self.request_timeout_secs.to_string()),
                message: "request timeout must be at least 1 second".to_string(),
            });
        }
        if self.request_timeout_secs > 3600 {
            errors.push(ConfigError::ValidationError {
                path: "request_timeout_secs".to_string(),
                source_location: None,
                value: Some(self.request_timeout_secs.to_string()),
                message: "request timeout should not exceed 1 hour (3600 seconds)".to_string(),
            });
        }

        // Validate max connections
        if self.max_connections == 0 {
            errors.push(ConfigError::ValidationError {
                path: "max_connections".to_string(),
                source_location: None,
                value: Some(self.max_connections.to_string()),
                message: "max connections must be at least 1".to_string(),
            });
        }

        // Validate API prefix starts with /
        if !self.api_prefix.starts_with('/') {
            errors.push(ConfigError::ValidationError {
                path: "api_prefix".to_string(),
                source_location: None,
                value: Some(self.api_prefix.clone()),
                message: "API prefix must start with '/'".to_string(),
            });
        }

        // Validate log level
        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.log_level.to_lowercase().as_str()) {
            errors.push(ConfigError::ValidationError {
                path: "log_level".to_string(),
                source_location: None,
                value: Some(self.log_level.clone()),
                message: "log level must be one of: trace, debug, info, warn, error".to_string(),
            });
        }

        // Validate log format
        let valid_log_formats = ["json", "pretty", "compact"];
        if !valid_log_formats.contains(&self.log_format.to_lowercase().as_str()) {
            errors.push(ConfigError::ValidationError {
                path: "log_format".to_string(),
                source_location: None,
                value: Some(self.log_format.clone()),
                message: "log format must be one of: json, pretty, compact".to_string(),
            });
        }

        // Validate rate limiting
        if self.rate_limit_requests == 0 {
            errors.push(ConfigError::ValidationError {
                path: "rate_limit_requests".to_string(),
                source_location: None,
                value: Some(self.rate_limit_requests.to_string()),
                message: "rate limit requests must be at least 1".to_string(),
            });
        }
        if self.rate_limit_window_secs == 0 {
            errors.push(ConfigError::ValidationError {
                path: "rate_limit_window_secs".to_string(),
                source_location: None,
                value: Some(self.rate_limit_window_secs.to_string()),
                message: "rate limit window must be at least 1 second".to_string(),
            });
        }

        match ConfigErrors::from_vec(errors) {
            Some(errs) => Validation::Failure(errs),
            None => Validation::Success(()),
        }
    }
}

/// Shared application state containing configuration.
#[derive(Clone)]
struct AppState {
    config: Arc<ServerConfig>,
}

/// Health check response.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Configuration info response (for debugging).
#[derive(Serialize)]
struct ConfigInfoResponse {
    host: String,
    port: u16,
    api_prefix: String,
    max_connections: u32,
    tls_enabled: bool,
}

/// Health check endpoint.
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Configuration info endpoint (useful for debugging).
async fn config_info(State(state): State<AppState>) -> Json<ConfigInfoResponse> {
    Json(ConfigInfoResponse {
        host: state.config.host.clone(),
        port: state.config.port,
        api_prefix: state.config.api_prefix.clone(),
        max_connections: state.config.max_connections,
        tls_enabled: state.config.tls_cert.is_some(),
    })
}

/// Example API endpoint.
async fn api_root(State(state): State<AppState>) -> (StatusCode, String) {
    (
        StatusCode::OK,
        format!(
            "Welcome to the API! Server running on {}:{}",
            state.config.host, state.config.port
        ),
    )
}

/// Build the Axum router with all routes.
fn build_router(state: AppState) -> Router {
    let api_prefix = state.config.api_prefix.clone();

    // API routes
    let api_routes = Router::new()
        .route("/", get(api_root))
        .route("/config", get(config_info));

    // Main router
    Router::new()
        .route("/health", get(health_check))
        .nest(&api_prefix, api_routes)
        .with_state(state)
}

#[tokio::main]
async fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("Loading server configuration...");
    println!();

    // Load and validate configuration BEFORE starting the server
    // This is the "premortem" - finding all configuration problems upfront
    let config_result = Config::<ServerConfig>::builder()
        // Start with sensible defaults
        .source(Defaults::from(ServerConfig::default()))
        // Load from config file (optional)
        .source(Toml::file("config.toml").optional())
        // Environment variables override everything
        // SERVER_HOST, SERVER_PORT, SERVER_MAX_CONNECTIONS, etc.
        .source(Env::prefix("SERVER_"))
        .build();

    let config = match config_result {
        Ok(config) => {
            println!("Configuration validated successfully!");
            println!();
            config
        }
        Err(errors) => {
            // Premortem: all configuration errors are reported at once
            // The server never starts with invalid configuration
            eprintln!("Configuration validation failed!");
            eprintln!();
            eprintln!("Found {} configuration error(s):", errors.len());
            eprintln!();
            for (i, error) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, error);
            }
            eprintln!();
            eprintln!("Please fix the configuration errors and try again.");
            std::process::exit(1);
        }
    };

    // Display the validated configuration
    println!("Server Configuration:");
    println!("  Host: {}", config.host);
    println!("  Port: {}", config.port);
    println!(
        "  TLS: {}",
        if config.tls_cert.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("  Max Body Size: {} MB", config.max_body_size_mb);
    println!("  Request Timeout: {}s", config.request_timeout_secs);
    println!("  Max Connections: {}", config.max_connections);
    println!("  API Prefix: {}", config.api_prefix);
    println!("  Log Level: {}", config.log_level);
    println!();

    // Create application state from the Config wrapper
    let server_config = config.clone();
    let state = AppState {
        config: Arc::new(server_config.into_inner()),
    };

    // Build the router
    let app = build_router(state);

    // Parse the socket address
    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Invalid socket address");

    // Start the server
    println!("Starting server on http://{}", addr);
    println!();
    println!("Available endpoints:");
    println!("  GET /health - Health check");
    println!(
        "  GET {api_prefix}/ - API root",
        api_prefix = config.api_prefix
    );
    println!(
        "  GET {api_prefix}/config - Configuration info",
        api_prefix = config.api_prefix
    );
    println!();

    // Note: In a real application, you would also set up:
    // - TLS/HTTPS using the tls_cert and tls_key paths
    // - Request body size limits using max_body_size_mb
    // - Connection timeouts using request_timeout_secs and idle_timeout_secs
    // - CORS middleware using cors_allowed_origins
    // - Rate limiting using rate_limit_requests and rate_limit_window_secs

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, app).await.expect("Server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use stillwater::Validation;

    #[test]
    fn test_valid_config() {
        let config = ServerConfig::default();
        let result = config.validate();
        assert!(matches!(result, Validation::Success(())));
    }

    #[test]
    fn test_invalid_port() {
        let config = ServerConfig {
            port: 0,
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Failure(_)));
    }

    #[test]
    fn test_tls_requires_both_cert_and_key() {
        // Cert without key
        let config = ServerConfig {
            tls_cert: Some("/path/to/cert.pem".to_string()),
            tls_key: None,
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Failure(_)));

        // Key without cert
        let config = ServerConfig {
            tls_cert: None,
            tls_key: Some("/path/to/key.pem".to_string()),
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Failure(_)));

        // Both present - valid
        let config = ServerConfig {
            tls_cert: Some("/path/to/cert.pem".to_string()),
            tls_key: Some("/path/to/key.pem".to_string()),
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Success(())));
    }

    #[test]
    fn test_api_prefix_must_start_with_slash() {
        let config = ServerConfig {
            api_prefix: "api/v1".to_string(), // Missing leading slash
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Failure(_)));
    }

    #[test]
    fn test_invalid_log_level() {
        let config = ServerConfig {
            log_level: "verbose".to_string(), // Invalid
            ..Default::default()
        };
        let result = config.validate();
        assert!(matches!(result, Validation::Failure(_)));
    }

    #[test]
    fn test_multiple_validation_errors_accumulated() {
        let config = ServerConfig {
            port: 0,
            host: "".to_string(),
            api_prefix: "no-slash".to_string(),
            log_level: "invalid".to_string(),
            ..Default::default()
        };
        let result = config.validate();

        if let Validation::Failure(errors) = result {
            // Should accumulate multiple errors
            assert!(
                errors.len() >= 4,
                "Expected at least 4 errors, got {}",
                errors.len()
            );
        } else {
            panic!("Expected validation failure");
        }
    }
}
