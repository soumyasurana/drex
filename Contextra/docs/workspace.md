# Workspace Structure

## Overview

Contextra is organized as a Cargo workspace consisting of reusable libraries and deployable services.

The workspace is designed around a simple principle:

> **Business logic belongs in libraries. Transport logic belongs in services.**

This separation allows the same implementation to be reused by HTTP APIs, background workers, CLI tools, benchmarks, and future microservices.

---

# Repository Layout

```text
contextra/

services/
libs/
proto/
sdk/
configs/
deployments/
docs/
tests/
```

Each top-level directory has a specific responsibility.

---

# Services

The `services/` directory contains executable applications.

Every service has its own `main.rs` and is independently deployable.

Examples include:

```text
services/

gateway/
worker/
cli/
```

Future services may include:

```text
context-service/
retrieval-service/
memory-service/
provider-service/
storage-service/
embedding-service/
```

Services should remain small.

Their primary responsibilities are:

- Exposing HTTP or gRPC APIs
- Authentication
- Request validation
- Middleware
- Configuration
- Dependency injection
- Calling library APIs

Services should **not** contain domain logic.

---

# Libraries

The `libs/` directory contains reusable domain libraries.

Libraries implement all business logic.

Unlike services, libraries know nothing about HTTP, Axum, gRPC, or deployment.

Current libraries include:

```text
common
config
telemetry
errors
types
storage
providers
embeddings
retrieval
memory
context
prompts
ingestion
orchestration
evaluation
```

Each library owns a single responsibility.

---

# Proto

The `proto/` directory contains Protocol Buffer definitions.

These definitions describe communication between services.

Example:

```text
context.proto

retrieval.proto

provider.proto
```

All internal gRPC APIs originate here.

---

# SDK

The `sdk/` directory contains client libraries.

Supported SDKs include:

- Rust
- Python
- TypeScript

Applications should communicate with Contextra through SDKs whenever possible.

---

# Configurations

The `configs/` directory contains environment-specific configuration.

```text
development.toml

testing.toml

staging.toml

production.toml
```

Services load the appropriate configuration at startup.

---

# Deployments

Deployment resources are grouped separately from application code.

Examples include:

- Docker
- Kubernetes
- Helm
- Terraform

Infrastructure code is version controlled alongside the application.

---

# Tests

Tests are organized by purpose rather than by library.

```text
tests/

integration/

e2e/

benchmarks/

load/
```

Individual libraries also contain unit tests.

---

# Dependency Rules

Libraries are organized into layers.

Dependencies always point downward.

```text
Gateway

↓

Orchestration

↓

Context

↓

Memory
Retrieval

↓

Embeddings

↓

Providers

↓

Storage

↓

Core Libraries
```

No library may depend on a higher layer.

Circular dependencies are prohibited.

---

# Shared Libraries

Several libraries are considered foundational.

## errors

Defines shared error types.

## types

Defines shared domain types.

## config

Loads and validates configuration.

## telemetry

Provides logging, tracing, and metrics.

## common

Contains shared utilities that do not belong to a specific domain.

These libraries are intentionally dependency-light.

---

# Development Workflow

When implementing a new feature:

1. Determine the appropriate library.
2. Implement business logic inside the library.
3. Expose the functionality through a service if required.
4. Add integration tests.
5. Update documentation.

Business logic should never be implemented directly inside services.

---

# Why This Structure?

The workspace organization provides several advantages.

- Clear separation of concerns
- Independent testing
- Easy code reuse
- Faster compilation through modular crates
- Straightforward migration to microservices
- Better maintainability

This architecture allows Contextra to begin as a modular workspace while remaining ready for independent service deployment as the platform grows.