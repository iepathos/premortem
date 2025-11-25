//! Value types for configuration representation.
//!
//! This module provides the `Value` enum for representing configuration values
//! in an intermediate format before deserialization into the target type.

use std::collections::BTreeMap;

use crate::error::SourceLocation;

/// Raw value representation for configuration data.
///
/// This enum represents configuration values in a generic format that can
/// be loaded from various sources (TOML, JSON, YAML, environment variables)
/// before being deserialized into the target configuration struct.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    /// Null/missing value
    #[default]
    Null,
    /// Boolean value
    Bool(bool),
    /// Integer value
    Integer(i64),
    /// Floating-point value
    Float(f64),
    /// String value
    String(String),
    /// Array of values
    Array(Vec<Value>),
    /// Table/object of key-value pairs
    Table(BTreeMap<String, Value>),
}

impl Value {
    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get this value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get this value as an integer.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to get this value as a float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to get this value as a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get this value as an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Try to get this value as a table.
    pub fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(table) => Some(table),
            _ => None,
        }
    }

    /// Get a value by dot-notation path (e.g., "database.host").
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        self.get_path_parts(&parts)
    }

    fn get_path_parts(&self, parts: &[&str]) -> Option<&Value> {
        if parts.is_empty() {
            return Some(self);
        }

        match self {
            Value::Table(table) => table
                .get(parts[0])
                .and_then(|v| v.get_path_parts(&parts[1..])),
            _ => None,
        }
    }

    /// Get a human-readable type name for this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Table(_) => "table",
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Integer(i64::from(i))
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value>> From<BTreeMap<String, T>> for Value {
    fn from(m: BTreeMap<String, T>) -> Self {
        Value::Table(m.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

/// A configuration value with source tracking.
///
/// This struct wraps a `Value` with information about where it originated,
/// enabling detailed error messages that pinpoint the exact source.
#[derive(Debug, Clone)]
pub struct ConfigValue {
    /// The actual value
    pub value: Value,
    /// Where this value came from
    pub source: SourceLocation,
}

impl ConfigValue {
    /// Create a new config value with source tracking.
    pub fn new(value: impl Into<Value>, source: SourceLocation) -> Self {
        Self {
            value: value.into(),
            source,
        }
    }

    /// Create a config value without a specific source location.
    pub fn anonymous(value: impl Into<Value>) -> Self {
        Self {
            value: value.into(),
            source: SourceLocation::new("unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_type_checks() {
        assert!(Value::Null.is_null());
        assert!(!Value::Bool(true).is_null());

        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Integer(42).as_integer(), Some(42));
        assert_eq!(Value::Float(2.71).as_float(), Some(2.71));
        assert_eq!(Value::Integer(42).as_float(), Some(42.0));
        assert_eq!(Value::String("hello".to_string()).as_str(), Some("hello"));
    }

    #[test]
    fn test_value_get_path() {
        let mut inner = BTreeMap::new();
        inner.insert("host".to_string(), Value::String("localhost".to_string()));
        inner.insert("port".to_string(), Value::Integer(5432));

        let mut root = BTreeMap::new();
        root.insert("database".to_string(), Value::Table(inner));

        let value = Value::Table(root);

        assert_eq!(
            value.get_path("database.host").and_then(|v| v.as_str()),
            Some("localhost")
        );
        assert_eq!(
            value.get_path("database.port").and_then(|v| v.as_integer()),
            Some(5432)
        );
        assert!(value.get_path("database.password").is_none());
        assert!(value.get_path("other").is_none());
    }

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Integer(42).type_name(), "integer");
        assert_eq!(Value::Float(2.71).type_name(), "float");
        assert_eq!(Value::String("test".to_string()).type_name(), "string");
        assert_eq!(Value::Array(vec![]).type_name(), "array");
        assert_eq!(Value::Table(BTreeMap::new()).type_name(), "table");
    }

    #[test]
    fn test_value_from_conversions() {
        let _: Value = true.into();
        let _: Value = 42i64.into();
        let _: Value = 42i32.into();
        let _: Value = 2.71f64.into();
        let _: Value = "hello".into();
        let _: Value = String::from("hello").into();
        let _: Value = vec![1i64, 2, 3].into();
    }

    #[test]
    fn test_config_value() {
        let cv = ConfigValue::new("localhost", SourceLocation::new("config.toml").with_line(5));
        assert_eq!(cv.value.as_str(), Some("localhost"));
        assert_eq!(cv.source.source, "config.toml");
        assert_eq!(cv.source.line, Some(5));
    }
}
