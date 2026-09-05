# Services

## Overview

Contextra is built around a service-oriented architecture.

Services are deployable applications responsible for exposing functionality over network interfaces, executing background workloads, or providing operational tooling.

Unlike libraries, services contain little or no business logic.

Their primary responsibilities include:

- Transport
- Authentication
- Authorization
- Configuration
- Dependency injection
- Request validation
- Middleware
- Calling domain libraries

Business logic always resides inside the `libs/` directory.

---

# Service Architecture

```text
                Client

                  │

                  ▼

         Gateway Service

                  │

                  ▼

        Domain Libraries

                  │

        ┌─────────┴─────────┐

        ▼                   ▼

     PostgreSQL          AI Providers
```

Services should be as thin as possible.

---

# Gateway

## Purpose

The Gateway is the public entry point into Contextra.

Every external request enters through this service.

Responsibilities include:

- HTTP REST API
- gRPC Gateway (future)
- Authentication
- Authorization
- Rate limiting
- Middleware
- OpenAPI
- Request validation
- API versioning

The Gateway never contains AI logic.

It delegates all work to libraries.

---

# Worker

## Purpose

Executes asynchronous workloads.

Examples include:

- Document ingestion
- Chunk generation
- Embedding generation
- Reindexing
- Cache invalidation
- Scheduled maintenance
- Background synchronization

Workers consume tasks from a message broker.

Workers never expose HTTP APIs.

---

# CLI

## Purpose

Administrative tooling.

Examples:

- Run migrations
- Create API keys
- Rebuild indexes
- Import documents
- Health diagnostics
- Development utilities

The CLI communicates directly with libraries rather than through HTTP.

---

# Future Services

As Contextra grows, mature libraries may become independently deployable services.

---

## Context Service

Responsibilities:

- Context assembly
- Context ranking
- Context compression
- Prompt optimization

Consumes:

- Memory
- Retrieval

Produces:

- Optimized LLM context

---

## Retrieval Service

Responsibilities:

- Semantic search
- Hybrid retrieval
- Metadata filtering
- Result reranking

Depends on:

- Storage
- Embeddings

---

## Memory Service

Responsibilities:

- Conversation history
- Long-term memory
- Summaries
- Memory retrieval

Provides conversational context.

---

## Embedding Service

Responsibilities:

- Embedding generation
- Batch embedding
- Embedding cache
- Provider selection

---

## Provider Service

Responsibilities:

- OpenAI
- Anthropic
- Gemini
- Ollama

Provides a unified interface for all external AI providers.

---

## Storage Service

Responsibilities:

- PostgreSQL
- Redis
- Qdrant
- Blob storage
- Repository APIs

Owns persistence.

---

## Ingestion Service

Responsibilities:

- File uploads
- Parsing
- Chunking
- Metadata extraction
- Index preparation

Produces searchable documents.

---

# Communication

## External

External clients communicate using:

- REST
- JSON

Future support:

- WebSockets
- Server-Sent Events

---

## Internal

Services communicate using:

- gRPC

Reasons:

- Strong typing
- Streaming support
- Low latency
- Language interoperability

---

## Async

Long-running operations communicate through a message broker.

Examples:

```text
Upload PDF

↓

Queue

↓

Worker

↓

Chunking

↓

Embedding

↓

Indexing

↓

Completed
```

This prevents expensive operations from blocking requests.

---

# Deployment

Every service is independently deployable.

Each service owns:

- Dockerfile
- Configuration
- Health endpoint
- Metrics
- Logs
- Version

Services may be scaled independently.

For example:

```text
Gateway

2 replicas

Memory

2 replicas

Retrieval

8 replicas

Embedding

20 replicas
```

Scaling decisions depend on workload characteristics.

---

# Observability

Every service exposes:

- Health endpoint
- Readiness endpoint
- Metrics
- Structured logs
- Distributed traces

Observability is mandatory.

---

# Service Principles

Every service should satisfy the following rules.

## Thin

Services coordinate.

Libraries implement.

---

## Stateless

Persistent state belongs in storage systems.

---

## Independently Deployable

Every service should be deployable without requiring changes to unrelated services.

---

## Replaceable

A service should be replaceable without affecting higher layers.

---

## Observable

Every request should be traceable across the platform.

---

# Current State

At the current stage of development, only three services exist.

- Gateway
- Worker
- CLI

All domain functionality currently executes in-process through shared libraries.

As the platform evolves, selected libraries may be promoted into independently deployable services without changing their internal implementation.

This gradual evolution avoids premature complexity while preserving a clear migration path toward a distributed architecture.