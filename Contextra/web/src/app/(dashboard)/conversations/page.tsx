'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { motion } from 'framer-motion';
import {
  History,
  Clock,
  ArrowRight,
  Search,
  BrainCircuit,
  Plus,
  RefreshCw,
  MessageSquare,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';
import { api } from '@/lib/api';
import { toast } from 'sonner';

export default function ConversationsPage() {
  const router = useRouter();
  const {
    conversations,
    conversationsLoading,
    settings,
    fetchConversations,
    setActiveConversationId,
  } = useAppStore();

  const [searchQuery, setSearchQuery] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    fetchConversations();
  }, [fetchConversations]);

  const filtered = conversations.filter(
    (c) =>
      c.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
      c.summary.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const handleOpenConversation = (convId: string) => {
    setActiveConversationId(convId);
    router.push('/chat');
  };

  const handleCreateNewConversation = async () => {
    if (isCreating) return;
    setIsCreating(true);
    try {
      const created = await api.createConversation('New Context Session', settings.api_key);
      if (created) {
        toast.success('Created new conversation session');
        setActiveConversationId(created.id);
        router.push('/chat');
      } else {
        const fallbackId = `conv_${Date.now()}`;
        setActiveConversationId(fallbackId);
        router.push('/chat');
      }
    } catch {
      toast.error('Failed to create conversation');
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Page Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-white flex items-center space-x-2">
            <History className="w-6 h-6 text-teal-400" />
            <span>Conversation Memory History</span>
          </h1>
          <p className="text-sm text-zinc-400 mt-1">
            Timeline of stored conversation threads, dynamic importance summaries, and token memory footprints.
          </p>
        </div>

        <div className="flex items-center space-x-3">
          <button
            onClick={() => fetchConversations()}
            className="px-3.5 py-2.5 rounded-xl glass-card text-xs text-zinc-300 hover:text-white flex items-center space-x-2 transition-colors"
          >
            <RefreshCw className={`w-4 h-4 ${conversationsLoading ? 'animate-spin' : ''}`} />
            <span>Sync</span>
          </button>
          <button
            onClick={handleCreateNewConversation}
            disabled={isCreating}
            className="px-4 py-2.5 rounded-xl bg-gradient-to-r from-teal-600 to-indigo-600 hover:from-teal-500 hover:to-indigo-500 text-white font-medium text-sm flex items-center justify-center space-x-2 shadow-lg shadow-teal-500/25 transition-all hover:scale-[1.02]"
          >
            {isCreating ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Plus className="w-4 h-4" />}
            <span>New Session</span>
          </button>
        </div>
      </div>

      {/* Search Filter */}
      <div className="glass-panel p-4 rounded-2xl">
        <div className="relative w-full max-w-md">
          <Search className="w-4 h-4 text-zinc-400 absolute left-3 top-3" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search conversation memory..."
            className="w-full pl-9 pr-4 py-2 bg-zinc-900/80 border border-white/10 rounded-xl text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none focus:border-teal-500/50"
          />
        </div>
      </div>

      {/* Timeline UI List */}
      {conversationsLoading ? (
        <div className="p-12 text-center text-zinc-400 text-sm flex items-center justify-center space-x-2">
          <RefreshCw className="w-4 h-4 animate-spin text-teal-400" />
          <span>Loading conversations from backend...</span>
        </div>
      ) : filtered.length === 0 ? (
        <div className="glass-panel rounded-2xl p-12 text-center text-zinc-500 space-y-3">
          <MessageSquare className="w-12 h-12 text-zinc-600 mx-auto" />
          <p className="text-sm font-semibold text-zinc-300">No conversation threads stored</p>
          <p className="text-xs text-zinc-500">Start a new conversation session to build persistent long-term memory.</p>
        </div>
      ) : (
        <div className="relative border-l-2 border-white/10 ml-4 pl-6 space-y-6">
          {filtered.map((conv, idx) => (
            <motion.div
              key={conv.id}
              initial={{ opacity: 0, x: -15 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ duration: 0.2, delay: idx * 0.05 }}
              className="relative group"
            >
              {/* Timeline Dot */}
              <div className="absolute -left-[31px] top-1.5 w-4 h-4 rounded-full bg-zinc-900 border-2 border-teal-500 group-hover:scale-125 group-hover:bg-teal-500 transition-all shadow-md shadow-teal-500/20" />

              <div className="glass-card glass-card-hover rounded-2xl p-6 space-y-4">
                <div className="flex flex-col md:flex-row md:items-center justify-between gap-2 border-b border-white/5 pb-3">
                  <div className="flex items-center space-x-3">
                    <div className="w-8 h-8 rounded-xl bg-teal-500/10 border border-teal-500/20 text-teal-400 flex items-center justify-center font-mono font-bold text-xs">
                      {conv.message_count || 0}
                    </div>
                    <div>
                      <h3 className="text-base font-bold text-white group-hover:text-teal-300 transition-colors">
                        {conv.title}
                      </h3>
                      <p className="text-[11px] text-zinc-500 font-mono">ID: {conv.id}</p>
                    </div>
                  </div>

                  <div className="flex items-center space-x-4 text-xs font-mono text-zinc-400">
                    <button
                      onClick={() => handleOpenConversation(conv.id)}
                      className="px-3 py-1.5 rounded-xl bg-teal-500/10 hover:bg-teal-500/20 text-teal-300 border border-teal-500/30 flex items-center space-x-1.5 transition-all text-xs font-semibold"
                    >
                      <span>Resume Session</span>
                      <ArrowRight className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {/* Memory Summary Card */}
                {conv.summary && (
                  <div className="p-4 rounded-xl bg-zinc-950/80 border border-white/5 space-y-2">
                    <div className="flex items-center space-x-2 text-xs font-semibold text-teal-400">
                      <BrainCircuit className="w-4 h-4" />
                      <span>Rolling Importance Summary:</span>
                    </div>
                    <p className="text-xs text-zinc-300 leading-relaxed font-sans">{conv.summary}</p>
                  </div>
                )}
              </div>
            </motion.div>
          ))}
        </div>
      )}
    </div>
  );
}
