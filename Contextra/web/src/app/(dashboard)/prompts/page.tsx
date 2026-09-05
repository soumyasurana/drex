'use client';

import React, { useState } from 'react';
import dynamic from 'next/dynamic';
import { motion, AnimatePresence } from 'framer-motion';
import {
  Code2,
  Save,
  Play,
  History,
  Plus,
  Sparkles,
  Layers,
  Copy,
  Check,
  CheckCircle2,
  FileCode,
  Sliders,
  X,
} from 'lucide-react';
import { useAppStore } from '@/lib/store';
import { PromptTemplate } from '@/types';
import { toast } from 'sonner';

// Dynamically import Monaco Editor to avoid SSR window issues
const Editor = dynamic(() => import('@monaco-editor/react'), { ssr: false });

export default function PromptStudioPage() {
  const { prompts, activePromptId, setActivePromptId, updatePromptTemplate, addPromptTemplate } = useAppStore();

  const activePrompt = prompts.find((p) => p.id === activePromptId) || prompts[0];

  const [editorText, setEditorText] = useState(activePrompt?.template_text || '');
  const [testVariables, setTestVariables] = useState<Record<string, string>>({
    user_query: 'Explain Contextra RRF Merging algorithm.',
    retrieved_context: 'Contextra merges Qdrant vector scores with BM25 keyword ranks via RRF k=60.',
    conversation_summary: 'User previously inquired about system latency benchmarks.',
  });
  const [testResult, setTestResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [createModalOpen, setCreateModalOpen] = useState(false);

  // Form state
  const [newName, setNewName] = useState('');
  const [newDesc, setNewDesc] = useState('');

  const handleSelectPrompt = (p: PromptTemplate) => {
    setActivePromptId(p.id);
    setEditorText(p.template_text);
    setTestResult(null);
  };

  const handleSave = () => {
    updatePromptTemplate(activePrompt.id, editorText);
    toast.success(`Saved prompt template "${activePrompt.name}"!`);
  };

  const handleTestPrompt = () => {
    setTesting(true);
    setTestResult(null);
    setTimeout(() => {
      let rendered = editorText;
      Object.entries(testVariables).forEach(([k, v]) => {
        rendered = rendered.replace(new RegExp(`{{\\s*${k}\\s*}}`, 'g'), v);
      });
      setTestResult(
        `--- SIMULATED LLM RESPONSE ---\n\nContextra RRF Merging algorithm calculates unified relevance by combining vector rank and keyword rank using constant k=60.\n\nExecution Time: 128 ms | Model: GPT-4o`
      );
      setTesting(false);
    }, 800);
  };

  const handleCreatePrompt = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newName.trim()) return;

    const newP: PromptTemplate = {
      id: `prompt_${Date.now()}`,
      name: newName,
      description: newDesc || 'Custom prompt template',
      version: 'v1.0.0',
      variables: ['user_query', 'retrieved_context'],
      template_text: `You are an AI assistant.\n\nCONTEXT:\n{{retrieved_context}}\n\nQUERY:\n{{user_query}}`,
      created_at: new Date().toISOString().split('T')[0],
      updated_at: 'Just now',
    };

    addPromptTemplate(newP);
    setEditorText(newP.template_text);
    setCreateModalOpen(false);
    setNewName('');
    setNewDesc('');
    toast.success(`Created template "${newP.name}"`);
  };

  return (
    <div className="h-[calc(100vh-7rem)] flex flex-col lg:flex-row gap-6 overflow-hidden">
      {/* Sidebar: Prompt Library & Versions */}
      <div className="w-full lg:w-72 glass-panel rounded-2xl p-4 flex flex-col justify-between overflow-y-auto space-y-4 border border-white/10 shrink-0">
        <div className="space-y-4">
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h2 className="text-sm font-bold text-white flex items-center space-x-2">
              <Code2 className="w-4 h-4 text-amber-400" />
              <span>Prompt Library</span>
            </h2>
            <button
              onClick={() => setCreateModalOpen(true)}
              className="p-1 rounded-lg bg-amber-500/10 hover:bg-amber-500/20 text-amber-400 border border-amber-500/30"
              title="New Prompt Template"
            >
              <Plus className="w-4 h-4" />
            </button>
          </div>

          <div className="space-y-1.5">
            {prompts.map((p) => {
              const isSelected = p.id === activePrompt.id;
              return (
                <button
                  key={p.id}
                  onClick={() => handleSelectPrompt(p)}
                  className={`w-full text-left p-3 rounded-xl border text-xs transition-all space-y-1 ${
                    isSelected
                      ? 'bg-amber-500/15 border-amber-500/40 text-white shadow-md shadow-amber-500/10'
                      : 'bg-zinc-900/60 border-white/5 text-zinc-400 hover:text-zinc-200 hover:bg-white/5'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-zinc-100 truncate">{p.name}</span>
                    <span className="font-mono text-[10px] bg-amber-500/20 text-amber-300 px-1.5 py-0.5 rounded">
                      {p.version}
                    </span>
                  </div>
                  <p className="text-[11px] text-zinc-400 line-clamp-1">{p.description}</p>
                </button>
              );
            })}
          </div>
        </div>

        {/* Version History Card */}
        <div className="p-3 rounded-xl bg-zinc-950 border border-white/10 text-xs space-y-2">
          <div className="flex items-center justify-between text-zinc-300 font-semibold">
            <span className="flex items-center space-x-1.5">
              <History className="w-3.5 h-3.5 text-amber-400" />
              <span>Version History</span>
            </span>
            <span className="font-mono text-[11px] text-amber-400">{activePrompt.version}</span>
          </div>
          <p className="text-[11px] text-zinc-400">Last edited: {activePrompt.updated_at}</p>
        </div>
      </div>

      {/* Main Monaco Code Editor Area */}
      <div className="flex-1 glass-panel rounded-2xl flex flex-col justify-between overflow-hidden border border-white/10">
        {/* Editor Toolbar */}
        <div className="px-6 py-3.5 border-b border-white/10 bg-zinc-950/80 flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <FileCode className="w-5 h-5 text-amber-400" />
            <div>
              <h2 className="text-sm font-bold text-white flex items-center space-x-2">
                <span>{activePrompt.name}</span>
                <span className="text-xs text-zinc-400 font-mono font-normal">({activePrompt.version})</span>
              </h2>
              <p className="text-[11px] text-zinc-400">{activePrompt.description}</p>
            </div>
          </div>

          <div className="flex items-center space-x-3">
            <button
              onClick={handleSave}
              className="px-3.5 py-1.5 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-white font-medium text-xs flex items-center space-x-1.5 transition-all border border-white/10"
            >
              <Save className="w-3.5 h-3.5 text-emerald-400" />
              <span>Save Template</span>
            </button>
            <button
              onClick={handleTestPrompt}
              disabled={testing}
              className="px-4 py-1.5 rounded-xl bg-gradient-to-r from-amber-500 to-orange-600 hover:from-amber-400 hover:to-orange-500 text-white font-semibold text-xs flex items-center space-x-1.5 shadow-md shadow-amber-500/20 transition-all"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>{testing ? 'Testing...' : 'Test Execution'}</span>
            </button>
          </div>
        </div>

        {/* Monaco Editor Container */}
        <div className="flex-1 w-full bg-[#1e1e1e] relative">
          <Editor
            height="100%"
            defaultLanguage="markdown"
            theme="vs-dark"
            value={editorText}
            onChange={(val) => setEditorText(val || '')}
            options={{
              fontSize: 13,
              minimap: { enabled: false },
              wordWrap: 'on',
              lineNumbers: 'on',
              scrollBeyondLastLine: false,
              padding: { top: 12 },
            }}
          />
        </div>

        {/* Test Output Panel if generated */}
        {testResult && (
          <div className="p-4 border-t border-white/10 bg-zinc-950 text-xs font-mono text-emerald-400 max-h-40 overflow-y-auto whitespace-pre-wrap">
            {testResult}
          </div>
        )}
      </div>

      {/* Right Variable Inspector */}
      <div className="w-full lg:w-80 glass-panel rounded-2xl p-4 flex flex-col justify-between overflow-y-auto space-y-4 border border-white/10 shrink-0">
        <div>
          <div className="flex items-center justify-between border-b border-white/10 pb-3">
            <h3 className="text-xs font-bold text-white uppercase tracking-wider flex items-center space-x-2">
              <Sliders className="w-4 h-4 text-amber-400" />
              <span>Template Variables</span>
            </h3>
            <span className="text-[10px] text-zinc-400 font-mono">{activePrompt.variables.length} vars</span>
          </div>

          <div className="space-y-3 mt-4 text-xs">
            {activePrompt.variables.map((v) => (
              <div key={v} className="space-y-1">
                <label className="block text-amber-300 font-mono font-semibold text-[11px]">
                  {`{{ ${v} }}`}
                </label>
                <textarea
                  rows={2}
                  value={testVariables[v] || ''}
                  onChange={(e) =>
                    setTestVariables((prev) => ({ ...prev, [v]: e.target.value }))
                  }
                  className="w-full p-2 bg-zinc-900 border border-white/10 rounded-xl text-zinc-200 text-xs focus:outline-none focus:border-amber-500/50 font-sans"
                />
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Create Template Modal */}
      <AnimatePresence>
        {createModalOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-md">
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              className="w-full max-w-md glass-panel rounded-2xl p-6 border border-white/10 shadow-2xl relative"
            >
              <div className="flex items-center justify-between border-b border-white/10 pb-4 mb-4">
                <h3 className="text-lg font-bold text-white flex items-center space-x-2">
                  <Code2 className="w-5 h-5 text-amber-400" />
                  <span>Create Prompt Template</span>
                </h3>
                <button onClick={() => setCreateModalOpen(false)} className="p-1 rounded-lg text-zinc-400 hover:text-white">
                  <X className="w-5 h-5" />
                </button>
              </div>

              <form onSubmit={handleCreatePrompt} className="space-y-4">
                <div>
                  <label className="block text-xs font-semibold text-zinc-300 mb-1">Template Name</label>
                  <input
                    type="text"
                    required
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    placeholder="e.g. system_prompt_v3"
                    className="w-full px-3.5 py-2 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 focus:outline-none focus:border-amber-500/50"
                  />
                </div>

                <div>
                  <label className="block text-xs font-semibold text-zinc-300 mb-1">Description</label>
                  <input
                    type="text"
                    value={newDesc}
                    onChange={(e) => setNewDesc(e.target.value)}
                    placeholder="Brief description of the prompt..."
                    className="w-full px-3.5 py-2 bg-zinc-900 border border-white/10 rounded-xl text-sm text-zinc-100 focus:outline-none focus:border-amber-500/50"
                  />
                </div>

                <div className="flex items-center justify-end space-x-3 pt-4 border-t border-white/10">
                  <button
                    type="button"
                    onClick={() => setCreateModalOpen(false)}
                    className="px-4 py-2 bg-zinc-800 text-zinc-300 hover:text-white rounded-xl text-xs font-medium"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    className="px-4 py-2 bg-gradient-to-r from-amber-500 to-orange-600 hover:from-amber-400 hover:to-orange-500 text-white rounded-xl text-xs font-semibold shadow-md"
                  >
                    Create Template
                  </button>
                </div>
              </form>
            </motion.div>
          </div>
        )}
      </AnimatePresence>
    </div>
  );
}
