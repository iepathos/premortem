---
number: 2
title: Remote Configuration Source Support
category: compatibility
priority: medium
status: draft
dependencies: []
created: 2025-11-25
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
6. **Retry Logic**: Configurable retry count with exponential backoff
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
- [ ] `.retry(3)` sets retry count with exponential backoff
- [ ] `.path("data.config")` extracts nested path from response
- [ ] `.named("vault-secrets")` sets custom source name
- [ ] Connection errors are reported as `SourceError::ConnectionError`
- [ ] Parse errors include the source URL and response details
- [ ] Timeout errors are clearly distinguished from connection failures
- [ ] Authentication failures (401/403) have specific error messages
- [ ] MockEnv supports mocking HTTP responses for testing
- [ ] Feature flag `remote` controls compilation
- [ ] `Remote` is re-exported from `premortem::sources` and `premortem` root
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
       retries: u32,
       path: Option<String>,
       name: Option<String>,
   }
   ```

5. **Builder Pattern Implementation**
   ```rust
   impl Remote {
       pub fn url(url: impl Into<String>) -> Self {
           Self {
               url: url.into(),
               format: Format::default(),
               auth: Auth::None,
               headers: Vec::new(),
               required: true,
               timeout: Duration::from_secs(30),
               retries: 0,
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

       pub fn retry(mut self, count: u32) -> Self {
           self.retries = count;
           self
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
   ```

### Architecture Changes

1. **New File**: `src/sources/remote_source.rs`
2. **Modified**: `src/env.rs` - Add `fetch_url` method to `ConfigEnv` trait
3. **Modified**: `src/sources/mod.rs` - Module registration
4. **Modified**: `src/lib.rs` - Re-export

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
    Other { message: String },
}
```

Error scenarios:
- **Connection Timeout**: `ConnectionError` with "request timed out" message
- **Connection Refused**: `ConnectionError` with "connection refused" message
- **DNS Failure**: `ConnectionError` with "could not resolve host" message
- **HTTP 401**: `ConnectionError` with "authentication failed" message
- **HTTP 403**: `ConnectionError` with "access forbidden" message
- **HTTP 404**: `NotFound` error
- **HTTP 5xx**: `ConnectionError` with status code and body excerpt
- **Parse Error**: `ParseError` with format and error details

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
  - `src/error.rs` - ConnectionError variant
  - `src/sources/mod.rs` - module registration
  - `src/lib.rs` - re-export
- **External Dependencies**: `reqwest` crate (blocking feature)

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
   - `test_remote_retry_config` - Retry behavior (mock multiple failures then success)

5. **Security**
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
- Document ConnectionError variant

## Implementation Notes

### Consul Integration Example

```rust
// HashiCorp Consul KV
let consul = Remote::url("http://localhost:8500/v1/kv/myapp/config?raw")
    .format(Format::Json)
    .header("X-Consul-Token", std::env::var("CONSUL_TOKEN").unwrap())
    .timeout(Duration::from_secs(5))
    .retry(3);
```

### Vault Integration Example

```rust
// HashiCorp Vault KV v2
let vault = Remote::url("http://localhost:8200/v1/secret/data/myapp")
    .format(Format::Json)
    .bearer_token(std::env::var("VAULT_TOKEN").unwrap())
    .path("data.data")  // Vault wraps secrets in data.data
    .timeout(Duration::from_secs(10));
```

### AWS Parameter Store (via HTTP)

```rust
// AWS SSM via HTTP endpoint (requires IAM auth setup)
let ssm = Remote::url("https://ssm.us-east-1.amazonaws.com/")
    .format(Format::Json)
    .header("X-Amz-Target", "AmazonSSM.GetParameters")
    // AWS Signature V4 would need separate auth implementation
```

### Retry Behavior

Exponential backoff with jitter:
- Retry 1: 100ms + random(0-50ms)
- Retry 2: 200ms + random(0-100ms)
- Retry 3: 400ms + random(0-200ms)
- Max delay capped at 10 seconds

Only retry on:
- Connection failures
- Timeout errors
- HTTP 5xx responses

Do not retry on:
- HTTP 4xx responses (client errors)
- Parse errors

### Feature Flag Testing

```bash
cargo test --features remote
cargo test --features "remote,toml"
cargo test --features "remote,yaml"
cargo test --all-features
```

## Migration and Compatibility

### Breaking Changes

- `SourceErrorKind` gains new `ConnectionError` variant

### Migration Requirements

None - users opt-in by enabling the `remote` feature flag.

### Compatibility Considerations

- The `remote` feature should work independently and in combination with other features
- All existing tests must continue to pass
- The `full` feature already includes `remote` in its list
