# Contextra Technical Reference

**What it is:** A production-grade context engineering platform for AI applications—built in Rust as a modular workspace. Think of it as "RAG infrastructure" that manages the complete lifecycle of context: ingestion → retrieval → memory → orchestration → execution.

**Not a standalone service:** It is a Rust workspace with reusable libraries (`libs/`) and deployable services (`services/`). You can use the libraries directly (CLI local mode) or call the Gateway REST API.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Services (deployable)                  │
│   ┌─────────┐   ┌─────────┐   ┌─────────────────────────┐  │
│   │ Gateway │   │ Worker  │   │ CLI (REST or local mode)│  │
│   │ (HTTP)  │   │ (queue) │   │                         │  │
│   └────┬────┘   └────┬────┘   └─────────────────────────┘  │
└────────┼─────────────┼─────────────────────────────────────┘
         │             │
┌────────▼─────────────▼─────────────────────────────────────┐
│                    Domain Libraries                          │
│  orchestration → context → memory + retrieval                │
│                         → embeddings → providers             │
│                         → ingestion                          │
│                         → evaluation                         │
│                         → prompts                            │
│                         → storage (postgres/redis/qdrant)    │
└──────────────────────────────────────────────────────────────┘
```

---

## Public API Surface

### Gateway REST API (Port 3000)

Base path: `/api/v1`

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/documents` | GET, POST | List all documents; create new document (ingestion) |
| `/documents/:id` | GET | Get specific document |
| `/collections` | GET, POST | List all collections; create collection |
| `/collections/:id` | GET | Get specific collection |
| `/conversations` | GET, POST | List conversations; create conversation |
| `/conversations/:id/messages` | GET, POST | List messages; execute chat |
| `/conversations/:id/messages/stream` | POST | Stream chat response (SSE) |
| `/docs` | GET | Swagger UI (OpenAPI documentation) |

**Request/Response types:** All JSON. See `gateway/src/lib.rs` for schema definitions.

**Key structs:**
- `CreateDocumentRequest { source_path: String }`
- `CreateCollectionRequest { name: String, metadata: Metadata }`
- `CreateConversationRequest { title: Option<String>, metadata: Metadata }`
- `ChatExecutionRequest { message: String }`
- `ChatExecutionResponse { id, model, message, finish_reason }`

### CLI (`contextra` binary)

Two modes:
1. **Gateway mode** (default): Calls Gateway REST API; requires running gateway service
2. **Local mode** (`--local`): Uses libraries directly with mock providers; no external services or API keys required

```bash
# Gateway mode (requires running gateway)
contextra ingest ./docs/file.md
contextra chat "What is Context Engineering?"
contextra collections list

# Local mode (uses internal mocks, no external dependencies)
contextra --local ingest ./docs/file.md
contextra --local chat "What is Context Engineering?"
contextra --local eval run --dataset ./tests/fixtures/benchmark.json --k 5
```

**Local mode limitations:**
- Uses `CliLLMProvider` (returns canned responses)
- Uses `CliEmbeddingProvider` (returns dummy embeddings)
- Uses `InMemoryVectorStore` (data lost on restart)
- Does **not** support streaming (`chat_stream` returns error)
- Cannot use external LLMs or real embeddings

---

## Data Model

### Core Types (`libs/types`)

```rust
// ID types (UUID v7 internally)
DocumentId(Uuid), CollectionId(Uuid), ConversationId(Uuid), UserId(Uuid)

// Main entities
struct Document {
    pub id: DocumentId,
    pub collection_id: CollectionId,
    pub content: String,
    pub metadata: Metadata,  // HashMap<String, serde_json::Value>
}

struct Chunk {
    pub id: Uuid,
    pub document_id: DocumentId,
    pub content: String,
    pub metadata: Metadata,
}

struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub metadata: Metadata,
}

struct Message {
    pub id: Uuid,
    pub conversation_id: ConversationId,
    pub role: Role,  // System | User | Assistant | Tool
    pub content: String,
    pub metadata: Metadata,
}

struct ConversationSession { ... }
struct LongTermMemory { ... }
```

### Storage Backends

- **PostgreSQL**: Documents, collections, conversations, messages, API keys
- **Redis**: Session cache, temporary state, embedding cache
- **Qdrant (or InMemoryVectorStore)**: Vector embeddings for semantic search

---

## Dependencies & Runtime Requirements

### External Services

| Service | Port | Purpose |
|---------|------|---------|
| PostgreSQL | 5432 | Relational data persistence |
| Redis | 6379 | Caching, session storage |
| Qdrant | 6333/6334 | Vector storage & similarity search |

### Environment Variables

**Required for Gateway/Worker:**
```bash
DATABASE_URL=postgres://user:pass@host:5432/contextra
REDIS_URL=redis://host:6379
QDRANT_URL=http://host:6333
```

**Required for chat (when using real LLM providers):**
```bash
# At least one provider API key needed for real chat functionality
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
GEMINI_API_KEY=...
```

**Optional:**
```bash
# Config overrides (see libs/settings)
CONTEXTRA_ENV=development|production
CONTEXTRA__SERVER__PORT=3000
CONTEXTRA__DATABASE__MAX_CONNECTIONS=10
```

### Cargo.toml Workspace

- Rust 1.90+ required
- 17 libraries + 3 services + e2e tests
- Key external deps: `tokio`, `axum`, `sqlx`, `redis`, `qdrant-client`, `reqwest`

---

## Build, Run, Test

### Build
```bash
cargo build
cargo build --release
```

### Run Locally (development)
```bash
# Start infrastructure
docker compose -f deployments/docker/docker-compose.yml up -d postgres redis qdrant

# Run gateway
DATABASE_URL=postgres://postgres:postgrespassword@localhost:5432/contextra \
REDIS_URL=redis://localhost:6379 \
QDRANT_URL=http://localhost:6333 \
cargo run -p gateway

# Install and run CLI
cargo install --path services/cli
contextra --local ingest ./docs/file.md
```

### Test
```bash
cargo test                    # All tests
cargo test -p gateway        # Specific package
cargo test -p e2e-tests      # End-to-end tests
cargo clippy -- -D warnings   # Lint
cargo fmt --check             # Format check
```

---

## Key Library APIs

### Retrieval (`libs/retrieval`)

```rust
// Main interface
#[async_trait]
pub trait Retriever {
    async fn retrieve(&self, request: RetrievalRequest) -> Result<Vec<RetrievedDocument>, ContextraError>;
}

// Usage
let retriever = VectorRetriever::new(vector_store, embedding_provider);
let request = RetrievalRequest::hybrid("query", "collection", 10);
let docs = retriever.retrieve(request).await?;
```

### Context Engine (`libs/context`)

```rust
let engine = ContextEngine::new(retriever, memory_store, conversation_store);
let request = ContextRequest::new("query", user_id, conversation_id, "docs");
let package = engine.assemble(request).await?;  // Returns ContextPackage with chat_request
let response = provider.chat(package.chat_request).await?;
```

### Ingestion (`libs/ingestion`)

```rust
let pipeline = IngestionPipeline::new(
    parser,      // PlainTextParser | MarkdownParser | PdfParser | HtmlParser
    chunker,     // FixedSizeChunker | StructureAwareChunker
    embedding_provider,
    vector_store,
    "collection_name",
    collection_id,
);
let result = pipeline.ingest_path("./doc.md").await?;  // Returns IngestionResult
```

### Embeddings (`libs/embeddings`)

```rust
// Providers
OpenAIEmbeddingProvider::new(api_key, "text-embedding-3-small", 1536)
OllamaEmbeddingProvider::new("nomic-embed-text", 768)

// With caching
CachedEmbeddingProvider::new(provider, EmbeddingCache::new(redis_cache))
```

### LLM Providers (`libs/providers`)

```rust
let factory = ProviderFactory::new(settings);
let provider = factory.create_configured_llm_provider()?;  // OpenAI, Anthropic, or Gemini

let response = provider.chat(ChatRequest::new("gpt-4.1-mini", vec![
    ChatMessage::system("You are helpful"),
    ChatMessage::user("Hello"),
])).await?;
```

---

## Unusual / Easy to Misuse

### 1. Configuration Loading Order
Settings are loaded from (lowest to highest priority):
1. `configs/default.toml`
2. `configs/{CONTEXTRA_ENV}.toml`
3. `configs/local.toml` (gitignored)
4. Environment variables with `CONTEXTRA__` prefix (double underscore = nested)

Also supports legacy env vars: `DATABASE_URL`, `REDIS_URL`, `QDRANT_URL` mapped automatically.

### 2. CLI Local Mode Uses Mocks
Local mode (`--local`) uses internal mock implementations:
- `CliLLMProvider` — returns canned responses, does not call external APIs
- `CliEmbeddingProvider` — returns dummy embeddings (`[0.1; 1536]`)
- `InMemoryVectorStore` — data lost on restart

**No API keys required**, but responses are not from real LLMs. Use gateway mode with `OPENAI_API_KEY` (or other provider keys) for actual LLM functionality.

### 3. Vector Store Collection Creation
Collections in Qdrant are created lazily on first `upsert_vectors`. Call `create_collection()` explicitly if you need specific vector dimensions or want to ensure it exists before operations.

### 4. InMemoryVectorStore vs QdrantVectorStore
- `InMemoryVectorStore`: For tests/development only; data lost on restart
- `QdrantVectorStore`: Production persistence; requires running Qdrant

### 5. Authentication
- Gateway accepts `Authorization: Bearer <token>` OR `X-API-Key: <token>` header
- Bearer tokens validated against database; API keys looked up in `api_keys` table
- CLI local mode bypasses auth entirely (mock providers have no auth)

### 6. Token Budgets vs Context Limits
- `context_limit`: Max retrieved documents/chunks to include
- `memory_limit`: Max long-term memory items
- `token_budget`: Approximate token limit for final prompt (via truncation)
These work together but are separate concerns.

### 7. Worker Queue System
Background jobs (ingestion, evaluation) are processed by the `worker` service using Redis as queue. Gateway enqueues; worker dequeues and processes. Without worker, document creation via Gateway returns immediately but embedding never happens.

### 8. Migration Location
SQLx migrations are in `./migrations` relative to workspace root, embedded in binary via `sqlx::migrate!`.

### 9. Gateway Graceful Degradation
Gateway starts without LLM provider configured (uses `NoopLLMProvider`), but the `/conversations/:id/messages` endpoint returns 503 if no real provider is configured. Document and collection APIs remain functional.

---

## Integration Points

**To use Contextra as a library in another Rust project:**
```toml
[dependencies]
gateway = { path = "../contextra/services/gateway" }
storage = { path = "../contextra/libs/storage" }
context = { path = "../contextra/libs/context" }
```

**To call Contextra from another language:**
Use the Gateway REST API (`http://localhost:3000/api/v1`). OpenAPI docs at `/docs`.

**To extend Contextra:**
- Add new provider: Implement `LLMProvider` trait in `libs/providers`
- Add new chunker: Implement `Chunker` trait in `libs/ingestion`
- Add new retriever: Implement `Retriever` trait in `libs/retrieval`

---

## Testing Strategy

- **Unit tests**: In each crate's `src/tests/` or inline `#[cfg(test)]`
- **Integration**: `tests/e2e` spawns in-process Gateway + CLI roundtrips
- **Mock implementations**: Most libraries provide mock implementations for testing

Run e2e tests: `cargo test -p e2e-tests`

---

Last updated: 2026-09-05
