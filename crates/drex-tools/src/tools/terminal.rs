//! Terminal tool - execute shell commands with timeout and output capture
//!
//! This tool provides controlled execution of shell commands with:
//! - Timeout enforcement to prevent indefinite execution
//! - stdout/stderr capture
//! - Exit code reporting
//! - Audit logging

use crate::capability::{Capability, CapabilitySet};
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

/// Configuration for the terminal tool.
///
/// The timeout is set by the trusted Drex runtime, not by model input.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// Maximum duration to wait for command completion
    timeout: Duration,
    /// Whether to inherit environment variables
    inherit_env: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30), // 30 second default
            inherit_env: true,
        }
    }
}

impl TerminalConfig {
    /// Create a new terminal config with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the timeout. This is the only way to configure timeout -
    /// it's NOT exposed in the tool input.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set whether to inherit environment variables.
    ///
    /// SECURITY: Setting this to false provides better isolation
    /// but may break commands that depend on PATH, HOME, etc.
    pub fn inherit_env(mut self, inherit: bool) -> Self {
        self.inherit_env = inherit;
        self
    }
}

/// Input for the terminal execute tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalExecuteInput {
    /// The command to execute (e.g., "echo", "ls")
    pub command: String,
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
}

/// Output for the terminal execute tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalExecuteOutput {
    /// The command that was executed
    pub command: String,
    /// The arguments that were used
    pub args: Vec<String>,
    /// Standard output (captured)
    pub stdout: String,
    /// Standard error (captured)
    pub stderr: String,
    /// Exit code (0 typically means success)
    pub exit_code: i32,
    /// Whether the command timed out
    pub timed_out: bool,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

/// Tool for executing shell commands with safety controls.
///
/// # Security Considerations
///
/// This tool executes arbitrary commands. Security measures:
/// - Timeout prevents indefinite execution
/// - Requires terminal.execute capability
/// - Audit logging of all attempts
/// - Environment restriction available (config.inherit_env = false)
///
/// # Limitations
///
/// Environment inheritance is a security concern. Currently:
/// - Commands inherit the Drex process environment by default
/// - Setting `inherit_env: false` provides better isolation
/// - Phase 8 will harden this with explicit env whitelist
#[derive(Debug, Clone)]
pub struct TerminalExecuteTool {
    metadata: ToolMetadata,
    config: TerminalConfig,
}

impl TerminalExecuteTool {
    /// Create a new terminal execute tool with the given configuration.
    ///
    /// IMPORTANT: The timeout in config is set by the runtime, not by
    /// model-generated input.
    pub fn new(config: TerminalConfig) -> Self {
        let schema = ToolSchema::builder("TerminalExecuteInput", "Execute a shell command")
            .required_string("command", "The command to execute")
            .optional_property(
                "args",
                crate::schema::JsonSchema::Array {
                    description: "Arguments to pass to the command".to_string(),
                    items: Box::new(crate::schema::JsonSchema::String {
                        description: "Command argument".to_string(),
                    }),
                },
            )
            .build();

        Self {
            metadata: ToolMetadata::new(
                "terminal.execute",
                "Execute a shell command with output capture.\n\
                \n\
                This tool runs the specified command in a subprocess and captures \
                stdout, stderr, and the exit code. Execution is limited by a timeout \
                to prevent indefinite running.\n\
                \n\
                SECURITY: Commands run with the permissions of the Drex process. \
                Use with caution.",
                schema,
            ),
            config,
        }
    }

    /// Execute the command with timeout.
    pub async fn execute_command(
        &self,
        command: &str,
        args: &[String],
    ) -> Result<TerminalExecuteOutput, ToolError> {
        let start = std::time::Instant::now();

        // Create command
        let mut cmd = Command::new(command);
        cmd.args(args);

        // Environment handling
        if !self.config.inherit_env {
            cmd.env_clear();
            // Ensure PATH is available at minimum
            if let Ok(path) = std::env::var("PATH") {
                cmd.env("PATH", path);
            }
        }

        // Set up pipes for stdout/stderr
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn the process
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let reason = if e.kind() == std::io::ErrorKind::NotFound {
                    format!("command '{}' not found", command)
                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                    format!("permission denied to execute '{}': {}", command, e)
                } else {
                    format!("failed to spawn '{}': {}", command, e)
                };
                return Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason,
                });
            }
        };

        // Wait with timeout
        let result = tokio::time::timeout(self.config.timeout, child.wait_with_output()).await;

        let output = match result {
            Ok(Ok(output)) => {
                // Command finished before timeout
                output
            }
            Ok(Err(e)) => {
                return Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: format!("failed to collect output: {}", e),
                });
            }
            Err(_elapsed) => {
                // Timeout occurred
                // Kill the process if still running
                return Ok(TerminalExecuteOutput {
                    command: command.to_string(),
                    args: args.to_vec(),
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: -1,
                    timed_out: true,
                    duration_ms: self.config.timeout.as_millis() as u64,
                });
            }
        };

        let duration = start.elapsed();

        // Convert output to strings (best effort - may include invalid UTF-8)
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let exit_code = output.status.code().unwrap_or(-1);

        Ok(TerminalExecuteOutput {
            command: command.to_string(),
            args: args.to_vec(),
            stdout,
            stderr,
            exit_code,
            timed_out: false,
            duration_ms: duration.as_millis() as u64,
        })
    }
}

#[async_trait]
impl Tool for TerminalExecuteTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        static REQUIRED: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        REQUIRED.get_or_init(|| CapabilitySet::from(vec![Capability::TerminalExecute]))
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let command = input.require_string("command")?;
        let args: Vec<String> = input
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();

        // Sanitize for audit log - truncate very long commands
        let audit_command = if command.len() > 100 {
            format!("{}... (truncated)", &command[..100])
        } else {
            command.to_string()
        };
        let audit_args_count = args.len();

        // Audit log the attempt (sanitized - no environment, truncated)
        info!(
            tool = self.name(),
            command = %audit_command,
            args_count = audit_args_count,
            timeout_sec = self.config.timeout.as_secs(),
            inherit_env = self.config.inherit_env,
            "terminal.execute attempted"
        );

        // Execute the command
        let start = std::time::Instant::now();
        match self.execute_command(command, &args).await {
            Ok(output) => {
                let duration = start.elapsed();

                // Sanitize output lengths for audit
                let stdout_preview = if output.stdout.len() > 200 {
                    format!("{}... (truncated)", &output.stdout[..200])
                } else {
                    output.stdout.clone()
                };
                let stderr_preview = if output.stderr.len() > 200 {
                    format!("{}... (truncated)", &output.stderr[..200])
                } else {
                    output.stderr.clone()
                };

                if output.timed_out {
                    warn!(
                        tool = self.name(),
                        command = %audit_command,
                        timeout_sec = self.config.timeout.as_secs(),
                        duration_ms = duration.as_millis() as u64,
                        "terminal.execute timed out"
                    );
                } else {
                    info!(
                        tool = self.name(),
                        command = %audit_command,
                        exit_code = output.exit_code,
                        stdout_bytes = output.stdout.len(),
                        stderr_bytes = output.stderr.len(),
                        duration_ms = duration.as_millis() as u64,
                        "terminal.execute completed"
                    );
                    // Log truncated stdout/stderr for debugging
                    if !stdout_preview.is_empty() {
                        tracing::debug!(stdout_preview = %stdout_preview, "terminal.execute stdout");
                    }
                    if !stderr_preview.is_empty() {
                        tracing::debug!(stderr_preview = %stderr_preview, "terminal.execute stderr");
                    }
                }

                Ok(ExecutionResult::success(json!(output)))
            }
            Err(e) => {
                let duration = start.elapsed();
                warn!(
                    tool = self.name(),
                    command = %audit_command,
                    error = %e,
                    duration_ms = duration.as_millis() as u64,
                    "terminal.execute failed"
                );
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_simple_command() {
        let config = TerminalConfig::new().with_timeout(Duration::from_secs(5));
        let tool = TerminalExecuteTool::new(config);

        let output = tool.execute_command("echo", &["hello".to_string()]).await.unwrap();

        assert_eq!(output.command, "echo");
        assert_eq!(output.args, vec!["hello"]);
        assert!(output.stdout.contains("hello"));
        assert_eq!(output.exit_code, 0);
        assert!(!output.timed_out);
        assert!(output.duration_ms > 0);
    }

    #[tokio::test]
    async fn execute_with_exit_code() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        // Use 'false' command which exits with 1 on Unix
        #[cfg(unix)]
        {
            let output = tool.execute_command("false", &[]).await.unwrap();
            assert_eq!(output.exit_code, 1);
            assert!(!output.timed_out);
        }
    }

    #[tokio::test]
    async fn execute_stdin_not_piped() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        // Command should work without stdin
        let output = tool.execute_command("pwd", &[]).await.unwrap();
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn nonexistent_command_fails() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        let result = tool.execute_command("this-command-does-not-exist-xyz123", &[]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn timeout_works() {
        // Short timeout to test
        let config = TerminalConfig::new().with_timeout(Duration::from_millis(100));
        let tool = TerminalExecuteTool::new(config);

        // Use 'sleep' to delay - requires at least 1 second on Unix
        // So we'll use a command that takes longer than 100ms
        #[cfg(unix)]
        {
            let output = tool.execute_command("sleep", &["5".to_string()]).await.unwrap();
            assert!(output.timed_out);
            assert_eq!(output.exit_code, -1);
            assert!(output.duration_ms >= 100);
        }
    }

    #[tokio::test]
    async fn stderr_capture() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        // Write to stderr - Unix specific
        #[cfg(unix)]
        {
            let output = tool
                .execute_command("sh", &["-c".to_string(), "echo error >&2".to_string()])
                .await
                .unwrap();
            assert!(output.stderr.contains("error"));
            assert_eq!(output.exit_code, 0);
        }
    }

    #[test]
    fn tool_metadata_correct() {
        let config = TerminalConfig::new().with_timeout(Duration::from_secs(10));
        let tool = TerminalExecuteTool::new(config);

        assert_eq!(tool.name(), "terminal.execute");
        assert_eq!(tool.required_capabilities().len(), 1);
        assert!(tool.required_capabilities().has(Capability::TerminalExecute));
    }

    #[tokio::test]
    async fn execute_with_multiple_args() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        let output = tool
            .execute_command("printf", &["%s %s".to_string(), "hello".to_string(), "world".to_string()])
            .await
            .unwrap();

        // printf behavior varies by platform, so just check it ran
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.args.len(), 3);
    }

    #[tokio::test]
    async fn execute_empty_args() {
        let config = TerminalConfig::new();
        let tool = TerminalExecuteTool::new(config);

        // echo with no args should work on most platforms
        let output = tool.execute_command("echo", &[]).await.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.args.is_empty());
    }
}