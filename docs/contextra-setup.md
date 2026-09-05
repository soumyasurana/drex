# Contextra Setup Guide

This document provides detailed setup instructions for Contextra, the persistent memory subsystem used by Drex.

---

## What is Contextra?

Contextra is a **context engineering platform** that provides:

- **Persistent Memory**: Store and retrieve memories with semantic search
- **Context Assembly**: Build optimal context for LLM requests
- **Document Ingestion**: Process PDFs, text, and other documents
- **Vector Search**: Semantic retrieval using Qdrant
- **Conversation History**: Episodic memory for conversations

Drex uses Contextra as its memory backend. Without Contextra, Drex operates with only ephemeral in-memory storage.

---

## Architecture

```
Drex Agent → drex-memory → ContextraMemoryStore → Contextra Gateway → [PostgreSQL, Redis, Qdrant]
```

| Layer | Technology | Purpose |
|-------|------------|---------|
| **Gateway** | Rust (Axum) | REST API for memory operations |
| **PostgreSQL** | SQL | Relational data, conversation history |
| **Redis** | Key/Value | Caching, sessions, pub/sub |
| **Qdrant** | Vector DB | Semantic search and embeddings |

---

## Quick Start (Development)

### Prerequisites

- Docker & Docker Compose
- Rust 1.90+
- curl (for health checks)

### Step 1: Start Infrastructure

```bash
# From Drex root directory
cd /path/to/DREX

# Start PostgreSQL, Redis, and Qdrant
docker compose up -d

# Verify services
docker ps

# Wait for healthy status
docker compose ps
```

### Step 2: Build Contextra

```bash
cd Contextra

# Build the Gateway service (first time takes 5-10 minutes)
cargo build --release -p gateway
```

### Step 3: Run Migrations

```bash
# Contextra uses SQLx for database migrations
# These run automatically on first startup, or you can apply manually:

# Option 1: Let Gateway auto-migrate on start
# (default behavior)

# Option 2: Use sqlx-cli for manual migration
cargo install sqlx-cli
DATABASE_URL=postgres://postgres:postgrespassword@localhost:5432/drex \
  sqlx migrate run --source Contextra/libs/storage/migrations
```

### Step 4: Start Contextra Gateway

In a **separate terminal**:

```bash
cd Contextra

export DATABASE_URL=postgres://postgres:postgrespassword@localhost:5432/drex
export REDIS_URL=redis://localhost:6379
export QDRANT_URL=http://localhost:6333
export RUST_LOG=info

cargo run --release -p gateway
```

The Gateway will start on `http://localhost:3000`.

### Step 5: Verify

```bash
# Check health
curl http://localhost:3000/health
# Expected: {"status":"healthy"}

# List collections
curl http://localhost:3000/collections
# Expected: [] or list of existing collections
```

---

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | Required | PostgreSQL connection string |
| `REDIS_URL` | Required | Redis connection string |
| `QDRANT_URL` | Required | Qdrant server URL |
| `SERVER_PORT` | 3000 | Gateway HTTP port |
| `RUST_LOG` | info | Log level (trace/debug/info/warn/error) |
| `CONTEXTRA_API_KEY` | None | Optional API key for authentication |

### Drex Integration

Drex connects to Contextra via the `drex-memory` crate:

```rust
// From Drex perspective
use drex_memory::ContextraMemoryStore;

let store = ContextraMemoryStore::new(
    gateway_url: "http://localhost:3000",
    api_key: None,
);
```

Configuration values should be set in:
- `.env` file (recommended for development)
- Environment variables
- Or `crates/drex-config/configs/default.toml`

---

## Troubleshooting

### "Connection refused" errors

**Problem**: Contextra Gateway not running

**Solution**:
```bash
# Check if Gateway is running
curl http://localhost:3000/health

# If not, restart it
cd Contextra
cargo run --release -p gateway
```

### PostgreSQL connection fails

**Problem**: Database not running or wrong credentials

**Solution**:
```bash
# Check PostgreSQL
docker ps | grep drex-postgres

# Check logs
docker logs drex-postgres

# Restart
docker compose restart postgres

# Verify connection
psql postgres://postgres:postgrespassword@localhost:5432/drex -c "SELECT 1"
```

### Redis connection fails

**Problem**: Redis not running

**Solution**:
```bash
# Check Redis
docker ps | grep drex-redis

# Test connection
redis-cli -u redis://localhost:6379 ping

# Restart
docker compose restart redis
```

### Qdrant connection fails

**Problem**: Qdrant not running

**Solution**:
```bash
# Check Qdrant
docker ps | grep qdrant

# Test connection
curl http://localhost:6333/healthz

# Restart
docker compose restart qdrant
```

### "Permission denied" on database

**Problem**: Database user/database doesn't exist

**Solution**:
```bash
# Enter PostgreSQL container
docker exec -it drex-postgres psql -U postgres

# Inside PostgreSQL:
CREATE DATABASE drex;
GRANT ALL PRIVILEGES ON DATABASE drex TO postgres;
\q
```

---

## Advanced Setup

### Using Contextra's Docker Compose

If you prefer to run everything via Docker:

```bash
cd Contextra

# Start all services including Gateway and Worker
docker compose -f deployments/docker/docker-compose.yml up -d

# Note: This may conflict with Drex's docker-compose on port 5432/6379
# Either:
#   1. Use Drex's docker-compose for infrastructure, Contextra for Gateway only
#   2. Or modify one compose file to use different ports
```

### Production Deployment

For production, consider:

1. **Database**: Use managed PostgreSQL (AWS RDS, Google Cloud SQL, etc.)
2. **Redis**: Use managed Redis (AWS ElastiCache, Redis Cloud, etc.)
3. **Qdrant**: Use managed Qdrant (Qdrant Cloud) or deploy Qdrant cluster
4. **TLS**: Enable HTTPS for Gateway
5. **Authentication**: Set `CONTEXTRA_API_KEY` for API security
6. **Worker**: Run separate Worker service for background jobs

---

## Memory Testing

After setup, verify memory works:

```bash
# Store a memory
curl -X POST http://localhost:3000/memory \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Drex is a personal AI operating system",
    "kind": "Fact",
    "metadata": {}
  }'

# Search for memories
curl -X POST http://localhost:3000/memory/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "artificial intelligence"
  }'
```

---

## Drex Integration Details

### Memory Types Mapping

| Drex Kind | Contextra Backend | Description |
|-----------|-------------------|-------------|
| `Working` | Redis | Session-scoped temporary memory |
| `Episodic` | PostgreSQL | Conversation history |
| `Semantic` | Qdrant | Knowledge facts with vector search |
| `Preference` | Qdrant | User preferences |
| `Procedural` | Qdrant | Skills and capabilities |

### Security

- Contextra data is isolated per user (when user_id is provided)
- Drex's `MemoryPolicy` enforces sensitivity and confidence rules
- API keys can be used for authentication if needed

### Health Check

Drex health check includes Contextra:

```bash
cargo run --bin drex -- health

# Expected output includes:
# Memory: ✓ Healthy
```

---

## Next Steps

Once Contextra is running:

1. Start Ollama: `ollama serve`
2. Build Drex: `cargo build --release`
3. Run Drex: `cargo run --bin drex -- ask "Hello Drex"`

See the main README.md for complete Drex setup.
