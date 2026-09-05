# Development Guide

## Overview

This document defines the development standards and **local setup instructions** for Contextra.

---

# Quick Start — Local Dev Environment

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Rust | ≥ 1.90 | Build the workspace |
| Docker | ≥ 24 | Run infrastructure services |
| Docker Compose | ≥ 2.20 | Orchestrate local stack |
| `curl` / `jq` | any | Manual API testing |

## 1. Clone the Repository

```bash
git clone https://github.com/soumyasurana/Contextra.git
cd Contextra
```

## 2. Start Infrastructure Services

Docker Compose brings up PostgreSQL, Redis, and Qdrant in the background:

```bash
docker compose -f deployments/docker/docker-compose.yml up -d postgres redis qdrant
```

Wait for all services to become healthy (≈ 10–15 seconds):

```bash
docker compose -f deployments/docker/docker-compose.yml ps
```

All three services should report `healthy`.

## 3. Set Environment Variables

Copy and adapt the example environment file:

```bash
# Minimum required for local development
export DATABASE_URL="postgres://postgres:postgrespassword@localhost:5432/contextra"
export REDIS_URL="redis://localhost:6379"
export QDRANT_URL="http://localhost:6333"
export CONTEXTRA_ENV=development
```

Or create a `.env` file in the workspace root — `dotenvy` will load it automatically.

## 4. Run Database Migrations

```bash
cargo run -p gateway -- migrate   # if a migrate subcommand is wired
# or manually apply SQL files from migrations/ once they exist
```

## 5. Build the Workspace

```bash
cargo build
```

This compiles all libraries and services including `gateway`, `worker`, and `contextra` (CLI).

## 6. Start the Gateway Service

```bash
cargo run -p gateway
```

The HTTP API is served at `http://127.0.0.1:3000`.

- Swagger UI: [http://127.0.0.1:3000/docs](http://127.0.0.1:3000/docs)
- OpenAPI JSON: [http://127.0.0.1:3000/api-docs/openapi.json](http://127.0.0.1:3000/api-docs/openapi.json)

## 7. Start the Worker Service

In a separate terminal:

```bash
cargo run -p worker
```

Worker configuration (all optional — shown with defaults):

```bash
REDIS_URL=redis://localhost:6379   # Queue backend
WORKER_CONCURRENCY=4               # Concurrent job tasks
WORKER_SWEEP_INTERVAL_SECS=900     # Memory sweep interval (15 min)
WORKER_LOG_LEVEL=info
```

---

# Using the CLI (`contextra`)

Build the CLI binary:

```bash
cargo build -p cli
# Binary at: ./target/debug/contextra
```

Or install globally:

```bash
cargo install --path services/cli
```

## Ingest a Document

```bash
# REST mode (sends to Gateway)
contextra --gateway-url http://127.0.0.1:3000 ingest ./docs/architecture.md

# Local/offline mode (runs entirely in-process — no Gateway needed)
contextra --local ingest ./docs/architecture.md
```

## List Collections

```bash
contextra collections list

# Local mode
contextra --local collections list
```

## Chat

```bash
# Single message
contextra chat "Explain Contextra's retrieval pipeline"

# Interactive REPL
contextra chat

# Resume existing conversation
contextra chat --conversation-id <uuid>

# Local mode (in-process LLM stub)
contextra --local chat "What is context engineering?"
```

## Run Evaluation

```bash
# Default built-in test dataset
contextra --local eval run

# Custom benchmark dataset
contextra --local eval run --dataset ./tests/fixtures/benchmark.json --k 5
```

---

# Running the Full Stack with Docker Compose

Start every service (Gateway + Worker + all infrastructure):

```bash
docker compose -f deployments/docker/docker-compose.yml up --build
```

Stop and remove containers:

```bash
docker compose -f deployments/docker/docker-compose.yml down
```

Remove volumes (wipe all persisted data):

```bash
docker compose -f deployments/docker/docker-compose.yml down -v
```

---

# Testing

## Unit Tests (all workspace packages)

```bash
cargo test
```

## A Specific Package

```bash
cargo test -p worker
cargo test -p gateway
cargo test -p cli
```

## End-to-End Tests

```bash
cargo test -p e2e-tests
```

These tests spin up an in-process Gateway server and exercise:
- CLI local engine ingestion → chat roundtrip
- CLI local evaluation pipeline
- Gateway REST ingest → conversation create → chat roundtrip

## Lint and Format

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

---

# Philosophy

> **Business logic belongs in libraries. Transport belongs in services.**

Libraries implement domain behaviour. Services expose that behaviour to users. This separation must never be violated.

## Workspace Organization

```text
services/         Deployable applications (gateway, worker, cli)
libs/             Reusable domain libraries
tests/            Integration and end-to-end test suites
deployments/      Container and infrastructure definitions
docs/             Architecture and operational documentation
```

## Library Rules

- **Single Responsibility**: one library, one domain
- **Transport Independence**: no HTTP/gRPC imports in libraries
- **Dependency Direction**: always downward (no circular deps)
- **Public API**: small, stable, well-documented

---

# Error Handling

Every library returns `Result`. Avoid:

```rust
panic!()
unwrap()   // only in tests with #[allow(clippy::unwrap_used)]
expect()   // only in tests with #[allow(clippy::expect_used)]
```

---

# Configuration

All configurable values belong in environment variables or TOML config files.

Key variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | — | PostgreSQL connection string |
| `REDIS_URL` | `redis://localhost:6379` | Redis for job queue and cache |
| `QDRANT_URL` | `http://localhost:6333` | Qdrant vector store endpoint |
| `CONTEXTRA_ENV` | `development` | Environment name |
| `WORKER_CONCURRENCY` | `4` | Worker task concurrency |
| `WORKER_SWEEP_INTERVAL_SECS` | `900` | Memory sweep interval |
| `CONTEXTRA_GATEWAY_URL` | `http://127.0.0.1:3000` | CLI default Gateway URL |
| `CONTEXTRA_AUTH_TOKEN` | — | CLI Bearer token |

---

# Logging & Observability

Use structured logging. Every significant operation emits:

- Request ID / Trace ID
- Duration
- Status / error code

Worker jobs also emit:
- `worker_job_duration_seconds` (histogram by job type and status)
- `worker_jobs_total` (counter by job type and status)
- `worker_jobs_retried_total` (counter by job type)

---

# Commits

```text
feat(context): implement context assembler
fix(storage): rollback failed transactions
refactor(retrieval): simplify reranking pipeline
docs(api): update authentication section
```

---

# Pull Requests

Every PR must:
- Compile: `cargo build`
- Pass tests: `cargo test`
- Pass lint: `cargo clippy -- -D warnings`
- Be formatted: `cargo fmt --check`
- Include documentation updates where relevant