import {
  DocumentResource,
  CollectionResource,
  ConversationResource,
  ChatMessage,
} from '@/types';

export const GATEWAY_URL = process.env.NEXT_PUBLIC_GATEWAY_URL || 'https://contextra.blocklogsecurity.com';

async function fetchWithTimeout(url: string, options: RequestInit = {}, timeoutMs = 5000) {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, { ...options, signal: controller.signal });
    clearTimeout(id);
    return res;
  } catch (err) {
    clearTimeout(id);
    throw err;
  }
}

function authHeaders(apiKey: string): Record<string, string> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (apiKey) {
    headers['x-api-key'] = apiKey;
  }
  return headers;
}

// ── Response mapping helpers ──────────────────────────────────────────
// The backend returns lean resource shapes; we map them to the richer
// frontend types, filling in defaults for fields that don't exist in
// the API response.

interface BackendDocumentResource {
  id: string;
  collection_id: string;
  content: string;
  metadata: Record<string, unknown>;
}

interface BackendCollectionResource {
  id: string;
  name: string;
  metadata: Record<string, unknown>;
}

interface BackendConversationResource {
  id: string;
  title: string | null;
  metadata: Record<string, unknown>;
}

interface BackendMessageResource {
  id: string;
  conversation_id: string;
  role: string;
  content: string;
  metadata: Record<string, unknown>;
}

interface BackendChatExecutionResponse {
  id: string;
  model: string;
  message: string;
  finish_reason: string | null;
}

interface PageResponse<T> {
  items: T[];
  next_cursor: string | null;
  has_more: boolean;
  total_count: number | null;
}

function mapDocument(doc: BackendDocumentResource): DocumentResource {
  const meta = doc.metadata || {};
  return {
    id: doc.id,
    name: (meta.name as string) || (meta.source_path as string) || doc.id,
    collection_id: doc.collection_id,
    collection_name: (meta.collection_name as string) || '',
    status: 'Ingested',
    chunks_count: (meta.chunks_count as number) || 0,
    file_size: (meta.file_size as string) || '—',
    uploaded_at: (meta.uploaded_at as string) || (meta.created_at as string) || '',
    metadata: meta as Record<string, string | number | boolean | null | undefined>,
    raw_content: doc.content,
  };
}

function mapCollection(col: BackendCollectionResource): CollectionResource {
  const meta = col.metadata || {};
  return {
    id: col.id,
    name: col.name,
    description: (meta.description as string) || '',
    documents_count: (meta.documents_count as number) || 0,
    chunks_count: (meta.chunks_count as number) || 0,
    created_at: (meta.created_at as string) || '',
  };
}

function mapConversation(conv: BackendConversationResource): ConversationResource {
  const meta = conv.metadata || {};
  return {
    id: conv.id,
    title: conv.title || 'Untitled Conversation',
    message_count: (meta.message_count as number) || 0,
    summary: (meta.summary as string) || '',
    memory_size_kb: (meta.memory_size_kb as number) || 0,
    token_count: (meta.token_count as number) || 0,
    last_updated: (meta.last_updated as string) || '',
  };
}

function mapMessage(msg: BackendMessageResource): ChatMessage {
  return {
    id: msg.id,
    conversation_id: msg.conversation_id,
    role: msg.role as 'user' | 'assistant' | 'system',
    content: msg.content,
    timestamp: (msg.metadata?.timestamp as string) || '',
  };
}

// ── Public API ────────────────────────────────────────────────────────

export const api = {
  // ─── Documents ───────────────────────────────────────────────────
  async getDocuments(apiKey: string): Promise<{ items: DocumentResource[]; totalCount: number }> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/documents`, {
        headers: authHeaders(apiKey),
      });
      if (!res.ok) throw new Error(`API ${res.status}`);
      const data: PageResponse<BackendDocumentResource> = await res.json();
      return {
        items: (data.items || []).map(mapDocument),
        totalCount: data.total_count ?? data.items?.length ?? 0,
      };
    } catch {
      return { items: [], totalCount: 0 };
    }
  },

  async createDocument(sourcePath: string, apiKey: string): Promise<DocumentResource | null> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/documents`, {
        method: 'POST',
        headers: authHeaders(apiKey),
        body: JSON.stringify({ source_path: sourcePath }),
      });
      if (!res.ok) throw new Error('Failed to create document');
      const data: BackendDocumentResource = await res.json();
      return mapDocument(data);
    } catch {
      return null;
    }
  },

  // ─── Collections ─────────────────────────────────────────────────
  async getCollections(apiKey: string): Promise<{ items: CollectionResource[]; totalCount: number }> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/collections`, {
        headers: authHeaders(apiKey),
      });
      if (!res.ok) throw new Error(`API ${res.status}`);
      const data: PageResponse<BackendCollectionResource> = await res.json();
      return {
        items: (data.items || []).map(mapCollection),
        totalCount: data.total_count ?? data.items?.length ?? 0,
      };
    } catch {
      return { items: [], totalCount: 0 };
    }
  },

  async createCollection(name: string, metadata: Record<string, unknown> = {}, apiKey: string): Promise<CollectionResource | null> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/collections`, {
        method: 'POST',
        headers: authHeaders(apiKey),
        body: JSON.stringify({ name, metadata }),
      });
      if (!res.ok) throw new Error('Failed to create collection');
      const data: BackendCollectionResource = await res.json();
      return mapCollection(data);
    } catch {
      return null;
    }
  },

  // ─── Conversations ───────────────────────────────────────────────
  async getConversations(apiKey: string): Promise<{ items: ConversationResource[]; totalCount: number }> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/conversations`, {
        headers: authHeaders(apiKey),
      });
      if (!res.ok) throw new Error(`API ${res.status}`);
      const data: PageResponse<BackendConversationResource> = await res.json();
      return {
        items: (data.items || []).map(mapConversation),
        totalCount: data.total_count ?? data.items?.length ?? 0,
      };
    } catch {
      return { items: [], totalCount: 0 };
    }
  },

  async createConversation(title: string | undefined, apiKey: string): Promise<ConversationResource | null> {
    try {
      const res = await fetchWithTimeout(`${GATEWAY_URL}/api/v1/conversations`, {
        method: 'POST',
        headers: authHeaders(apiKey),
        body: JSON.stringify({ title }),
      });
      if (!res.ok) throw new Error('Failed to create conversation');
      const data: BackendConversationResource = await res.json();
      return mapConversation(data);
    } catch {
      return null;
    }
  },

  // ─── Messages ────────────────────────────────────────────────────
  async getMessages(conversationId: string, apiKey: string): Promise<ChatMessage[]> {
    try {
      const res = await fetchWithTimeout(
        `${GATEWAY_URL}/api/v1/conversations/${conversationId}/messages`,
        { headers: authHeaders(apiKey) },
      );
      if (!res.ok) throw new Error(`API ${res.status}`);
      const data: PageResponse<BackendMessageResource> = await res.json();
      return (data.items || []).map(mapMessage);
    } catch {
      return [];
    }
  },

  async sendMessage(conversationId: string, message: string, apiKey: string): Promise<ChatMessage | null> {
    try {
      const res = await fetchWithTimeout(
        `${GATEWAY_URL}/api/v1/conversations/${conversationId}/messages`,
        {
          method: 'POST',
          headers: authHeaders(apiKey),
          body: JSON.stringify({ message }),
        },
      );
      if (!res.ok) throw new Error('Failed to send message');
      const data: BackendChatExecutionResponse = await res.json();
      return {
        id: data.id || `msg_${Date.now()}`,
        conversation_id: conversationId,
        role: 'assistant',
        content: data.message,
        timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      };
    } catch {
      return null;
    }
  },

  // ─── Health ──────────────────────────────────────────────────────
  async checkHealth(apiKey: string): Promise<boolean> {
    try {
      const res = await fetchWithTimeout(
        `${GATEWAY_URL}/api/v1/collections?limit=1`,
        { headers: authHeaders(apiKey) },
        2000,
      );
      return res.ok;
    } catch {
      return false;
    }
  },
};
