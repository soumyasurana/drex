# Storage

## Overview

The Storage library is the persistence layer of Contextra.

Its responsibility is to provide a unified interface for storing and retrieving structured data, vector embeddings, cached information, and binary assets.

Higher-level libraries never interact directly with databases.

Instead, all persistence is performed through the Storage library.

This ensures that storage technologies remain interchangeable while keeping business logic independent of infrastructure.

---

# Responsibilities

The Storage library is responsible for:

- Relational data persistence
- Vector storage
- Repository abstractions
- Transactions
- File storage
- Migrations
- Query execution
- Connection pooling

Storage does **not** implement retrieval, memory, or AI logic.

---

# Architecture

```text
                Context

                    │

                    ▼

              Repository API

                    │

          Storage Library

      ┌────────┼──────────┐

      ▼        ▼          ▼

 PostgreSQL  Redis    Vector Store

                    │

                    ▼

                 File Storage
```

Storage exposes repositories.

Repositories communicate with databases.

Libraries never communicate with databases directly.

---

# Components

## Database

Stores structured relational data.

Examples include:

- Conversations
- Documents
- Users
- Collections
- Prompt templates
- Metadata
- Jobs

PostgreSQL is the primary relational database.

---

## Vector Store

Stores embeddings for semantic search.

Supported implementations include:

- Qdrant
- pgvector
- Chroma

The storage crate now exposes a backend-agnostic `VectorStore` trait, a
`QdrantVectorStore` implementation, and an `InMemoryVectorStore` mock for unit
tests.

Only one vector database is required at runtime.

The implementation is selected through configuration.

---

## Cache

Redis provides temporary storage for:

- Embedding cache
- Session cache
- Context cache
- Rate limits
- Background tasks

Cached data is considered disposable.

The cache layer exposes a generic `Cache` trait and a `RedisCache` implementation.
`SessionStore` builds on top of that cache interface to store per-conversation
session state with a TTL.

---

## Blob Storage

Stores binary content.

Examples:

- PDFs
- Images
- Office documents
- Markdown
- Audio
- Video

Blob storage is independent of relational data.

---

# Repository Pattern

Business logic never executes SQL directly.

Repositories expose typed interfaces.

Example:

```text
ConversationRepository

DocumentRepository

ChunkRepository

MemoryRepository
```

Each repository owns persistence for exactly one aggregate.

---

# Transactions

Storage provides transactional guarantees.

Example:

```text
Create Document

↓

Store Metadata

↓

Create Chunks

↓

Store Embeddings

↓

Commit
```

If any step fails, the transaction is rolled back.

---

# Storage Layers

```text
Application

↓

Repository

↓

Storage Engine

↓

Database Driver
```

Higher layers never depend on implementation details.

---

# Data Model

The storage layer manages several categories of information.

## Documents

Uploaded files.

Metadata.

Ownership.

Collections.

---

## Chunks

Text fragments generated during ingestion.

Chunks are optimized for retrieval.

---

## Embeddings

Vector representations of chunks.

Embeddings are provider independent.

---

## Conversations

Conversation metadata.

Sessions.

Participants.

---

## Messages

Individual conversation messages.

User messages.

Assistant responses.

Tool calls.

---

## Memories

Long-term information extracted from conversations.

Examples:

- Preferences
- Facts
- User profile
- Organization knowledge

---

## Context Cache

Recently generated contexts.

Improves latency for repeated requests.

---

# Storage Interfaces

Every backend implements a common interface.

Examples include:

```text
Repository

VectorStore

BlobStore

CacheStore
```

Higher libraries communicate only through these interfaces.

---

# Design Principles

## Provider Independent

Business logic never depends on PostgreSQL or Qdrant directly.

---

## Strong Typing

All repositories expose strongly typed APIs.

---

## Transactions First

Data consistency is preferred over partial writes.

---

## Replaceable

Storage engines can be replaced through configuration.

---

## Observable

Every operation emits tracing spans and metrics.

---

# Future Support

The architecture supports additional storage systems.

Relational databases:

- PostgreSQL
- MySQL
- SQLite

Vector databases:

- Qdrant
- pgvector
- Chroma
- Pinecone
- Weaviate

Blob storage:

- Local filesystem
- Amazon S3
- Google Cloud Storage
- Azure Blob Storage

Caches:

- Redis
- DragonflyDB

Support for new backends requires implementing the corresponding storage interfaces.

---

# Summary

The Storage library isolates all persistence concerns behind well-defined interfaces.

Higher-level libraries never interact directly with infrastructure.

By separating persistence from business logic, Contextra remains portable, testable, and capable of supporting multiple databases and storage providers without changing application code.
