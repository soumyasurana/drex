# Retrieval

## Overview

The Retrieval library is responsible for locating, ranking, and preparing information that may be relevant to a request.

Retrieval is a core component of Contextra's Context Engine, but it is not the final step.

Its responsibility is to maximize recall while preserving relevance.

The output of retrieval is consumed by the Context Engine, which performs additional ranking, compression, and optimization before producing the final context for an LLM.

---

# Philosophy

Traditional RAG systems perform retrieval once and immediately pass the results to a language model.

Contextra separates these concerns.

Retrieval answers the question:

> "What information might be useful?"

The Context Engine answers:

> "What information should actually be given to the model?"

This distinction enables better reasoning, improved token efficiency, and higher quality responses.

---

# Retrieval Pipeline

```text
User Query

      │

      ▼

Query Analysis

      │

      ▼

Query Expansion

      │

      ▼

Metadata Filtering

      │

      ▼

Candidate Retrieval

      │

      ▼

Hybrid Search

      │

      ▼

Reranking

      │

      ▼

Deduplication

      │

      ▼

Context Engine
```

Each stage is independent and replaceable.

---

# Query Analysis

The first stage analyzes the incoming request.

Responsibilities include:

- Intent detection
- Entity extraction
- Query normalization
- Language detection
- Collection selection

The objective is to understand *what* should be searched before performing retrieval.

---

# Query Expansion

Some queries benefit from expansion.

Example:

```text
Query

↓

"Rust ownership"

↓

Expanded Query

↓

Rust ownership
Borrow checker
Lifetimes
Ownership rules
```

Expansion increases recall without changing user intent.

---

# Metadata Filtering

Metadata reduces the search space before expensive retrieval operations.

Examples include:

- Collection
- User
- Organization
- Language
- File type
- Access permissions
- Tags
- Creation date

Metadata filtering occurs before vector search whenever possible.

---

# Candidate Retrieval

Retrieval generates an initial candidate set.

Possible retrieval strategies include:

- Vector similarity
- Keyword search
- Hybrid search
- Graph traversal

The objective is high recall.

Precision is improved later through reranking.

---

# Semantic Retrieval

Semantic retrieval compares vector embeddings.

Advantages:

- Meaning-aware
- Language independent
- Handles paraphrases
- Finds conceptually similar content

Semantic retrieval forms the foundation of modern retrieval systems.

---

# Hybrid Retrieval

Hybrid retrieval combines multiple retrieval strategies.

Example:

```text
Vector Search

+

Keyword Search

+

Metadata Search

↓

Merged Candidates
```

Hybrid retrieval generally improves robustness over vector search alone.

---

# Reranking

The initial candidate set is reordered.

Ranking signals may include:

- Semantic similarity
- Keyword overlap
- Metadata relevance
- Document quality
- Recency
- Source confidence
- User personalization

Reranking improves precision without sacrificing recall.

---

# Deduplication

Multiple retrieved chunks may contain the same information.

The retrieval library removes:

- Exact duplicates
- Near duplicates
- Overlapping chunks
- Repeated metadata

This reduces unnecessary context.

---

# Retrieval Strategies

Contextra supports multiple retrieval strategies.

## Semantic

Vector similarity.

---

## Keyword

Traditional lexical retrieval.

---

## Hybrid

Vector + keyword.

---

## Metadata

Filter-based retrieval.

---

## Hierarchical

Retrieve document first.

Retrieve chunks second.

---

## Graph Retrieval

Traverse relationships between documents.

---

## Context-Aware Retrieval

Use conversation history and memory to influence retrieval.

---

# Retrieval Pipeline Design

The retrieval library is modular.

Example:

```text
Query Analyzer

↓

Retriever

↓

Ranker

↓

Reranker

↓

Deduplicator

↓

Result
```

Each stage may be replaced independently.

---

# Interfaces

The retrieval library communicates through abstract interfaces.

Examples include:

```text
Retriever

HybridRetriever

Reranker

QueryExpander

ResultRanker
```

Concrete implementations remain hidden behind these interfaces.

---

# Observability

Every retrieval stage emits telemetry.

Metrics include:

- Query latency
- Recall
- Candidate count
- Rerank latency
- Cache hit ratio

Tracing spans measure each stage independently.

---

# Design Principles

## High Recall

Missing relevant information is generally worse than retrieving slightly more information.

---

## Precision Through Ranking

Initial retrieval maximizes recall.

Later stages improve precision.

---

## Provider Independent

Retrieval never depends directly on a specific vector database.

---

## Modular

Every stage may be replaced independently.

---

## Observable

Retrieval quality should be measurable.

---

# Future Capabilities

The retrieval architecture supports future enhancements including:

- Multi-vector retrieval
- Agent-assisted retrieval
- Knowledge graph traversal
- Multi-modal retrieval
- Adaptive retrieval
- Personalized ranking
- Cross-collection retrieval
- Federated retrieval
- Temporal retrieval
- Context-aware retrieval

These capabilities can be introduced without redesigning the pipeline.

---

# Relationship to the Context Engine

Retrieval is responsible for finding information.

The Context Engine is responsible for deciding how that information should be used.

The separation between retrieval and context engineering is one of the defining architectural principles of Contextra.

Rather than treating retrieval as the final step before inference, Contextra treats it as one stage in a larger context optimization pipeline.