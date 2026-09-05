# Drex Project Overview

**Drex** is a private, local-first personal AI agent being built in Rust.

---

## Current State

### Contextra (Memory Layer)

The `contextra/` subdirectory contains an existing, production-grade context engineering platform.

**What Contextra does:**
- Document ingestion & chunking (PDF, HTML, Markdown, plain text)
- Semantic + hybrid + keyword retrieval with reranking
- Vector storage (Qdrant or in-memory) for embeddings
- Conversation memory (+ long-term memory with importance scoring)
- LLM provider abstraction (OpenAI, Anthropic, Gemini)
- REST API Gateway with auth, rate limiting, OpenAPI docs
- Background job processing (worker queue)
- CLI tool with local/offline execution modes

**Relationship to Drex:** Contextra exists as a standalone codebase. Drex will integrate with Contextra's capabilities (document storage, retrieval, memory, embeddings) rather than reimplementing them. Future Drex components will either call Contextra's REST API or link against its libraries.

**See:** [`contextra/CLAUDE.md`](./contextra/CLAUDE.md) for complete Contextra technical reference.

---

## Future Work

Drex will be developed as new Rust crates in a separate location from Contextra:

```
DREX/
├── contextra/              # Existing: Contextra codebase (separate)
├── crates/
│   ├── drex-core/          # New: agent loop, state machine
│   ├── drex-models/        # New: structured LLM outputs, schemas
│   ├── drex-tools/         # New: tool implementations
│   └── drex-cli/           # New: Drex CLI (may wrap contextra CLI)
```

### Planned Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Drex CLI                              │
│   - User-facing interface                                    │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                      Drex Core                               │
│   - Agent loop (plan → execute → observe → adapt)           │
│   - Tool orchestration                                       │
│   - State persistence                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                    Drex Tools                                │
│   - Filesystem operations                                    │
│   - Web search / browsing                                    │
│   - Code execution                                           │
│   - Shell command execution                                  │
└─────────────────────┬───────────────────────────────────────┘
                      │ calls into
┌─────────────────────▼───────────────────────────────────────┐
│                  Contextra (Memory)                        │
│   - Document storage & retrieval                           │
│   - Conversation memory                                     │
│   - Embeddings & vector search                              │
│   - LLM provider orchestration                              │
└─────────────────────────────────────────────────────────────┘
```

---

## Design Principles

1. **Local-first**: All data stays on your machine by default
2. **Modular**: Each crate has a single, well-defined responsibility
3. **Contextra-integrated**: Reuse Contextra's existing capabilities instead of reimplementing them
4. **Rust-native**: Leverage Rust's performance and safety for agent workloads
5. **Pluggable LLMs**: Support multiple providers via Contextra's provider abstraction

---

## Getting Started

1. **Understand Contextra:** Read [`contextra/CLAUDE.md`](./contextra/CLAUDE.md) first.

2. **Run Contextra locally:**
   ```bash
   cd contextra
   docker compose -f deployments/docker/docker-compose.yml up -d postgres redis qdrant
   cargo run -p gateway
   ```

3. **Future Drex crates** will be added under `crates/` in this repository.

---

Last updated: 2026-09-05
