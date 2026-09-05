//! Security Audit - Security hardening and reviews
//!
//! This module provides security audit functionality for Drex:
//! - Credential isolation audit
//! - Network boundary review
//! - Sandbox configuration for risky tools
//! - Audit trail logging
//! - Encryption at rest review
//!
//! # Security Principles
//!
//! 1. **Least Privilege**: Tools only get access they need
//! 2. **Data Isolation**: Credentials never in memory/crash dumps
//! 3. **Defense in Depth**: Multiple layers of protection
//! 4. **Audit Everything**: All security events are logged
//! 5. **Fail Secure**: Failures default to most restrictive

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Security level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    /// Unrestricted - no special controls.
    Unrestricted,
    /// Standard - basic security controls.
    Standard,
    /// Elevated - stronger controls.
    Elevated,
    /// Maximum - strictest controls.
    Maximum,
}

impl SecurityLevel {
    /// Get the numeric value for ordering.
    pub fn value(&self) -> u8 {
        match self {
            SecurityLevel::Unrestricted => 0,
            SecurityLevel::Standard => 1,
            SecurityLevel::Elevated => 2,
            SecurityLevel::Maximum => 3,
        }
    }
}

/// Result of a security audit.
#[derive(Debug, Clone)]
pub struct AuditResult {
    /// Audit passed.
    pub passed: bool,
    /// Severity of worst issue found.
    pub worst_severity: Option<SecuritySeverity>,
    /// List of findings.
    pub findings: Vec<SecurityFinding>,
    /// Recommendations for remediation.
    pub recommendations: Vec<String>,
    /// Audit timestamp.
    pub timestamp: SystemTime,
}

impl AuditResult {
    /// Create a result indicating no issues found.
    pub fn pass() -> Self {
        Self {
            passed: true,
            worst_severity: None,
            findings: Vec::new(),
            recommendations: Vec::new(),
            timestamp: SystemTime::now(),
        }
    }

    /// Create a result with findings.
    pub fn with_findings(findings: Vec<SecurityFinding>) -> Self {
        let worst = findings.iter().map(|f| f.severity).max();
        let passed = findings.iter().all(|f| f.severity < SecuritySeverity::High);

        Self {
            passed,
            worst_severity: worst,
            findings,
            recommendations: Vec::new(),
            timestamp: SystemTime::now(),
        }
    }
}

/// Security finding severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// A security finding from an audit.
#[derive(Debug, Clone)]
pub struct SecurityFinding {
    /// Finding severity.
    pub severity: SecuritySeverity,
    /// Category of finding.
    pub category: String,
    /// Description of the issue.
    pub description: String,
    /// Location where issue was found.
    pub location: Option<String>,
    /// Recommendation for fixing.
    pub recommendation: String,
}

/// Credential isolation status.
#[derive(Debug, Clone)]
pub struct CredentialIsolationStatus {
    /// Whether credentials are stored separately.
    pub separate_credential_store: bool,
    /// Whether credentials are encrypted at rest.
    pub encrypted_at_rest: bool,
    /// Whether credentials are in environment only (no files).
    pub env_only: bool,
    /// Duration credentials are held in memory.
    pub memory_retention_secs: u64,
    /// Whether memory is cleared after use.
    pub memory_cleared: bool,
}

impl Default for CredentialIsolationStatus {
    fn default() -> Self {
        Self {
            separate_credential_store: false, // Placeholder
            encrypted_at_rest: false,         // Placeholder
            env_only: false,                  // Placeholder
            memory_retention_secs: 0,         // Placeholder
            memory_cleared: false,            // Placeholder
        }
    }
}

/// Network boundary status.
#[derive(Debug, Clone)]
pub struct NetworkBoundaryStatus {
    /// Outbound connections allowed.
    pub outbound_allowed: bool,
    /// Restricted endpoints list.
    pub restricted_endpoints: Vec<String>,
    /// Proxy configuration.
    pub proxy_configured: bool,
    /// No external services flag.
    pub no_external_services: bool,
    /// Localhost only mode.
    pub localhost_only: bool,
}

impl Default for NetworkBoundaryStatus {
    fn default() -> Self {
        Self {
            outbound_allowed: true,      // Placeholder
            restricted_endpoints: Vec::new(),
            proxy_configured: false,     // Placeholder
            no_external_services: false, // Could be restricted
            localhost_only: false,       // Could be restricted
        }
    }
}

/// Tool sandboxing configuration.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Tool name.
    pub tool_name: String,
    /// Required security level.
    pub required_level: SecurityLevel,
    /// File system restrictions.
    pub fs_restrictions: Vec<String>,
    /// Network restrictions.
    pub network_restricted: bool,
    /// Execution timeout.
    pub timeout_secs: u64,
    /// Allowed syscalls (empty = all).
    pub allowed_syscalls: Vec<String>,
}

/// Audit trail entry.
#[derive(Debug, Clone)]
pub struct AuditTrailEntry {
    /// Entry ID.
    pub id: String,
    /// Event timestamp.
    pub timestamp: SystemTime,
    /// Event type.
    pub event_type: String,
    /// User or system component.
    pub actor: String,
    /// Action performed.
    pub action: String,
    /// Target of action.
    pub target: String,
    /// Whether successful.
    pub success: bool,
    /// Additional details.
    pub details: Option<String>,
}

/// Encryption at rest status.
#[derive(Debug, Clone)]
pub struct EncryptionStatus {
    /// Disk encryption enabled.
    pub disk_encryption: bool,
    /// Database encryption enabled.
    pub db_encryption: bool,
    /// Memory encryption (if applicable).
    pub memory_encryption: bool,
    /// Key management.
    pub key_management: String,
    /// Encryption algorithm.
    pub algorithm: String,
}

impl Default for EncryptionStatus {
    fn default() -> Self {
        Self {
            disk_encryption: false, // System-level
            db_encryption: false,   // Would check DB config
            memory_encryption: false, // Rarely available
            key_management: "None".to_string(),
            algorithm: "None".to_string(),
        }
    }
}

/// Security auditor.
pub struct SecurityAuditor {
    credentials: Arc<RwLock<CredentialIsolationStatus>>,
    network: Arc<RwLock<NetworkBoundaryStatus>>,
    sandboxes: Arc<RwLock<HashMap<String, SandboxConfig>>>,
    audit_trail: Arc<RwLock<Vec<AuditTrailEntry>>>,
    encryption: Arc<RwLock<EncryptionStatus>>,
}

impl SecurityAuditor {
    /// Create a new security auditor.
    pub fn new() -> Self {
        Self {
            credentials: Arc::new(RwLock::new(CredentialIsolationStatus::default())),
            network: Arc::new(RwLock::new(NetworkBoundaryStatus::default())),
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            audit_trail: Arc::new(RwLock::new(Vec::new())),
            encryption: Arc::new(RwLock::new(EncryptionStatus::default())),
        }
    }

    /// Audit 8.1: Credential Isolation.
    pub async fn audit_credential_isolation(&self) -> AuditResult {
        info!("Running credential isolation audit");
        let status = self.credentials.read().await.clone();
        let mut findings = Vec::new();

        if !status.separate_credential_store {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Credential Storage".to_string(),
                description: "Credentials not stored in separate credential store".to_string(),
                location: None,
                recommendation: "Use a dedicated credential store or vault".to_string(),
            });
        }

        if !status.encrypted_at_rest {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Credential Storage".to_string(),
                description: "Credentials not encrypted at rest".to_string(),
                location: None,
                recommendation: "Enable encryption for credential files".to_string(),
            });
        }

        if !status.env_only {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Medium,
                category: "Credential Storage".to_string(),
                description: "Credentials may be stored in files (not env-only)".to_string(),
                location: None,
                recommendation: "Prefer environment variables for credentials".to_string(),
            });
        }

        if status.memory_retention_secs > 300 {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Medium,
                category: "Memory Security".to_string(),
                description: format!(
                    "Credentials held in memory for {} seconds",
                    status.memory_retention_secs
                ),
                location: None,
                recommendation: "Clear credentials from memory immediately after use".to_string(),
            });
        }

        if !status.memory_cleared {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Memory Security".to_string(),
                description: "Memory not cleared after credential use".to_string(),
                location: None,
                recommendation: "Implement secure memory wiping".to_string(),
            });
        }

        AuditResult::with_findings(findings)
    }

    /// Audit 8.2: Network Boundary Review.
    pub async fn audit_network_boundary(&self) -> AuditResult {
        info!("Running network boundary audit");
        let status = self.network.read().await.clone();
        let mut findings = Vec::new();

        if status.outbound_allowed && !status.proxy_configured {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Medium,
                category: "Network Security".to_string(),
                description: "Outbound connections allowed without proxy".to_string(),
                location: None,
                recommendation: "Configure outbound proxy for all external connections".to_string(),
            });
        }

        if status.restricted_endpoints.is_empty() {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Low,
                category: "Network Security".to_string(),
                description: "No restricted endpoints configured".to_string(),
                location: None,
                recommendation: "Maintain a list of restricted endpoints".to_string(),
            });
        }

        if !status.no_external_services && !status.localhost_only {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Info,
                category: "Network Security".to_string(),
                description: "External services may be accessible".to_string(),
                location: None,
                recommendation: "Consider enabling localhost-only mode for sensitive deployments".to_string(),
            });
        }

        AuditResult::with_findings(findings)
    }

    /// Audit 8.3: Sandbox Configuration for High-Risk Tools.
    pub async fn audit_sandbox_config(&self) -> AuditResult {
        info!("Running sandbox configuration audit");
        let sandboxes = self.sandboxes.read().await.clone();
        let mut findings = Vec::new();

        if sandboxes.is_empty() {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Sandbox".to_string(),
                description: "No sandbox configurations defined".to_string(),
                location: None,
                recommendation: "Define sandbox configurations for all high-risk tools".to_string(),
            });
        }

        for (tool, config) in &sandboxes {
            if config.timeout_secs == 0 {
                findings.push(SecurityFinding {
                    severity: SecuritySeverity::High,
                    category: "Sandbox".to_string(),
                    description: format!("Tool '{}' has no execution timeout", tool),
                    location: None,
                    recommendation: "Set execution timeout for all tools".to_string(),
                });
            }

            if !config.network_restricted {
                findings.push(SecurityFinding {
                    severity: SecuritySeverity::Medium,
                    category: "Sandbox".to_string(),
                    description: format!("Tool '{}' has unrestricted network access", tool),
                    location: None,
                    recommendation: "Restrict network access for high-risk tools".to_string(),
                });
            }

            if config.fs_restrictions.is_empty() {
                findings.push(SecurityFinding {
                    severity: SecuritySeverity::Medium,
                    category: "Sandbox".to_string(),
                    description: format!("Tool '{}' has no filesystem restrictions", tool),
                    location: None,
                    recommendation: "Define filesystem restrictions for high-risk tools".to_string(),
                });
            }
        }

        AuditResult::with_findings(findings)
    }

    /// Audit 8.4: Audit Trail Review.
    pub async fn audit_audit_trail(&self) -> AuditResult {
        info!("Running audit trail review");
        let entries = self.audit_trail.read().await.clone();
        let mut findings = Vec::new();

        if entries.is_empty() {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Audit Trail".to_string(),
                description: "No audit trail entries found".to_string(),
                location: None,
                recommendation: "Ensure all security-relevant events are logged".to_string(),
            });
        }

        AuditResult::with_findings(findings)
    }

    /// Audit 8.5: Encryption at Rest Review.
    pub async fn audit_encryption(&self) -> AuditResult {
        info!("Running encryption at rest audit");
        let status = self.encryption.read().await.clone();
        let mut findings = Vec::new();

        if !status.disk_encryption {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::Medium,
                category: "Encryption".to_string(),
                description: "Disk encryption not enabled".to_string(),
                location: None,
                recommendation: "Enable full disk encryption".to_string(),
            });
        }

        if !status.db_encryption {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Encryption".to_string(),
                description: "Database encryption not enabled".to_string(),
                location: None,
                recommendation: "Enable database-level encryption".to_string(),
            });
        }

        if status.key_management == "None" {
            findings.push(SecurityFinding {
                severity: SecuritySeverity::High,
                category: "Encryption".to_string(),
                description: "No key management configured".to_string(),
                location: None,
                recommendation: "Implement proper key management (KMS, etc.)".to_string(),
            });
        }

        AuditResult::with_findings(findings)
    }

    /// Run all security audits.
    pub async fn run_full_audit(&self) -> Vec<(String, AuditResult)> {
        let mut results = Vec::new();

        results.push(("Credential Isolation".to_string(), self.audit_credential_isolation().await));
        results.push(("Network Boundary".to_string(), self.audit_network_boundary().await));
        results.push(("Sandbox Configuration".to_string(), self.audit_sandbox_config().await));
        results.push(("Audit Trail".to_string(), self.audit_audit_trail().await));
        results.push(("Encryption at Rest".to_string(), self.audit_encryption().await));

        results
    }

    /// Log an audit trail entry.
    pub async fn log_audit(&self, entry: AuditTrailEntry) {
        let mut trail = self.audit_trail.write().await;
        trail.push(entry);
    }

    /// Get audit trail entries.
    pub async fn get_audit_trail(&self) -> Vec<AuditTrailEntry> {
        self.audit_trail.read().await.clone()
    }
}

impl Default for SecurityAuditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Run security audit and return summary.
pub async fn run_security_audit() -> SecurityAuditSummary {
    let auditor = SecurityAuditor::new();
    let results = auditor.run_full_audit().await;

    let total_findings: usize = results.iter().map(|(_, r)| r.findings.len()).sum();
    let critical_count = results.iter()
        .flat_map(|(_, r)| &r.findings)
        .filter(|f| f.severity == SecuritySeverity::Critical)
        .count();
    let high_count = results.iter()
        .flat_map(|(_, r)| &r.findings)
        .filter(|f| f.severity == SecuritySeverity::High)
        .count();

    let all_passed = results.iter().all(|(_, r)| r.passed);

    SecurityAuditSummary {
        all_passed,
        total_findings,
        critical_count,
        high_count,
        results,
        timestamp: SystemTime::now(),
    }
}

/// Security audit summary.
#[derive(Debug, Clone)]
pub struct SecurityAuditSummary {
    /// All audits passed.
    pub all_passed: bool,
    /// Total findings across all audits.
    pub total_findings: usize,
    /// Number of critical findings.
    pub critical_count: usize,
    /// Number of high findings.
    pub high_count: usize,
    /// Individual audit results.
    pub results: Vec<(String, AuditResult)>,
    /// Audit timestamp.
    pub timestamp: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_auditor() {
        let auditor = SecurityAuditor::new();

        let credentials = auditor.audit_credential_isolation().await;
        // Should find issues with default config
        assert!(!credentials.passed);
        assert!(!credentials.findings.is_empty());
    }

    #[tokio::test]
    async fn test_network_boundary_audit() {
        let auditor = SecurityAuditor::new();

        let network = auditor.audit_network_boundary().await;
        // Default config has some issues
        assert!(!network.findings.is_empty());
    }

    #[tokio::test]
    async fn test_sandbox_audit_empty() {
        let auditor = SecurityAuditor::new();

        let sandboxes = auditor.audit_sandbox_config().await;
        // Empty sandbox config should fail
        assert!(!sandboxes.passed);
    }

    #[tokio::test]
    async fn test_full_audit() {
        let auditor = SecurityAuditor::new();

        let results = auditor.run_full_audit().await;
        assert_eq!(results.len(), 5);

        // Should have issues with default config
        let all_passed = results.iter().all(|(_, r)| r.passed);
        assert!(!all_passed);
    }

    #[tokio::test]
    async fn test_audit_trail_logging() {
        let auditor = SecurityAuditor::new();

        let entry = AuditTrailEntry {
            id: "test-1".to_string(),
            timestamp: SystemTime::now(),
            event_type: "test".to_string(),
            actor: "test".to_string(),
            action: "test".to_string(),
            target: "test".to_string(),
            success: true,
            details: None,
        };

        auditor.log_audit(entry).await;

        let trail = auditor.get_audit_trail().await;
        assert_eq!(trail.len(), 1);
    }

    #[test]
    fn test_security_severity_ordering() {
        assert!(SecuritySeverity::Critical > SecuritySeverity::High);
        assert!(SecuritySeverity::High > SecuritySeverity::Medium);
        assert!(SecuritySeverity::Medium > SecuritySeverity::Low);
        assert!(SecuritySeverity::Low > SecuritySeverity::Info);
    }

    #[test]
    fn test_security_level_value() {
        assert_eq!(SecurityLevel::Unrestricted.value(), 0);
        assert_eq!(SecurityLevel::Standard.value(), 1);
        assert_eq!(SecurityLevel::Elevated.value(), 2);
        assert_eq!(SecurityLevel::Maximum.value(), 3);
    }

    #[tokio::test]
    async fn test_security_audit_pass() {
        let result = AuditResult::pass();
        assert!(result.passed);
        assert!(result.findings.is_empty());
    }
}
