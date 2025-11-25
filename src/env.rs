//! ConfigEnv trait for testable I/O.
//!
//! This module provides the `ConfigEnv` trait that abstracts file system and
//! environment variable access, enabling dependency injection for testing.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Environment trait for configuration I/O operations.
///
/// This trait abstracts file system and environment variable access,
/// enabling dependency injection for testing.
///
/// # Stillwater Integration
///
/// Used as the `Env` parameter in `Effect<T, E, Env>`:
///
/// ```ignore
/// fn load<E: ConfigEnv>(&self) -> Effect<ConfigValues, ConfigErrors, E>
/// ```
///
/// # Example
///
/// ```ignore
/// // Production
/// let config = Config::<App>::builder()
///     .source(Toml::file("config.toml"))
///     .build();  // Uses RealEnv
///
/// // Testing
/// let env = MockEnv::new()
///     .with_file("config.toml", "[server]\nport = 8080");
/// let config = Config::<App>::builder()
///     .source(Toml::file("config.toml"))
///     .build_with_env(&env);
/// ```
pub trait ConfigEnv: Send + Sync {
    /// Read a file's contents as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if:
    /// - File does not exist (`ErrorKind::NotFound`)
    /// - File is not valid UTF-8
    /// - Permission denied
    /// - Other I/O errors
    fn read_file(&self, path: &Path) -> io::Result<String>;

    /// Check if a file exists.
    fn file_exists(&self, path: &Path) -> bool;

    /// Check if a path is a directory.
    fn is_directory(&self, path: &Path) -> bool;

    /// Get an environment variable by name.
    ///
    /// Returns `None` if the variable is not set.
    fn get_env(&self, name: &str) -> Option<String>;

    /// Get all environment variables matching a prefix.
    ///
    /// Returns tuples of (full_name, value).
    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)>;

    /// Get all environment variables.
    ///
    /// Used by Env source when no prefix is specified.
    fn all_env_vars(&self) -> Vec<(String, String)>;
}

/// Production environment using standard library I/O.
///
/// This is a zero-cost abstraction - all methods are simple wrappers
/// around std functions.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealEnv;

impl RealEnv {
    /// Create a new real environment.
    pub fn new() -> Self {
        Self
    }
}

impl ConfigEnv for RealEnv {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_directory(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn get_env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        std::env::vars()
            .filter(|(k, _)| k.starts_with(prefix))
            .collect()
    }

    fn all_env_vars(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }
}

/// Mock file state for testing.
#[derive(Debug, Clone)]
enum MockFile {
    Content(String),
    NotFound,
    PermissionDenied,
}

/// Mock environment for testing configuration loading.
///
/// # Example
///
/// ```
/// use premortem::env::MockEnv;
///
/// let env = MockEnv::new()
///     .with_file("config.toml", r#"
///         [database]
///         host = "localhost"
///         port = 5432
///     "#)
///     .with_file("secrets.toml", r#"
///         [database]
///         password = "secret123"
///     "#)
///     .with_env("APP_DATABASE_HOST", "prod-db.example.com")
///     .with_env("APP_LOG_LEVEL", "debug");
/// ```
#[derive(Debug, Default)]
pub struct MockEnv {
    files: RwLock<HashMap<PathBuf, MockFile>>,
    env_vars: RwLock<HashMap<String, String>>,
    directories: RwLock<Vec<PathBuf>>,
}

impl MockEnv {
    /// Create a new empty mock environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file with content.
    ///
    /// The path can be relative or absolute.
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<String>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::Content(content.into()));
        self
    }

    /// Add a file that will return "not found" error.
    ///
    /// Useful for testing optional file handling.
    pub fn with_missing_file(self, path: impl Into<PathBuf>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::NotFound);
        self
    }

    /// Add a file that will return "permission denied" error.
    pub fn with_unreadable_file(self, path: impl Into<PathBuf>) -> Self {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::PermissionDenied);
        self
    }

    /// Add a directory path.
    pub fn with_directory(self, path: impl Into<PathBuf>) -> Self {
        self.directories.write().unwrap().push(path.into());
        self
    }

    /// Set an environment variable.
    pub fn with_env(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars
            .write()
            .unwrap()
            .insert(name.into(), value.into());
        self
    }

    /// Set multiple environment variables from an iterator.
    pub fn with_envs<I, K, V>(self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut env_vars = self.env_vars.write().unwrap();
        for (k, v) in vars {
            env_vars.insert(k.into(), v.into());
        }
        drop(env_vars);
        self
    }

    /// Mutate the mock environment after creation.
    ///
    /// Useful for tests that modify files during execution.
    pub fn set_file(&self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files
            .write()
            .unwrap()
            .insert(path.into(), MockFile::Content(content.into()));
    }

    /// Remove a file from the mock environment.
    pub fn remove_file(&self, path: impl AsRef<Path>) {
        self.files.write().unwrap().remove(path.as_ref());
    }

    /// Update an environment variable.
    pub fn set_env(&self, name: impl Into<String>, value: impl Into<String>) {
        self.env_vars
            .write()
            .unwrap()
            .insert(name.into(), value.into());
    }

    /// Remove an environment variable.
    pub fn remove_env(&self, name: &str) {
        self.env_vars.write().unwrap().remove(name);
    }
}

impl ConfigEnv for MockEnv {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        let files = self.files.read().unwrap();

        match files.get(path) {
            Some(MockFile::Content(content)) => Ok(content.clone()),
            Some(MockFile::NotFound) | None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("mock file not found: {}", path.display()),
            )),
            Some(MockFile::PermissionDenied) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("mock permission denied: {}", path.display()),
            )),
        }
    }

    fn file_exists(&self, path: &Path) -> bool {
        let files = self.files.read().unwrap();
        matches!(files.get(path), Some(MockFile::Content(_)))
    }

    fn is_directory(&self, path: &Path) -> bool {
        self.directories
            .read()
            .unwrap()
            .contains(&path.to_path_buf())
    }

    fn get_env(&self, name: &str) -> Option<String> {
        self.env_vars.read().unwrap().get(name).cloned()
    }

    fn env_vars_with_prefix(&self, prefix: &str) -> Vec<(String, String)> {
        self.env_vars
            .read()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    fn all_env_vars(&self) -> Vec<(String, String)> {
        self.env_vars
            .read()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_env_file_exists() {
        let env = RealEnv::new();
        // Cargo.toml should exist in the project root
        assert!(env.file_exists(Path::new("Cargo.toml")));
        assert!(!env.file_exists(Path::new("nonexistent.toml")));
    }

    #[test]
    fn test_mock_env_files() {
        let env = MockEnv::new()
            .with_file("config.toml", "host = \"localhost\"")
            .with_file("other.toml", "port = 8080");

        assert!(env.file_exists(Path::new("config.toml")));
        assert!(env.file_exists(Path::new("other.toml")));
        assert!(!env.file_exists(Path::new("missing.toml")));

        let content = env.read_file(Path::new("config.toml")).unwrap();
        assert_eq!(content, "host = \"localhost\"");
    }

    #[test]
    fn test_mock_env_missing_file() {
        let env = MockEnv::new();

        let result = env.read_file(Path::new("missing.toml"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn test_mock_env_permission_denied() {
        let env = MockEnv::new().with_unreadable_file("secret.toml");

        let result = env.read_file(Path::new("secret.toml"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn test_mock_env_vars() {
        let env = MockEnv::new()
            .with_env("APP_HOST", "localhost")
            .with_env("APP_PORT", "8080")
            .with_env("OTHER_VAR", "value");

        assert_eq!(env.get_env("APP_HOST"), Some("localhost".to_string()));
        assert_eq!(env.get_env("APP_PORT"), Some("8080".to_string()));
        assert_eq!(env.get_env("MISSING"), None);

        let app_vars = env.env_vars_with_prefix("APP_");
        assert_eq!(app_vars.len(), 2);

        let all_vars = env.all_env_vars();
        assert_eq!(all_vars.len(), 3);
    }

    #[test]
    fn test_mock_env_mutations() {
        let env = MockEnv::new()
            .with_file("config.toml", "original")
            .with_env("VAR", "original");

        // Mutate file
        env.set_file("config.toml", "modified");
        assert_eq!(env.read_file(Path::new("config.toml")).unwrap(), "modified");

        // Mutate env var
        env.set_env("VAR", "modified");
        assert_eq!(env.get_env("VAR"), Some("modified".to_string()));

        // Remove file
        env.remove_file("config.toml");
        assert!(!env.file_exists(Path::new("config.toml")));

        // Remove env var
        env.remove_env("VAR");
        assert_eq!(env.get_env("VAR"), None);
    }

    #[test]
    fn test_mock_env_directories() {
        let env = MockEnv::new()
            .with_directory("/etc/myapp")
            .with_file("/etc/myapp/config.toml", "content");

        assert!(env.is_directory(Path::new("/etc/myapp")));
        assert!(!env.is_directory(Path::new("/etc/myapp/config.toml")));
        assert!(!env.is_directory(Path::new("/other")));
    }
}
