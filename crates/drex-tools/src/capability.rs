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

    /// Permission to make HTTP requests via browser
    BrowserRequest,
}

impl Capability {
    /// Get a human-readable description of this capability.
    pub fn description(&self) -> &'static str {
        match self {
            Self::FileSystemRead => "Read files from the filesystem",
            Self::FileSystemWrite => "Write files to the filesystem",
            Self::TerminalExecute => "Execute terminal/shell commands",
            Self::BrowserRequest => "Make HTTP requests via browser",
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
            _ => None,
        }
    }

    /// Check if this capability is a filesystem-related capability.
    pub fn is_filesystem(&self) -> bool {
        matches!(self, Self::FileSystemRead | Self::FileSystemWrite)
    }

    /// Check if this capability is dangerous (potentially destructive).
    pub fn is_dangerous(&self) -> bool {
        matches!(self, Self::FileSystemWrite | Self::TerminalExecute)
    }

    /// Get all available capabilities.
    pub fn all() -> &'static [Capability] {
        &[
            Self::FileSystemRead,
            Self::FileSystemWrite,
            Self::TerminalExecute,
            Self::BrowserRequest,
        ]
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
}
