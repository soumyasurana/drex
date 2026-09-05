import {
  DocumentResource,
  CollectionResource,
  ChatMessage,
  ConversationResource,
  RetrievedChunk,
  PromptTemplate,
  EvalBenchmark,
  ActivityItem,
  SystemStatusItem,
  SystemSettings,
} from '@/types';

export const INITIAL_SETTINGS: SystemSettings = {
  llm_provider: 'openai',
  llm_model: 'gpt-4o',
  embedding_provider: 'openai',
  embedding_model: 'text-embedding-3-small',
  chunk_size: 512,
  chunk_overlap: 64,
  retrieval_k: 5,
  temperature: 0.2,
  top_p: 0.9,
  max_tokens: 2048,
  enable_reranker: true,
  gateway_url: process.env.NEXT_PUBLIC_GATEWAY_URL || 'https://contextra.blocklogsecurity.com',
  api_key: 'ctx_live_9f830a7b12e34d5e9a8f',
};

export const MOCK_METRICS = {
  total_documents: 142,
  total_chunks: 18450,
  total_collections: 8,
  total_conversations: 1240,
  total_prompts: 24,
  total_embeddings: 18450,
  total_requests: 342910,
  avg_latency_ms: 142,
};

export const MOCK_REQUESTS_OVER_TIME = [
  { time: '00:00', requests: 1200, latency: 135, tokens: 45000 },
  { time: '03:00', requests: 850, latency: 128, tokens: 32000 },
  { time: '06:00', requests: 1950, latency: 140, tokens: 68000 },
  { time: '09:00', requests: 4300, latency: 165, tokens: 142000 },
  { time: '12:00', requests: 6100, latency: 152, tokens: 210000 },
  { time: '15:00', requests: 5800, latency: 148, tokens: 198000 },
  { time: '18:00', requests: 3900, latency: 138, tokens: 125000 },
  { time: '21:00', requests: 2400, latency: 130, tokens: 82000 },
];

export const MOCK_PROVIDER_USAGE = [
  { name: 'OpenAI GPT-4o', value: 55, color: '#6366f1' },
  { name: 'Anthropic Claude 3.5', value: 30, color: '#a855f7' },
  { name: 'Google Gemini 1.5', value: 10, color: '#38bdf8' },
  { name: 'Ollama Llama 3 (Local)', value: 5, color: '#10b981' },
];

export const MOCK_LATENCY_BREAKDOWN = [
  { stage: 'Query Embedding', p50: 18, p95: 35, p99: 52 },
  { stage: 'Vector Search (Qdrant)', p50: 24, p95: 45, p99: 78 },
  { stage: 'Keyword Search (BM25)', p50: 12, p95: 22, p99: 38 },
  { stage: 'Hybrid RRF Merge', p50: 8, p95: 14, p99: 25 },
  { stage: 'Reranker (Cohere)', p50: 45, p95: 92, p99: 135 },
  { stage: 'Context Assembly', p50: 5, p95: 9, p99: 14 },
];

export const MOCK_DOCUMENTS: DocumentResource[] = [
  {
    id: 'doc_arch_001',
    name: 'system_architecture_spec.md',
    collection_id: 'col_core_docs',
    collection_name: 'Core System Docs',
    status: 'Ingested',
    chunks_count: 84,
    file_size: '1.2 MB',
    uploaded_at: '2026-07-26 14:20',
    metadata: { author: 'Architect Team', version: '2.4.0', format: 'markdown' },
    raw_content: `# Contextra Architecture Specification\n\nContextra is a modular Rust platform designed for enterprise AI context engineering.\n\n## Core Principles\n- Business logic belongs in libraries.\n- Transport belongs in services.\n- High throughput vector retrieval using Qdrant & Redis.`,
  },
  {
    id: 'doc_api_002',
    name: 'gateway_api_reference.openapi.json',
    collection_id: 'col_api_specs',
    collection_name: 'API Specifications',
    status: 'Ingested',
    chunks_count: 156,
    file_size: '3.4 MB',
    uploaded_at: '2026-07-26 11:05',
    metadata: { version: 'v1.0.0', type: 'openapi' },
    raw_content: `{\n  "openapi": "3.0.3",\n  "info": { "title": "Contextra Gateway API", "version": "1.0.0" }\n}`,
  },
  {
    id: 'doc_mem_003',
    name: 'memory_scoring_algorithm.pdf',
    collection_id: 'col_research',
    collection_name: 'AI Research Papers',
    status: 'Ingested',
    chunks_count: 230,
    file_size: '5.8 MB',
    uploaded_at: '2026-07-25 18:42',
    metadata: { topic: 'Importance Scoring & Summarization', pages: 14 },
    raw_content: `Importance scoring calculates memory retention priority using recency decay, frequency weights, and semantic relevance to the active user task graph.`,
  },
  {
    id: 'doc_eval_004',
    name: 'retrieval_benchmark_2026.csv',
    collection_id: 'col_benchmarks',
    collection_name: 'Eval Datasets',
    status: 'Ingested',
    chunks_count: 412,
    file_size: '12.1 MB',
    uploaded_at: '2026-07-24 09:15',
    metadata: { samples: 5000, target: 'Recall@5' },
  },
  {
    id: 'doc_err_005',
    name: 'legacy_unsupported_binary.dat',
    collection_id: 'col_raw_data',
    collection_name: 'Raw Unstructured Data',
    status: 'Failed',
    chunks_count: 0,
    file_size: '800 KB',
    uploaded_at: '2026-07-26 16:01',
    metadata: { error: 'ParserError: Unsupported MIME type application/octet-stream' },
  },
  {
    id: 'doc_proc_006',
    name: 'qdrant_indexing_guide.md',
    collection_id: 'col_core_docs',
    collection_name: 'Core System Docs',
    status: 'Processing',
    chunks_count: 42,
    file_size: '450 KB',
    uploaded_at: 'Just now',
    metadata: { index_type: 'HNSW', distance: 'Cosine' },
  },
];

export const MOCK_COLLECTIONS: CollectionResource[] = [
  {
    id: 'col_core_docs',
    name: 'Core System Docs',
    description: 'System specifications, architecture documents, and library interface contracts.',
    documents_count: 45,
    chunks_count: 4820,
    created_at: '2026-06-10',
  },
  {
    id: 'col_api_specs',
    name: 'API Specifications',
    description: 'REST API, gRPC schemas, OpenAPI contracts, and SDK references.',
    documents_count: 28,
    chunks_count: 3100,
    created_at: '2026-06-15',
  },
  {
    id: 'col_research',
    name: 'AI Research Papers',
    description: 'Academic papers on hybrid retrieval, RRF reranking, and memory decay algorithms.',
    documents_count: 32,
    chunks_count: 6400,
    created_at: '2026-07-01',
  },
  {
    id: 'col_benchmarks',
    name: 'Eval Datasets',
    description: 'Ground truth datasets for Recall@K, Precision@K, and MRR evaluation runs.',
    documents_count: 14,
    chunks_count: 2800,
    created_at: '2026-07-12',
  },
  {
    id: 'col_raw_data',
    name: 'Raw Unstructured Data',
    description: 'Unprocessed text dumps, user uploads, and web crawl dumps.',
    documents_count: 23,
    chunks_count: 1330,
    created_at: '2026-07-20',
  },
];

export const MOCK_CONVERSATIONS: ConversationResource[] = [
  {
    id: 'conv_8f3a1d90',
    title: 'Explaining Hybrid Retrieval & RRF Merging',
    message_count: 8,
    summary: 'Discussion on how Reciprocal Rank Fusion combines BM25 keyword rankings with Qdrant cosine vector scores to produce zero-shot robust context rank.',
    memory_size_kb: 24,
    token_count: 3840,
    last_updated: '10 mins ago',
  },
  {
    id: 'conv_72b4c10e',
    title: 'Rust Async Worker Queue Graceful Shutdown',
    message_count: 14,
    summary: 'Debugging Tokio watch broadcast signals and Redis BRPOPLPUSH job recovery during worker SIGTERM event.',
    memory_size_kb: 48,
    token_count: 7210,
    last_updated: '2 hours ago',
  },
  {
    id: 'conv_11a90e3f',
    title: 'Prompt Token Budgeting Strategy',
    message_count: 5,
    summary: 'Calculating static system prompt tokens vs dynamic retrieved memory chunks to prevent context overflow.',
    memory_size_kb: 16,
    token_count: 2400,
    last_updated: '1 day ago',
  },
  {
    id: 'conv_99c812d4',
    title: 'Evaluating Recall@5 across OpenAI vs Ollama Embeddings',
    message_count: 12,
    summary: 'Benchmark comparison showing OpenAI text-embedding-3-small achieving 0.94 Recall@5 vs Nomic Embed at 0.88.',
    memory_size_kb: 38,
    token_count: 5900,
    last_updated: '3 days ago',
  },
];

export const MOCK_CHAT_MESSAGES: ChatMessage[] = [
  {
    id: 'msg_001',
    conversation_id: 'conv_8f3a1d90',
    role: 'user',
    content: 'How does Contextra combine vector search results with keyword search, and what reranking algorithm is used?',
    timestamp: '14:32:05',
  },
  {
    id: 'msg_002',
    conversation_id: 'conv_8f3a1d90',
    role: 'assistant',
    content: `Contextra executes a **Hybrid Retrieval Pipeline** engineered for maximum precision and recall:

1. **Dual Retrieval**:
   - **Vector Search**: Performs cosine similarity search over vector embeddings in Qdrant (using HNSW index).
   - **Keyword Search**: Performs BM25 full-text match over chunk text in PostgreSQL / Redis payload index.

2. **Reciprocal Rank Fusion (RRF)**:
   Scores from both retrievers are merged using the RRF formula:
   $$\\text{RRF\\_Score}(d) = \\sum_{m \\in M} \\frac{1}{k + r_m(d)}$$
   where $k=60$ and $r_m(d)$ is the rank of document $d$ in system $m$.

3. **Reranking & Context Assembly**:
   The top 25 merged candidates are passed to a cross-encoder Reranker model, returning the final Top K chunks bounded by your token budget.

\`\`\`rust
// libs/retrieval/src/hybrid.rs
pub async fn fuse_rankings(
    vector_results: &[ScoredChunk],
    keyword_results: &[ScoredChunk],
    k: f64,
) -> Vec<ScoredChunk> {
    // RRF implementation in Rust
}
\`\`\``,
    timestamp: '14:32:06',
    latency_ms: 138,
    tokens_used: 485,
    citations: [
      {
        chunk_id: 'chk_99812',
        document_id: 'doc_arch_001',
        document_name: 'system_architecture_spec.md',
        snippet: 'Hybrid retriever combines Qdrant cosine vector search with Postgres BM25 keyword matching via RRF constant k=60.',
        score: 0.962,
        page_number: 4,
      },
      {
        chunk_id: 'chk_99815',
        document_id: 'doc_mem_003',
        document_name: 'memory_scoring_algorithm.pdf',
        snippet: 'Cross-encoder reranking prioritizes chunks matching active conversation intent window before final prompt token assembly.',
        score: 0.894,
        page_number: 8,
      },
    ],
  },
];

export const MOCK_RETRIEVED_CHUNKS: RetrievedChunk[] = [
  {
    id: 'chk_99812',
    document_id: 'doc_arch_001',
    document_name: 'system_architecture_spec.md',
    collection_name: 'Core System Docs',
    score: 0.962,
    content: 'Contextra retrieval pipeline: 1) Query normalization & entity extraction. 2) Concurrent Qdrant dense vector search & Postgres BM25 keyword search. 3) Reciprocal Rank Fusion (RRF k=60). 4) Cross-encoder neural reranking.',
    chunk_index: 4,
    metadata: { section: 'Retrieval Engine', type: 'technical_spec' },
  },
  {
    id: 'chk_99815',
    document_id: 'doc_mem_003',
    document_name: 'memory_scoring_algorithm.pdf',
    collection_name: 'AI Research Papers',
    score: 0.894,
    content: 'Memory importance scoring uses exponential recency decay coupled with frequency-weighted keyword overlap to maintain a dynamic 4KB conversation summary buffer.',
    chunk_index: 12,
    metadata: { section: 'Summarization', author: 'AI Lab' },
  },
  {
    id: 'chk_99820',
    document_id: 'doc_api_002',
    document_name: 'gateway_api_reference.openapi.json',
    collection_name: 'API Specifications',
    score: 0.841,
    content: 'POST /api/v1/conversations/{id}/messages/stream returns a Server-Sent Events (SSE) stream delivering token deltas, citation references, and final execution metrics.',
    chunk_index: 28,
    metadata: { endpoint: '/messages/stream', protocol: 'SSE' },
  },
  {
    id: 'chk_99831',
    document_id: 'doc_arch_001',
    document_name: 'system_architecture_spec.md',
    collection_name: 'Core System Docs',
    score: 0.788,
    content: 'Context Assembler enforces a hard token limit by packing system prompt, long-term memory summary, and retrieved chunks into an optimized prompt tree.',
    chunk_index: 9,
    metadata: { section: 'Context Assembly' },
  },
];

export const MOCK_PROMPTS: PromptTemplate[] = [
  {
    id: 'prompt_rag_qa_v2',
    name: 'rag_contextual_qa',
    description: 'Production RAG prompt with strict citation constraints and zero-hallucination guardrails.',
    version: 'v2.1.0',
    variables: ['user_query', 'retrieved_context', 'conversation_summary'],
    template_text: `You are Contextra AI, a precise domain assistant.

Use ONLY the provided context below to answer the user request. If the answer cannot be directly derived from the context, state "I cannot verify this with the provided context."

--- RETRIEVED CONTEXT ---
{{retrieved_context}}

--- CONVERSATION SUMMARY ---
{{conversation_summary}}

--- USER QUESTION ---
{{user_query}}

Respond clearly using markdown. Cite source document IDs where appropriate.`,
    created_at: '2026-07-20',
    updated_at: '2026-07-26',
  },
  {
    id: 'prompt_mem_summarize',
    name: 'memory_rolling_summary',
    description: 'Compresses long conversation logs into a dense structured summary.',
    version: 'v1.4.0',
    variables: ['previous_summary', 'new_messages'],
    template_text: `Summarize the key facts, user preferences, and unresolved questions from the new conversation messages, merging them seamlessly into the previous summary.

PREVIOUS SUMMARY:
{{previous_summary}}

NEW MESSAGES:
{{new_messages}}

Output concise bullet points preserving exact code symbols, IDs, and user constraints.`,
    created_at: '2026-07-15',
    updated_at: '2026-07-24',
  },
  {
    id: 'prompt_eval_judge',
    name: 'llm_as_a_judge_eval',
    description: 'Evaluates correctness and faithfulness of generated answers against ground truth.',
    version: 'v1.0.0',
    variables: ['question', 'ground_truth', 'generated_answer'],
    template_text: `Rate the generated answer on a scale from 1.0 to 5.0 for Faithfulness and Correctness.

QUESTION: {{question}}
GROUND TRUTH: {{ground_truth}}
GENERATED ANSWER: {{generated_answer}}

Output JSON: { "faithfulness": 4.8, "correctness": 5.0, "reasoning": "..." }`,
    created_at: '2026-07-18',
    updated_at: '2026-07-18',
  },
];

export const MOCK_EVAL_BENCHMARKS: EvalBenchmark[] = [
  {
    id: 'eval_run_9910',
    name: 'RAG Retrieval Quality Suite v2.4',
    dataset_name: 'tech_docs_ground_truth_500.json',
    sample_count: 500,
    pass_rate: 96.4,
    recall_at_k: 0.942,
    precision_at_k: 0.891,
    mrr: 0.915,
    avg_latency_ms: 134,
    status: 'Completed',
    run_at: '2026-07-26 15:30',
  },
  {
    id: 'eval_run_9908',
    name: 'Finance QA Benchmark (Hybrid vs Dense)',
    dataset_name: 'finance_sec_filings_1000.json',
    sample_count: 1000,
    pass_rate: 92.1,
    recall_at_k: 0.895,
    precision_at_k: 0.842,
    mrr: 0.864,
    avg_latency_ms: 168,
    status: 'Completed',
    run_at: '2026-07-25 11:10',
  },
  {
    id: 'eval_run_9905',
    name: 'Ollama Llama 3 8B Local Embeddings Test',
    dataset_name: 'tech_docs_ground_truth_500.json',
    sample_count: 500,
    pass_rate: 88.0,
    recall_at_k: 0.851,
    precision_at_k: 0.798,
    mrr: 0.820,
    avg_latency_ms: 92,
    status: 'Completed',
    run_at: '2026-07-23 20:45',
  },
];

export const MOCK_ACTIVITIES: ActivityItem[] = [
  {
    id: 'act_01',
    type: 'ingestion',
    title: 'Document Ingested',
    description: 'system_architecture_spec.md parsed & chunked into 84 vectors',
    timestamp: '5 mins ago',
    status: 'success',
  },
  {
    id: 'act_02',
    type: 'chat',
    title: 'High-Volume Chat Session',
    description: 'Conversation conv_8f3a1d90 executed 8 messages via GPT-4o',
    timestamp: '12 mins ago',
    status: 'info',
  },
  {
    id: 'act_03',
    type: 'eval',
    title: 'Eval Run Completed',
    description: 'RAG Retrieval Quality Suite v2.4 finished with 96.4% pass rate',
    timestamp: '45 mins ago',
    status: 'success',
  },
  {
    id: 'act_04',
    type: 'ingestion',
    title: 'Ingestion Error',
    description: 'Failed to parse legacy_unsupported_binary.dat (Unsupported MIME type)',
    timestamp: '2 hours ago',
    status: 'error',
  },
  {
    id: 'act_05',
    type: 'prompt',
    title: 'Prompt Version Published',
    description: 'rag_contextual_qa updated to version v2.1.0',
    timestamp: '4 hours ago',
    status: 'info',
  },
];

export const MOCK_SYSTEM_STATUS: SystemStatusItem[] = [
  { name: 'Gateway REST API', service: 'Gateway', status: 'healthy', latency_ms: 4, uptime: '99.99%' },
  { name: 'PostgreSQL DB', service: 'Postgres', status: 'healthy', latency_ms: 8, uptime: '99.95%' },
  { name: 'Redis Queue & Cache', service: 'Redis', status: 'healthy', latency_ms: 2, uptime: '100.0%' },
  { name: 'Qdrant Vector Cluster', service: 'Qdrant', status: 'healthy', latency_ms: 14, uptime: '99.98%' },
  { name: 'Background Worker Pool', service: 'Worker', status: 'healthy', latency_ms: 11, uptime: '99.90%' },
];
