export type DocumentStatus = 'Ingested' | 'Processing' | 'Failed';

export interface Metadata {
  [key: string]: string | number | boolean | null | undefined;
}

export interface DocumentResource {
  id: string;
  name: string;
  collection_id: string;
  collection_name?: string;
  status: DocumentStatus;
  chunks_count: number;
  file_size: string;
  uploaded_at: string;
  metadata: Metadata;
  raw_content?: string;
}

export interface CollectionResource {
  id: string;
  name: string;
  description: string;
  documents_count: number;
  chunks_count: number;
  created_at: string;
  metadata?: Metadata;
}

export interface Citation {
  chunk_id: string;
  document_id: string;
  document_name: string;
  snippet: string;
  score: number;
  page_number?: number;
}

export interface ChatMessage {
  id: string;
  conversation_id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
  citations?: Citation[];
  latency_ms?: number;
  tokens_used?: number;
}

export interface ConversationResource {
  id: string;
  title: string;
  message_count: number;
  summary: string;
  memory_size_kb: number;
  token_count: number;
  last_updated: string;
  metadata?: Metadata;
}

export interface RetrievalStepInfo {
  id: string;
  name: string;
  description: string;
  latency_ms: number;
  input_count: number;
  output_count: number;
  status: 'completed' | 'processing' | 'idle';
  details: {
    label: string;
    value: string | number;
  }[];
}

export interface RetrievedChunk {
  id: string;
  document_id: string;
  document_name: string;
  collection_name: string;
  score: number;
  content: string;
  chunk_index: number;
  metadata: Record<string, string>;
}

export interface PromptTemplate {
  id: string;
  name: string;
  description: string;
  version: string;
  template_text: string;
  variables: string[];
  created_at: string;
  updated_at: string;
}

export interface PromptVersion {
  version: string;
  created_at: string;
  author: string;
  change_summary: string;
}

export interface EvalBenchmark {
  id: string;
  name: string;
  dataset_name: string;
  sample_count: number;
  pass_rate: number; // e.g. 94.8
  recall_at_k: number; // e.g. 0.92
  precision_at_k: number; // e.g. 0.88
  mrr: number; // e.g. 0.89
  avg_latency_ms: number;
  status: 'Completed' | 'Running' | 'Failed';
  run_at: string;
}

export interface SystemStatusItem {
  name: string;
  service: 'Postgres' | 'Redis' | 'Qdrant' | 'Worker' | 'Gateway';
  status: 'healthy' | 'degraded' | 'offline';
  latency_ms: number;
  uptime: string;
}

export interface ActivityItem {
  id: string;
  type: 'ingestion' | 'chat' | 'eval' | 'prompt' | 'collection';
  title: string;
  description: string;
  timestamp: string;
  status: 'success' | 'info' | 'warning' | 'error';
}

export interface SystemSettings {
  llm_provider: 'openai' | 'anthropic' | 'gemini' | 'ollama';
  llm_model: string;
  embedding_provider: 'openai' | 'ollama' | 'huggingface';
  embedding_model: string;
  chunk_size: number;
  chunk_overlap: number;
  retrieval_k: number;
  temperature: number;
  top_p: number;
  max_tokens: number;
  enable_reranker: boolean;
  gateway_url: string;
  api_key: string;
}
