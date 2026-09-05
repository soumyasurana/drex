'use client';

import React, { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { useAppStore } from '@/lib/store';
import {
  Search,
  LayoutDashboard,
  FileText,
  FolderArchive,
  MessageSquare,
  History,
  GitMerge,
  Code2,
  BarChart3,
  Settings,
  Sparkles,
  ArrowRight,
  Database,
  Terminal,
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

export function CommandPalette() {
  const router = useRouter();
  const { commandPaletteOpen, setCommandPaletteOpen, documents, collections } = useAppStore();
  const [query, setQuery] = useState('');

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setCommandPaletteOpen(!commandPaletteOpen);
      }
      if (e.key === 'Escape' && commandPaletteOpen) {
        setCommandPaletteOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [commandPaletteOpen, setCommandPaletteOpen]);

  const navItems = [
    { name: 'Dashboard', icon: LayoutDashboard, path: '/dashboard', cat: 'Navigation' },
    { name: 'Documents', icon: FileText, path: '/documents', cat: 'Navigation' },
    { name: 'Collections', icon: FolderArchive, path: '/collections', cat: 'Navigation' },
    { name: 'Playground Chat', icon: MessageSquare, path: '/chat', cat: 'Navigation' },
    { name: 'Conversations History', icon: History, path: '/conversations', cat: 'Navigation' },
    { name: 'Retrieval Explorer', icon: GitMerge, path: '/retrieval', cat: 'Navigation' },
    { name: 'Prompt Studio', icon: Code2, path: '/prompts', cat: 'Navigation' },
    { name: 'Evaluation Benchmarks', icon: BarChart3, path: '/evaluation', cat: 'Navigation' },
    { name: 'Settings & API Keys', icon: Settings, path: '/settings', cat: 'Navigation' },
  ];

  const filteredNav = navItems.filter((i) =>
    i.name.toLowerCase().includes(query.toLowerCase())
  );

  const filteredDocs = documents
    .filter((d) => d.name.toLowerCase().includes(query.toLowerCase()))
    .slice(0, 3);

  const handleSelect = (path: string) => {
    setCommandPaletteOpen(false);
    setQuery('');
    router.push(path);
  };

  return (
    <AnimatePresence>
      {commandPaletteOpen && (
        <div className="fixed inset-0 z-50 flex items-start justify-center pt-24 px-4 bg-black/70 backdrop-blur-md">
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: -10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: -10 }}
            transition={{ duration: 0.15 }}
            className="w-full max-w-2xl overflow-hidden glass-panel rounded-2xl border border-white/10 shadow-2xl"
          >
            {/* Input Header */}
            <div className="flex items-center px-4 py-3.5 border-b border-white/10 bg-zinc-900/60">
              <Search className="w-5 h-5 text-zinc-400 mr-3 shrink-0" />
              <input
                type="text"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Type a command or search documents..."
                autoFocus
                className="w-full bg-transparent text-sm text-zinc-100 placeholder-zinc-500 focus:outline-none"
              />
              <kbd className="px-2 py-0.5 text-xs text-zinc-400 bg-zinc-800 rounded border border-zinc-700">
                ESC
              </kbd>
            </div>

            {/* Results Body */}
            <div className="max-h-96 overflow-y-auto p-2 space-y-4">
              {/* Navigation Commands */}
              {filteredNav.length > 0 && (
                <div>
                  <div className="px-3 py-1.5 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
                    Navigation
                  </div>
                  <div className="space-y-0.5">
                    {filteredNav.map((item) => {
                      const Icon = item.icon;
                      return (
                        <button
                          key={item.path}
                          onClick={() => handleSelect(item.path)}
                          className="w-full flex items-center justify-between px-3 py-2 rounded-xl text-sm text-zinc-300 hover:text-white hover:bg-indigo-600/20 hover:border-indigo-500/30 border border-transparent transition-all group"
                        >
                          <div className="flex items-center space-x-3">
                            <Icon className="w-4 h-4 text-zinc-400 group-hover:text-indigo-400" />
                            <span>{item.name}</span>
                          </div>
                          <ArrowRight className="w-4 h-4 opacity-0 group-hover:opacity-100 text-indigo-400 transition-opacity" />
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Documents Matches */}
              {filteredDocs.length > 0 && (
                <div>
                  <div className="px-3 py-1.5 text-[11px] font-semibold text-zinc-400 uppercase tracking-wider">
                    Documents
                  </div>
                  <div className="space-y-0.5">
                    {filteredDocs.map((doc) => (
                      <button
                        key={doc.id}
                        onClick={() => handleSelect('/documents')}
                        className="w-full flex items-center justify-between px-3 py-2 rounded-xl text-sm text-zinc-300 hover:text-white hover:bg-zinc-800/80 transition-all group"
                      >
                        <div className="flex items-center space-x-3">
                          <FileText className="w-4 h-4 text-emerald-400" />
                          <span className="truncate">{doc.name}</span>
                        </div>
                        <span className="text-xs text-zinc-500">{doc.chunks_count} chunks</span>
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {filteredNav.length === 0 && filteredDocs.length === 0 && (
                <div className="py-8 text-center text-sm text-zinc-400">
                  No matching commands or documents found for &quot;{query}&quot;
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="px-4 py-2.5 border-t border-white/10 bg-zinc-950/80 flex items-center justify-between text-xs text-zinc-400">
              <div className="flex items-center space-x-2">
                <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
                <span>Contextra AI Platform v1.0</span>
              </div>
              <div className="flex items-center space-x-3">
                <span>Use <kbd className="px-1 py-0.5 bg-zinc-800 rounded border border-zinc-700">↑</kbd> <kbd className="px-1 py-0.5 bg-zinc-800 rounded border border-zinc-700">↓</kbd> to navigate</span>
                <span><kbd className="px-1 py-0.5 bg-zinc-800 rounded border border-zinc-700">↵</kbd> to select</span>
              </div>
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
