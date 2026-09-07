//! Execution Mode - Permission boundaries for different execution contexts
//!
//! This module provides fine-grained permission control for autonomous vs interactive execution:
//! - ExecutionMode: Interactive, Autonomous, Supervised
//! - PermissionSet: Defines what operations are allowed
//! - PermissionChecker: Validates actions against current mode
//! - CapabilityRequirements: What capabilities need for each tool

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Execution mode determines permission level.
/// Enum values are ordered by privilege: DryRun=0 (least) to Interactive=3 (most)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ExecutionMode {
    /// Dry-run mode - validate but don't execute (least privileges).
    #[default]
    DryRun,
    /// Autonomous mode - running unattended, restricted permissions.
    Autonomous,
    /// Supervised mode - autonomous but notifies and can be interrupted.
    Supervised,
    /// Interactive mode - user is present, full permissions with confirmation.
    Interactive,
}

impl ExecutionMode {
    /// Get permission set for this mode.
    pub fn default_permissions(&self) -> PermissionSet {
        match self {
            Self::Interactive => PermissionSet::full(),
            Self::Autonomous => PermissionSet::restricted(),
            Self::Supervised => PermissionSet::supervised(),
            Self::DryRun => PermissionSet::none(),
        }
    }

    /// Can this mode execute without human confirmation?
    pub fn can_execute_unattended(&self) -> bool {
        matches!(self, Self::Autonomous | Self::Supervised)
    }

    /// Can this mode modify files?
    pub fn can_modify_files(&self) -> bool {
        matches!(self, Self::Interactive | Self::Supervised)
    }

    /// Can this mode execute terminal commands?
    pub fn can_execute_commands(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// Can this mode control the computer?
    pub fn can_control_computer(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// Can this mode access network?
    pub fn can_access_network(&self) -> bool {
        matches!(self, Self::Interactive | Self::Autonomous | Self::Supervised)
    }

    /// Can this mode access credentials/secrets?
    pub fn can_access_credentials(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// Can this mode modify system state?
    pub fn can_modify_system(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Autonomous => "autonomous",
            Self::Supervised => "supervised",
            Self::DryRun => "dry-run",
        }
    }
}

// Default impl is via the #[default] attribute on the DryRun variant

impl std::str::FromStr for ExecutionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "interactive" => Ok(Self::Interactive),
            "autonomous" => Ok(Self::Autonomous),
            "supervised" => Ok(Self::Supervised),
            "dryrun" | "dry-run" => Ok(Self::DryRun),
            _ => Err(format!("Unknown execution mode: {}", s)),
        }
    }
}

/// Permission set for fine-grained control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
    /// Read files.
    pub read_files: bool,
    /// Write/modify files.
    pub write_files: bool,
    /// Delete files.
    pub delete_files: bool,
    /// Execute terminal commands.
    pub execute_commands: bool,
    /// Network access (HTTP requests).
    pub network_access: bool,
    /// Computer control (mouse/keyboard).
    pub computer_control: bool,
    /// Screen capture.
    pub screen_capture: bool,
    /// Access credentials/secrets.
    pub access_credentials: bool,
    /// Audio capture.
    pub audio_capture: bool,
    /// Audio playback (TTS).
    pub audio_playback: bool,
    /// System information access.
    pub system_info: bool,
    /// Process management.
    pub process_management: bool,
    /// Maximum operations per minute (0 = no limit).
    pub rate_limit: u32,
    /// Maximum file size for reads (bytes, 0 = no limit).
    pub max_file_size: u64,
    /// Allowed directories (empty = all).
    pub allowed_directories: Vec<String>,
    /// Blocked directories.
    pub blocked_directories: Vec<String>,
    /// Allowed command patterns (empty = all allowed if execute_commands true).
    pub allowed_commands: Vec<String>,
    /// Allowed URL patterns (empty = all allowed if network_access true).
    pub allowed_urls: Vec<String>,
}

impl PermissionSet {
    /// Full permissions (interactive mode).
    pub fn full() -> Self {
        Self {
            read_files: true,
            write_files: true,
            delete_files: true,
            execute_commands: true,
            network_access: true,
            computer_control: true,
            screen_capture: true,
            access_credentials: true,
            audio_capture: true,
            audio_playback: true,
            system_info: true,
            process_management: true,
            rate_limit: 0,
            max_file_size: 0,
            allowed_directories: vec![],
            blocked_directories: vec![],
            allowed_commands: vec![],
            allowed_urls: vec![],
        }
    }

    /// Restricted permissions (unattended autonomous mode).
    pub fn restricted() -> Self {
        Self {
            read_files: true,
            write_files: false,  // No file writes
            delete_files: false,
            execute_commands: false,  // No command execution
            network_access: true,  // Can fetch but with SSRF protection
            computer_control: false,
            screen_capture: false,
            access_credentials: false,
            audio_capture: false,
            audio_playback: false,
            system_info: true,
            process_management: false,
            rate_limit: 60,  // 60 ops/minute
            max_file_size: 1024 * 1024,  // 1MB
            allowed_directories: vec![],
            blocked_directories: vec![
                "/etc".to_string(),
                "/root".to_string(),
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
            ],
            allowed_commands: vec![],
            allowed_urls: vec![],
        }
    }

    /// Supervised permissions (autonomous with notification).
    pub fn supervised() -> Self {
        Self {
            read_files: true,
            write_files: true,  // Can write but logs
            delete_files: false,
            execute_commands: false,  // Still no commands
            network_access: true,
            computer_control: false,
            screen_capture: false,
            access_credentials: false,
            audio_capture: false,
            audio_playback: false,
            system_info: true,
            process_management: false,
            rate_limit: 120,  // 120 ops/minute
            max_file_size: 10 * 1024 * 1024,  // 10MB
            allowed_directories: vec![],
            blocked_directories: vec![
                "/etc".to_string(),
                "/root".to_string(),
            ],
            allowed_commands: vec![],
            allowed_urls: vec![],
        }
    }

    /// No permissions (dry-run mode).
    pub fn none() -> Self {
        Self {
            read_files: false,
            write_files: false,
            delete_files: false,
            execute_commands: false,
            network_access: false,
            computer_control: false,
            screen_capture: false,
            access_credentials: false,
            audio_capture: false,
            audio_playback: false,
            system_info: true,  // Can still read system info
            process_management: false,
            rate_limit: 0,
            max_file_size: 0,
            allowed_directories: vec![],
            blocked_directories: vec![],
            allowed_commands: vec![],
            allowed_urls: vec![],
        }
    }

    /// Check if operation is allowed.
    pub fn check_operation(&self, op: &Operation) -> PermissionResult {
        let allowed = match op {
            Operation::ReadFile { path } => {
                if !self.read_files {
                    false
                } else if self.is_path_blocked(path) {
                    false
                } else {
                    true
                }
            }
            Operation::WriteFile { path } => {
                if !self.write_files {
                    false
                } else if self.is_path_blocked(path) {
                    false
                } else {
                    true
                }
            }
            Operation::DeleteFile { path } => {
                if !self.delete_files {
                    false
                } else if self.is_path_blocked(path) {
                    false
                } else {
                    true
                }
            }
            Operation::ExecuteCommand { command } => {
                if !self.execute_commands {
                    false
                } else if self.allowed_commands.is_empty() {
                    true
                } else {
                    self.allowed_commands.iter().any(|allowed| command.starts_with(allowed))
                }
            }
            Operation::NetworkRequest { url } => {
                if !self.network_access {
                    false
                } else if self.allowed_urls.is_empty() {
                    true
                } else {
                    self.allowed_urls.iter().any(|allowed| url.contains(allowed))
                }
            }
            Operation::ComputerControl => self.computer_control,
            Operation::ScreenCapture => self.screen_capture,
            Operation::AccessCredentials => self.access_credentials,
            Operation::AudioCapture => self.audio_capture,
            Operation::AudioPlayback => self.audio_playback,
            Operation::SystemInfo => self.system_info,
            Operation::ProcessManagement => self.process_management,
        };

        if allowed {
            PermissionResult::Allowed
        } else {
            PermissionResult::Denied {
                operation: op.clone(),
                reason: format!("Operation {:?} not permitted in current mode", op),
            }
        }
    }

    /// Check if path is blocked.
    fn is_path_blocked(&self, path: &str) -> bool {
        let expanded = shellexpand::tilde(path).to_string();
        self.blocked_directories.iter().any(|blocked| {
            expanded.starts_with(&shellexpand::tilde(blocked).to_string())
        })
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::restricted()
    }
}

/// Operation being checked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    ReadFile { path: String },
    WriteFile { path: String },
    DeleteFile { path: String },
    ExecuteCommand { command: String },
    NetworkRequest { url: String },
    ComputerControl,
    ScreenCapture,
    AccessCredentials,
    AudioCapture,
    AudioPlayback,
    SystemInfo,
    ProcessManagement,
}

/// Result of permission check.
#[derive(Debug, Clone)]
pub enum PermissionResult {
    Allowed,
    Denied { operation: Operation, reason: String },
}

impl PermissionResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        !self.is_allowed()
    }
}

/// Execution mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionModeConfig {
    /// Mode.
    pub mode: ExecutionMode,
    /// Custom permissions (overrides defaults).
    pub custom_permissions: Option<PermissionSet>,
    /// Confirmation required for high-risk operations.
    pub require_confirmation: bool,
    /// Audit log level.
    pub audit_level: AuditLevel,
    /// Maximum consecutive operations without review.
    pub max_unattended_ops: u32,
    /// Require human approval every N operations.
    pub human_review_interval: Option<u32>,
}

impl ExecutionModeConfig {
    /// Create config for mode.
    pub fn for_mode(mode: ExecutionMode) -> Self {
        Self {
            mode,
            custom_permissions: None,
            require_confirmation: mode != ExecutionMode::Autonomous,
            audit_level: match mode {
                ExecutionMode::Interactive => AuditLevel::Info,
                ExecutionMode::Autonomous => AuditLevel::Verbose,
                ExecutionMode::Supervised => AuditLevel::Verbose,
                ExecutionMode::DryRun => AuditLevel::Debug,
            },
            max_unattended_ops: match mode {
                ExecutionMode::Interactive => u32::MAX,
                ExecutionMode::Autonomous => 100,
                ExecutionMode::Supervised => 50,
                ExecutionMode::DryRun => u32::MAX,
            },
            human_review_interval: match mode {
                ExecutionMode::Interactive => None,
                ExecutionMode::Autonomous => Some(50),
                ExecutionMode::Supervised => Some(25),
                ExecutionMode::DryRun => None,
            },
        }
    }

    /// Get effective permissions.
    pub fn permissions(&self) -> PermissionSet {
        self.custom_permissions
            .clone()
            .unwrap_or_else(|| self.mode.default_permissions())
    }
}

impl Default for ExecutionModeConfig {
    fn default() -> Self {
        Self::for_mode(ExecutionMode::Interactive)
    }
}

/// Audit logging level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    Minimal,
    Info,
    Verbose,
    Debug,
}

/// Runtime permission checker.
pub struct PermissionChecker {
    /// Current mode config.
    pub config: ExecutionModeConfig,
    /// Operation stats.
    stats: RwLock<OperationStats>,
}

/// Operation statistics.
#[derive(Debug, Clone, Default)]
pub struct OperationStats {
    /// Operations performed.
    pub total_operations: u64,
    /// Operations denied.
    pub denied_operations: u64,
    /// Operations requiring confirmation.
    pub confirmed_operations: u64,
    /// Last operation time.
    pub last_operation: Option<std::time::Instant>,
}

impl PermissionChecker {
    /// Create new checker.
    pub fn new(config: ExecutionModeConfig) -> Self {
        Self {
            config,
            stats: RwLock::new(OperationStats::default()),
        }
    }

    /// Check if operation is permitted.
    pub async fn check(&self, op: &Operation) -> PermissionResult {
        let result = self.config.permissions().check_operation(op);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_operations += 1;
        stats.last_operation = Some(std::time::Instant::now());

        if result.is_denied() {
            stats.denied_operations += 1;
            warn!(
                "Permission denied: {:?} in {:?} mode",
                op, self.config.mode
            );
        } else {
            debug!(
                "Permission granted: {:?} in {:?} mode",
                op, self.config.mode
            );
        }

        result
    }

    /// Get current stats.
    pub async fn stats(&self) -> OperationStats {
        self.stats.read().await.clone()
    }

    /// Update mode.
    pub fn set_mode(&mut self, mode: ExecutionMode) {
        self.config = ExecutionModeConfig::for_mode(mode);
    }

    /// Update with custom config.
    pub fn set_config(&mut self, config: ExecutionModeConfig) {
        self.config = config;
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new(ExecutionModeConfig::default())
    }
}

/// Permission error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PermissionError {
    #[error("Permission denied: {operation:?} - {reason}")]
    Denied { operation: Operation, reason: String },

    #[error("Mode escalation required: {0}")]
    EscalationRequired(String),

    #[error("Rate limit exceeded: {0} ops/minute")]
    RateLimitExceeded(u32),

    #[error("Path blocked: {0}")]
    PathBlocked(String),
}

/// Capability requirements for tools.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRequirements {
    /// Required permissions.
    pub required: PermissionSet,
    /// Minimum mode required.
    pub min_mode: ExecutionMode,
    /// Description of what this tool does.
    pub description: String,
    /// Risk level.
    pub risk_level: RiskLevel,
}

/// Risk level for operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum RiskLevel {
    /// Read-only, safe.
    #[default]
    Low,
    /// Modifies state but reversible.
    Medium,
    /// Destructive or external effects.
    High,
    /// System-level changes.
    Critical,
}

impl CapabilityRequirements {
    /// Create requirements for a read-only tool.
    pub fn read_only() -> Self {
        Self {
            required: PermissionSet {
                read_files: true,
                ..PermissionSet::none()
            },
            min_mode: ExecutionMode::Autonomous,
            description: "Read-only operation".to_string(),
            risk_level: RiskLevel::Low,
        }
    }

    /// Create requirements for a write tool.
    pub fn write() -> Self {
        Self {
            required: PermissionSet {
                read_files: true,
                write_files: true,
                ..PermissionSet::none()
            },
            min_mode: ExecutionMode::Supervised,
            description: "Write operation".to_string(),
            risk_level: RiskLevel::Medium,
        }
    }

    /// Create requirements for network tool.
    pub fn network() -> Self {
        Self {
            required: PermissionSet {
                network_access: true,
                ..PermissionSet::none()
            },
            min_mode: ExecutionMode::Autonomous,
            description: "Network request".to_string(),
            risk_level: RiskLevel::Medium,
        }
    }

    /// Create requirements for terminal tool.
    pub fn terminal() -> Self {
        Self {
            required: PermissionSet {
                execute_commands: true,
                ..PermissionSet::none()
            },
            min_mode: ExecutionMode::Interactive,
            description: "Command execution".to_string(),
            risk_level: RiskLevel::High,
        }
    }

    /// Create requirements for computer control.
    pub fn computer_control() -> Self {
        Self {
            required: PermissionSet {
                computer_control: true,
                screen_capture: true,
                ..PermissionSet::none()
            },
            min_mode: ExecutionMode::Interactive,
            description: "Computer control".to_string(),
            risk_level: RiskLevel::Critical,
        }
    }
}

/// Tool capability registry.
pub struct ToolCapabilityRegistry {
    /// Map tool name to requirements.
    capabilities: HashMap<String, CapabilityRequirements>,
}

impl ToolCapabilityRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        let mut reg = Self {
            capabilities: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Register default capabilities.
    fn register_defaults(&mut self) {
        // Read tools
        self.register("read", CapabilityRequirements::read_only());
        self.register("grep", CapabilityRequirements::read_only());
        self.register("glob", CapabilityRequirements::read_only());

        // Write tools
        self.register("write", CapabilityRequirements::write());
        self.register("edit", CapabilityRequirements::write());

        // Network tools
        self.register("fetch", CapabilityRequirements::network());
        self.register("web_search", CapabilityRequirements::network());

        // Terminal
        self.register("execute", CapabilityRequirements::terminal());

        // Vision/Control
        let mut control_req = CapabilityRequirements::computer_control();
        control_req.required.computer_control = true;
        self.register("capture_screen", control_req.clone());
        self.register("click", control_req.clone());
        self.register("type", control_req);
    }

    /// Register tool capability.
    pub fn register(&mut self, tool: impl Into<String>, requirements: CapabilityRequirements) {
        self.capabilities.insert(tool.into(), requirements);
    }

    /// Check if tool can run in current mode.
    pub fn can_run(&self, tool: &str, mode: ExecutionMode) -> bool {
        self.capabilities
            .get(tool)
            .map(|req| mode as u8 >= req.min_mode as u8)
            .unwrap_or(true) // Unknown tools default to allowed
    }

    /// Get requirements for tool.
    pub fn requirements(&self, tool: &str) -> Option<&CapabilityRequirements> {
        self.capabilities.get(tool)
    }

    /// List tools available in mode.
    pub fn available_tools(&self, mode: ExecutionMode) -> Vec<&String> {
        self.capabilities
            .iter()
            .filter(|(_, req)| mode as u8 >= req.min_mode as u8)
            .map(|(name, _)| name)
            .collect()
    }
}

impl Default for ToolCapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_mode_default() {
        // Default is now DryRun for safety (most restrictive)
        assert_eq!(ExecutionMode::default(), ExecutionMode::DryRun);
    }

    #[test]
    fn test_execution_mode_parsing() {
        assert_eq!(
            "interactive".parse::<ExecutionMode>().unwrap(),
            ExecutionMode::Interactive
        );
        assert_eq!(
            "AUTONOMOUS".parse::<ExecutionMode>().unwrap(),
            ExecutionMode::Autonomous
        );
        assert!("unknown".parse::<ExecutionMode>().is_err());
    }

    #[test]
    fn test_permission_set_restricted() {
        let perms = PermissionSet::restricted();
        assert!(perms.read_files);
        assert!(!perms.write_files);
        assert!(!perms.execute_commands);
        assert!(!perms.computer_control);
    }

    #[test]
    fn test_operation_check_read_file() {
        let perms = PermissionSet::restricted();
        let op = Operation::ReadFile {
            path: "/tmp/test.txt".to_string(),
        };
        assert!(perms.check_operation(&op).is_allowed());
    }

    #[test]
    fn test_operation_check_blocked_path() {
        let perms = PermissionSet::restricted();
        let op = Operation::ReadFile {
            path: "/etc/passwd".to_string(),
        };
        // Should be blocked due to blocked_directories containing /etc
        assert!(perms.check_operation(&op).is_denied());
    }

    #[test]
    fn test_execution_mode_permissions() {
        assert!(ExecutionMode::Interactive.can_execute_commands());
        assert!(!ExecutionMode::Autonomous.can_execute_commands());
        assert!(!ExecutionMode::Supervised.can_execute_commands());
    }

    #[test]
    fn test_execution_mode_can_control_computer() {
        assert!(ExecutionMode::Interactive.can_control_computer());
        assert!(!ExecutionMode::Autonomous.can_control_computer());
    }

    #[test]
    fn test_capability_requirements_mode() {
        let terminal_req = CapabilityRequirements::terminal();
        assert_eq!(terminal_req.min_mode, ExecutionMode::Interactive);
        assert_eq!(terminal_req.risk_level, RiskLevel::High);
    }

    #[test]
    fn test_tool_registry() {
        let registry = ToolCapabilityRegistry::new();

        assert!(registry.can_run("read", ExecutionMode::Autonomous));
        assert!(!registry.can_run("execute", ExecutionMode::Autonomous));
        assert!(registry.can_run("execute", ExecutionMode::Interactive));
    }

    #[test]
    fn test_permission_checker_stats() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let checker = PermissionChecker::new(ExecutionModeConfig::for_mode(
                ExecutionMode::Autonomous,
            ));

            let op = Operation::ReadFile {
                path: "/tmp/test.txt".to_string(),
            };
            checker.check(&op).await;

            let stats = checker.stats().await;
            assert_eq!(stats.total_operations, 1);
        });
    }

    #[test]
    fn test_dry_run_permissions() {
        let perms = PermissionSet::none();
        assert!(!perms.read_files);
        assert!(!perms.write_files);
        assert!(!perms.network_access);
    }

    #[test]
    fn test_supervised_permissions() {
        let perms = PermissionSet::supervised();
        assert!(perms.read_files);
        assert!(perms.write_files); // Can write but logs
        assert!(!perms.execute_commands);
        assert!(!perms.delete_files);
    }
}
