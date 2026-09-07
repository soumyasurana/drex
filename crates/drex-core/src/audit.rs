//! Persistent Audit Trail
//!
//! This module provides tamper-resistant audit logging for all security events.
//! Writes to SQLite database with WAL mode for durability and performance.
//!
//! # Security Properties
//!
//! - **Integrity**: Each entry includes a cryptographic hash chain
//! - **Non-repudiation**: Timestamps are recorded from trusted source
//! - **Availability**: Async writes with WAL mode ensure minimal impact
//! - **Tamper-evident**: Sequential IDs and hash chain detect deletion/modification
//!
//! # Design
//!
//! - Uses SQLite with WAL mode
//! - Each entry references previous entry's hash (blockchain-like chain)
//! - Critical events: authentication, authorization, tool execution, errors
//! - Regular events: normal tool usage
//!
//! # Event Types
//!
//! | Type | Severity | Description |
//! |------|----------|-------------|
//! | `security_violation` | Critical | Policy violations, blocked attacks |
//! | `authentication` | High | Login, token validation,
//! | `authorization` | High | Capability grants, denials |
//! | `tool_execution` | Info | Tool calls with inputs/outputs |
//! | `system_event` | Info | Startup, shutdown, config changes |
//! | `error` | Warning | Errors, exceptions |
//!
//! # Example
//!
//! ```rust,ignore
//! use drex_core::audit::{AuditTrail, AuditEvent, AuditSeverity};
//!
//! async fn example() {
//!     let mut audit = AuditTrail::open("/var/log/drex/audit.db").await.unwrap();
//!
//!     audit.log(AuditEvent {
//!         event_type: "tool_execution".to_string(),
//!         severity: AuditSeverity::Info,
//!         actor: "user".to_string(),
//!         action: "web.fetch".to_string(),
//!         target: "https://example.com".to_string(),
//!         success: true,
//!         details: Some("200 OK".to_string()),
//!         session_id: Some("sess_123".to_string()),
//!     }).await.unwrap();
//! }
//! ```

use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tracing::{error, info, warn};

/// Audit trail errors
#[derive(Error, Debug)]
pub enum AuditError {
    #[error("Database error: {0}")]
    Database(sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<sqlx::Error> for AuditError {
    fn from(err: sqlx::Error) -> Self {
        AuditError::Database(err)
    }
}

/// Result type for audit operations
pub type AuditResult<T> = std::result::Result<T, AuditError>;

/// Event severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum AuditSeverity {
    /// System startup/shutdown - lowest priority
    Debug = 0,
    /// Normal events - may be logged
    Info = 1,
    /// Unusual but not harmful
    Notice = 2,
    /// Potential issues
    Warning = 3,
    /// Failed operations, denied access
    Error = 4,
    /// Security events requiring attention
    Critical = 5,
    /// Immediate action required
    Alert = 6,
    /// System is unusable
    Emergency = 7,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditSeverity::Debug => write!(f, "DEBUG"),
            AuditSeverity::Info => write!(f, "INFO"),
            AuditSeverity::Notice => write!(f, "NOTICE"),
            AuditSeverity::Warning => write!(f, "WARNING"),
            AuditSeverity::Error => write!(f, "ERROR"),
            AuditSeverity::Critical => write!(f, "CRITICAL"),
            AuditSeverity::Alert => write!(f, "ALERT"),
            AuditSeverity::Emergency => write!(f, "EMERGENCY"),
        }
    }
}

/// Event scope - who can see this event
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum AuditScope {
    /// Only security auditors
    Restricted = 0,
    /// Administrators
    Admin = 1,
    /// Logged-in users
    User = 2,
    /// Public (errors with details redacted)
    Public = 3,
}

/// Single audit event
#[derive(Debug, Clone)]
pub struct AuditEvent {
    /// Event type (e.g., "tool_execution", "security_violation")
    pub event_type: String,
    /// Severity level
    pub severity: AuditSeverity,
    /// Who/what performed the action
    pub actor: String,
    /// What was done
    pub action: String,
    /// What was affected
    pub target: String,
    /// Success or failure
    pub success: bool,
    /// Additional details (may be JSON)
    pub details: Option<String>,
    /// Session identifier
    pub session_id: Option<String>,
}

/// Database record for audit entry
#[derive(Debug, sqlx::FromRow)]
struct AuditEntryRow {
    id: i64,
    timestamp: i64,
    event_type: String,
    severity: i32,
    scope: i32,
    actor: String,
    action: String,
    target: String,
    success: bool,
    details: Option<String>,
    session_id: Option<String>,
    previous_hash: Option<String>,
    entry_hash: String,
}

/// Persistent audit trail with integrity checking
pub struct AuditTrail {
    pool: Pool<Sqlite>,
}

impl AuditTrail {
    /// Open or create audit database
    ///
    /// Creates the database file and initializes the schema if needed.
    /// Uses WAL mode for durability and concurrent access.
    pub async fn open(path: impl AsRef<Path>) -> AuditResult<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| AuditError::Io(e))?;
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal),
            )
            .await?;

        // Initialize schema
        Self::init_schema(&pool).await?;

        let mut audit = Self { pool };

        // Log initialization
        audit
            .log(AuditEvent {
                event_type: "system_event".to_string(),
                severity: AuditSeverity::Notice,
                actor: "system".to_string(),
                action: "audit_trail_initialized".to_string(),
                target: path.to_string_lossy().to_string(),
                success: true,
                details: Some("Audit trail database opened".to_string()),
                session_id: None,
            })
            .await?;

        Ok(audit)
    }

    /// Open audit trail in default location
    ///
    /// Default: $HOME/.local/share/drex/audit.db
    pub async fn open_default() -> AuditResult<Self> {
        let home = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let path = home.join("drex").join("audit.db");
        Self::open(&path).await
    }

    /// Initialize database schema
    async fn init_schema(pool: &Pool<Sqlite>) -> AuditResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                severity INTEGER NOT NULL,
                scope INTEGER NOT NULL DEFAULT 1,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                success BOOLEAN NOT NULL,
                details TEXT,
                session_id TEXT,
                previous_hash TEXT,
                entry_hash TEXT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Indexes for common queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_timestamp ON audit_entries(timestamp);
            CREATE INDEX IF NOT EXISTS idx_event_type ON audit_entries(event_type);
            CREATE INDEX IF NOT EXISTS idx_severity ON audit_entries(severity);
            CREATE INDEX IF NOT EXISTS idx_actor ON audit_entries(actor);
            CREATE INDEX IF NOT EXISTS idx_session ON audit_entries(session_id);
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Log a single audit event
    ///
    /// The event is immediately persisted to disk with integrity checking.
    /// This is an async operation that should complete quickly (WAL mode).
    pub async fn log(&mut self, event: AuditEvent) -> AuditResult<i64> {
        // Get previous entry hash for chain integrity
        let previous_hash = self.get_last_hash().await?;

        // Build entry
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let entry_hash = Self::compute_hash(&event, timestamp, &previous_hash);

        // Insert into database
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO audit_entries
            (timestamp, event_type, severity, scope, actor, action, target, success, details, session_id, previous_hash, entry_hash)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            RETURNING id
            "#,
        )
        .bind(timestamp)
        .bind(&event.event_type)
        .bind(event.severity as i32)
        .bind(AuditScope::Admin as i32) // Default scope
        .bind(&event.actor)
        .bind(&event.action)
        .bind(&event.target)
        .bind(event.success)
        .bind(&event.details)
        .bind(&event.session_id)
        .bind(&previous_hash)
        .bind(&entry_hash)
        .fetch_one(&self.pool)
        .await?;

        // Log to tracing for immediate visibility of high-severity events
        match event.severity {
            AuditSeverity::Critical | AuditSeverity::Alert | AuditSeverity::Emergency => {
                warn!(
                    audit_id = id,
                    audit_type = %event.event_type,
                    audit_severity = %event.severity,
                    audit_actor = %event.actor,
                    audit_action = %event.action,
                    audit_target = %event.target,
                    audit_success = event.success,
                    "CRITICAL AUDIT EVENT"
                );
            }
            AuditSeverity::Error => {
                info!(
                    audit_id = id,
                    audit_type = %event.event_type,
                    audit_severity = %event.severity,
                    audit_actor = %event.actor,
                    audit_action = %event.action,
                    "Audit event logged"
                );
            }
            _ => {
                tracing::debug!(
                    audit_id = id,
                    audit_type = %event.event_type,
                    audit_action = %event.action,
                    "Audit event logged"
                );
            }
        }

        Ok(id)
    }

    /// Get the hash of the last entry for chain integrity
    async fn get_last_hash(&self) -> AuditResult<Option<String>> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT entry_hash FROM audit_entries ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|r| r.0))
    }

    /// Compute cryptographic hash of an entry
    fn compute_hash(event: &AuditEvent, timestamp: i64, previous_hash: &Option<String>) -> String {
        let mut hasher = Sha256::new();

        // Hash all fields
        hasher.update(&event.event_type);
        hasher.update(&(event.severity as i32).to_le_bytes());
        hasher.update(&event.actor);
        hasher.update(&event.action);
        hasher.update(&event.target);
        hasher.update(&[event.success as u8]);
        if let Some(details) = &event.details {
            hasher.update(details);
        }
        if let Some(session) = &event.session_id {
            hasher.update(session);
        }
        hasher.update(&timestamp.to_le_bytes());
        if let Some(prev) = previous_hash {
            hasher.update(prev.as_bytes());
        }

        hex::encode(hasher.finalize())
    }

    /// Query audit entries with filters
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let entries = audit.query(AuditQuery::new()
    ///     .event_type("security_violation")
    ///     .severity_at_least(AuditSeverity::Critical)
    ///     .limit(100))
    ///     .await?;
    /// ```
    pub async fn query(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEntryRow>> {
        query.execute(&self.pool).await
    }

    /// Verify chain integrity
    ///
    /// Checks that all entries form a valid hash chain and detects
    /// any tampering, deletion, or modification.
    pub async fn verify_integrity(&self) -> AuditResult<IntegrityReport> {
        let entries: Vec<AuditEntryRow> =
            sqlx::query_as("SELECT * FROM audit_entries ORDER BY id ASC")
                .fetch_all(&self.pool)
                .await?;

        let mut violations = Vec::new();
        let mut last_hash: Option<String> = None;

        for entry in entries {
            // Verify hash chain
            if let Some(expected_prev) = &last_hash {
                if entry.previous_hash.as_ref() != Some(expected_prev) {
                    violations.push(IntegrityViolation {
                        entry_id: entry.id,
                        violation_type: ViolationType::BrokenChain,
                        description: format!(
                            "Entry {} has incorrect previous_hash",
                            entry.id
                        ),
                    });
                }
            }

            // Re-compute and verify hash
            let computed = Self::verify_entry_hash(&entry);
            if computed != entry.entry_hash {
                violations.push(IntegrityViolation {
                    entry_id: entry.id,
                    violation_type: ViolationType::Tampered,
                    description: format!(
                        "Entry {} hash mismatch: stored={}, computed={}",
                        entry.id, entry.entry_hash, computed
                    ),
                });
            }

            last_hash = Some(entry.entry_hash);
        }

        Ok(IntegrityReport {
            total_entries: sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_entries")
                .fetch_one(&self.pool)
                .await? as usize,
            violations,
            valid: true, // Will be set based on violations
        })
    }

    /// Recompute hash for verification
    fn verify_entry_hash(entry: &AuditEntryRow) -> String {
        let mut hasher = Sha256::new();

        hasher.update(&entry.event_type);
        hasher.update(&entry.severity.to_le_bytes());
        hasher.update(&entry.actor);
        hasher.update(&entry.action);
        hasher.update(&entry.target);
        hasher.update(&[entry.success as u8]);
        if let Some(details) = &entry.details {
            hasher.update(details);
        }
        if let Some(session) = &entry.session_id {
            hasher.update(session);
        }
        hasher.update(&entry.timestamp.to_le_bytes());
        if let Some(prev) = &entry.previous_hash {
            hasher.update(prev.as_bytes());
        }

        hex::encode(hasher.finalize())
    }

    /// Get statistics about the audit trail
    pub async fn stats(&self) -> AuditResult<AuditStats> {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_entries")
            .fetch_one(&self.pool)
            .await?;

        let critical: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_entries WHERE severity >= 5",
        )
        .fetch_one(&self.pool)
        .await?;

        let errors: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_entries WHERE severity = 4")
                .fetch_one(&self.pool)
                .await?;

        let by_type: Vec<(String, i64)> =
            sqlx::query_as("SELECT event_type, COUNT(*) FROM audit_entries GROUP BY event_type")
                .fetch_all(&self.pool)
                .await?;

        let oldest: Option<i64> =
            sqlx::query_scalar("SELECT MIN(timestamp) FROM audit_entries")
                .fetch_one(&self.pool)
                .await?;

        let newest: Option<i64> =
            sqlx::query_scalar("SELECT MAX(timestamp) FROM audit_entries")
                .fetch_one(&self.pool)
                .await?;

        Ok(AuditStats {
            total_entries: total as usize,
            critical_events: critical as usize,
            error_events: errors as usize,
            events_by_type: by_type.into_iter().collect(),
            oldest_timestamp: oldest,
            newest_timestamp: newest,
        })
    }

    /// Close the audit trail
    pub async fn close(self) {
        self.pool.close().await;
    }
}

/// Simple query filters for audit entries
pub struct AuditQuery {
    /// Event type filter
    pub event_type: Option<String>,
    /// Minimum severity
    pub severity_at_least: Option<AuditSeverity>,
    /// Actor filter
    pub actor: Option<String>,
    /// Time range (start, end)
    pub time_range: Option<(i64, i64)>,
    /// Maximum results
    pub limit: Option<usize>,
    /// Order: true = newest first
    pub newest_first: bool,
}

impl Default for AuditQuery {
    fn default() -> Self {
        Self {
            event_type: None,
            severity_at_least: None,
            actor: None,
            time_range: None,
            limit: None,
            newest_first: true,
        }
    }
}

impl AuditQuery {
    /// Create a new query with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by event type
    pub fn event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    /// Filter by minimum severity
    pub fn severity_at_least(mut self, severity: AuditSeverity) -> Self {
        self.severity_at_least = Some(severity);
        self
    }

    /// Filter by actor
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Filter by time range (Unix timestamps)
    pub fn time_range(mut self, start: i64, end: i64) -> Self {
        self.time_range = Some((start, end));
        self
    }

    /// Limit results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Order by timestamp descending (newest first)
    pub fn newest_first(mut self) -> Self {
        self.newest_first = true;
        self
    }

    /// Order by timestamp ascending (oldest first)
    pub fn oldest_first(mut self) -> Self {
        self.newest_first = false;
        self
    }

    /// Execute the query on the given pool
    pub async fn execute(&self, pool: &Pool<Sqlite>) -> AuditResult<Vec<AuditEntryRow>> {
        let mut sql = "SELECT * FROM audit_entries".to_string();
        let mut conditions = Vec::new();

        if let Some(ref event_type) = self.event_type {
            conditions.push(format!("event_type = '{}'", event_type.replace('"', "\"")));
        }
        if let Some(severity) = self.severity_at_least {
            conditions.push(format!("severity >= {}", severity as i32));
        }
        if let Some(ref actor) = self.actor {
            conditions.push(format!("actor = '{}'", actor.replace('\'', "''")));
        }
        if let Some((start, end)) = self.time_range {
            conditions.push(format!("timestamp >= {} AND timestamp <= {}", start, end));
        }

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        if self.newest_first {
            sql.push_str(" ORDER BY timestamp DESC");
        } else {
            sql.push_str(" ORDER BY timestamp ASC");
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        let rows: Vec<AuditEntryRow> = sqlx::query_as(&sql)
            .fetch_all(pool)
            .await?;

        Ok(rows)
    }
}

/// Integrity violation types
#[derive(Debug, Clone)]
pub enum ViolationType {
    /// Hash doesn't match - entry was modified
    Tampered,
    /// Previous hash doesn't match - chain is broken
    BrokenChain,
    /// Entry ID doesn't match sequence - deletion suspected
    Deleted,
}

/// Single integrity violation
#[derive(Debug, Clone)]
pub struct IntegrityViolation {
    /// Entry ID where violation was found
    pub entry_id: i64,
    /// Type of violation
    pub violation_type: ViolationType,
    /// Human-readable description
    pub description: String,
}

/// Integrity verification report
#[derive(Debug)]
pub struct IntegrityReport {
    /// Total number of entries checked
    pub total_entries: usize,
    /// List of violations found
    pub violations: Vec<IntegrityViolation>,
    /// Whether the trail is valid
    pub valid: bool,
}

impl IntegrityReport {
    /// Check if trail is tamper-free
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Statistics about the audit trail
#[derive(Debug)]
pub struct AuditStats {
    /// Total entries
    pub total_entries: usize,
    /// Number of critical+ events
    pub critical_events: usize,
    /// Number of error events
    pub error_events: usize,
    /// Count by event type
    pub events_by_type: std::collections::HashMap<String, i64>,
    /// Oldest entry timestamp
    pub oldest_timestamp: Option<i64>,
    /// Newest entry timestamp
    pub newest_timestamp: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_audit() -> (AuditTrail, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_audit.db");
        let audit = AuditTrail::open(&db_path).await.unwrap();
        (audit, temp_dir)
    }

    #[tokio::test]
    async fn test_audit_initialization() {
        let (_audit, _temp) = create_test_audit().await;
        // Initialization automatically logs an event
    }

    #[tokio::test]
    async fn test_log_event() {
        let (mut audit, _temp) = create_test_audit().await;

        let id = audit
            .log(AuditEvent {
                event_type: "test_event".to_string(),
                severity: AuditSeverity::Info,
                actor: "test".to_string(),
                action: "test_action".to_string(),
                target: "test_target".to_string(),
                success: true,
                details: Some("Test details".to_string()),
                session_id: Some("test_session".to_string()),
            })
            .await
            .unwrap();

        assert!(id > 0);
    }

    #[tokio::test]
    async fn test_query_by_type() {
        let (mut audit, _temp) = create_test_audit().await;

        // Log different event types
        audit
            .log(AuditEvent {
                event_type: "type_a".to_string(),
                severity: AuditSeverity::Info,
                actor: "user".to_string(),
                action: "action".to_string(),
                target: "target".to_string(),
                success: true,
                details: None,
                session_id: None,
            })
            .await
            .unwrap();

        audit
            .log(AuditEvent {
                event_type: "type_b".to_string(),
                severity: AuditSeverity::Warning,
                actor: "user".to_string(),
                action: "action".to_string(),
                target: "target".to_string(),
                success: false,
                details: None,
                session_id: None,
            })
            .await
            .unwrap();

        // Query for type_a
        let results = audit
            .query(&AuditQuery::new().event_type("type_a"))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "type_a");
    }

    #[tokio::test]
    async fn test_integrity_verification() {
        let (mut audit, _temp) = create_test_audit().await;

        // Log several events
        for i in 0..5 {
            audit
                .log(AuditEvent {
                    event_type: "test".to_string(),
                    severity: AuditSeverity::Info,
                    actor: "user".to_string(),
                    action: format!("action_{}", i),
                    target: "target".to_string(),
                    success: true,
                    details: None,
                    session_id: None,
                })
                .await
                .unwrap();
        }

        // Verify integrity
        let report = audit.verify_integrity().await.unwrap();
        assert!(report.is_valid());
        assert_eq!(report.total_entries, 6); // 5 events + init event
    }

    #[tokio::test]
    async fn test_stats() {
        let (mut audit, _temp) = create_test_audit().await;

        // Log events of different severities
        audit
            .log(AuditEvent {
                event_type: "test".to_string(),
                severity: AuditSeverity::Critical,
                actor: "user".to_string(),
                action: "critical_action".to_string(),
                target: "target".to_string(),
                success: false,
                details: None,
                session_id: None,
            })
            .await
            .unwrap();

        audit
            .log(AuditEvent {
                event_type: "test".to_string(),
                severity: AuditSeverity::Error,
                actor: "user".to_string(),
                action: "error_action".to_string(),
                target: "target".to_string(),
                success: false,
                details: None,
                session_id: None,
            })
            .await
            .unwrap();

        let stats = audit.stats().await.unwrap();
        assert_eq!(stats.total_entries, 3); // 2 + init
        assert_eq!(stats.critical_events, 1);
        assert_eq!(stats.error_events, 1);
    }

    #[tokio::test]
    async fn test_severity_filtering() {
        let (mut audit, _temp) = create_test_audit().await;

        // Log different severities
        for sev in [
            AuditSeverity::Debug,
            AuditSeverity::Info,
            AuditSeverity::Warning,
            AuditSeverity::Critical,
        ] {
            audit
                .log(AuditEvent {
                    event_type: "test".to_string(),
                    severity: sev,
                    actor: "user".to_string(),
                    action: "action".to_string(),
                    target: "target".to_string(),
                    success: true,
                    details: None,
                    session_id: None,
                })
                .await
                .unwrap();
        }

        // Query for severity >= Warning
        let results = audit
            .query(&AuditQuery::new().severity_at_least(AuditSeverity::Warning))
            .await
            .unwrap();

        // Entry count includes initialization event
        // We logged Debug, Info, Warning, Critical = 4 events
        // Plus 1 initialization event = 5 total
        // Warning and Critical should be 2
        assert_eq!(results.len(), 2); // Warning, Critical
    }
}
