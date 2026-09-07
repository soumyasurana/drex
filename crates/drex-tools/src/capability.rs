//! Capability-based authorization for tools
//!
//! This module defines the capability types and provides utilities for
//! checking and managing tool permissions.
//!
//! # Design Principles
//!
//! 1. **Strongly Typed**: Capabilities are enums, not strings
//! 2. **Hierarchical**: Capabilities can be nested (e.g., filesystem.read is a specific
//!    filesystem permission)
//! 3. **Explicit**: Tools must declare required capabilities
//! 4. **Enforced**: Authorization happens before tool execution in the runtime
//!
//! # Example
//!
//! ```rust
//! use drex_tools::capability::{Capability, CapabilitySet};
//!
//! // Create a set of granted capabilities
//! let granted = CapabilitySet::from(vec![
//!     Capability::FileSystemRead,
//!     Capability::FileSystemWrite,
//! ]);
//!
//! // Check if a capability is granted
//! assert!(granted.has(Capability::FileSystemRead));
//! assert!(!granted.has(Capability::TerminalExecute));
//!
//! // Check if all required capabilities are granted
//! let required = CapabilitySet::from(vec![Capability::FileSystemRead]);
//! assert!(granted.has_all(&required));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// Execution context type for permission boundaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionContext {
    /// Interactive execution - user is present and approving
    Interactive,
    /// Autonomous execution - agent running without direct user oversight
    Autonomous,
    /// Background execution - long-running daemon processes
    Background,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::Interactive
    }
}

impl ExecutionContext {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Interactive => "User-present interactive execution",
            Self::Autonomous => "Agent autonomous execution",
            Self::Background => "Background daemon execution",
        }
    }

    /// Check if this is an interactive context
    pub fn is_interactive(&self) -> bool {
        matches!(self, Self::Interactive)
    }

    /// Check if this is an autonomous context
    pub fn is_autonomous(&self) -> bool {
        matches!(self, Self::Autonomous | Self::Background)
    }

    /// Check if this is a background context
    pub fn is_background(&self) -> bool {
        matches!(self, Self::Background)
    }
}

/// A capability represents a permission that Drex can grant to tools.
///
/// Capabilities are strongly typed and organized hierarchically where
/// applicable. New capabilities can be added without breaking existing
/// tools or registry code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Permission to read files from the filesystem
    FileSystemRead,
    /// Permission to write files to the filesystem
    FileSystemWrite,
    /// Permission to execute terminal/shell commands
    TerminalExecute,
    /// Permission to make HTTP requests via browser (external)
    BrowserRequest,
    /// Permission to make HTTP requests to localhost/internal (restricted)
    BrowserRequestInternal,
    /// Permission to read from the memory store
    MemoryRead,
    /// Permission to write to the memory store
    MemoryWrite,
    /// Permission to execute commands with elevated privileges
    PrivilegedExecution,
    /// Permission to modify file/directory permissions
    PermissionModify,
    /// Permission to send notifications/alerts
    NotificationSend,
    /// Permission to schedule future events
    EventSchedule,
    /// Permission to control computer (mouse, keyboard, screen)
    ComputerControl,
    /// Permission to capture audio/video
    MediaCapture,
}

impl Capability {
    /// Get the context level required for this capability.
    ///
    /// Returns the minimum execution context where this capability is allowed.
    pub fn min_context(&self) -> ExecutionContext {
        match self {
            // Interactive only - user must be present
            Self::TerminalExecute
            | Self::PrivilegedExecution
            | Self::PermissionModify
            | Self::ComputerControl
            | Self::MediaCapture => ExecutionContext::Interactive,

            // Autonomous allowed but with restrictions
            Self::FileSystemWrite | Self::MemoryWrite => ExecutionContext::Autonomous,

            // Background allowed (scheduled tasks)
            Self::EventSchedule => ExecutionContext::Background,

            // Generally safe for all contexts
            _ => ExecutionContext::Autonomous,
        }
    }

    /// Check if this capability is allowed in autonomous mode.
    pub fn allowed_in_autonomous(&self) -> bool {
        self.min_context() != ExecutionContext::Interactive
    }

    /// Check if this capability requires explicit user confirmation in autonomous mode.
    pub fn requires_confirmation(&self) -> bool {
        matches!(
            self,
            Self::TerminalExecute
                | Self::PrivilegedExecution
                | Self::BrowserRequest
                | Self::ComputerControl
                | Self::MediaCapture
        )
    }

    /// Get a human-readable description of this capability.
    pub fn description(&self) -> &'static str {
        match self {
            Self::FileSystemRead => "Read files from the filesystem",
            Self::FileSystemWrite => "Write files to the filesystem",
            Self::TerminalExecute => "Execute terminal/shell commands",
            Self::BrowserRequest => "Make HTTP requests to external sites",
            Self::BrowserRequestInternal => "Make HTTP requests to internal services",
            Self::MemoryRead => "Read from the memory store",
            Self::MemoryWrite => "Write to the memory store",
            Self::PrivilegedExecution => "Execute with elevated privileges",
            Self::PermissionModify => "Modify file/directory permissions",
            Self::NotificationSend => "Send notifications and alerts",
            Self::EventSchedule => "Schedule future events",
            Self::ComputerControl => "Control computer (mouse, keyboard, screen)",
            Self::MediaCapture => "Capture audio and video",
        }
    }

    /// Get the capability as a unique string identifier.
    ///
    /// This is useful for serialization and display purposes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FileSystemRead => "filesystem.read",
            Self::FileSystemWrite => "filesystem.write",
            Self::TerminalExecute => "terminal.execute",
            Self::BrowserRequest => "browser.request",
            Self::BrowserRequestInternal => "browser.request_internal",
            Self::MemoryRead => "memory.read",
            Self::MemoryWrite => "memory.write",
            Self::PrivilegedExecution => "execution.privileged",
            Self::PermissionModify => "permission.modify",
            Self::NotificationSend => "notification.send",
            Self::EventSchedule => "event.schedule",
            Self::ComputerControl => "control.computer",
            Self::MediaCapture => "capture.media",
        }
    }

    /// Parse a capability from its string representation.
    ///
    /// Returns `None` if the string doesn't match any known capability.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "filesystem.read" => Some(Self::FileSystemRead),
            "filesystem.write" => Some(Self::FileSystemWrite),
            "terminal.execute" => Some(Self::TerminalExecute),
            "browser.request" => Some(Self::BrowserRequest),
            "browser.request_internal" => Some(Self::BrowserRequestInternal),
            "memory.read" => Some(Self::MemoryRead),
            "memory.write" => Some(Self::MemoryWrite),
            "execution.privileged" => Some(Self::PrivilegedExecution),
            "permission.modify" => Some(Self::PermissionModify),
            "notification.send" => Some(Self::NotificationSend),
            "event.schedule" => Some(Self::EventSchedule),
            "control.computer" => Some(Self::ComputerControl),
            "capture.media" => Some(Self::MediaCapture),
            _ => None,
        }
    }

    /// Check if this capability is a filesystem-related capability.
    pub fn is_filesystem(&self) -> bool {
        matches!(self, Self::FileSystemRead | Self::FileSystemWrite)
    }

    /// Check if this capability is dangerous (potentially destructive).
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            Self::FileSystemWrite
                | Self::TerminalExecute
                | Self::MemoryWrite
                | Self::PrivilegedExecution
                | Self::PermissionModify
                | Self::ComputerControl
        )
    }

    /// Get all available capabilities.
    pub fn all() -> &'static [Capability] {
        use std::sync::OnceLock;
        static ALL_CAPS: OnceLock<Vec<Capability>> = OnceLock::new();
        ALL_CAPS.get_or_init(|| {
            vec![
                Self::FileSystemRead,
                Self::FileSystemWrite,
                Self::TerminalExecute,
                Self::BrowserRequest,
                Self::BrowserRequestInternal,
                Self::MemoryRead,
                Self::MemoryWrite,
                Self::PrivilegedExecution,
                Self::PermissionModify,
                Self::NotificationSend,
                Self::EventSchedule,
                Self::ComputerControl,
                Self::MediaCapture,
            ]
        })
    }

    /// Get capabilities allowed for autonomous execution.
    pub fn autonomous_allowed() -> &'static [Capability] {
        use std::sync::OnceLock;
        static AUTO_ALLOWED: OnceLock<Vec<Capability>> = OnceLock::new();
        AUTO_ALLOWED.get_or_init(|| {
            Self::all()
                .iter()
                .filter(|c| c.allowed_in_autonomous())
                .copied()
                .collect()
        })
    }

    /// Get capabilities that require interactive execution.
    pub fn interactive_only() -> &'static [Capability] {
        use std::sync::OnceLock;
        static INTERACTIVE: OnceLock<Vec<Capability>> = OnceLock::new();
        INTERACTIVE.get_or_init(|| {
            Self::all()
                .iter()
                .filter(|c| !c.allowed_in_autonomous())
                .copied()
                .collect()
        })
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Capability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s).ok_or_else(|| format!("unknown capability: {}", s))
    }
}

/// A set of capabilities for efficient checking.
///
/// This is more efficient than using a `Vec<Capability>` for repeated
/// lookups and provides convenient set operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    capabilities: HashSet<Capability>,
}

impl CapabilitySet {
    /// Create an empty capability set.
    pub fn new() -> Self {
        Self {
            capabilities: HashSet::new(),
        }
    }

    /// Create a capability set with specific capabilities.
    pub fn with_capabilities(capabilities: &[Capability]) -> Self {
        Self {
            capabilities: capabilities.iter().copied().collect(),
        }
    }

    /// Create a capability set with no capabilities (harmless tools only).
    pub fn harmless() -> Self {
        Self::new()
    }

    /// Create a capability set with all capabilities.
    pub fn all() -> Self {
        Self::with_capabilities(Capability::all())
    }

    /// Check if a capability is in the set.
    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Check if all capabilities in another set are present.
    pub fn has_all(&self, other: &CapabilitySet) -> bool {
        other.capabilities.iter().all(|c| self.has(*c))
    }

    /// Check if any capability in another set is present.
    pub fn has_any(&self, other: &CapabilitySet) -> bool {
        other.capabilities.iter().any(|c| self.has(*c))
    }

    /// Add a capability to the set.
    pub fn add(&mut self, capability: Capability) -> &mut Self {
        self.capabilities.insert(capability);
        self
    }

    /// Remove a capability from the set.
    pub fn remove(&mut self, capability: Capability) -> &mut Self {
        self.capabilities.remove(&capability);
        self
    }

    /// Check if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Get the number of capabilities in the set.
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Get missing capabilities compared to another set.
    ///
    /// Returns capabilities that are in `other` but not in `self`.
    pub fn missing(&self, other: &CapabilitySet) -> Vec<Capability> {
        other
            .capabilities
            .iter()
            .filter(|c| !self.has(**c))
            .copied()
            .collect()
    }

    /// Iterate over capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }

    /// Convert to a sorted vector.
    pub fn to_vec(&self) -> Vec<Capability> {
        let mut vec: Vec<_> = self.capabilities.iter().copied().collect();
        vec.sort_by_key(|c| c.as_str());
        vec
    }

    /// Check if this set grants filesystem access.
    pub fn has_filesystem_access(&self) -> bool {
        self.capabilities.iter().any(|c| c.is_filesystem())
    }

    /// Check if this set contains dangerous capabilities.
    pub fn has_dangerous(&self) -> bool {
        self.capabilities.iter().any(|c| c.is_dangerous())
    }

    /// Check if this set contains any interactive-only capabilities.
    pub fn has_interactive_only(&self) -> bool {
        self.capabilities.iter().any(|c| !c.allowed_in_autonomous())
    }

    /// Filter capabilities for autonomous execution.
    ///
    /// Returns a new CapabilitySet with only capabilities allowed
    /// in autonomous mode.
    pub fn for_autonomous(&self) -> Self {
        Self {
            capabilities: self
                .capabilities
                .iter()
                .filter(|c| c.allowed_in_autonomous())
                .copied()
                .collect(),
        }
    }

    /// Filter capabilities for execution context.
    ///
    /// Returns a new CapabilitySet with only capabilities allowed
    /// for the given execution context.
    pub fn for_context(&self, context: ExecutionContext) -> Self {
        match context {
            ExecutionContext::Interactive => self.clone(),
            ExecutionContext::Autonomous | ExecutionContext::Background => self.for_autonomous(),
        }
    }

    /// Get the set of capabilities that require user confirmation.
    pub fn requiring_confirmation(&self) -> Self {
        Self {
            capabilities: self
                .capabilities
                .iter()
                .filter(|c| c.requires_confirmation())
                .copied()
                .collect(),
        }
    }
}

impl From<Vec<Capability>> for CapabilitySet {
    fn from(capabilities: Vec<Capability>) -> Self {
        Self::with_capabilities(&capabilities)
    }
}

impl From<&[Capability]> for CapabilitySet {
    fn from(capabilities: &[Capability]) -> Self {
        Self::with_capabilities(capabilities)
    }
}

impl<'a> FromIterator<&'a Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = &'a Capability>>(iter: I) -> Self {
        let capabilities: HashSet<_> = iter.into_iter().copied().collect();
        Self { capabilities }
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        Self {
            capabilities: iter.into_iter().collect(),
        }
    }
}

impl Extend<Capability> for CapabilitySet {
    fn extend<T: IntoIterator<Item = Capability>>(&mut self, iter: T) {
        self.capabilities.extend(iter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_as_str_roundtrip() {
        for cap in Capability::all() {
            let s = cap.as_str();
            let parsed = Capability::from_str(s);
            assert_eq!(parsed, Some(*cap), "roundtrip failed for {:?}", cap);
        }
    }

    #[test]
    fn capability_from_str_invalid() {
        assert_eq!(Capability::from_str("unknown"), None);
        assert_eq!(Capability::from_str(""), None);
        assert_eq!(Capability::from_str("filesystem"), None);
    }

    #[test]
    fn capability_display() {
        assert_eq!(Capability::FileSystemRead.to_string(), "filesystem.read");
        assert_eq!(Capability::TerminalExecute.to_string(), "terminal.execute");
    }

    #[test]
    fn capability_is_filesystem() {
        assert!(Capability::FileSystemRead.is_filesystem());
        assert!(Capability::FileSystemWrite.is_filesystem());
        assert!(!Capability::TerminalExecute.is_filesystem());
        assert!(!Capability::BrowserRequest.is_filesystem());
    }

    #[test]
    fn capability_is_dangerous() {
        assert!(Capability::FileSystemWrite.is_dangerous());
        assert!(Capability::TerminalExecute.is_dangerous());
        assert!(!Capability::FileSystemRead.is_dangerous());
        assert!(!Capability::BrowserRequest.is_dangerous());
    }

    #[test]
    fn capability_set_new_is_empty() {
        let set = CapabilitySet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn capability_set_add_and_has() {
        let mut set = CapabilitySet::new();
        set.add(Capability::FileSystemRead);
        assert!(set.has(Capability::FileSystemRead));
        assert!(!set.has(Capability::FileSystemWrite));
    }

    #[test]
    fn capability_set_has_all() {
        let set = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::FileSystemWrite,
        ]);

        let required = CapabilitySet::from(vec![Capability::FileSystemRead]);
        assert!(set.has_all(&required));

        let not_met = CapabilitySet::from(vec![Capability::TerminalExecute]);
        assert!(!set.has_all(&not_met));
    }

    #[test]
    fn capability_set_has_any() {
        let set = CapabilitySet::from(vec![Capability::FileSystemRead]);

        let with_match = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
        ]);
        assert!(set.has_any(&with_match));

        let no_match = CapabilitySet::from(vec![Capability::TerminalExecute]);
        assert!(!set.has_any(&no_match));
    }

    #[test]
    fn capability_set_missing() {
        let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
        let required = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
        ]);

        let missing = granted.missing(&required);
        assert_eq!(missing.len(), 1);
        assert!(missing.contains(&Capability::TerminalExecute));
    }

    #[test]
    fn capability_set_harmless() {
        let set = CapabilitySet::harmless();
        assert!(set.is_empty());
    }

    #[test]
    fn capability_set_all() {
        let set = CapabilitySet::all();
        assert_eq!(set.len(), Capability::all().len());
        for cap in Capability::all() {
            assert!(set.has(*cap));
        }
    }

    #[test]
    fn capability_set_from_vec() {
        let set: CapabilitySet = vec![Capability::BrowserRequest].into();
        assert!(set.has(Capability::BrowserRequest));
    }

    #[test]
    fn capability_set_collect() {
        let set: CapabilitySet = [Capability::FileSystemRead, Capability::FileSystemWrite]
            .iter()
            .collect();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn capability_set_has_filesystem_access() {
        let fs_set = CapabilitySet::from(vec![Capability::FileSystemRead]);
        assert!(fs_set.has_filesystem_access());

        let no_fs_set = CapabilitySet::from(vec![Capability::BrowserRequest]);
        assert!(!no_fs_set.has_filesystem_access());
    }

    #[test]
    fn capability_set_has_dangerous() {
        let dangerous = CapabilitySet::from(vec![Capability::FileSystemWrite]);
        assert!(dangerous.has_dangerous());

        let safe = CapabilitySet::from(vec![Capability::FileSystemRead]);
        assert!(!safe.has_dangerous());
    }

    #[test]
    fn execution_context_default_is_interactive() {
        let ctx = ExecutionContext::default();
        assert_eq!(ctx, ExecutionContext::Interactive);
        assert!(ctx.is_interactive());
        assert!(!ctx.is_autonomous());
    }

    #[test]
    fn execution_context_autonomous_check() {
        let auto = ExecutionContext::Autonomous;
        assert!(!auto.is_interactive());
        assert!(auto.is_autonomous());

        let background = ExecutionContext::Background;
        assert!(!background.is_interactive());
        assert!(background.is_autonomous());
        assert!(background.is_background());
    }

    #[test]
    fn capability_min_context_terminal_is_interactive() {
        assert_eq!(
            Capability::TerminalExecute.min_context(),
            ExecutionContext::Interactive
        );
        assert_eq!(
            Capability::ComputerControl.min_context(),
            ExecutionContext::Interactive
        );
    }

    #[test]
    fn capability_min_context_read_is_autonomous() {
        assert_eq!(
            Capability::FileSystemRead.min_context(),
            ExecutionContext::Autonomous
        );
        assert_eq!(
            Capability::BrowserRequest.min_context(),
            ExecutionContext::Autonomous
        );
    }

    #[test]
    fn capability_allowed_in_autonomous() {
        assert!(Capability::FileSystemRead.allowed_in_autonomous());
        assert!(!Capability::TerminalExecute.allowed_in_autonomous());
        assert!(!Capability::ComputerControl.allowed_in_autonomous());
    }

    #[test]
    fn capability_requires_confirmation() {
        assert!(Capability::TerminalExecute.requires_confirmation());
        assert!(Capability::ComputerControl.requires_confirmation());
        assert!(!Capability::FileSystemRead.requires_confirmation());
    }

    #[test]
    fn capability_set_for_autonomous() {
        let full = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::FileSystemWrite,
            Capability::TerminalExecute,
        ]);
        let auto = full.for_autonomous();

        assert!(auto.has(Capability::FileSystemRead));
        assert!(auto.has(Capability::FileSystemWrite));
        assert!(!auto.has(Capability::TerminalExecute));
    }

    #[test]
    fn capability_set_for_context_interactive_preserves_all() {
        let full = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
        ]);
        let interactive = full.for_context(ExecutionContext::Interactive);

        assert!(interactive.has(Capability::FileSystemRead));
        assert!(interactive.has(Capability::TerminalExecute));
    }

    #[test]
    fn capability_set_for_context_autonomous_filters() {
        let full = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
        ]);
        let auto = full.for_context(ExecutionContext::Autonomous);

        assert!(auto.has(Capability::FileSystemRead));
        assert!(!auto.has(Capability::TerminalExecute));
    }

    #[test]
    fn capability_set_has_interactive_only() {
        let interactive = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
        ]);
        assert!(interactive.has_interactive_only());

        let auto_only = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::MemoryRead,
        ]);
        assert!(!auto_only.has_interactive_only());
    }

    #[test]
    fn capability_set_requiring_confirmation() {
        let mixed = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::TerminalExecute,
            Capability::BrowserRequest,
        ]);
        let confirmation = mixed.requiring_confirmation();

        assert_eq!(confirmation.len(), 2);
        assert!(confirmation.has(Capability::TerminalExecute));
        assert!(confirmation.has(Capability::BrowserRequest));
        assert!(!confirmation.has(Capability::FileSystemRead));
    }

    #[test]
    fn capability_autonomous_allowed_list() {
        let auto_allowed = Capability::autonomous_allowed();
        assert!(!auto_allowed.contains(&Capability::TerminalExecute));
        assert!(!auto_allowed.contains(&Capability::ComputerControl));
        assert!(auto_allowed.contains(&Capability::FileSystemRead));
    }

    #[test]
    fn capability_interactive_only_list() {
        let interactive = Capability::interactive_only();
        assert!(interactive.contains(&Capability::TerminalExecute));
        assert!(interactive.contains(&Capability::ComputerControl));
        assert!(!interactive.contains(&Capability::FileSystemRead));
    }
}
