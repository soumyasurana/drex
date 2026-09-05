# Libraries

## Overview

Contextra is built around a collection of reusable domain libraries.

Libraries contain all business logic and remain independent of transport layers such as HTTP, gRPC, CLI, or background workers.

Every library has a single responsibility and communicates with other libraries through well-defined interfaces.

---

# Library Dependency Graph

```text
Gateway Service
        │
        ▼
Orchestration
        │
        ▼
Context
      ↙     ↘
Memory   Retrieval
           │
           ▼
     Embeddings
           │
           ▼
      Providers
           │
           ▼
       Storage

──────────────

Errors
Types
Config
Telemetry
Core
```

The lower a library appears in the graph, the fewer dependencies it should have.

Dependencies always point downward.

Circular dependencies are never permitted.

---

# Core Libraries

These libraries form the foundation of the entire platform.

---

## core

Purpose

Contains fundamental utilities shared across the workspace.

Examples include:

- UUID helpers
- Time utilities
- Pagination
- Generic helper traits
- Serialization helpers

The core library should remain lightweight.

Domain-specific code does **not** belong here.

---

## errors

Purpose

Provides the standard error system used throughout Contextra.

Responsibilities:

- Shared error definitions
- Result aliases
- Error conversions
- Error categories

Every public API should return a shared result type.

Example:

```rust
ContextraResult<T>
```

---

## types

Purpose

Defines shared domain objects.

Examples include:

- ChatMessage
- Conversation
- ContextRequest
- ContextResponse
- Embedding
- SearchResult
- DocumentMetadata

These are **not database models**.

They represent the language spoken between libraries.

---

## config

Purpose

Loads and validates application configuration.

Responsibilities include:

- Environment loading
- TOML configuration
- Validation
- Configuration merging
- Typed configuration structures

Configuration is loaded once and shared across the application.

---

## telemetry

Purpose

Provides logging, tracing, metrics, and observability.

Responsibilities:

- tracing initialization
- OpenTelemetry
- Metrics
- Structured logging
- Span propagation

Every executable initializes telemetry through this library.

---

# Domain Libraries

---

## storage

Purpose

Provides persistent storage abstractions.

Responsibilities:

- PostgreSQL
- Redis
- Qdrant
- Repository pattern
- Transactions
- Database models
- Migrations

Storage should never depend on retrieval, context, or providers.

---

## providers

Purpose

Abstracts external AI systems.

Supported provider categories:

- Language Models
- Embedding Models
- Vector Stores
- Rerankers

Every provider implements a common interface.

No provider-specific implementation should leak into higher libraries.

---

## embeddings

Purpose

Generates vector embeddings.

Responsibilities:

- Batch generation
- Caching
- Retry handling
- Model selection
- Provider integration

Embeddings depend on providers but not on retrieval.

---

## retrieval

Purpose

Finds relevant information.

Responsibilities:

- Semantic retrieval
- Hybrid retrieval
- Metadata filtering
- Similarity search
- Reranking
- Query optimization

Retrieval never communicates directly with language models.

---

## memory

Purpose

Maintains conversational state.

Responsibilities:

- Conversation history
- Long-term memory
- Summaries
- Importance scoring
- Memory retrieval

Memory provides contextual information to the Context Engine.

---

## context

Purpose

The Context Engine is the heart of Contextra.

Responsibilities:

- Context assembly
- Ranking
- Compression
- Prompt optimization
- Memory integration
- Retrieval integration

Its output is the optimized context sent to downstream language models.

Everything in the platform ultimately exists to improve context quality.

---

## prompts

Purpose

Manages prompt templates.

Responsibilities:

- Template storage
- Variable substitution
- Prompt versioning
- Prompt composition

Prompt logic remains separate from context generation.

---

## ingestion

Purpose

Processes external content.

Responsibilities:

- File parsing
- Text extraction
- Chunk generation
- Metadata extraction
- Index preparation

Ingestion prepares data for embedding and retrieval.

---

## orchestration

Purpose

Coordinates workflows.

Examples include:

- Chat execution
- Document ingestion
- Multi-step pipelines
- Evaluation
- Workflow execution

The orchestration library contains coordination logic but no domain implementations.

---

## evaluation

Purpose

Measures system quality.

Responsibilities:

- Retrieval evaluation
- Context evaluation
- Prompt evaluation
- Performance metrics
- Benchmarks

Evaluation enables continuous improvement of AI quality.

---

# Library Rules

Every library must satisfy the following principles.

## Single Responsibility

Each library owns exactly one domain.

---

## Transport Independent

Libraries must not depend on:

- HTTP
- Axum
- gRPC
- CLI
- Deployment

Only services expose transport layers.

---

## Stable Public API

Libraries expose a minimal, well-defined public interface.

Internal implementation details remain private.

---

## Testability

Every library should be testable in isolation.

Business logic should not require running services.

---

## Minimal Dependencies

Libraries should depend only on what they require.

Avoid introducing unnecessary transitive dependencies.

---

# Future Growth

As libraries mature, they may be wrapped by independently deployable services.

Example:

```text
libs/context

↓

services/context-service
```

The library remains the source of truth while the service provides network access.

This approach allows Contextra to evolve into a distributed platform without rewriting business logic.