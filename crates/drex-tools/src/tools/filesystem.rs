//! Filesystem tool - safely read files from a restricted root directory
//!
//! This tool provides read-only access to files within a configured
//! allowed root directory. It protects against path traversal attacks
//! and ensures files outside the allowed root cannot be accessed.

use crate::capability::{Capability, CapabilitySet};
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use tracing::{info, warn};

/// Configuration for the filesystem tool.
///
/// This must be set by the trusted Drex runtime and cannot
/// be modified by model-generated input.
#[derive(Debug, Clone)]
pub struct FileSystemConfig {
    /// The allowed root directory for file access
    allowed_root: PathBuf,
    /// Maximum file size to read (in bytes). Larger files are rejected.
    max_file_size: usize,
}

impl Default for FileSystemConfig {
    fn default() -> Self {
        Self {
            allowed_root: PathBuf::from("/tmp"),
            max_file_size: 10 * 1024 * 1024, // 10 MB default
        }
    }
}

impl FileSystemConfig {
    /// Create a new filesystem config with the specified allowed root.
    pub fn new(allowed_root: impl AsRef<Path>) -> Self {
        Self {
            allowed_root: allowed_root.as_ref().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
        }
    }

    /// Set the maximum file size.
    pub fn with_max_size(mut self, bytes: usize) -> Self {
        self.max_file_size = bytes;
        self
    }
}

/// Input for the filesystem read tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemReadInput {
    /// The path to read (relative to allowed root)
    pub path: String,
}

/// Output for the filesystem read tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemReadOutput {
    /// The file contents
    pub content: String,
    /// The resolved path (for verification)
    pub resolved_path: String,
    /// File size in bytes
    pub size_bytes: usize,
}

/// Error types specific to filesystem operations.
#[derive(Debug, Clone, PartialEq)]
pub enum FileSystemError {
    PathOutsideAllowedRoot { path: String, root: String },
    PathContainsTraversal { path: String },
    DirectoryNotFile { path: String },
    FileNotFound { path: String },
    PermissionDenied { path: String },
    FileTooLarge { path: String, size: usize, max: usize },
    NotAbsolutePath { path: String },
}

impl std::fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathOutsideAllowedRoot { path, root } => {
                write!(f, "path '{}' is outside allowed root '{}'", path, root)
            }
            Self::PathContainsTraversal { path } => {
                write!(f, "path '{}' contains directory traversal attempts", path)
            }
            Self::DirectoryNotFile { path } => {
                write!(f, "'{}' is a directory, not a file", path)
            }
            Self::FileNotFound { path } => write!(f, "file not found: '{}'", path),
            Self::PermissionDenied { path } => {
                write!(f, "permission denied accessing '{}'", path)
            }
            Self::FileTooLarge { path, size, max } => {
                write!(
                    f,
                    "file '{}' is too large ({} bytes, max {} bytes)",
                    path, size, max
                )
            }
            Self::NotAbsolutePath { path } => {
                write!(
                    f,
                    "path '{}' is not absolute, cannot verify outside root",
                    path
                )
            }
        }
    }
}

impl std::error::Error for FileSystemError {}

/// Tool for safely reading files from the filesystem.
///
/// The tool is restricted to files within a configured allowed root.
/// Path traversal attempts are rejected before file access.
#[derive(Debug, Clone)]
pub struct FileSystemReadTool {
    metadata: ToolMetadata,
    config: FileSystemConfig,
}

impl FileSystemReadTool {
    /// Create a new filesystem read tool with the given configuration.
    ///
    /// The `allowed_root` must be provided by the trusted Drex runtime.
    pub fn new(config: FileSystemConfig) -> Self {
        let schema = ToolSchema::builder("FileSystemReadInput", "Read a file from the filesystem")
            .required_string("path", "The relative path to the file to read")
            .build();

        Self {
            metadata: ToolMetadata::new(
                "filesystem.read",
                "Read a file from the filesystem within the allowed root directory.\n\
                \n\
                This tool provides safe, read-only access to files. Path traversal \
                attempts (../) are rejected. Only paths within the configured \
                allowed root can be accessed.",
                schema,
            ),
            config,
        }
    }

    /// Validate and resolve a path.
    ///
    /// Returns the canonical, absolute path if valid, or an error if:
    /// - The path contains traversal components (..)
    /// - The resolved path is outside the allowed root
    pub fn validate_path(&self, input_path: &str) -> Result<PathBuf, FileSystemError> {
        // Reject paths with traversal components
        if input_path.contains("..") {
            return Err(FileSystemError::PathContainsTraversal {
                path: input_path.to_string(),
            });
        }

        // Validate each path component
        let path = Path::new(input_path);
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    return Err(FileSystemError::PathContainsTraversal {
                        path: input_path.to_string(),
                    });
                }
                Component::RootDir | Component::Prefix(_) => {
                    // Absolute paths or paths with prefixes are handled below
                }
                Component::Normal(_) | Component::CurDir => {
                    // These are fine
                }
            }
        }

        // Join with allowed root
        let resolved = self.config.allowed_root.join(path);

        // Canonicalize to resolve any symlinks and get absolute path
        // This also validates the file exists
        let canonical = resolved.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileSystemError::FileNotFound {
                    path: input_path.to_string(),
                }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                FileSystemError::PermissionDenied {
                    path: input_path.to_string(),
                }
            } else {
                FileSystemError::FileNotFound {
                    path: input_path.to_string(),
                }
            }
        })?;

        // Canonicalize the allowed root for comparison
        let canonical_root = self.config.allowed_root.canonicalize().map_err(|_| {
            FileSystemError::PermissionDenied {
                path: format!("allowed root: {:?}", self.config.allowed_root),
            }
        })?;

        // Verify the canonical path is within or equal to the allowed root
        // This handles symlink traversal and normalization
        if !canonical.starts_with(&canonical_root) {
            return Err(FileSystemError::PathOutsideAllowedRoot {
                path: input_path.to_string(),
                root: canonical_root.to_string_lossy().to_string(),
            });
        }

        Ok(canonical)
    }

    /// Read a file at the given path.
    pub async fn read_file(&self, path: &Path) -> Result<FileSystemReadOutput, FileSystemError> {
        // Check it's a file, not a directory
        let metadata = tokio::fs::metadata(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FileSystemError::FileNotFound {
                    path: path.to_string_lossy().to_string(),
                }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                FileSystemError::PermissionDenied {
                    path: path.to_string_lossy().to_string(),
                }
            } else {
                FileSystemError::FileNotFound {
                    path: path.to_string_lossy().to_string(),
                }
            }
        })?;

        if metadata.is_dir() {
            return Err(FileSystemError::DirectoryNotFile {
                path: path.to_string_lossy().to_string(),
            });
        }

        let size = metadata.len() as usize;
        if size > self.config.max_file_size {
            return Err(FileSystemError::FileTooLarge {
                path: path.to_string_lossy().to_string(),
                size,
                max: self.config.max_file_size,
            });
        }

        // Read file content
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                FileSystemError::PermissionDenied {
                    path: path.to_string_lossy().to_string(),
                }
            } else {
                FileSystemError::PermissionDenied {
                    path: path.to_string_lossy().to_string(),
                }
            }
        })?;

        Ok(FileSystemReadOutput {
            content,
            resolved_path: path.to_string_lossy().to_string(),
            size_bytes: size,
        })
    }
}

#[async_trait]
impl Tool for FileSystemReadTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        static REQUIRED: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        REQUIRED.get_or_init(|| CapabilitySet::from(vec![Capability::FileSystemRead]))
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let path_str = input.require_string("path")?;

        // Audit log the attempt (sanitized)
        info!(
            tool = self.name(),
            path_requested = %path_str,
            allowed_root = %self.config.allowed_root.display(),
            "filesystem.read attempted"
        );

        // Validate path before any filesystem access
        let validated_path = match self.validate_path(path_str) {
            Ok(path) => {
                info!(
                    tool = self.name(),
                    path_requested = %path_str,
                    path_resolved = %path.display(),
                    "path validation succeeded"
                );
                path
            }
            Err(e) => {
                warn!(
                    tool = self.name(),
                    path_requested = %path_str,
                    error = %e,
                    "path validation failed"
                );
                return Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: e.to_string(),
                });
            }
        };

        // Read the file
        let start = std::time::Instant::now();
        match self.read_file(&validated_path).await {
            Ok(output) => {
                let duration = start.elapsed();
                info!(
                    tool = self.name(),
                    path_requested = %path_str,
                    path_resolved = %output.resolved_path,
                    size_bytes = output.size_bytes,
                    duration_ms = duration.as_millis() as u64,
                    "filesystem.read succeeded"
                );
                Ok(ExecutionResult::success(json!(output)).with_duration(duration.as_millis() as u64))
            }
            Err(e) => {
                let duration = start.elapsed();
                warn!(
                    tool = self.name(),
                    path_requested = %path_str,
                    path_resolved = %validated_path.display(),
                    error = %e,
                    duration_ms = duration.as_millis() as u64,
                    "filesystem.read failed"
                );
                Err(ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: e.to_string(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_setup() -> (FileSystemReadTool, TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = FileSystemConfig::new(&root);
        let tool = FileSystemReadTool::new(config);

        (tool, temp_dir, root)
    }

    #[tokio::test]
    async fn read_file_success() {
        let (tool, _temp_dir, root) = create_test_setup().await;

        // Create a test file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "hello world").await.unwrap();

        // Validate the path
        let validated = tool.validate_path("test.txt").unwrap();
        assert_eq!(validated, test_file);

        // Read the file
        let output = tool.read_file(&validated).await.unwrap();
        assert_eq!(output.content, "hello world");
        assert_eq!(output.size_bytes, 11);
    }

    #[tokio::test]
    async fn read_file_not_found() {
        let (tool, _temp_dir, _root) = create_test_setup().await;

        let result = tool.validate_path("nonexistent.txt");
        assert!(matches!(result, Err(FileSystemError::FileNotFound { .. })));
    }

    #[tokio::test]
    async fn read_directory_rejected() {
        let (tool, _temp_dir, root) = create_test_setup().await;

        // Create a directory
        let test_dir = root.join("testdir");
        fs::create_dir(&test_dir).await.unwrap();

        let result = tool.read_file(&test_dir).await;
        assert!(matches!(result, Err(FileSystemError::DirectoryNotFile { .. })));
    }

    #[tokio::test]
    async fn traversal_rejected() {
        let (tool, _temp_dir, _root) = create_test_setup().await;

        // Create a file outside the temp dir
        let outside_path = "../etc/passwd";
        let result = tool.validate_path(outside_path);
        assert!(matches!(
            result,
            Err(FileSystemError::PathContainsTraversal { path }) if path == "../etc/passwd"
        ));
    }

    #[tokio::test]
    async fn absolute_path_outside_root_rejected() {
        let (tool, _temp_dir, _root) = create_test_setup().await;

        // On Unix systems, use /etc/passwd
        #[cfg(unix)]
        {
            let result = tool.validate_path("/etc/passwd");
            // This will fail because we canonicalize and check if it starts with allowed_root
            // /etc/passwd won't canonicalize to be under temp_dir, so it should fail
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn nested_path_allowed() {
        let (tool, _temp_dir, root) = create_test_setup().await;

        // Create nested directory and file
        let nested = root.join("subdir").join("nested.txt");
        fs::create_dir_all(nested.parent().unwrap()).await.unwrap();
        fs::write(&nested, "nested content").await.unwrap();

        let result = tool.validate_path("subdir/nested.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), nested);
    }

    #[tokio::test]
    async fn dot_path_normalized() {
        let (tool, _temp_dir, root) = create_test_setup().await;

        // Create a file
        let test_file = root.join("test.txt");
        fs::write(&test_file, "content").await.unwrap();

        // Path with ./ should work
        let validated = tool.validate_path("./test.txt").unwrap();
        assert_eq!(validated, test_file);
    }

    #[tokio::test]
    async fn tool_metadata_correct() {
        let config = FileSystemConfig::new("/tmp");
        let tool = FileSystemReadTool::new(config);

        assert_eq!(tool.name(), "filesystem.read");
        assert_eq!(tool.required_capabilities().len(), 1);
        assert!(tool.required_capabilities().has(Capability::FileSystemRead));
    }

    #[test]
    fn filesystem_error_display() {
        let err = FileSystemError::PathContainsTraversal {
            path: "../test".to_string(),
        };
        assert!(err.to_string().contains("traversal"));

        let err = FileSystemError::PathOutsideAllowedRoot {
            path: "/etc".to_string(),
            root: "/tmp".to_string(),
        };
        assert!(err.to_string().contains("outside allowed root"));

        let err = FileSystemError::FileNotFound {
            path: "missing.txt".to_string(),
        };
        assert!(err.to_string().contains("not found"));

        let err = FileSystemError::DirectoryNotFile {
            path: "/tmp".to_string(),
        };
        assert!(err.to_string().contains("directory"));
    }
}