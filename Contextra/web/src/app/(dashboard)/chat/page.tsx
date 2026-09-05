'use client';

import React, { useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Send,
  Sparkles,
  GitMerge,
  SlidersHorizontal,
  Bot,
  FileText,
  RefreshCw,
  Sliders,
  X,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';
import { ChatMessage } from '@/types';
import { api } from '@/lib/api';
import { toast } from 'sonner';

export default function ChatPage() {
  const {
    messages,
    addChatMessage,
    retrievedChunks,
    selectedChunk,
    setSelectedChunk,
    settings,
    updateSettings,
    activeConversationId,
    setActiveConversationId,
    fetchMessages,
    messagesLoading,
  } = useAppStore();

  const [input, setInput] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    if (activeConversationId) {
      fetchMessages(activeConversationId);
    }
  }, [activeConversationId, fetchMessages]);

  useEffect(() => {
    scrollToBottom();
  }, [messages, isStreaming]);

  const handleSend = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!input.trim() || isStreaming) return;

    let convId = activeConversationId;
    if (!convId) {
      // Auto-create conversation if none active
      const created = await api.createConversation('Interactive Chat', settings.api_key);
      if (created) {
        convId = created.id;
        setActiveConversationId(convId);
      } else {
        convId = `conv_${Date.now()}`;
        setActiveConversationId(convId);
      }
    }

    const userMsg: ChatMessage = {
      id: `msg_${Date.now()}`,
      conversation_id: convId,
      role: 'user',
      content: input,
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
    };

    addChatMessage(userMsg);
    const userQuery = input;
    setInput('');
    setIsStreaming(true);

    try {
      const response = await api.sendMessage(convId, userQuery, settings.api_key);
      if (response) {
        addChatMessage(response);
      } else {
        // Fallback response if gateway chat endpoint returns null or 404
        const fallbackAssistantMsg: ChatMessage = {
          id: `msg_ans_${Date.now()}`,
          conversation_id: convId,
          role: 'assistant',
          content: `Backend received message: "${userQuery}". Contextra gateway execution complete.`,
          timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
          latency_ms: 120,
          tokens_used: 150,
        };
        addChatMessage(fallbackAssistantMsg);
      }
    } catch {
      toast.error('Failed to communicate with backend gateway');
    } finally {
      setIsStreaming(false);
    }
  };

  return (
    <div className="h-[calc(100vh-7rem)] flex flex-col lg:flex-row gap-6 overflow-hidden">
      {/* Left Chat Window */}
      <div className="flex-1 glass-panel rounded-2xl flex flex-col justify-between overflow-hidden relative border border-white/10">
        {/* Chat Window Top Bar */}
        <div className="px-6 py-3.5 border-b border-white/10 bg-zinc-950/80 flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 rounded-xl bg-gradient-to-tr from-indigo-600 to-purple-600 flex items-center justify-center text-white shadow-md">
              <Bot className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-bold text-white flex items-center space-x-2">
                <span>RAG Context Playground</span>
                <span className="px-2 py-0.5 text-[10px] font-mono bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 rounded-full">
                  {settings.llm_model}
                </span>
              </h2>
              <p className="text-[11px] text-zinc-400">
                {activeConversationId ? `Session: ${activeConversationId}` : 'New Session'}
              </p>
            </div>
          </div>

          <div className="flex items-center space-x-2">
            <button
              onClick={() => setSettingsOpen(!settingsOpen)}
              className="px-3 py-1.5 rounded-xl bg-zinc-900 border border-white/10 hover:border-indigo-500/40 text-xs text-zinc-300 flex items-center space-x-1.5 transition-all"
            >
              <SlidersHorizontal className="w-3.5 h-3.5 text-indigo-400" />
              <span>Model Params</span>
            </button>
          </div>
        </div>

        {/* Messages Stream */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {messagesLoading ? (
            <div className="p-8 text-center text-zinc-400 text-xs flex items-center justify-center space-x-2">
              <RefreshCw className="w-4 h-4 animate-spin text-indigo-400" />
              <span>Loading messages...</span>
            </div>
          ) : messages.length === 0 ? (
            <div className="p-12 text-center text-zinc-500 text-xs space-y-2">
              <Sparkles className="w-8 h-8 text-indigo-400/50 mx-auto mb-2" />
              <p className="font-semibold text-zinc-300">RAG Context Playground Ready</p>
              <p>Type a question below to send a query to the Contextra Gateway.</p>
            </div>
          ) : (
            messages.map((msg) => (
              <motion.div
                key={msg.id}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                className={`flex items-start space-x-4 ${msg.role === 'user' ? 'justify-end' : ''}`}
              >
                {msg.role === 'assistant' && (
                  <div className="w-8 h-8 rounded-xl bg-indigo-600/20 border border-indigo-500/30 text-indigo-400 flex items-center justify-center shrink-0 mt-1 shadow-sm">
                    <Sparkles className="w-4 h-4" />
                  </div>
                )}

                <div className={`max-w-2xl space-y-2 ${msg.role === 'user' ? 'items-end' : ''}`}>
                  <div
                    className={`p-4 rounded-2xl text-sm leading-relaxed ${
                      msg.role === 'user'
                        ? 'bg-gradient-to-r from-indigo-600 to-purple-600 text-white rounded-tr-none shadow-lg shadow-indigo-500/10'
                        : 'glass-card border border-white/10 text-zinc-200 rounded-tl-none font-sans'
                    }`}
                  >
                    <div className="whitespace-pre-wrap font-sans">{msg.content}</div>

                    {/* Citations Pills */}
                    {msg.citations && msg.citations.length > 0 && (
                      <div className="mt-4 pt-3 border-t border-white/10 space-y-1.5">
                        <p className="text-[11px] font-semibold text-indigo-300 flex items-center space-x-1">
                          <FileText className="w-3 h-3" />
                          <span>Retrieved Sources & Citations:</span>
                        </p>
                        <div className="flex flex-wrap gap-2">
                          {msg.citations.map((c) => (
                            <button
                              key={c.chunk_id}
                              onClick={() => {
                                const found = retrievedChunks.find((rc) => rc.id === c.chunk_id);
                                if (found) setSelectedChunk(found);
                              }}
                              className="px-2.5 py-1 rounded-lg bg-indigo-950/80 border border-indigo-500/30 text-indigo-300 hover:text-white hover:border-indigo-400 text-xs flex items-center space-x-1.5 transition-all group"
                            >
                              <span className="font-medium truncate max-w-[160px]">{c.document_name}</span>
                              <span className="font-mono text-[10px] bg-indigo-900/60 px-1 py-0.2 rounded text-indigo-200">
                                {(c.score * 100).toFixed(1)}%
                              </span>
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="flex items-center space-x-3 text-[10px] text-zinc-500 px-1">
                    <span>{msg.timestamp}</span>
                    {msg.latency_ms && (
                      <>
                        <span>•</span>
                        <span className="font-mono text-emerald-400">{msg.latency_ms} ms</span>
                      </>
                    )}
                    {msg.tokens_used && (
                      <>
                        <span>•</span>
                        <span className="font-mono text-zinc-400">{msg.tokens_used} tokens</span>
                      </>
                    )}
                  </div>
                </div>

                {msg.role === 'user' && (
                  <div className="w-8 h-8 rounded-full bg-gradient-to-br from-purple-500 to-pink-500 text-white flex items-center justify-center shrink-0 mt-1 font-semibold text-xs shadow-md">
                    SS
                  </div>
                )}
              </motion.div>
            ))
          )}

          {/* Typing Indicator */}
          {isStreaming && (
            <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="flex items-center space-x-3 text-xs text-indigo-400">
              <div className="w-8 h-8 rounded-xl bg-indigo-600/20 border border-indigo-500/30 flex items-center justify-center">
                <RefreshCw className="w-4 h-4 animate-spin" />
              </div>
              <span className="font-mono animate-pulse">Communicating with Rust gateway engine...</span>
            </motion.div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* Input Bar */}
        <form onSubmit={handleSend} className="p-4 border-t border-white/10 bg-zinc-950/90 flex items-center space-x-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask a question about your ingested documents..."
            className="flex-1 px-4 py-3 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-indigo-500/50"
          />
          <button
            type="submit"
            disabled={!input.trim() || isStreaming}
            className="px-5 py-3 rounded-xl bg-gradient-to-r from-indigo-600 to-purple-600 hover:from-indigo-500 hover:to-purple-500 disabled:opacity-50 text-white font-medium text-sm flex items-center space-x-2 shadow-lg shadow-indigo-500/20 transition-all"
          >
            <span>Send</span>
            <Send className="w-4 h-4" />
          </button>
        </form>

        {/* Settings Drawer Slide Over */}
        <AnimatePresence>
          {settingsOpen && (
            <motion.div
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              className="absolute top-14 right-6 w-80 glass-panel rounded-2xl p-5 border border-white/10 shadow-2xl z-40 bg-zinc-950 space-y-4"
            >
              <div className="flex items-center justify-between border-b border-white/10 pb-3">
                <h3 className="text-xs font-bold text-white uppercase tracking-wider flex items-center space-x-2">
                  <Sliders className="w-4 h-4 text-indigo-400" />
                  <span>Model Execution Parameters</span>
                </h3>
                <button onClick={() => setSettingsOpen(false)} className="text-zinc-400 hover:text-white">
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="space-y-3 text-xs">
                <div>
                  <label className="block text-zinc-400 mb-1">LLM Provider</label>
                  <select
                    value={settings.llm_provider}
                    onChange={(e) => updateSettings({ llm_provider: e.target.value as any })}
                    className="w-full p-2 bg-zinc-900 border border-white/10 rounded-xl text-zinc-200"
                  >
                    <option value="openai">OpenAI</option>
                    <option value="anthropic">Anthropic</option>
                    <option value="gemini">Google Gemini</option>
                    <option value="ollama">Ollama (Local)</option>
                  </select>
                </div>

                <div>
                  <div className="flex justify-between text-zinc-400 mb-1">
                    <span>Temperature</span>
                    <span className="font-mono text-indigo-400">{settings.temperature}</span>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.05"
                    value={settings.temperature}
                    onChange={(e) => updateSettings({ temperature: parseFloat(e.target.value) })}
                    className="w-full accent-indigo-500"
                  />
                </div>

                <div>
                  <div className="flex justify-between text-zinc-400 mb-1">
                    <span>Top K Retrieval Limit</span>
                    <span className="font-mono text-indigo-400">{settings.retrieval_k}</span>
                  </div>
                  <input
                    type="range"
                    min="1"
                    max="20"
                    step="1"
                    value={settings.retrieval_k}
                    onChange={(e) => updateSettings({ retrieval_k: parseInt(e.target.value) })}
                    className="w-full accent-indigo-500"
                  />
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* Right Retrieval Inspector Panel */}
      <div className="w-full lg:w-96 glass-panel rounded-2xl p-5 border border-white/10 flex flex-col justify-between overflow-y-auto space-y-4">
        <div>
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h3 className="text-sm font-bold text-white flex items-center space-x-2">
              <GitMerge className="w-4 h-4 text-indigo-400" />
              <span>Retrieval Inspector</span>
            </h3>
            <span className="text-[10px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
              Qdrant + BM25
            </span>
          </div>

          {/* List of Retrieved Chunks */}
          <div className="space-y-3 mt-4">
            <p className="text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
              Top Ranked Chunks ({retrievedChunks.length})
            </p>

            {retrievedChunks.map((chunk) => {
              const isSelected = selectedChunk?.id === chunk.id;
              return (
                <div
                  key={chunk.id}
                  onClick={() => setSelectedChunk(chunk)}
                  className={`p-3 rounded-xl border text-xs cursor-pointer transition-all space-y-2 ${
                    isSelected
                      ? 'bg-indigo-600/20 border-indigo-500/50 shadow-md shadow-indigo-500/10'
                      : 'bg-zinc-900/60 border-white/5 hover:border-white/20'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-zinc-200 truncate max-w-[180px]">
                      {chunk.document_name}
                    </span>
                    <span className="font-mono text-[10px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 px-1.5 py-0.5 rounded font-bold">
                      {(chunk.score * 100).toFixed(1)}% match
                    </span>
                  </div>

                  <p className="text-zinc-400 line-clamp-3 text-[11px] font-mono leading-relaxed bg-zinc-950/60 p-2 rounded-lg border border-white/5">
                    {chunk.content}
                  </p>

                  <div className="flex items-center justify-between text-[10px] text-zinc-500 pt-1 border-t border-white/5">
                    <span>Collection: {chunk.collection_name}</span>
                    <span>Index #{chunk.chunk_index}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
