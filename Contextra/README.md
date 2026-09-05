# Contextra

## A Production-Grade Context Engineering Platform for AI Applications

Build intelligent AI applications by managing the complete lifecycle of context — from document ingestion and semantic retrieval to memory, orchestration, and provider execution.

![Rust](https://img.shields.io/badge/Rust-2024-orange?logo=rust)
![License](https://img.shields.io/badge/License-MIT-blue)
![Status](https://img.shields.io/badge/Status-Active%20Development-success)
![Architecture](https://img.shields.io/badge/Architecture-Modular%20Workspace-blueviolet)

> A modular Rust platform for building context-aware AI systems through document ingestion, semantic retrieval, memory management, prompt orchestration, and multi-provider LLM integration.

---

## Overview

Contextra is a production-grade AI context engineering platform built in Rust.

Unlike traditional Retrieval-Augmented Generation (RAG) frameworks that focus primarily on retrieval, Contextra treats **context** as a first-class engineering problem. It provides reusable infrastructure for constructing, optimizing, and managing context throughout the entire lifecycle of an AI application.

The platform is designed as a modular Rust workspace of independently deployable microservices sharing business logic through well-defined domain libraries.

---

## Why Contextra?

Most AI frameworks solve isolated problems — retrieval, vector databases, prompt templating, or agent execution in isolation.

Contextra focuses on **the complete context lifecycle**.

Instead of asking:

> "How do I retrieve documents?"

Contextra asks:

> "What is the best possible context for this model given everything the system knows?"

Retrieval becomes only one component of a much larger Context Engineering Platform.

---

## Quick Start

### Prerequisites

- Rust ≥ 1.90
- Docker & Docker Compose ≥ 2.20

### Clone

```bash
git clone https://github.com/soumyasurana/Contextra.git
cd Contextra
```

### Start Infrastructure

```bash
docker compose -f deployments/docker/docker-compose.yml up -d postgres redis qdrant
```

### Build

```bash
cargo build
```

### Run Tests

```bash
cargo test
```

### Start the Gateway

```bash
DATABASE_URL=postgres://postgres:postgrespassword@localhost:5432/contextra \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6333 \
cargo run -p gateway
```

API docs available at: http://127.0.0.1:3000/docs

---

## CLI (`contextra`)

Install the CLI:

```bash
cargo install --path services/cli
```

### Ingest a Document

```bash
# Via Gateway REST API
contextra ingest ./docs/architecture.md

# Offline/local mode (no network required)
contextra --local ingest ./docs/architecture.md
```

### Chat

```bash
# Single message
contextra chat "Explain Contextra's retrieval pipeline"

# Interactive REPL
contextra chat

# Offline mode
contextra --local chat "What is context engineering?"
```

### List Collections

```bash
contextra collections list
```

### Run Evaluation

```bash
# Built-in benchmark dataset
contextra --local eval run

# Custom dataset
contextra --local eval run --dataset ./tests/fixtures/benchmark.json --k 5
```

### Global Options

```
--gateway-url <URL>     Gateway REST API endpoint [env: CONTEXTRA_GATEWAY_URL] [default: http://127.0.0.1:3000]
--auth-token <TOKEN>    Bearer auth token         [env: CONTEXTRA_AUTH_TOKEN]
--local                 Run in offline/local mode [env: CONTEXTRA_LOCAL]
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                         Services                             │
│   ┌──────────┐   ┌──────────┐   ┌──────────────────────┐    │
│   │ Gateway  │   │  Worker  │   │    CLI (contextra)    │    │
│   │ (HTTP)   │   │ (Queue)  │   │ (REST + Local modes)  │    │
│   └────┬─────┘   └────┬─────┘   └──────────┬───────────┘    │
└────────┼──────────────┼────────────────────┼────────────────┘
         │              │                    │
┌────────▼──────────────▼────────────────────▼────────────────┐
│                      Domain Libraries                         │
│                                                               │
│  orchestration → context → memory + retrieval                 │
│                          → embeddings → providers             │
│                          → ingestion                          │
│                          → evaluation                         │
│                          → prompts                            │
│                          → storage (Postgres/Redis/Qdrant)    │
└──────────────────────────────────────────────────────────────┘
```

### Services

| Service | Description |
|---------|-------------|
| `services/gateway` | REST API server (Axum) — documents, collections, conversations, chat |
| `services/worker` | Background job processor — ingestion, evaluation, memory sweeps |
| `services/cli` | Command-line client with REST and local/offline execution modes |

### Libraries

| Library | Description |
|---------|-------------|
| `libs/types` | Shared domain types (`DocumentId`, `CollectionId`, `Chunk`, etc.) |
| `libs/errors` | Unified `ContextraError` type |
| `libs/common` | Shared utilities — pagination, cursors |
| `libs/settings` | Config loading via `config-rs` (TOML + env) |
| `libs/auth` | Authentication context and JWT verification |
| `libs/storage` | `RedisCache`, `PostgresStore`, `QdrantVectorStore`, `InMemoryVectorStore` |
| `libs/providers` | OpenAI / Anthropic / Gemini LLM provider abstraction |
| `libs/embeddings` | OpenAI and Ollama embedding providers with caching |
| `libs/ingestion` | Document parsing, chunking, embedding, and vector upsert pipeline |
| `libs/retrieval` | Semantic, hybrid, and metadata-filtered retrieval with reranking |
| `libs/memory` | Conversation memory, importance scoring, rolling summarization |
| `libs/context` | Context assembly and token-budget optimization |
| `libs/prompts` | Prompt template registry with Handlebars and versioning |
| `libs/orchestration` | Chat request orchestration pipeline |
| `libs/evaluation` | Retrieval and generation quality benchmarking |
| `libs/telemetry` | Structured logging, metrics, and OpenTelemetry tracing |

---

## Infrastructure

### Docker Compose (local dev)

```bash
# Start all services
docker compose -f deployments/docker/docker-compose.yml up --build

# Infrastructure only (for local `cargo run`)
docker compose -f deployments/docker/docker-compose.yml up -d postgres redis qdrant

# Teardown
docker compose -f deployments/docker/docker-compose.yml down

# Teardown + delete volumes
docker compose -f deployments/docker/docker-compose.yml down -v
```

| Service | Port |
|---------|------|
| PostgreSQL | 5432 |
| Redis | 6379 |
| Qdrant | 6333 / 6334 |
| Gateway | 3000 |

---

## Testing

```bash
# All workspace tests
cargo test

# Specific package
cargo test -p worker
cargo test -p gateway
cargo test -p cli

# End-to-end tests (in-process Gateway + CLI roundtrip)
cargo test -p e2e-tests

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --check
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [docs/development.md](docs/development.md) | Local setup, Docker Compose, CLI usage, testing |
| [docs/architecture.md](docs/architecture.md) | System architecture and design principles |
| [docs/api.md](docs/api.md) | REST API reference |
| [docs/libraries.md](docs/libraries.md) | Library catalog and responsibilities |
| [docs/retrieval.md](docs/retrieval.md) | Retrieval pipeline design |
| [docs/memory.md](docs/memory.md) | Memory and summarization system |
| [docs/orchestration.md](docs/orchestration.md) | Orchestration pipeline |
| [docs/providers.md](docs/providers.md) | LLM and embedding provider guide |
| [docs/storage.md](docs/storage.md) | Storage backends |

---

## License

MIT — see [LICENSE](LICENSE)
