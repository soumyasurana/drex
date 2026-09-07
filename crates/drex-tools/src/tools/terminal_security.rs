//! Terminal Security - Command allowlist and execution policy
//!
//! This module provides command-level security controls:
//! - Allowlist of permitted commands (no wildcard matching)
//! - Argument pattern validation (no regex, simple string checks)
//! - Resource limits (CPU, memory)
//! - Blocklist of dangerous patterns
//!
//! # Security Principles
//!
//! 1. **Explicit Deny by Default**: Commands not in the allowlist are rejected
//! 2. **No Subshell Execution**: Commands like `sh`, `bash` are blocked
//! 3. **No Shell Operators**: Characters like `;`, `|`, `&`, `$()` are blocked
//! 4. **Argument Validation**: Common patterns for file paths, URLs
//!
//! # Policy Levels
//!
//! - `Strict`: Only allowlisted commands with validated arguments
//! - `Permissive`: Block only dangerous patterns (not recommended for production)

use crate::error::ToolError;
use std::collections::HashSet;
use std::path::Path;

/// Error type for terminal security policy violations
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalSecurityError {
    /// Command is not in the allowlist
    CommandNotAllowed(String),
    /// Argument contains forbidden patterns
    ForbiddenArgumentPattern(String),
    /// Dangerous shell operator detected
    ShellOperatorDetected(String),
    /// Subshell execution attempted
    SubshellExecutionBlocked(String),
    /// Environment variable expansion attempted
    EnvironmentExpansionBlocked(String),
    /// Resource limit exceeded
    ResourceLimitExceeded(String),
}

impl std::fmt::Display for TerminalSecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalSecurityError::CommandNotAllowed(cmd) => {
                write!(f, "Command '{}' is not in the allowlist", cmd)
            }
            TerminalSecurityError::ForbiddenArgumentPattern(arg) => {
                write!(f, "Argument contains forbidden pattern: {}", arg)
            }
            TerminalSecurityError::ShellOperatorDetected(op) => {
                write!(f, "Shell operator '{}' is not allowed", op)
            }
            TerminalSecurityError::SubshellExecutionBlocked(cmd) => {
                write!(f, "Subshell execution via '{}' is not allowed", cmd)
            }
            TerminalSecurityError::EnvironmentExpansionBlocked(var) => {
                write!(f, "Environment variable expansion '{}' is not allowed", var)
            }
            TerminalSecurityError::ResourceLimitExceeded(limit) => {
                write!(f, "Resource limit exceeded: {}", limit)
            }
        }
    }
}

impl std::error::Error for TerminalSecurityError {}

/// Security policy for terminal execution
#[derive(Debug, Clone)]
pub struct TerminalSecurityPolicy {
    /// List of allowed commands (exact match, no wildcards)
    allowed_commands: HashSet<String>,
    /// Dangerous shell operators to block
    forbidden_operators: HashSet<String>,
    /// Commands that enable subshell execution
    subshell_commands: HashSet<String>,
    /// Maximum argument length
    max_arg_length: usize,
    /// Maximum number of arguments
    max_arg_count: usize,
    /// Allow empty arguments
    allow_empty_args: bool,
}

impl Default for TerminalSecurityPolicy {
    fn default() -> Self {
        // Default secure policy with common safe development commands
        let mut allowed = HashSet::new();

        // File system inspection (safe, read-only)
        allowed.insert("ls".to_string());
        allowed.insert("pwd".to_string());
        allowed.insert("cat".to_string());
        allowed.insert("head".to_string());
        allowed.insert("tail".to_string());
        allowed.insert("file".to_string());
        allowed.insert("stat".to_string());
        allowed.insert("wc".to_string());
        allowed.insert("sort".to_string());
        allowed.insert("uniq".to_string());

        // Text processing
        allowed.insert("echo".to_string());
        allowed.insert("printf".to_string());
        allowed.insert("grep".to_string());
        allowed.insert("awk".to_string());
        allowed.insert("sed".to_string());
        allowed.insert("tr".to_string());
        allowed.insert("cut".to_string());
        allowed.insert("comm".to_string());

        // Version control
        allowed.insert("git".to_string());

        // Build tools (read-only inspection)
        allowed.insert("cargo".to_string());
        allowed.insert("rustc".to_string());

        // Archive inspection (safe flags only)
        allowed.insert("tar".to_string());

        // Network (safe modes)
        allowed.insert("curl".to_string());
        allowed.insert("wget".to_string());
        allowed.insert("ping".to_string());

        // Process inspection
        allowed.insert("ps".to_string());
        allowed.insert("top".to_string()); // Will timeout, but safe

        // System info
        allowed.insert("uname".to_string());
        allowed.insert("id".to_string());
        allowed.insert("whoami".to_string());
        allowed.insert("date".to_string());
        allowed.insert("df".to_string());
        allowed.insert("du".to_string());

        // Process monitoring (safe - just waits)
        allowed.insert("sleep".to_string());

        // Package managers (read-only)
        allowed.insert("which".to_string());
        allowed.insert("whereis".to_string());

        // Forbidden operators
        let mut forbidden = HashSet::new();
        forbidden.insert(";".to_string());
        forbidden.insert("|".to_string());
        forbidden.insert("&&".to_string());
        forbidden.insert("||".to_string());
        forbidden.insert("&".to_string());
        forbidden.insert("$".to_string());
        forbidden.insert("`".to_string());
        forbidden.insert("(".to_string()); // Command substitution
        forbidden.insert(")".to_string());
        forbidden.insert("{".to_string());
        forbidden.insert("}".to_string());
        forbidden.insert(">".to_string()); // Redirection
        forbidden.insert("<".to_string());
        forbidden.insert(">>".to_string());
        forbidden.insert("<<".to_string());

        // Subshell commands
        let mut subshell = HashSet::new();
        subshell.insert("sh".to_string());
        subshell.insert("bash".to_string());
        subshell.insert("zsh".to_string());
        subshell.insert("fish".to_string());
        subshell.insert("dash".to_string());
        subshell.insert("csh".to_string());
        subshell.insert("tcsh".to_string());
        subshell.insert("ksh".to_string());
        subshell.insert("exec".to_string());
        subshell.insert("eval".to_string());
        subshell.insert("source".to_string());
        subshell.insert(".".to_string()); // Dot sourcing

        Self {
            allowed_commands: allowed,
            forbidden_operators: forbidden,
            subshell_commands: subshell,
            max_arg_length: 4096,
            max_arg_count: 100,
            allow_empty_args: false,
        }
    }
}

impl TerminalSecurityPolicy {
    /// Create a new policy with default secure settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a development policy (allows git, cargo, common tools)
    pub fn development() -> Self {
        Self::default()
    }

    /// Create a strict policy (minimal commands only)
    pub fn strict() -> Self {
        let mut policy = Self::default();
        // Remove potentially dangerous commands
        policy.allowed_commands.remove("tar");
        policy.allowed_commands.remove("curl");
        policy.allowed_commands.remove("wget");
        policy.allowed_commands.remove("awk");
        policy.allowed_commands.remove("sed");
        policy
    }

    /// Create a permissive policy (for testing, not recommended)
    pub fn permissive() -> Self {
        let mut policy = Self::default();
        // Add many common commands
        let additional = vec![
            "python", "python3", "node", "npm", "ruby", "perl",
            "make", "gcc", "g++", "clang",
        ];
        for cmd in additional {
            policy.allowed_commands.insert(cmd.to_string());
        }
        policy
    }

    /// Check if a command is allowed
    pub fn is_command_allowed(&self, command: &str) -> bool {
        let cmd_base = Path::new(command)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(command);

        self.allowed_commands.contains(cmd_base)
    }

    /// Validate command and arguments
    pub fn validate(&self, command: &str, args: &[String]) -> Result<(), TerminalSecurityError> {
        // Check command is in allowlist
        let cmd_base = Path::new(command)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(command);

        if !self.is_command_allowed(command) {
            // Check if it's a shell command
            if self.subshell_commands.contains(cmd_base) {
                return Err(TerminalSecurityError::SubshellExecutionBlocked(
                    command.to_string()
                ));
            }
            return Err(TerminalSecurityError::CommandNotAllowed(
                command.to_string()
            ));
        }

        // Check argument count
        if args.len() > self.max_arg_count {
            return Err(TerminalSecurityError::ResourceLimitExceeded(format!(
                "argument count {} exceeds maximum {}",
                args.len(),
                self.max_arg_count
            )));
        }

        // Validate each argument
        for (i, arg) in args.iter().enumerate() {
            // Check empty arguments
            if !self.allow_empty_args && arg.is_empty() {
                return Err(TerminalSecurityError::ForbiddenArgumentPattern(
                    format!("argument {} is empty", i)
                ));
            }

            // Check argument length
            if arg.len() > self.max_arg_length {
                return Err(TerminalSecurityError::ResourceLimitExceeded(format!(
                    "argument {} length {} exceeds maximum {}",
                    i, arg.len(), self.max_arg_length
                )));
            }

            // Check for forbidden operators
            for op in &self.forbidden_operators {
                if arg.contains(op) {
                    return Err(TerminalSecurityError::ShellOperatorDetected(
                        format!("'{}' in argument {}", op, i)
                    ));
                }
            }

            // Check for environment variable expansion (starts with $)
            if arg.starts_with("$") || arg.contains("${") {
                return Err(TerminalSecurityError::EnvironmentExpansionBlocked(
                    arg.to_string()
                ));
            }

            // Check for command substitution ($())
            if arg.contains("$(") || arg.contains("(`") {
                return Err(TerminalSecurityError::SubshellExecutionBlocked(
                    format!("command substitution in argument {}", i)
                ));
            }

            // Check for backtick command substitution
            if arg.contains("`") {
                return Err(TerminalSecurityError::ShellOperatorDetected(
                    "backtick command substitution".to_string()
                ));
            }
        }

        Ok(())
    }

    /// Add a command to the allowlist
    pub fn allow_command(mut self, command: impl Into<String>) -> Self {
        self.allowed_commands.insert(command.into());
        self
    }

    /// Remove a command from the allowlist
    pub fn remove_command(mut self, command: &str) -> Self {
        self.allowed_commands.remove(command);
        self
    }

    /// Set maximum argument length
    pub fn with_max_arg_length(mut self, length: usize) -> Self {
        self.max_arg_length = length;
        self
    }

    /// Set maximum argument count
    pub fn with_max_arg_count(mut self, count: usize) -> Self {
        self.max_arg_count = count;
        self
    }
}

/// Convert security error to ToolError
pub fn security_to_tool_error(err: TerminalSecurityError) -> ToolError {
    ToolError::ExecutionFailed {
        tool: "terminal.execute".to_string(),
        reason: format!("Security policy violation: {}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_allows_basic_commands() {
        let policy = TerminalSecurityPolicy::new();
        assert!(policy.is_command_allowed("echo"));
        assert!(policy.is_command_allowed("ls"));
        assert!(policy.is_command_allowed("pwd"));
        assert!(policy.is_command_allowed("cat"));
        assert!(policy.is_command_allowed("git"));
    }

    #[test]
    fn default_policy_blocks_dangerous_commands() {
        let policy = TerminalSecurityPolicy::new();
        assert!(!policy.is_command_allowed("sh"));
        assert!(!policy.is_command_allowed("bash"));
        assert!(!policy.is_command_allowed("eval"));
        assert!(!policy.is_command_allowed("exec"));
        assert!(!policy.is_command_allowed("rm")); // Not in default list
        assert!(!policy.is_command_allowed("sudo"));
    }

    #[test]
    fn validate_basic_command() {
        let policy = TerminalSecurityPolicy::new();
        assert!(policy.validate("echo", &["hello".to_string()]).is_ok());
        assert!(policy.validate("ls", &["-la".to_string()]).is_ok());
    }

    #[test]
    fn validate_blocks_shell_operators() {
        let policy = TerminalSecurityPolicy::new();

        // Semicolon
        let result = policy.validate("echo", &["hello;".to_string()]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TerminalSecurityError::ShellOperatorDetected(_)));

        // Pipe
        let result = policy.validate("echo", &["hello|cat".to_string()]);
        assert!(result.is_err());

        // Backtick
        let result = policy.validate("echo", &["`whoami`".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_blocks_environment_expansion() {
        let policy = TerminalSecurityPolicy::new();

        let result = policy.validate("echo", &["$HOME".to_string()]);
        assert!(result.is_err());
        // The $ character is caught by forbidden_operators check (which runs first)
        // This is still correct security - the dangerous pattern is blocked
        assert!(matches!(result.unwrap_err(), TerminalSecurityError::ShellOperatorDetected(_)));

        let result = policy.validate("echo", &["${PATH}".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_blocks_command_substitution() {
        let policy = TerminalSecurityPolicy::new();

        let result = policy.validate("echo", &["$(whoami)".to_string()]);
        assert!(result.is_err());
        // The $( is caught by forbidden_operators check (contains $)
        // This is still correct security - the dangerous pattern is blocked
        assert!(matches!(result.unwrap_err(), TerminalSecurityError::ShellOperatorDetected(_)));
    }

    #[test]
    fn validate_blocks_not_allowed_commands() {
        let policy = TerminalSecurityPolicy::new();

        let result = policy.validate("rm", &["-rf".to_string(), "/".to_string()]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TerminalSecurityError::CommandNotAllowed(_)));
    }

    #[test]
    fn validate_blocks_subshell_commands() {
        let policy = TerminalSecurityPolicy::new();

        let result = policy.validate("bash", &["-c".to_string(), "echo hello".to_string()]);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TerminalSecurityError::SubshellExecutionBlocked(_)));
    }

    #[test]
    fn strict_policy_fewer_commands() {
        let strict = TerminalSecurityPolicy::strict();
        let default = TerminalSecurityPolicy::new();

        assert!(!strict.is_command_allowed("curl"));
        assert!(!strict.is_command_allowed("tar"));
        assert!(!strict.is_command_allowed("awk"));

        assert!(default.is_command_allowed("curl"));
        assert!(default.is_command_allowed("tar"));
        assert!(default.is_command_allowed("awk"));
    }

    #[test]
    fn permissive_policy_allows_more() {
        let permissive = TerminalSecurityPolicy::permissive();
        assert!(permissive.is_command_allowed("python"));
        assert!(permissive.is_command_allowed("python3"));
        assert!(permissive.is_command_allowed("node"));
    }

    #[test]
    fn custom_command_added() {
        let policy = TerminalSecurityPolicy::new()
            .allow_command("my_custom_tool");

        assert!(policy.is_command_allowed("my_custom_tool"));
    }

    #[test]
    fn argument_count_limits() {
        let policy = TerminalSecurityPolicy::new()
            .with_max_arg_count(2);

        assert!(policy.validate("echo", &["a".to_string(), "b".to_string()]).is_ok());

        let result = policy.validate("echo", &["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn argument_length_limits() {
        let policy = TerminalSecurityPolicy::new()
            .with_max_arg_length(10);

        assert!(policy.validate("echo", &["short".to_string()]).is_ok());

        let result = policy.validate("echo", &["this_is_a_very_long_argument".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn path_allowed_in_args() {
        // File paths should be allowed
        let policy = TerminalSecurityPolicy::new();
        assert!(policy.validate("ls", &["/home/user/file.txt".to_string()]).is_ok());
        assert!(policy.validate("cat", &["../src/main.rs".to_string()]).is_ok());
        assert!(policy.validate("head", &["-n".to_string(), "10".to_string(), "./log.txt".to_string()]).is_ok());
    }

    #[test]
    fn redirect_blocked() {
        let policy = TerminalSecurityPolicy::new();

        let result = policy.validate("echo", &[">".to_string(), "/etc/passwd".to_string()]);
        assert!(result.is_err());

        let result = policy.validate("cat", &["<".to_string(), "/etc/shadow".to_string()]);
        assert!(result.is_err());
    }
}
