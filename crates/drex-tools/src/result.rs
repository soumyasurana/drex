//! Result types returned by tool execution

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The status of a tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Tool executed successfully
    Success,
    /// Tool execution was cancelled
    Cancelled,
    /// Tool execution failed
    Failed,
}

impl ExecutionStatus {
    /// Check if execution was successful
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Check if execution failed
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    /// Check if execution was cancelled
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }
}

/// The result of executing a tool.
///
/// Tools return structured results that can contain arbitrary JSON data,
/// not just strings. This allows tools to communicate rich information
/// to calling agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionResult {
    /// The execution status
    pub status: ExecutionStatus,
    /// The result data (None if failed)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<Value>,
    /// Error message (only present if status is Failed)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    /// Execution duration in milliseconds (if known)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub duration_ms: Option<u64>,
}

impl ExecutionResult {
    /// Create a successful result with data
    pub fn success(data: impl Into<Value>) -> Self {
        Self {
            status: ExecutionStatus::Success,
            data: Some(data.into()),
            error: None,
            duration_ms: None,
        }
    }

    /// Create a successful result with JSON data
    pub fn success_json(data: Value) -> Self {
        Self {
            status: ExecutionStatus::Success,
            data: Some(data),
            error: None,
            duration_ms: None,
        }
    }

    /// Create a failed result with an error message
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            status: ExecutionStatus::Failed,
            data: None,
            error: Some(error.into()),
            duration_ms: None,
        }
    }

    /// Create a cancelled result
    pub fn cancelled() -> Self {
        Self {
            status: ExecutionStatus::Cancelled,
            data: None,
            error: None,
            duration_ms: None,
        }
    }

    /// Add duration information
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    /// Get the result data as a reference
    pub fn data(&self) -> Option<&Value> {
        self.data.as_ref()
    }

    /// Get the result data as a specific type (deserialized from JSON)
    pub fn data_as<T: for<'de> Deserialize<'de>>(&self) -> Option<T> {
        self.data.as_ref().and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Get the error message
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Check if the result contains some data (useful for optional returns)
    pub fn has_data(&self) -> bool {
        self.data.is_some()
    }

    /// Check if the execution was successful
    pub fn is_success(&self) -> bool {
        matches!(self.status, ExecutionStatus::Success)
    }

    /// Check if the execution failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, ExecutionStatus::Failed)
    }

    /// Check if the execution was cancelled
    pub fn is_cancelled(&self) -> bool {
        matches!(self.status, ExecutionStatus::Cancelled)
    }
}

impl From<String> for ExecutionResult {
    fn from(s: String) -> Self {
        Self::success(s)
    }
}

impl From<&str> for ExecutionResult {
    fn from(s: &str) -> Self {
        Self::success(s.to_string())
    }
}

impl From<Value> for ExecutionResult {
    fn from(value: Value) -> Self {
        Self::success_json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn result_success() {
        let result = ExecutionResult::success("hello");
        assert!(result.status.is_success());
        assert_eq!(result.data(), Some(&json!("hello")));
        assert!(result.error().is_none());
    }

    #[test]
    fn result_success_json() {
        let data = json!({"key": "value", "count": 42});
        let result = ExecutionResult::success_json(data.clone());
        assert!(result.status.is_success());
        assert_eq!(result.data(), Some(&data));
    }

    #[test]
    fn result_failed() {
        let result = ExecutionResult::failed("something went wrong");
        assert!(result.status.is_failed());
        assert!(result.data().is_none());
        assert_eq!(result.error(), Some("something went wrong"));
    }

    #[test]
    fn result_cancelled() {
        let result = ExecutionResult::cancelled();
        assert!(result.status.is_cancelled());
        assert!(result.data().is_none());
        assert!(result.error().is_none());
    }

    #[test]
    fn result_with_duration() {
        let result = ExecutionResult::success("test").with_duration(150);
        assert_eq!(result.duration_ms, Some(150));
    }

    #[test]
    fn result_data_as_deserialization() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct TestData {
            message: String,
            count: i32,
        }

        let result = ExecutionResult::success_json(json!({
            "message": "hello",
            "count": 42
        }));

        let data: Option<TestData> = result.data_as();
        assert!(data.is_some());
        assert_eq!(data.unwrap().message, "hello");
    }

    #[test]
    fn result_from_string() {
        let result: ExecutionResult = "test message".into();
        assert!(result.status.is_success());
        assert_eq!(result.data(), Some(&json!("test message")));
    }

    #[test]
    fn result_from_json_value() {
        let result: ExecutionResult = json!({"x": 1, "y": 2}).into();
        assert!(result.status.is_success());
        assert_eq!(result.data(), Some(&json!({"x": 1, "y": 2})));
    }
}
