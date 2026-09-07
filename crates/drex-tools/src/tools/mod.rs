//! Built-in tools provided by the drex-tools crate

pub mod echo;
pub mod filesystem;
pub mod terminal;
pub mod terminal_security;
pub mod git;
pub mod web;
pub mod web_security;
pub mod memory;
pub mod memory_cleanup;
pub mod memory_inspect;
pub mod computer_control;

#[cfg(feature = "browser")]
pub mod browser;

pub use computer_control::{
    ComputerControlTool, ComputerControlConfig, ComputerControlInput, ComputerControlOutput,
    ClickInput, ClickOutput, TypeInput, TypeOutput, CaptureInput, CaptureOutput, CaptureRegion,
};

pub use echo::EchoTool;
pub use filesystem::{FileSystemConfig, FileSystemError, FileSystemReadTool};
pub use terminal::{TerminalConfig, TerminalExecuteTool};
pub use terminal_security::{TerminalSecurityPolicy, TerminalSecurityError};
pub use git::{GitConfig, GitStatusTool, GitDiffTool, GitStatusOutput, GitDiffOutput};
pub use web::{WebFetchConfig, WebFetchTool, WebFetchOutput};
pub use memory::{MemoryTool, MemoryInput, MemoryAction, MemoryStoreOutput, MemoryRetrieveOutput};
pub use memory_cleanup::{MemoryCleanupTool, MemoryCleanupInput, CleanupAction, MemoryCleanupOutput, CleanupCandidate, PreservedRecord};
pub use memory_inspect::{MemoryInspectTool, MemoryInspectInput, MemoryInspectOutput, InspectCandidate, InspectPreserved};

#[cfg(feature = "browser")]
pub use browser::{Browser, BrowserConfig, BrowserError, Page};
