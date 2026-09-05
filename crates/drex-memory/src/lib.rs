//! Drex Memory System
//!
//! This crate provides a backend-agnostic memory storage abstraction for Drex.
//! It defines memory types, store operations, and metadata structures.
//!
//! ## Design Philosophy
//!
//! - **Backend-agnostic**: The contract is independent of any specific storage
//!   implementation (PostgreSQL, Redis, Qdrant, etc.).
//!
//! - **Strongly typed**: Memory IDs, kinds, and operations use domain-specific
//!   types to prevent misuse.
//!
//! ## Contextra Mapping
//!
//! Drex's memory contract is designed to work with Contextra's storage infrastructure.
//! The Contextra integration has been extended to support full CRUD operations.

mod memory;
mod query;
mod store;

pub mod contextra;
pub mod policy;

pub use memory::{
    Memory, MemoryId, MemoryKind, MemoryMetadata, MemoryPatch, MemorySource, SensitivityLevel,
};
pub use query::MemoryQuery;
pub use store::{MemoryStore, MemoryStoreError};
pub use contextra::ContextraMemoryStore;
pub use policy::{
    Confidence, MemoryDecision, MemoryPolicy, PolicyContext, PolicyEnforcingStore, Provenance,
    RuleBasedPolicy, TaskTrustLevel, Ttl,
};

#[cfg(test)]
mod tests;
