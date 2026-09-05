//! Git tools - read-only repository inspection
//!
//! This module provides safe, read-only access to Git repositories.
//! All operations respect the filesystem security model and require
//! the filesystem.read capability.
//!
//! # Security
//!
//! - Repository paths must be within the allowed root
//! - Path traversal is rejected before any git operations
//! - Only read-only git operations are supported
//! - No commit, push, checkout, reset, or other mutating operations

use crate::capability::{Capability, CapabilitySet};
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

/// Configuration for Git tools.
///
/// The allowed root must be set by the trusted Drex runtime.
#[derive(Debug, Clone)]
pub struct GitConfig {
    /// The allowed root directory for git repositories
    allowed_root: PathBuf,
    /// Timeout for git operations
    timeout: Duration,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            allowed_root: PathBuf::from("/tmp"),
            timeout: Duration::from_secs(30),
        }
    }
}

impl GitConfig {
    /// Create a new git config with the specified allowed root.
    pub fn new(allowed_root: impl AsRef<Path>) -> Self {
        Self {
            allowed_root: allowed_root.as_ref().to_path_buf(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Set the timeout for git operations.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Validate that a repository path is within the allowed root.
///
/// This uses the same validation logic as the filesystem tool.
fn validate_repo_path(root: &Path, repo_path: &str) -> ToolResult<PathBuf> {
    // Reject paths with traversal components
    if repo_path.contains("..") {
        return Err(ToolError::ExecutionFailed {
            tool: "git".to_string(),
            reason: format!("path '{}' contains directory traversal attempts", repo_path),
        });
    }

    // Join with allowed root
    let resolved = root.join(repo_path);

    // Canonicalize to resolve any symlinks
    let canonical = resolved.canonicalize().map_err(|e| {
        ToolError::ExecutionFailed {
            tool: "git".to_string(),
            reason: format!("failed to resolve path '{}': {}", repo_path, e),
        }
    })?;

    // Canonicalize the allowed root
    let canonical_root = root.canonicalize().map_err(|e| {
        ToolError::ExecutionFailed {
            tool: "git".to_string(),
            reason: format!("failed to resolve allowed root: {}", e),
        }
    })?;

    // Verify the path is within the allowed root
    if !canonical.starts_with(&canonical_root) {
        return Err(ToolError::ExecutionFailed {
            tool: "git".to_string(),
            reason: format!(
                "path '{}' is outside allowed root '{}'",
                canonical.display(),
                canonical_root.display()
            ),
        });
    }

    // Verify it's actually a git repository
    let git_dir = canonical.join(".git");
    if !git_dir.exists() {
        return Err(ToolError::ExecutionFailed {
            tool: "git".to_string(),
            reason: format!("'{}' is not a git repository", repo_path),
        });
    }

    Ok(canonical)
}

/// Input for git status tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusInput {
    /// Path to the git repository (relative to allowed root)
    pub repo_path: String,
}

/// Output for git status tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusOutput {
    /// The repository path that was inspected
    pub repo_path: String,
    /// Current branch name
    pub branch: String,
    /// Files that are modified but not staged
    pub modified: Vec<String>,
    /// Files that are staged for commit
    pub staged: Vec<String>,
    /// Untracked files
    pub untracked: Vec<String>,
    /// Whether the working tree is clean
    pub is_clean: bool,
}

/// Tool for reading git repository status.
#[derive(Debug, Clone)]
pub struct GitStatusTool {
    metadata: ToolMetadata,
    config: GitConfig,
}

impl GitStatusTool {
    /// Create a new git status tool.
    pub fn new(config: GitConfig) -> Self {
        let schema = ToolSchema::builder("GitStatusInput", "Get git repository status")
            .required_string("repo_path", "Path to the git repository")
            .build();

        Self {
            metadata: ToolMetadata::new(
                "git.status",
                "Get the status of a git repository.\n\
                \n\
                Returns information about the current branch, modified files, \
                staged changes, and untracked files. This is a read-only operation.",
                schema,
            ),
            config,
        }
    }

    /// Execute git status command.
    async fn git_status(&self, repo_path: &Path) -> ToolResult<GitStatusOutput> {
        let start = std::time::Instant::now();

        // Get current branch
        let branch_output = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().to_string(),
                reason: format!("failed to get branch: {}", e),
            })?;

        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();

        // Get status in porcelain format
        let status_output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().to_string(),
                reason: format!("failed to get status: {}", e),
            })?;

        let status_str = String::from_utf8_lossy(&status_output.stdout);

        let mut modified = Vec::new();
        let mut staged = Vec::new();
        let mut untracked = Vec::new();

        for line in status_str.lines() {
            if line.len() < 3 {
                continue;
            }
            let status_code = &line[0..2];
            let filename = line[3..].to_string();

            match status_code {
                "??" => untracked.push(filename),
                " M" | "M " | "MM" => modified.push(filename),
                "A " | "AM" => staged.push(filename),
                _ => {
                    // Handle other status codes
                    if status_code.starts_with('M') {
                        modified.push(filename);
                    } else if status_code.starts_with('A') {
                        staged.push(filename);
                    }
                }
            }
        }

        let _duration = start.elapsed();
        let is_clean = modified.is_empty() && staged.is_empty() && untracked.is_empty();

        Ok(GitStatusOutput {
            repo_path: repo_path.to_string_lossy().to_string(),
            branch,
            modified,
            staged,
            untracked,
            is_clean,
        })
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        static REQUIRED: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        REQUIRED.get_or_init(|| CapabilitySet::from(vec![Capability::FileSystemRead]))
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let repo_path_str = input.require_string("repo_path")?;

        // Audit log
        info!(
            tool = self.name(),
            repo_path_requested = %repo_path_str,
            allowed_root = %self.config.allowed_root.display(),
            "git.status attempted"
        );

        // Validate path before any git operations
        let validated = match validate_repo_path(&self.config.allowed_root, repo_path_str) {
            Ok(path) => {
                info!(
                    tool = self.name(),
                    repo_path_requested = %repo_path_str,
                    repo_path_resolved = %path.display(),
                    "path validation succeeded"
                );
                path
            }
            Err(e) => {
                warn!(
                    tool = self.name(),
                    repo_path_requested = %repo_path_str,
                    error = %e,
                    "path validation failed"
                );
                return Err(e);
            }
        };

        // Execute git status with timeout
        let start = std::time::Instant::now();
        match tokio::time::timeout(self.config.timeout, self.git_status(&validated)).await {
            Ok(Ok(output)) => {
                let duration = start.elapsed();
                info!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    branch = %output.branch,
                    modified_count = output.modified.len(),
                    staged_count = output.staged.len(),
                    untracked_count = output.untracked.len(),
                    is_clean = output.is_clean,
                    duration_ms = duration.as_millis() as u64,
                    "git.status succeeded"
                );
                Ok(ExecutionResult::success(json!(output))
                    .with_duration(duration.as_millis() as u64))
            }
            Ok(Err(e)) => {
                warn!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    error = %e,
                    "git.status failed"
                );
                Err(e)
            }
            Err(_) => {
                warn!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    timeout_sec = self.config.timeout.as_secs(),
                    "git.status timed out"
                );
                Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: format!("git status timed out after {}s", self.config.timeout.as_secs()),
                })
            }
        }
    }
}

/// Input for git diff tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffInput {
    /// Path to the git repository (relative to allowed root)
    pub repo_path: String,
    /// Optional: specific file to diff (if not specified, diffs all)
    #[serde(default)]
    pub file: Option<String>,
}

/// Output for git diff tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffOutput {
    /// The repository path that was inspected
    pub repo_path: String,
    /// The file that was diffed (if specified)
    pub file: Option<String>,
    /// The diff output
    pub diff: String,
    /// Number of files changed (if diffing all)
    pub files_changed: Option<usize>,
}

/// Tool for reading git diff.
#[derive(Debug, Clone)]
pub struct GitDiffTool {
    metadata: ToolMetadata,
    config: GitConfig,
}

impl GitDiffTool {
    /// Create a new git diff tool.
    pub fn new(config: GitConfig) -> Self {
        let schema = ToolSchema::builder("GitDiffInput", "Get git diff")
            .required_string("repo_path", "Path to the git repository")
            .optional_string("file", "Optional: specific file to diff")
            .build();

        Self {
            metadata: ToolMetadata::new(
                "git.diff",
                "Get the diff of changes in a git repository.\n\
                \n\
                Returns the unified diff of uncommitted changes. Optionally \
                specify a file to get only that file's diff. This is a read-only operation.",
                schema,
            ),
            config,
        }
    }

    /// Execute git diff command.
    async fn git_diff(&self, repo_path: &Path, file: Option<&str>) -> ToolResult<GitDiffOutput> {
        let start = std::time::Instant::now();

        let mut args = vec!["diff"];
        if let Some(f) = file {
            args.push(f);
        }

        let output = Command::new("git")
            .args(&args)
            .current_dir(repo_path)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().to_string(),
                reason: format!("failed to execute git diff: {}", e),
            })?;

        let diff = String::from_utf8_lossy(&output.stdout).to_string();

        // Count files changed (rough count based on diff headers)
        let files_changed = diff.lines().filter(|l| l.starts_with("diff --git")).count();

        let _duration = start.elapsed();

        Ok(GitDiffOutput {
            repo_path: repo_path.to_string_lossy().to_string(),
            file: file.map(|s| s.to_string()),
            diff,
            files_changed: if file.is_none() { Some(files_changed) } else { None },
        })
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        static REQUIRED: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        REQUIRED.get_or_init(|| CapabilitySet::from(vec![Capability::FileSystemRead]))
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let repo_path_str = input.require_string("repo_path")?;
        let file = input.get_string("file").map(|s| s.to_string());

        // Audit log
        info!(
            tool = self.name(),
            repo_path_requested = %repo_path_str,
            file = ?file,
            allowed_root = %self.config.allowed_root.display(),
            "git.diff attempted"
        );

        // Validate path before any git operations
        let validated = match validate_repo_path(&self.config.allowed_root, repo_path_str) {
            Ok(path) => {
                info!(
                    tool = self.name(),
                    repo_path_requested = %repo_path_str,
                    repo_path_resolved = %path.display(),
                    "path validation succeeded"
                );
                path
            }
            Err(e) => {
                warn!(
                    tool = self.name(),
                    repo_path_requested = %repo_path_str,
                    error = %e,
                    "path validation failed"
                );
                return Err(e);
            }
        };

        // Execute git diff with timeout
        let start = std::time::Instant::now();
        let file_ref = file.as_deref();
        match tokio::time::timeout(self.config.timeout, self.git_diff(&validated, file_ref)).await {
            Ok(Ok(output)) => {
                let duration = start.elapsed();
                info!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    diff_bytes = output.diff.len(),
                    files_changed = ?output.files_changed,
                    duration_ms = duration.as_millis() as u64,
                    "git.diff succeeded"
                );
                Ok(ExecutionResult::success(json!(output))
                    .with_duration(duration.as_millis() as u64))
            }
            Ok(Err(e)) => {
                warn!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    error = %e,
                    "git.diff failed"
                );
                Err(e)
            }
            Err(_) => {
                warn!(
                    tool = self.name(),
                    repo_path = %validated.display(),
                    timeout_sec = self.config.timeout.as_secs(),
                    "git.diff timed out"
                );
                Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: format!("git diff timed out after {}s", self.config.timeout.as_secs()),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::process::Command;

    async fn create_test_repo() -> (PathBuf, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .await
            .expect("git init failed");

        // Configure git user for commits
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .await
            .unwrap();

        // Create initial file and commit
        tokio::fs::write(repo_path.join("README.md"), "# Test Repo")
            .await
            .unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&repo_path)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_path)
            .output()
            .await
            .unwrap();

        (repo_path, temp_dir)
    }

    #[tokio::test]
    async fn git_status_on_clean_repo() {
        let (repo_path, _temp) = create_test_repo().await;
        let config = GitConfig::new(&repo_path);
        let tool = GitStatusTool::new(config);

        let input = ToolInput::from_json(json!({"repo_path": "."})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_ok());
        let output: GitStatusOutput = serde_json::from_value(result.unwrap().data.unwrap()).unwrap();
        assert_eq!(output.branch, "master"); // or main depending on git version
        assert!(output.is_clean);
    }

    #[tokio::test]
    async fn git_status_with_changes() {
        let (repo_path, _temp) = create_test_repo().await;
        let config = GitConfig::new(&repo_path);
        let tool = GitStatusTool::new(config);

        // Modify a file
        tokio::fs::write(repo_path.join("README.md"), "# Modified")
            .await
            .unwrap();

        // Create untracked file
        tokio::fs::write(repo_path.join("new.txt"), "new content")
            .await
            .unwrap();

        let input = ToolInput::from_json(json!({"repo_path": "."})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_ok());
        let output: GitStatusOutput = serde_json::from_value(result.unwrap().data.unwrap()).unwrap();
        assert!(!output.is_clean);
        assert_eq!(output.modified.len(), 1);
        assert_eq!(output.untracked.len(), 1);
    }

    #[tokio::test]
    async fn git_diff_with_changes() {
        let (repo_path, _temp) = create_test_repo().await;
        let config = GitConfig::new(&repo_path);
        let tool = GitDiffTool::new(config);

        // Modify a file
        tokio::fs::write(repo_path.join("README.md"), "# Modified")
            .await
            .unwrap();

        let input =
            ToolInput::from_json(json!({"repo_path": ".", "file": "README.md"})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_ok());
        let output: GitDiffOutput = serde_json::from_value(result.unwrap().data.unwrap()).unwrap();
        assert!(output.diff.contains("Modified"));
    }

    #[tokio::test]
    async fn git_status_invalid_repo() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig::new(temp_dir.path());
        let tool = GitStatusTool::new(config);

        // Try on non-git directory
        let input = ToolInput::from_json(json!({"repo_path": "."})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a git repository"));
    }

    #[tokio::test]
    async fn git_traversal_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig::new(temp_dir.path());
        let tool = GitStatusTool::new(config);

        let input = ToolInput::from_json(json!({"repo_path": "../etc"})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[tokio::test]
    async fn git_status_nested_path() {
        let (repo_path, _temp) = create_test_repo().await;
        let parent = repo_path.parent().unwrap();
        let config = GitConfig::new(parent);
        let tool = GitStatusTool::new(config);

        // Access via nested path
        let repo_name = repo_path.file_name().unwrap().to_str().unwrap();
        let input = ToolInput::from_json(json!({"repo_path": repo_name})).unwrap();
        let result = tool.execute(&ToolContext::new(), input).await;

        assert!(result.is_ok());
    }

    #[test]
    fn git_tools_require_filesystem_read() {
        let status = GitStatusTool::new(GitConfig::default());
        let diff = GitDiffTool::new(GitConfig::default());

        assert!(status.required_capabilities().has(Capability::FileSystemRead));
        assert!(diff.required_capabilities().has(Capability::FileSystemRead));
    }
}
