---
number: 2
title: Remote Configuration Source Support
category: compatibility
priority: medium
status: draft
dependencies: []
created: 2025-11-25
updated: 2025-11-26
---

# Specification 002: Remote Configuration Source Support

**Category**: compatibility
**Priority**: medium
**Status**: draft
**Dependencies**: None

## Context

Premortem currently supports local configuration sources (TOML, JSON, environment variables), but many production systems require fetching configuration from remote services. The `remote` feature flag already exists in `Cargo.toml` but is not implemented. This specification defines a remote source system that supports common configuration backends while maintaining premortem's core principles of testability and error accumulation.

Common remote configuration sources in production environments include:
- **HashiCorp Consul** - Service mesh and KV store
- **HashiCorp Vault** - Secrets management
- **etcd** - Distributed key-value store (Kubernetes backing store)
- **AWS Parameter Store** - AWS Systems Manager parameter storage
- **AWS Secrets Manager** - AWS secrets management
- **Environment-specific URLs** - Custom HTTP endpoints serving JSON/YAML/TOML

This specification focuses on providing a flexible HTTP-based remote source with built-in support for common authentication patterns, while maintaining the testable `ConfigEnv` abstraction.

**Stillwater Integration**: This spec leverages stillwater 0.9.0's `RetryPolicy` for standardized backoff strategies and jitter support. The retry policy is a pure data structure describing retry behavior, while the actual retry execution happens at the I/O boundary—following the "pure core, imperative shell" pattern.

### Why Stillwater for Retry Logic?

1. **Tested, standardized backoff** - Stillwater's `RetryPolicy` provides four backoff strategies (constant, linear, exponential, fibonacci) that are well-tested and follow industry best practices.

2. **Pure data, testable logic** - `RetryPolicy` is a pure data structure. The `delay_for_attempt(n)` method computes delays without side effects, enabling unit tests without timers.

3. **Jitter support** - Built-in jitter strategies (proportional, full, decorrelated) prevent thundering herd problems in distributed systems.

4. **Consistent across crates** - Using stillwater's retry types ensures consistent retry behavior across premortem and other crates in the ecosystem.

5. **Blocking-compatible** - While stillwater's `Effect::retry*` methods require async, the `RetryPolicy` type itself is sync-compatible. We use `delay_for_attempt()` in a blocking loop with `std::thread::sleep()`.

## Objective

Implement a remote configuration source system that:
1. Loads configuration from HTTP/HTTPS endpoints
2. Supports common authentication methods (Bearer token, Basic auth, custom headers)
3. Provides rich error reporting with connection and parsing failures
4. Follows the existing `Source` trait implementation patterns
5. Maintains testability via `ConfigEnv` trait extension
6. Supports multiple response formats (JSON, TOML, YAML when enabled)
7. Enables retry and timeout configuration

## Requirements

### Functional Requirements

1. **HTTP Loading**: Fetch configuration from HTTP/HTTPS URLs
2. **Format Detection**: Auto-detect or explicitly specify response format (JSON, TOML, YAML)
3. **Authentication Support**:
   - Bearer token authentication
   - Basic authentication (username/password)
   - Custom header injection
   - No authentication (public endpoints)
4. **Required/Optional Modes**: Support both required (error on failure) and optional (empty on failure) modes
5. **Timeout Configuration**: Configurable connection and read timeouts
6. **Retry Logic**: Configurable retry via stillwater's `RetryPolicy` with backoff strategies (constant, linear, exponential, fibonacci) and optional jitter
7. **Response Parsing**: Parse responses as JSON, TOML, or YAML into `ConfigValues`
8. **Path Extraction**: Extract nested paths from response (e.g., fetch `/config` but use only `data.app` subtree)
9. **Custom Naming**: Allow custom source names for error messages

### Non-Functional Requirements

1. **Pure Core Pattern**: Separate network I/O from pure parsing logic via `ConfigEnv` extension
2. **Error Accumulation**: Return `ConfigErrors` for all error conditions
3. **Feature Flag**: Only compile when `remote` feature is enabled
4. **Consistent API**: Match the builder pattern of existing sources
5. **Security**: No credential logging, secure header handling
6. **Testability**: Full MockEnv support for unit testing without network calls

## Acceptance Criteria

- [ ] `Remote::url("https://example.com/config")` fetches configuration from URL
- [ ] `.format(Format::Json)` explicitly sets response format
- [ ] `.bearer_token("...")` adds Bearer token authentication
- [ ] `.basic_auth("user", "pass")` adds Basic authentication
- [ ] `.header("X-Custom", "value")` adds custom headers
- [ ] `.optional()` makes failures return empty `ConfigValues` instead of error
- [ ] `.required()` makes failures return `SourceError::ConnectionError`
- [ ] `.timeout(Duration::from_secs(30))` sets request timeout
- [ ] `.retry_policy(RetryPolicy::exponential(...).with_max_retries(3))` sets retry behavior using stillwater
- [ ] `.retry(3)` convenience method for simple exponential backoff with N retries
- [ ] `.path("data.config")` extracts nested path from response
- [ ] `.named("vault-secrets")` sets custom source name
- [ ] Connection errors are reported as `SourceError::ConnectionError`
- [ ] Retry exhaustion errors include attempt count and total duration (from stillwater's `RetryExhausted`)
- [ ] Parse errors include the source URL and response details
- [ ] Timeout errors are clearly distinguished from connection failures
- [ ] Authentication failures (401/403) have specific error messages
- [ ] MockEnv supports mocking HTTP responses for testing
- [ ] Feature flag `remote` controls compilation
- [ ] `Remote` is re-exported from `premortem::sources` and `premortem` root
- [ ] `RetryPolicy` is re-exported from `premortem` when `remote` feature is enabled
- [ ] No credentials appear in error messages or logs

## Technical Details

### Implementation Approach

1. **Add reqwest Dependency**
   ```toml
   # Cargo.toml
   reqwest = { version = "0.12", features = ["blocking", "json"], optional = true }

   [features]
   remote = ["dep:reqwest"]
   ```

2. **Extend ConfigEnv Trait**
   ```rust
   // In env.rs - add to ConfigEnv trait
   #[cfg(feature = "remote")]
   fn fetch_url(&self, request: &HttpRequest) -> Result<HttpResponse, std::io::Error>;
   ```

3. **HTTP Request/Response Types**
   ```rust
   #[cfg(feature = "remote")]
   #[derive(Debug, Clone)]
   pub struct HttpRequest {
       pub url: String,
       pub method: HttpMethod,
       pub headers: Vec<(String, String)>,
       pub timeout: Option<Duration>,
   }

   #[cfg(feature = "remote")]
   #[derive(Debug, Clone)]
   pub enum HttpMethod {
       Get,
       Post,
   }

   #[cfg(feature = "remote")]
   #[derive(Debug, Clone)]
   pub struct HttpResponse {
       pub status: u16,
       pub body: String,
       pub headers: Vec<(String, String)>,
   }
   ```

4. **Create remote_source.rs Module**
   ```rust
   use stillwater::retry::RetryPolicy;

   /// Response format for remote configuration
   #[derive(Debug, Clone, Copy, Default)]
   pub enum Format {
       #[default]
       Json,
       #[cfg(feature = "toml")]
       Toml,
       #[cfg(feature = "yaml")]
       Yaml,
       /// Auto-detect from Content-Type header
       Auto,
   }

   /// Authentication method for remote sources
   #[derive(Debug, Clone)]
   pub enum Auth {
       None,
       Bearer(String),
       Basic { username: String, password: String },
   }

   /// Remote configuration source.
   #[derive(Debug, Clone)]
   pub struct Remote {
       url: String,
       format: Format,
       auth: Auth,
       headers: Vec<(String, String)>,
       required: bool,
       timeout: Duration,
       retry_policy: Option<RetryPolicy>,
       path: Option<String>,
       name: Option<String>,
   }
   ```

5. **Builder Pattern Implementation**
   ```rust
   use stillwater::retry::RetryPolicy;

   impl Remote {
       pub fn url(url: impl Into<String>) -> Self {
           Self {
               url: url.into(),
               format: Format::default(),
               auth: Auth::None,
               headers: Vec::new(),
               required: true,
               timeout: Duration::from_secs(30),
               retry_policy: None,
               path: None,
               name: None,
           }
       }

       pub fn format(mut self, format: Format) -> Self {
           self.format = format;
           self
       }

       pub fn bearer_token(mut self, token: impl Into<String>) -> Self {
           self.auth = Auth::Bearer(token.into());
           self
       }

       pub fn basic_auth(
           mut self,
           username: impl Into<String>,
           password: impl Into<String>
       ) -> Self {
           self.auth = Auth::Basic {
               username: username.into(),
               password: password.into(),
           };
           self
       }

       pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
           self.headers.push((key.into(), value.into()));
           self
       }

       pub fn optional(mut self) -> Self {
           self.required = false;
           self
       }

       pub fn required(mut self) -> Self {
           self.required = true;
           self
       }

       pub fn timeout(mut self, timeout: Duration) -> Self {
           self.timeout = timeout;
           self
       }

       /// Set a custom retry policy using stillwater's RetryPolicy.
       ///
       /// Provides full control over backoff strategy, max retries, max delay, and jitter.
       pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
           self.retry_policy = Some(policy);
           self
       }

       /// Convenience method for simple exponential backoff with N retries.
       ///
       /// Equivalent to:
       /// ```ignore
       /// .retry_policy(
       ///     RetryPolicy::exponential(Duration::from_millis(100))
       ///         .with_max_retries(count)
       ///         .with_max_delay(Duration::from_secs(10))
       /// )
       /// ```
       pub fn retry(self, count: u32) -> Self {
           self.retry_policy(
               RetryPolicy::exponential(Duration::from_millis(100))
                   .with_max_retries(count)
                   .with_max_delay(Duration::from_secs(10))
           )
       }

       pub fn path(mut self, path: impl Into<String>) -> Self {
           self.path = Some(path.into());
           self
       }

       pub fn named(mut self, name: impl Into<String>) -> Self {
           self.name = Some(name.into());
           self
       }
   }
   ```

6. **Source Trait Implementation**
   ```rust
   impl Source for Remote {
       fn load(&self, env: &dyn ConfigEnv) -> Result<ConfigValues, ConfigErrors> {
           let request = self.build_request();

           let response = self.fetch_with_retry(env, &request)?;

           if !is_success(response.status) {
               return self.handle_error_status(response);
           }

           let format = self.detect_format(&response);
           let values = self.parse_response(&response.body, format)?;

           if let Some(ref path) = self.path {
               self.extract_path(values, path)
           } else {
               Ok(values)
           }
       }

       fn name(&self) -> &str {
           self.name.as_deref().unwrap_or(&self.url)
       }
   }

   impl Remote {
       /// Fetch with retry using stillwater's RetryPolicy for delay calculations.
       ///
       /// Uses the pure `delay_for_attempt()` method to compute delays without
       /// requiring async. The retry loop is blocking but uses tested backoff logic.
       fn fetch_with_retry(
           &self,
           env: &dyn ConfigEnv,
           request: &HttpRequest,
       ) -> Result<HttpResponse, ConfigErrors> {
           let policy = match &self.retry_policy {
               Some(p) => p,
               None => return self.fetch_once(env, request),
           };

           let start = std::time::Instant::now();
           let mut attempt = 0u32;
           let mut last_error = None;

           loop {
               match self.fetch_once(env, request) {
                   Ok(response) if is_retryable_status(response.status) => {
                       last_error = Some(self.status_to_error(&response));
                   }
                   Ok(response) => return Ok(response),
                   Err(e) if self.is_retryable_error(&e) => {
                       last_error = Some(e);
                   }
                   Err(e) => return Err(e), // Non-retryable error
               }

               // Check if we should retry
               if let Some(delay) = policy.delay_for_attempt(attempt) {
                   std::thread::sleep(delay);
                   attempt += 1;
               } else {
                   // Retries exhausted - return error with metadata
                   let total_duration = start.elapsed();
                   return Err(self.retry_exhausted_error(
                       last_error.unwrap(),
                       attempt + 1,
                       total_duration,
                   ));
               }
           }
       }

       /// Check if an error is retryable (transient).
       fn is_retryable_error(&self, errors: &ConfigErrors) -> bool {
           // Only retry connection errors, not parse errors or auth failures
           errors.iter().any(|e| matches!(
               e,
               ConfigError::SourceError { kind: SourceErrorKind::ConnectionError { status: None, .. }, .. }
           ))
       }
   }

   /// HTTP status codes that should trigger retry
   fn is_retryable_status(status: u16) -> bool {
       matches!(status, 500..=599 | 408 | 429)
   }
   ```

### Architecture Changes

1. **New File**: `src/sources/remote_source.rs`
2. **Modified**: `src/env.rs` - Add `fetch_url` method to `ConfigEnv` trait
3. **Modified**: `src/sources/mod.rs` - Module registration
4. **Modified**: `src/lib.rs` - Re-export `Remote`, `Format`, `Auth`, and `RetryPolicy`

```rust
// In lib.rs
#[cfg(feature = "remote")]
pub use sources::remote_source::{Auth, Format, Remote};

#[cfg(feature = "remote")]
pub use stillwater::retry::RetryPolicy;
```

### ConfigEnv Extension

```rust
// In env.rs

pub trait ConfigEnv: Send + Sync {
    // ... existing methods ...

    /// Fetch content from a URL (remote feature only).
    #[cfg(feature = "remote")]
    fn fetch_url(&self, request: &HttpRequest) -> Result<HttpResponse, std::io::Error>;
}

// RealEnv implementation
#[cfg(feature = "remote")]
impl ConfigEnv for RealEnv {
    fn fetch_url(&self, request: &HttpRequest) -> Result<HttpResponse, std::io::Error> {
        use reqwest::blocking::Client;

        let client = Client::builder()
            .timeout(request.timeout.unwrap_or(Duration::from_secs(30)))
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut req = match request.method {
            HttpMethod::Get => client.get(&request.url),
            HttpMethod::Post => client.post(&request.url),
        };

        for (key, value) in &request.headers {
            req = req.header(key, value);
        }

        let response = req.send()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))?;

        let status = response.status().as_u16();
        let headers = response.headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response.text()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(HttpResponse { status, body, headers })
    }
}

// MockEnv implementation
impl MockEnv {
    /// Add a mock HTTP response for a URL pattern.
    #[cfg(feature = "remote")]
    pub fn with_url_response(
        mut self,
        url: impl Into<String>,
        response: HttpResponse
    ) -> Self {
        self.url_responses.insert(url.into(), response);
        self
    }

    /// Add a mock HTTP error for a URL pattern.
    #[cfg(feature = "remote")]
    pub fn with_url_error(
        mut self,
        url: impl Into<String>,
        error: std::io::ErrorKind
    ) -> Self {
        self.url_errors.insert(url.into(), error);
        self
    }
}

#[cfg(feature = "remote")]
impl ConfigEnv for MockEnv {
    fn fetch_url(&self, request: &HttpRequest) -> Result<HttpResponse, std::io::Error> {
        if let Some(error_kind) = self.url_errors.get(&request.url) {
            return Err(std::io::Error::new(*error_kind, "mock error"));
        }

        self.url_responses
            .get(&request.url)
            .cloned()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no mock response for {}", request.url)
            ))
    }
}
```

### Error Handling

New error variant for `SourceErrorKind`:

```rust
pub enum SourceErrorKind {
    NotFound,
    IoError { message: String },
    ParseError { message: String },
    ConnectionError {
        url: String,
        status: Option<u16>,
        message: String,
    },
    /// Retries exhausted - includes metadata from stillwater's RetryExhausted pattern
    RetryExhausted {
        url: String,
        attempts: u32,
        total_duration: Duration,
        last_error: String,
    },
    Other { message: String },
}
```

Error scenarios:
- **Connection Timeout**: `ConnectionError` with "request timed out" message
- **Connection Refused**: `ConnectionError` with "connection refused" message
- **DNS Failure**: `ConnectionError` with "could not resolve host" message
- **HTTP 401**: `ConnectionError` with "authentication failed" message (not retried)
- **HTTP 403**: `ConnectionError` with "access forbidden" message (not retried)
- **HTTP 404**: `NotFound` error (not retried)
- **HTTP 5xx**: `ConnectionError` with status code and body excerpt (retried)
- **HTTP 408/429**: `ConnectionError` (retried - timeout/rate limit)
- **Retry Exhausted**: `RetryExhausted` with attempt count, total duration, and last error
- **Parse Error**: `ParseError` with format and error details (not retried)

### Security Considerations

1. **No Credential Logging**: Auth tokens and passwords must never appear in error messages
2. **Header Redaction**: Sensitive headers (Authorization, X-Api-Key) redacted in debug output
3. **HTTPS Preference**: Warn (not error) when using HTTP instead of HTTPS
4. **Certificate Validation**: Enable by default, provide `.danger_accept_invalid_certs()` for testing

## Dependencies

- **Prerequisites**: None
- **Affected Components**:
  - `Cargo.toml` - new dependency
  - `src/env.rs` - ConfigEnv trait extension
  - `src/error.rs` - ConnectionError and RetryExhausted variants
  - `src/sources/mod.rs` - module registration
  - `src/lib.rs` - re-export
- **External Dependencies**:
  - `reqwest` crate (blocking feature) - HTTP client
  - `stillwater` crate (already a dependency) - RetryPolicy for backoff strategies
- **Optional Dependencies**:
  - `stillwater/jitter` feature - for jitter support in retry policies

## Testing Strategy

### Unit Tests

1. **Basic Loading**
   - `test_remote_json_response` - Parse JSON response
   - `test_remote_toml_response` - Parse TOML response (with toml feature)
   - `test_remote_yaml_response` - Parse YAML response (with yaml feature)
   - `test_remote_auto_format_detection` - Detect format from Content-Type

2. **Authentication**
   - `test_remote_bearer_token` - Bearer token in Authorization header
   - `test_remote_basic_auth` - Basic auth encoding
   - `test_remote_custom_headers` - Custom header injection

3. **Error Handling**
   - `test_remote_connection_timeout` - Timeout returns ConnectionError
   - `test_remote_connection_refused` - Connection failure handling
   - `test_remote_404_required` - Missing required URL returns NotFound
   - `test_remote_404_optional` - Missing optional URL returns empty
   - `test_remote_401_error` - Authentication failure message
   - `test_remote_500_error` - Server error handling
   - `test_remote_parse_error` - Invalid response format

4. **Configuration**
   - `test_remote_path_extraction` - Extract nested path from response
   - `test_remote_custom_name` - Custom source naming
   - `test_remote_timeout_config` - Timeout configuration

5. **Retry Behavior** (using stillwater's RetryPolicy)
   - `test_remote_retry_exponential` - Exponential backoff with stillwater policy
   - `test_remote_retry_linear` - Linear backoff strategy
   - `test_remote_retry_with_jitter` - Jitter reduces thundering herd
   - `test_remote_retry_success_after_failures` - Mock failures then success
   - `test_remote_retry_exhausted_metadata` - Error includes attempts and duration
   - `test_remote_retry_only_transient` - 4xx errors not retried
   - `test_remote_retry_5xx` - 5xx errors are retried
   - `test_remote_retry_429_rate_limit` - Rate limit responses retried

6. **Security**
   - `test_credentials_not_in_errors` - Verify no credentials in error messages
   - `test_sensitive_headers_redacted` - Debug output hides secrets

### Integration Tests

- Test remote source in combination with local sources (layering)
- Test with real HTTP server in integration tests (optional, behind feature flag)

### Performance Tests

Not required for initial implementation.

### User Acceptance

- Common Consul/Vault response formats should parse correctly
- Error messages should clearly identify the URL and failure reason
- Timeouts should be respected and clearly reported

## Documentation Requirements

### Code Documentation

- Module-level documentation with examples (using `ignore` attribute)
- All public methods documented with examples
- Security considerations documented in module docs

### User Documentation

- Add remote source examples to CLAUDE.md
- Include remote in feature flag documentation
- Document authentication patterns
- Provide examples for Consul, Vault, and custom endpoints

### Architecture Updates

- Document ConfigEnv trait extension
- Document ConnectionError and RetryExhausted variants
- Document stillwater `RetryPolicy` integration pattern

## Implementation Notes

### Consul Integration Example

```rust
use stillwater::retry::RetryPolicy;

// HashiCorp Consul KV with custom retry policy
let consul = Remote::url("http://localhost:8500/v1/kv/myapp/config?raw")
    .format(Format::Json)
    .header("X-Consul-Token", std::env::var("CONSUL_TOKEN").unwrap())
    .timeout(Duration::from_secs(5))
    .retry_policy(
        RetryPolicy::exponential(Duration::from_millis(100))
            .with_max_retries(5)
            .with_max_delay(Duration::from_secs(5))
            .with_jitter(0.25)  // Requires stillwater jitter feature
    );

// Or use the convenience method
let consul_simple = Remote::url("http://localhost:8500/v1/kv/myapp/config?raw")
    .format(Format::Json)
    .header("X-Consul-Token", std::env::var("CONSUL_TOKEN").unwrap())
    .timeout(Duration::from_secs(5))
    .retry(3);  // Simple exponential backoff
```

### Vault Integration Example

```rust
use stillwater::retry::RetryPolicy;

// HashiCorp Vault KV v2 with linear backoff (for rate limiting)
let vault = Remote::url("http://localhost:8200/v1/secret/data/myapp")
    .format(Format::Json)
    .bearer_token(std::env::var("VAULT_TOKEN").unwrap())
    .path("data.data")  // Vault wraps secrets in data.data
    .timeout(Duration::from_secs(10))
    .retry_policy(
        RetryPolicy::linear(Duration::from_millis(500))
            .with_max_retries(3)
    );
```

### AWS Parameter Store (via HTTP)

```rust
// AWS SSM via HTTP endpoint (requires IAM auth setup)
let ssm = Remote::url("https://ssm.us-east-1.amazonaws.com/")
    .format(Format::Json)
    .header("X-Amz-Target", "AmazonSSM.GetParameters")
    // AWS Signature V4 would need separate auth implementation
```

### Retry Behavior (via stillwater)

Stillwater's `RetryPolicy` provides four backoff strategies:

1. **Constant**: Fixed delay between retries
   ```rust
   RetryPolicy::constant(Duration::from_millis(100))
   ```

2. **Linear**: Delay increases linearly: `base * (attempt + 1)`
   ```rust
   RetryPolicy::linear(Duration::from_millis(100))
   // Delays: 100ms, 200ms, 300ms, 400ms...
   ```

3. **Exponential**: Delay doubles: `base * 2^attempt`
   ```rust
   RetryPolicy::exponential(Duration::from_millis(100))
   // Delays: 100ms, 200ms, 400ms, 800ms...
   ```

4. **Fibonacci**: Delay follows Fibonacci sequence
   ```rust
   RetryPolicy::fibonacci(Duration::from_millis(100))
   // Delays: 100ms, 100ms, 200ms, 300ms, 500ms...
   ```

**Jitter support** (requires `stillwater/jitter` feature):
- `.with_jitter(0.25)` - ±25% proportional jitter
- `.with_full_jitter()` - Random between 0 and calculated delay
- `.with_decorrelated_jitter()` - AWS-style decorrelated jitter

**Bounds** (at least one required by stillwater):
- `.with_max_retries(5)` - Maximum retry attempts
- `.with_max_delay(Duration::from_secs(30))` - Cap on delay growth

**Only retry on transient errors:**
- Connection failures (timeout, refused, DNS)
- HTTP 5xx responses (server errors)
- HTTP 408 (request timeout)
- HTTP 429 (rate limited)

**Do not retry on:**
- HTTP 4xx responses (client errors like 401, 403, 404)
- Parse errors

### Feature Flag Testing

```bash
# Basic remote support
cargo test --features remote

# Remote with format support
cargo test --features "remote,toml"
cargo test --features "remote,yaml"

# Remote with jitter support (for advanced retry policies)
cargo test --features "remote" --features stillwater/jitter

# All features
cargo test --all-features
```

## Migration and Compatibility

### Breaking Changes

- `SourceErrorKind` gains new `ConnectionError` variant
- `SourceErrorKind` gains new `RetryExhausted` variant (with attempt/duration metadata)

### Migration Requirements

None - users opt-in by enabling the `remote` feature flag.

### Compatibility Considerations

- The `remote` feature should work independently and in combination with other features
- All existing tests must continue to pass
- The `full` feature already includes `remote` in its list
