import { create } from 'zustand';
import {
  DocumentResource,
  CollectionResource,
  ChatMessage,
  ConversationResource,
  RetrievedChunk,
  PromptTemplate,
  EvalBenchmark,
  SystemSettings,
} from '@/types';
import {
  INITIAL_SETTINGS,
  MOCK_PROMPTS,
  MOCK_EVAL_BENCHMARKS,
  MOCK_RETRIEVED_CHUNKS,
} from './mock-data';
import { api } from './api';

interface AppState {
  // System Settings
  settings: SystemSettings;
  updateSettings: (partial: Partial<SystemSettings>) => void;

  // Documents
  documents: DocumentResource[];
  documentsCount: number;
  documentsLoading: boolean;
  addDocument: (doc: DocumentResource) => void;
  deleteDocument: (id: string) => void;
  fetchDocuments: () => Promise<void>;

  // Collections
  collections: CollectionResource[];
  collectionsCount: number;
  collectionsLoading: boolean;
  addCollection: (col: CollectionResource) => void;
  deleteCollection: (id: string) => void;
  updateCollection: (id: string, name: string, description: string) => void;
  fetchCollections: () => Promise<void>;

  // Chat
  activeConversationId: string;
  conversations: ConversationResource[];
  conversationsCount: number;
  conversationsLoading: boolean;
  messages: ChatMessage[];
  messagesLoading: boolean;
  retrievedChunks: RetrievedChunk[];
  selectedChunk: RetrievedChunk | null;
  isStreaming: boolean;
  setActiveConversationId: (id: string) => void;
  setSelectedChunk: (chunk: RetrievedChunk | null) => void;
  addChatMessage: (msg: ChatMessage) => void;
  setStreaming: (streaming: boolean) => void;
  fetchConversations: () => Promise<void>;
  fetchMessages: (conversationId: string) => Promise<void>;

  // Prompts (no backend endpoint — keep mock for now)
  prompts: PromptTemplate[];
  activePromptId: string;
  setActivePromptId: (id: string) => void;
  updatePromptTemplate: (id: string, text: string) => void;
  addPromptTemplate: (prompt: PromptTemplate) => void;

  // Evaluations (no backend endpoint — keep mock for now)
  evals: EvalBenchmark[];
  addEvalRun: (evalRun: EvalBenchmark) => void;

  // Global UI
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
  apiConnected: boolean;
  setApiConnected: (connected: boolean) => void;

  // Aggregated dashboard fetch
  fetchDashboardData: () => Promise<void>;
  checkApiHealth: () => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  // Settings
  settings: INITIAL_SETTINGS,
  updateSettings: (partial) =>
    set((state) => ({
      settings: { ...state.settings, ...partial },
    })),

  // Documents — start empty, fetch from API
  documents: [],
  documentsCount: 0,
  documentsLoading: false,
  addDocument: (doc) =>
    set((state) => ({
      documents: [doc, ...state.documents],
      documentsCount: state.documentsCount + 1,
    })),
  deleteDocument: (id) =>
    set((state) => ({
      documents: state.documents.filter((d) => d.id !== id),
      documentsCount: Math.max(0, state.documentsCount - 1),
    })),
  fetchDocuments: async () => {
    set({ documentsLoading: true });
    const { settings } = get();
    const result = await api.getDocuments(settings.api_key);
    set({
      documents: result.items,
      documentsCount: result.totalCount,
      documentsLoading: false,
    });
  },

  // Collections — start empty, fetch from API
  collections: [],
  collectionsCount: 0,
  collectionsLoading: false,
  addCollection: (col) =>
    set((state) => ({
      collections: [col, ...state.collections],
      collectionsCount: state.collectionsCount + 1,
    })),
  deleteCollection: (id) =>
    set((state) => ({
      collections: state.collections.filter((c) => c.id !== id),
      collectionsCount: Math.max(0, state.collectionsCount - 1),
    })),
  updateCollection: (id, name, description) =>
    set((state) => ({
      collections: state.collections.map((c) =>
        c.id === id ? { ...c, name, description } : c
      ),
    })),
  fetchCollections: async () => {
    set({ collectionsLoading: true });
    const { settings } = get();
    const result = await api.getCollections(settings.api_key);
    set({
      collections: result.items,
      collectionsCount: result.totalCount,
      collectionsLoading: false,
    });
  },

  // Chat
  activeConversationId: '',
  conversations: [],
  conversationsCount: 0,
  conversationsLoading: false,
  messages: [],
  messagesLoading: false,
  retrievedChunks: MOCK_RETRIEVED_CHUNKS,
  selectedChunk: MOCK_RETRIEVED_CHUNKS[0] || null,
  isStreaming: false,
  setActiveConversationId: (id) => set({ activeConversationId: id }),
  setSelectedChunk: (chunk) => set({ selectedChunk: chunk }),
  addChatMessage: (msg) =>
    set((state) => ({
      messages: [...state.messages, msg],
    })),
  setStreaming: (streaming) => set({ isStreaming: streaming }),
  fetchConversations: async () => {
    set({ conversationsLoading: true });
    const { settings } = get();
    const result = await api.getConversations(settings.api_key);
    set({
      conversations: result.items,
      conversationsCount: result.totalCount,
      conversationsLoading: false,
    });
  },
  fetchMessages: async (conversationId: string) => {
    set({ messagesLoading: true });
    const { settings } = get();
    const msgs = await api.getMessages(conversationId, settings.api_key);
    set({
      messages: msgs,
      messagesLoading: false,
    });
  },

  // Prompts — no backend endpoint, keep mock data
  prompts: MOCK_PROMPTS,
  activePromptId: MOCK_PROMPTS[0].id,
  setActivePromptId: (id) => set({ activePromptId: id }),
  updatePromptTemplate: (id, text) =>
    set((state) => ({
      prompts: state.prompts.map((p) =>
        p.id === id ? { ...p, template_text: text, updated_at: 'Just now' } : p
      ),
    })),
  addPromptTemplate: (prompt) =>
    set((state) => ({
      prompts: [prompt, ...state.prompts],
      activePromptId: prompt.id,
    })),

  // Evals — no backend endpoint, keep mock data
  evals: MOCK_EVAL_BENCHMARKS,
  addEvalRun: (evalRun) =>
    set((state) => ({
      evals: [evalRun, ...state.evals],
    })),

  // UI
  commandPaletteOpen: false,
  setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),
  apiConnected: false,
  setApiConnected: (connected) => set({ apiConnected: connected }),

  // Aggregated dashboard fetch — hits all three list endpoints
  fetchDashboardData: async () => {
    const store = get();
    const apiKey = store.settings.api_key;

    // Fire all fetches in parallel
    const [docsResult, colsResult, convsResult, health] = await Promise.all([
      api.getDocuments(apiKey),
      api.getCollections(apiKey),
      api.getConversations(apiKey),
      api.checkHealth(apiKey),
    ]);

    set({
      documents: docsResult.items,
      documentsCount: docsResult.totalCount,
      collections: colsResult.items,
      collectionsCount: colsResult.totalCount,
      conversations: convsResult.items,
      conversationsCount: convsResult.totalCount,
      apiConnected: health,
    });
  },

  // Health check
  checkApiHealth: async () => {
    const { settings } = get();
    const healthy = await api.checkHealth(settings.api_key);
    set({ apiConnected: healthy });
  },
}));
